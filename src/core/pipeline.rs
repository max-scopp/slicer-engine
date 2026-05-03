use clipper2::Paths;

use crate::logging::{phases, PhaseTimer, ProcessLogger};
use crate::mesh::types::Mesh;
use crate::settings::params::SlicingParams;

use super::infill::{add_infill_to_layers, calculate_interior_region};
use super::slicer::slice_mesh;
use super::surfaces::{generate_top_bottom_surfaces_with_interior, SurfaceConfig};
use super::types::SliceLayer;
use super::walls::apply_single_wall_restrictions;

/// Central entry point for the complete slicing pipeline.
///
/// This function processes a mesh through the entire slicing pipeline, including
/// basic slicing, top/bottom surface generation, and Arachne variable-width
/// perimeter generation.  All pipeline progress is reported through `logger`
/// so that CLI and WebSocket callers receive the same verbosity and information.
///
/// # Arguments
/// * `mesh` - The triangle mesh to process
/// * `params` - Slicing parameters controlling all aspects of the slicing process
/// * `logger` - Pipeline logger; use [`NullLogger`] when logging is not needed
///
/// # Returns
/// A `Vec<SliceLayer>` with all processing applied (Arachne walls, surfaces, etc.).
///
/// # Example
/// ```
/// use slicer_engine::logging::NullLogger;
/// use slicer_engine::mesh::types::Mesh;
/// use slicer_engine::settings::params::SlicingParams;
/// use slicer_engine::core::process_mesh;
///
/// let mesh = Mesh::new(); // Load your mesh
/// let params = SlicingParams::default();
/// let layers = process_mesh(&mesh, &params, &NullLogger);
/// ```
pub fn process_mesh(
    mesh: &Mesh,
    params: &SlicingParams,
    logger: &dyn ProcessLogger,
) -> Vec<SliceLayer> {
    logger.log_info(&format!("processing mesh: {} triangles", mesh.faces.len()));

    let t_slicing = PhaseTimer::start(phases::SLICING, logger);
    logger.log_debug("slicing mesh…");
    let mut layers = slice_mesh(mesh, params.layer_height);
    logger.log_info(&format!("sliced into {} layers", layers.len()));
    t_slicing.finish();

    if logger.is_cancelled() {
        logger.log_info("slice cancelled after slicing phase");
        return layers;
    }

    // Generate Arachne walls FIRST from the raw mesh contours
    logger.log_debug(&format!(
        "generating Arachne walls (wall_count: {}, nozzle: {}mm)",
        params.wall_count, params.nozzle_diameter_mm
    ));
    let arachne_params = crate::arachne::ArachneParams::from_slicing_params(params);
    let t_arachne = PhaseTimer::start(phases::ARACHNE_WALLS, logger);
    let arachne_timings = crate::arachne::generate_arachne_walls(&mut layers, &arachne_params);
    t_arachne.finish();
    logger.log_debug(&format!(
        "arachne sub-timings (CPU total across threads): collapse_depth {} ms, bead_shrinks {} ms",
        arachne_timings.collapse_depth_ms, arachne_timings.bead_shrink_ms,
    ));
    logger.log_debug("Arachne wall generation complete");

    if logger.is_cancelled() {
        logger.log_info("slice cancelled after wall generation phase");
        return layers;
    }

    // Pre-compute infill interior regions while all Arachne walls are still
    // present.  These are passed to add_infill_to_layers so that the
    // subsequent apply_single_wall_restrictions step (which strips inner walls
    // from certain layers) cannot accidentally expand the infill boundary into
    // the space the stripped walls occupied.
    //
    // Without this, a layer that has a small top-surface feature (e.g. the top
    // of an embossed letter on a calibration cube) loses inner walls for ALL
    // of its islands, causing calculate_interior_region to see walls_per_island
    // = 1 everywhere and place sparse infill far into the wall zone.
    let pre_strip_infill_regions: Option<Vec<Paths>> = if params.infill_density > 0.0
        && (params.only_one_wall_first_layer || params.only_one_wall_top)
    {
        let t_snapshot = PhaseTimer::start(phases::INFILL_REGION_SNAPSHOT, logger);
        #[cfg(not(target_arch = "wasm32"))]
        let result = {
            use rayon::prelude::*;
            Some(
                layers
                    .par_iter()
                    .map(|layer| {
                        calculate_interior_region(
                            layer,
                            0.0,
                            params.nozzle_diameter_mm,
                            params.wall_count,
                        )
                    })
                    .collect(),
            )
        };
        #[cfg(target_arch = "wasm32")]
        let result = Some(
            layers
                .iter()
                .map(|layer| {
                    calculate_interior_region(
                        layer,
                        0.0,
                        params.nozzle_diameter_mm,
                        params.wall_count,
                    )
                })
                .collect(),
        );
        t_snapshot.finish();
        result
    } else {
        None
    };

    // Apply single-wall restrictions to first/last-of-run layers if configured.
    //
    // Per-island detection runs inside apply_single_wall_restrictions so that
    // only the islands that actually end their top-surface run get stripped.
    // Previously the whole layer was stripped whenever any one island qualified,
    // which caused the infill boundary to over-expand into the wall zone for
    // the unaffected (continuing) islands on the same layer.
    if params.only_one_wall_first_layer || params.only_one_wall_top {
        logger.log_debug("applying single-wall restrictions (per-island)");
        let t_wall_restrictions = PhaseTimer::start(phases::WALL_RESTRICTIONS, logger);
        apply_single_wall_restrictions(&mut layers, params);
        t_wall_restrictions.finish();
    }

    // Calculate interior regions (inside walls) for each layer where surfaces will go
    let interior_regions: Vec<Paths> = if params.top_layers > 0 || params.bottom_layers > 0 {
        logger.log_debug("calculating interior regions for surfaces");
        let t_interior = PhaseTimer::start(phases::INTERIOR_REGIONS, logger);
        #[cfg(not(target_arch = "wasm32"))]
        let result = {
            use rayon::prelude::*;
            layers
                .par_iter()
                .map(|layer| {
                    calculate_interior_region(
                        layer,
                        params.infill_overlap_percent,
                        params.nozzle_diameter_mm,
                        params.wall_count,
                    )
                })
                .collect()
        };
        #[cfg(target_arch = "wasm32")]
        let result = layers
            .iter()
            .map(|layer| {
                calculate_interior_region(
                    layer,
                    params.infill_overlap_percent,
                    params.nozzle_diameter_mm,
                    params.wall_count,
                )
            })
            .collect();
        t_interior.finish();
        result
    } else {
        vec![]
    };

    // Now generate top/bottom surfaces INSIDE the walls
    if params.top_layers > 0 || params.bottom_layers > 0 {
        let t_surfaces = PhaseTimer::start(phases::SURFACES, logger);
        logger.log_debug(&format!(
            "generating surfaces (top: {}, bottom: {}, angle: {}°)",
            params.top_layers, params.bottom_layers, params.surface_infill_angle
        ));
        let surface_timings = generate_top_bottom_surfaces_with_interior(
            &mut layers,
            &SurfaceConfig {
                top_layers: params.top_layers,
                bottom_layers: params.bottom_layers,
                layer_height: params.layer_height,
                infill_angle: params.surface_infill_angle,
                nozzle_diameter_mm: params.nozzle_diameter_mm,
                bridge_flow_ratio: params.bridge_flow_ratio,
            },
            Some(&interior_regions),
        );
        logger.log_debug("surface generation complete");
        t_surfaces.finish();
        logger.log_debug(&format!(
            "surface sub-timings: perimeter_snapshot {} ms, detection {} ms, infill_gen {} ms",
            surface_timings.perimeter_snapshot_ms,
            surface_timings.detection_ms,
            surface_timings.infill_gen_ms,
        ));
    }

    // Add infill
    if params.infill_density > 0.0 {
        let infill_pattern = params.infill_pattern;

        logger.log_debug(&format!(
            "generating {} infill at {:.0}% density, {}° base angle…",
            infill_pattern.name(),
            params.infill_density * 100.0,
            params.infill_base_angle
        ));

        let t_infill = PhaseTimer::start(phases::INFILL, logger);
        add_infill_to_layers(
            &mut layers,
            params.infill_density,
            infill_pattern,
            params.infill_base_angle,
            params.nozzle_diameter_mm,
            params.infill_perimeter_gap_mm,
            pre_strip_infill_regions.as_deref(),
        );
        t_infill.finish();
        logger.log_debug("infill generation complete");
    }

    // Optimize path order (Greedy TSP within role groups)
    let t_tsp = PhaseTimer::start("Path Ordering", logger);
    for layer in layers.iter_mut() {
        let path_count = layer.paths.len();
        if path_count <= 1 {
            continue;
        }

        let paths_vec: Vec<_> = layer.paths.iter().cloned().collect();
        let mut ordered_paths = clipper2::Paths::default();
        let mut ordered_roles = Vec::with_capacity(path_count);
        let mut ordered_widths = Vec::with_capacity(path_count);

        let mut current_pos = (0.0, 0.0);

        // Group into contiguous ranges of the same role to preserve wall/infill print order
        let mut groups = Vec::new();
        let mut current_group = Vec::new();
        let mut current_group_role = layer.role_for_path(0);

        for (i, _) in paths_vec.iter().enumerate() {
            let role = layer.role_for_path(i);
            if role != current_group_role && !current_group.is_empty() {
                groups.push((current_group_role, current_group.clone()));
                current_group.clear();
                current_group_role = role;
            }
            current_group.push(i);
        }
        if !current_group.is_empty() {
            groups.push((current_group_role, current_group));
        }

        for (role, mut remaining) in groups {
            let is_closed = matches!(
                role,
                crate::core::ExtrusionRole::OuterWall
                    | crate::core::ExtrusionRole::InnerWall
                    | crate::core::ExtrusionRole::Skirt
            );

            while !remaining.is_empty() {
                let mut best_i = 0;
                let mut min_dist_sq = f64::MAX;
                let mut best_reverse = false;

                for (i, &path_idx) in remaining.iter().enumerate() {
                    let path = &paths_vec[path_idx];
                    if path.is_empty() {
                        continue;
                    }

                    let p_start = path.iter().next().unwrap();
                    let dx1 = p_start.x() - current_pos.0;
                    let dy1 = p_start.y() - current_pos.1;
                    let dist1 = dx1 * dx1 + dy1 * dy1;

                    if dist1 < min_dist_sq {
                        min_dist_sq = dist1;
                        best_i = i;
                        best_reverse = false;
                    }

                    // For open paths, we could also print them backwards!
                    if !is_closed {
                        let p_end = path.iter().last().unwrap();
                        let dx2 = p_end.x() - current_pos.0;
                        let dy2 = p_end.y() - current_pos.1;
                        let dist2 = dx2 * dx2 + dy2 * dy2;
                        if dist2 < min_dist_sq {
                            min_dist_sq = dist2;
                            best_i = i;
                            best_reverse = true;
                        }
                    }
                }

                let best_path_idx = remaining.remove(best_i);
                let path = &paths_vec[best_path_idx];

                let mut final_path = clipper2::Path::default();
                if best_reverse {
                    for p in path.iter().rev() {
                        final_path.push(*p);
                    }
                } else {
                    for p in path.iter() {
                        final_path.push(*p);
                    }
                }

                if !final_path.is_empty() {
                    if is_closed {
                        let p = final_path.iter().next().unwrap();
                        current_pos = (p.x(), p.y());
                    } else {
                        let p = final_path.iter().last().unwrap();
                        current_pos = (p.x(), p.y());
                    }
                }

                ordered_paths.push(final_path);
                ordered_roles.push(layer.role_for_path(best_path_idx));
                ordered_widths.push(layer.width_for_path(best_path_idx));
            }
        }

        layer.paths = ordered_paths;
        layer.path_roles = ordered_roles;
        layer.path_widths = ordered_widths;
    }
    t_tsp.finish();

    layers
}
