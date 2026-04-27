use clipper2::Paths;

use crate::logging::{phases, PhaseTimer, ProcessLogger};
use crate::mesh::types::Mesh;
use crate::settings::params::SlicingParams;

use super::infill::{add_infill_to_layers, calculate_interior_region};
use super::slicer::slice_mesh;
use super::surfaces::generate_top_bottom_surfaces_with_interior;
use super::types::SliceLayer;
use super::walls::{apply_single_wall_restrictions, compute_layers_with_top_surface};

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
                        calculate_interior_region(layer, 0.0, params.nozzle_diameter_mm, params.wall_count)
                    })
                    .collect(),
            )
        };
        #[cfg(target_arch = "wasm32")]
        let result = Some(
            layers
                .iter()
                .map(|layer| {
                    calculate_interior_region(layer, 0.0, params.nozzle_diameter_mm, params.wall_count)
                })
                .collect(),
        );
        t_snapshot.finish();
        result
    } else {
        None
    };

    // Apply single-wall restrictions to first/last layers if configured.
    //
    // The "last layer of each top surface run" detection MUST run before
    // calculate_interior_region so that the interior of single-wall layers
    // collapses to "everything inside the outer wall" (deflate by 1×nozzle)
    // instead of a tiny disk inside the would-be innermost wall.  We derive
    // top-surface layers geometrically from perimeters here because surface
    // generation hasn't run yet, so path_roles don't carry TopSurface.
    if params.only_one_wall_first_layer || params.only_one_wall_top {
        logger.log_debug("applying single-wall restrictions");
        let t_wall_restrictions = PhaseTimer::start(phases::WALL_RESTRICTIONS, logger);
        let t_top_detect = PhaseTimer::start(phases::WALL_TOP_DETECT, logger);
        let has_top_surface = if params.only_one_wall_top {
            compute_layers_with_top_surface(&layers, params.top_layers)
        } else {
            vec![false; layers.len()]
        };
        t_top_detect.finish();
        let t_wall_apply = PhaseTimer::start(phases::WALL_APPLY, logger);
        apply_single_wall_restrictions(&mut layers, params, &has_top_surface);
        t_wall_apply.finish();
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
            params.top_layers,
            params.bottom_layers,
            params.layer_height,
            params.surface_infill_angle,
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
        use crate::infill::InfillPattern;

        let infill_pattern = InfillPattern::parse(&params.infill_pattern)
            .unwrap_or(InfillPattern::Rectilinear);

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
            pre_strip_infill_regions.as_deref(),
        );
        t_infill.finish();
        logger.log_debug("infill generation complete");
    }

    layers
}
