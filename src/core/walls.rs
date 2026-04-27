use clipper2::*;

use crate::settings::params::SlicingParams;

use super::surfaces::perimeter_paths_of;
use super::types::{ExtrusionRole, SliceLayer};

/// Apply single-wall restrictions to specific layers based on parameters.
///
/// This function modifies layers to use only the outer wall in specific cases:
/// 1. First layer (layer 0) if `only_one_wall_first_layer` is true.
/// 2. Last layer of every "top surface run" if `only_one_wall_top` is true,
///    where a run is a contiguous range of layers that have an exposed top
///    surface (i.e. are not fully covered by perimeters within `top_layers`
///    above them).
///
/// Inner walls are removed from these layers, leaving only outer walls.
///
/// # `has_top_surface`
///
/// Caller must pre-compute, per layer, whether a top surface will be drawn
/// there. This **must** be derived from perimeter geometry (see
/// [`compute_layers_with_top_surface`]), not from `path_roles`, because the
/// `TopSurface` role is only assigned later, after surfaces are generated.
/// Earlier versions used `path_roles.contains(&TopSurface)` and were a
/// permanent no-op for `only_one_wall_top`, leaving the topmost layer with
/// every wall and producing a visible inter-wall gap between the top surface
/// and the outer wall.
pub(crate) fn apply_single_wall_restrictions(
    layers: &mut [SliceLayer],
    params: &SlicingParams,
    has_top_surface: &[bool],
) {
    if layers.is_empty() {
        return;
    }

    // Process first layer restriction
    if params.only_one_wall_first_layer {
        remove_inner_walls_from_layer(&mut layers[0]);
    }

    // Process last-layer-of-each-top-surface-run restriction.
    if params.only_one_wall_top {
        debug_assert_eq!(
            has_top_surface.len(),
            layers.len(),
            "has_top_surface mask must align with layers"
        );

        let mut in_top_surface_run = false;
        let mut last_top_surface_idx = None;

        for (i, &has_top) in has_top_surface.iter().enumerate() {
            if has_top {
                in_top_surface_run = true;
                last_top_surface_idx = Some(i);
            } else if in_top_surface_run {
                if let Some(idx) = last_top_surface_idx {
                    remove_inner_walls_from_layer(&mut layers[idx]);
                }
                in_top_surface_run = false;
                last_top_surface_idx = None;
            }
        }

        // Handle the case where a top-surface run extends to the very last layer.
        if let Some(idx) = last_top_surface_idx {
            remove_inner_walls_from_layer(&mut layers[idx]);
        }
    }
}

/// Pre-compute, for every layer, whether it will receive a top surface.
///
/// Mirrors the geometric "covered above" test in
/// [`generate_top_bottom_surfaces_with_interior`] so that callers running
/// **before** surface generation (e.g. [`apply_single_wall_restrictions`])
/// can identify top-surface layers without inspecting `path_roles`.
///
/// A layer has a top surface iff its perimeter region is not fully covered
/// by the EvenOdd intersection of perimeters of the next `top_layers` layers
/// above it (or it sits within `top_layers` of the model top).
pub(crate) fn compute_layers_with_top_surface(
    layers: &[SliceLayer],
    top_layers: usize,
) -> Vec<bool> {
    if top_layers == 0 || layers.is_empty() {
        return vec![false; layers.len()];
    }

    let perimeters: Vec<Paths> = layers.iter().map(perimeter_paths_of).collect();
    let total = perimeters.len();

    (0..total)
        .map(|i| {
            // Same loop shape as the top branch of
            // generate_top_bottom_surfaces_with_interior so the two stay in
            // lock-step.
            let mut covered = perimeters[i].clone();
            for j in 1..=top_layers {
                if i + j >= total {
                    covered = Paths::new(vec![]);
                    break;
                }
                let neighbor = &perimeters[i + j];
                if neighbor.is_empty() {
                    covered = Paths::new(vec![]);
                    break;
                }
                covered =
                    intersect(covered, neighbor.clone(), FillRule::EvenOdd).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }
            let region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();
            !region.is_empty()
        })
        .collect()
}

/// Remove all inner walls from a layer, keeping only outer walls.
fn remove_inner_walls_from_layer(layer: &mut SliceLayer) {
    let mut new_paths = Paths::new(vec![]);
    let mut new_roles = Vec::new();
    let mut new_widths = Vec::new();

    for (i, path) in layer.paths.iter().enumerate() {
        let role = layer.role_for_path(i);
        // Keep everything except InnerWall
        if role != ExtrusionRole::InnerWall {
            new_paths.push(path.clone());
            new_roles.push(role);
            new_widths.push(layer.width_for_path(i));
        }
    }

    layer.paths = new_paths;
    layer.path_roles = new_roles;
    layer.path_widths = new_widths;
}
