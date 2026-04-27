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
    crate::arachne::generate_arachne_walls(&mut layers, &arachne_params);
    logger.log_debug("Arachne wall generation complete");

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
        let has_top_surface = if params.only_one_wall_top {
            compute_layers_with_top_surface(&layers, params.top_layers)
        } else {
            vec![false; layers.len()]
        };
        apply_single_wall_restrictions(&mut layers, params, &has_top_surface);
    }

    // Calculate interior regions (inside walls) for each layer where surfaces will go
    let interior_regions: Vec<Paths> = if params.top_layers > 0 || params.bottom_layers > 0 {
        logger.log_debug("calculating interior regions for surfaces");
        layers
            .iter()
            .map(|layer| {
                calculate_interior_region(
                    layer,
                    params.infill_overlap_percent,
                    params.nozzle_diameter_mm,
                )
            })
            .collect()
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
        generate_top_bottom_surfaces_with_interior(
            &mut layers,
            params.top_layers,
            params.bottom_layers,
            params.layer_height,
            params.surface_infill_angle,
            Some(&interior_regions),
        );
        logger.log_debug("surface generation complete");
        t_surfaces.finish();
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

        add_infill_to_layers(
            &mut layers,
            params.infill_density,
            infill_pattern,
            params.infill_base_angle,
            params.nozzle_diameter_mm,
        );
        logger.log_debug("infill generation complete");
    }

    layers
}
