use clipper2::*;

use super::types::{ExtrusionRole, SliceLayer};

/// Calculate the interior region of a layer where solid surfaces and sparse
/// infill should be printed (i.e. the area enclosed by the **innermost** wall
/// of every island, optionally shrunk by a configured overlap).
///
/// # Strategy
///
/// Use the `OuterWall` centerline paths directly as the outer extent of each
/// island.  Arachne's outermost bead sits at inward depth `d/2` from the raw
/// mesh contour, so these paths are already a well-formed Clipper2 `Paths`
/// with correct winding (CCW for solid islands, CW for holes).  **Winding is
/// preserved — not normalised** — so that holes are correctly represented as
/// void regions rather than being flipped into solid material.
///
/// From that outer extent we deflate inward by:
///   `(walls_per_island − 0.5) × nozzle_diameter − overlap_distance`
/// where `walls_per_island = ceil(total_wall_beads / outer_island_count)`.
/// The `−0.5 × d` term accounts for the half-bead depth that the `OuterWall`
/// centerline is already shifted inward from the true model boundary.
///
/// Returns an empty `Paths` when the interior collapses, signalling that
/// walls alone fill the cross-section (the "smart-skip" outcome).
pub(crate) fn calculate_interior_region(
    layer: &SliceLayer,
    overlap_percent: f64,
    nozzle_diameter: f64,
) -> Paths {
    // Use OuterWall paths as the outer extent of each island.
    // Winding is preserved (CCW for solid islands, CW for holes) so that
    // Clipper2 inflate correctly treats holes as voids rather than solid material.
    let outer_paths = Paths::new(
        layer
            .paths
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                if layer.role_for_path(i) == ExtrusionRole::OuterWall {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect(),
    );

    if outer_paths.is_empty() {
        return Paths::new(vec![]);
    }

    // Count total wall bead paths (outer + inner) and outer island paths so we
    // can estimate how many beads deep each island is.
    let total_wall_count = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            matches!(
                layer.role_for_path(*i),
                ExtrusionRole::OuterWall | ExtrusionRole::InnerWall
            )
        })
        .count();
    let outer_count = outer_paths.len().max(1);
    let walls_per_island = total_wall_count.div_ceil(outer_count);

    // The OuterWall bead centerline is already at depth d/2 from the raw mesh
    // contour.  We need to deflate inward by the remaining wall band thickness:
    //   (walls_per_island × d) − (d/2 already accounted for by the bead offset)
    //   = (walls_per_island − 0.5) × d
    // Subtract the configured overlap so surfaces bond to the innermost wall.
    let overlap_distance = nozzle_diameter * overlap_percent;
    let total_inward = (walls_per_island as f64 - 0.5) * nozzle_diameter - overlap_distance;

    if total_inward < 0.01 {
        // Walls fill the entire cross-section; return outer_paths as the
        // interior (degenerate single-wall case).
        return outer_paths;
    }

    // Empty result = correct "smart-skip" signal (walls alone fill the layer).
    clipper2::inflate(
        outer_paths,
        -total_inward,
        JoinType::Round,
        EndType::Polygon,
        2.0,
    )
}

/// Add infill paths to layers based on slicing parameters.
///
/// Takes a set of layers with perimeter paths and adds infill patterns within
/// the perimeter boundaries. Infill paths are assigned the [`ExtrusionRole::Infill`]
/// role for proper G-code annotation.
///
/// # Arguments
/// * `layers` - Slice layers with perimeter paths (will be modified in place)
/// * `infill_density` - Infill density as a fraction (0.0 = no infill, 1.0 = solid)
/// * `infill_pattern` - The pattern type to generate (rectilinear, grid, etc.)
/// * `infill_base_angle` - Base angle in degrees (alternating layers rotate +90° on top of this)
///
/// # Example
/// ```rust,no_run
/// use slicer_engine::core::{slice_mesh, add_infill_to_layers};
/// use slicer_engine::infill::InfillPattern;
/// # use slicer_engine::mesh::types::Mesh;
/// # let mesh = Mesh::new();
///
/// let mut layers = slice_mesh(&mesh, 0.2);
/// add_infill_to_layers(&mut layers, 0.2, InfillPattern::Rectilinear, 45.0, 0.4);
/// ```
pub fn add_infill_to_layers(
    layers: &mut [SliceLayer],
    infill_density: f64,
    infill_pattern: crate::infill::InfillPattern,
    infill_base_angle: f64,
    nozzle_diameter_mm: f64,
) {
    use crate::infill::generate_infill;

    if infill_density <= 0.0 {
        return; // No infill requested
    }

    for (layer_idx, layer) in layers.iter_mut().enumerate() {
        // Skip layers with no perimeters
        if layer.paths.is_empty() {
            continue;
        }

        // Compute the infill boundary: the region inside the innermost wall.
        //
        // `calculate_interior_region` builds the outer extent of all wall
        // centerlines (inflated by their half-widths), then deflates inward by
        // the total accumulated wall thickness — giving the exact inner face of
        // the innermost wall regardless of wall count or variable bead widths.
        // overlap_percent = 0.0 so sparse infill starts right at the inner
        // wall face; `generate_infill` adds its own small adhesion gap on top.
        let infill_area = calculate_interior_region(layer, 0.0, nozzle_diameter_mm);

        if infill_area.is_empty() {
            continue;
        }

        // Subtract solid surface regions so sparse infill is never placed
        // on top of existing solid top/bottom surface infill.
        let infill_area = if !layer.solid_regions.is_empty() {
            let remaining = difference(
                infill_area,
                layer.solid_regions.clone(),
                FillRule::Positive,
            )
            .unwrap_or_default();
            if remaining.is_empty() {
                // Entire layer is covered by solid surfaces — no sparse infill needed.
                continue;
            }
            remaining
        } else {
            infill_area
        };

        // Calculate angle offset for alternating patterns
        // Rectilinear infill alternates base_angle and base_angle+90° each layer
        let base_angle_rad = infill_base_angle.to_radians();
        let angle_offset = if layer_idx % 2 == 0 {
            base_angle_rad
        } else {
            base_angle_rad + std::f64::consts::FRAC_PI_2 // +90 degrees
        };

        // Generate infill paths within the computed area
        // Pass layer Z height for patterns like gyroid that need it
        let infill_paths =
            generate_infill(&infill_area, infill_pattern, infill_density, angle_offset, layer.z);

        // Add infill paths to the layer with proper role annotation
        for infill_path in infill_paths.iter() {
            layer.paths.push(infill_path.clone());
            layer.path_roles.push(ExtrusionRole::Infill);
        }
    }
}
