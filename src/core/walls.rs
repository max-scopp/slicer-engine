use clipper2::*;

use crate::settings::params::SlicingParams;

use super::surfaces::perimeter_paths_of;
use super::types::{ExtrusionRole, SliceLayer};

/// Apply single-wall restrictions to specific islands and layers based on parameters.
///
/// This function modifies layers to use only a single outer wall in two cases:
///
/// 1. **First layer** (`only_one_wall_first_layer`): all islands on layer 0
///    have their inner walls stripped unconditionally.
/// 2. **Last layer of each per-island top-surface run** (`only_one_wall_top`):
///    an island's inner walls are stripped only when that specific island is
///    at the end of its top-surface exposure run (the island's footprint
///    disappears or the solid ends above it).  Islands on the same layer that
///    continue upward keep all their walls.
///
/// The per-island approach fixes the previous layer-wide bug where a small
/// sub-feature ending mid-model (e.g. a raised ledge on the side of a cube)
/// would strip inner walls from every island on that layer — including the
/// main body — causing the infill boundary to over-expand into the wall zone.
pub(crate) fn apply_single_wall_restrictions(layers: &mut [SliceLayer], params: &SlicingParams) {
    if layers.is_empty() {
        return;
    }

    // First layer: strip all inner walls regardless of island (layer-wide is
    // correct here because the first layer is a single flat extrusion zone).
    if params.only_one_wall_first_layer {
        remove_inner_walls_from_layer(&mut layers[0]);
    }

    // Top surface: compute a per-island strip mask and selectively remove.
    if params.only_one_wall_top {
        let perimeters: Vec<Paths> = layers.iter().map(perimeter_paths_of).collect();
        let strip_masks = compute_per_island_strip_masks(layers, &perimeters, params.top_layers);
        for (i, strip_indices) in strip_masks.iter().enumerate() {
            if !strip_indices.is_empty() {
                remove_inner_walls_for_islands(&mut layers[i], strip_indices);
            }
        }
    }
}

/// Compute, for each layer, the `paths` indices of outer-wall paths whose
/// inner walls should be stripped.
///
/// An outer-wall path `P` at layer `i` is included in the mask iff:
///
/// 1. **Top-surface exposure**: the progressive intersection of `P`'s area
///    with the outer-wall perimeters of layers `i+1 … i+top_layers` leaves
///    some area of `P` uncovered — i.e. `P`'s top is geometrically exposed.
/// 2. **Run ends here**: either `i` is the last layer of the model, or
///    `intersect(P, perimeters[i+1])` is empty — meaning `P`'s island does
///    not exist in the layer immediately above, so this is the final exposed
///    layer of the run.
///
/// The two-step check eliminates the need for cross-layer island matching: the
/// run-end condition uses a single `intersect` query rather than tracking
/// island identity across layers.
fn compute_per_island_strip_masks(
    layers: &[SliceLayer],
    perimeters: &[Paths],
    top_layers: usize,
) -> Vec<Vec<usize>> {
    if top_layers == 0 {
        return vec![vec![]; layers.len()];
    }
    let total = perimeters.len();

    let compute_one = |layer_idx: usize, layer: &SliceLayer| -> Vec<usize> {
        layer
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layer.role_for_path(*i) == ExtrusionRole::OuterWall)
            .filter_map(|(path_idx, outer_path)| {
                let p_paths = Paths::new(vec![outer_path.clone()]);

                // ── Step 1: does P have exposed top surface at layer_idx? ──
                // Progressively intersect P's area with the layers above to
                // find how much of P is "covered".  The first layer above that
                // does not overlap P (or a layer boundary) collapses coverage
                // to empty, meaning P's top is exposed there.
                let mut covered = p_paths.clone();
                for j in 1..=top_layers {
                    if layer_idx + j >= total {
                        covered = Paths::new(vec![]);
                        break;
                    }
                    let neighbor = &perimeters[layer_idx + j];
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
                let exposed =
                    difference(p_paths.clone(), covered, FillRule::EvenOdd).unwrap_or_default();
                if exposed.is_empty() {
                    return None; // P is fully covered above — no top surface here
                }

                // ── Step 2: does the top-surface run end at layer_idx? ──
                // The run ends when P has no geometrical overlap with the next
                // layer (island disappears) or when there is no next layer.
                if layer_idx + 1 >= total {
                    return Some(path_idx); // last layer of model → run ends
                }
                let continues = intersect(
                    p_paths,
                    perimeters[layer_idx + 1].clone(),
                    FillRule::EvenOdd,
                )
                .unwrap_or_default();

                if continues.is_empty() {
                    Some(path_idx) // P ends here → strip inner walls for this island
                } else {
                    None // P continues upward → run not over yet
                }
            })
            .collect()
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        layers
            .par_iter()
            .enumerate()
            .map(|(i, layer)| compute_one(i, layer))
            .collect()
    }
    #[cfg(target_arch = "wasm32")]
    layers
        .iter()
        .enumerate()
        .map(|(i, layer)| compute_one(i, layer))
        .collect()
}

/// Remove inner walls only for the listed qualifying islands.
///
/// `strip_outer_indices` contains indices (into `layer.paths`) of outer-wall
/// paths whose associated inner walls should be stripped.  An `InnerWall`
/// path is removed only if [`Path::surrounds_path`] reports that it lies
/// inside at least one of the qualifying outer-wall paths.  All other paths
/// (outer walls, infill, surface paths) are preserved unchanged.
fn remove_inner_walls_for_islands(layer: &mut SliceLayer, strip_outer_indices: &[usize]) {
    let qualifying: Vec<_> = strip_outer_indices
        .iter()
        .filter_map(|&i| layer.paths.get(i))
        .cloned()
        .collect();
    if qualifying.is_empty() {
        return;
    }

    let mut new_paths = Paths::new(vec![]);
    let mut new_roles = Vec::new();
    let mut new_widths = Vec::new();

    for (i, path) in layer.paths.iter().enumerate() {
        let role = layer.role_for_path(i);
        let should_strip = role == ExtrusionRole::InnerWall
            && qualifying.iter().any(|outer| outer.surrounds_path(path));
        if !should_strip {
            new_paths.push(path.clone());
            new_roles.push(role);
            new_widths.push(layer.width_for_path(i));
        }
    }

    layer.paths = new_paths;
    layer.path_roles = new_roles;
    layer.path_widths = new_widths;
}

/// Remove all inner walls from a layer, keeping outer walls and other paths.
fn remove_inner_walls_from_layer(layer: &mut SliceLayer) {
    let mut new_paths = Paths::new(vec![]);
    let mut new_roles = Vec::new();
    let mut new_widths = Vec::new();

    for (i, path) in layer.paths.iter().enumerate() {
        let role = layer.role_for_path(i);
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

/// Classify wall paths whose centerline lies inside an unsupported region
/// as [`ExtrusionRole::OverhangPerimeter`].
///
/// A wall (`OuterWall` or `InnerWall`) is reclassified when at least
/// `OVERHANG_VERTEX_THRESHOLD` (50 %) of its vertices fall **inside or on
/// the boundary of** the layer's `unsupported_regions` polygon set.
///
/// ## Geometry contract — read this before changing the test policy
///
/// `perimeters[i]` is built by `perimeter_paths_of(layer)` and contains
/// the **OuterWall paths themselves** — i.e. the same closed centerline
/// polygons that the wall classifier is iterating over.  Consequently,
/// every wall vertex lies *exactly on* the boundary of `perimeters[i]`
/// and therefore on the **outer** boundary of any region derived from it
/// by subtracting another polygon set.
///
/// `unsupported_regions` is computed in
/// [`generate_top_bottom_surfaces_with_interior`] as
///
/// ```text
/// perimeters[i] − inflate(perimeters[i-1], +nozzle_diameter / 2)
/// ```
///
/// The inflation by `d/2` encodes the fact that the previous-layer bead
/// extends `d/2` beyond its centerline — that is the actual material a
/// current-layer wall can land on.  This means:
///
/// * For a slight outward lean (horizontal step `S < d/2`) the inflated
///   previous perimeter fully contains `perimeters[i]`, so
///   `unsupported_regions` is empty → no wall flagged.  This kills the
///   "80 % of the Benchy is overhang" false positive without any vertex-
///   fraction tuning.
/// * For a real overhang (`S > d/2`, ≈ 45° lean for 0.2 mm layer / 0.4 mm
///   nozzle) a meaningful air strip exists.  The wall centerline lies on
///   its outer boundary, so the parity test must count `IsOn` as
///   **inside** to flag it.
///
/// **Do not change `IsOn` to count as outside.**  That was tried as a
/// "stricter" guard against false positives — it instead suppresses
/// **all** overhang detection, because every wall vertex is on the
/// strip's outer boundary by construction.
///
/// **Do not pre-erode `unsupported_regions`.**  An earlier version
/// eroded by `0.6 × nozzle_diameter`, which moves the strip's outer
/// boundary past the wall centerline and likewise suppresses all
/// detection.
///
/// The classification runs **after**
/// [`generate_top_bottom_surfaces_with_interior`] so that
/// `unsupported_regions` is populated; it operates on whole paths only —
/// path splitting at the supported / unsupported boundary is left for a
/// future enhancement.
///
/// Without this step, a wall printed in mid-air (the top frame of the
/// Benchy steering-wheel window, for example) carries the normal wall
/// speed and 100 % flow, which sags badly.  After this step it is treated
/// as a bridge by the G-code generator (slow speed, reduced flow, fan
/// boost) so it lands cleanly on the supported anchor at each end.
pub(crate) fn classify_overhang_perimeters(layers: &mut [SliceLayer], _nozzle_diameter_mm: f64) {
    /// Minimum fraction of a wall path's vertices that must lie inside or
    /// on the boundary of `unsupported_regions` for the whole path to be
    /// reclassified as `OverhangPerimeter`.
    ///
    /// A simple majority (50 %) is appropriate because we classify whole
    /// paths — without splitting, the choice is binary, and the geometric
    /// `+d/2` support envelope already biases against false positives on
    /// slightly outward-leaning hulls.
    const OVERHANG_VERTEX_THRESHOLD: f64 = 0.5;

    for layer in layers.iter_mut() {
        if layer.unsupported_regions.is_empty() {
            continue;
        }
        let air = &layer.unsupported_regions;

        // Make sure path_roles is at least as long as paths so we can write
        // back without panicking.  Defaults preserve existing behaviour.
        while layer.path_roles.len() < layer.paths.len() {
            layer.path_roles.push(ExtrusionRole::OuterWall);
        }

        for (i, path) in layer.paths.iter().enumerate() {
            let role = layer.path_roles[i];
            if role != ExtrusionRole::OuterWall && role != ExtrusionRole::InnerWall {
                continue;
            }
            let mut total = 0_usize;
            let mut inside = 0_usize;
            for pt in path.iter() {
                total += 1;
                if point_inside_or_on_paths_eo(pt.x(), pt.y(), air) {
                    inside += 1;
                }
            }
            if total > 0 && (inside as f64) / (total as f64) >= OVERHANG_VERTEX_THRESHOLD {
                layer.path_roles[i] = ExtrusionRole::OverhangPerimeter;
            }
        }
    }
}

/// Even-odd point-in-polygon test against a `Paths` set.
///
/// Returns `true` when the point lies inside or on the boundary of an
/// odd number of sub-paths.  Boundary points (`IsOn`) **count as
/// inside** — this is required for overhang classification because the
/// wall paths themselves form the *outer* boundary of
/// `unsupported_regions` (which is built as
/// `perimeters[i] − inflate(perimeters[i-1], +d/2)`).  The geometric
/// guard against false positives lives in the `+d/2` inflation, not in
/// the boundary policy.
fn point_inside_or_on_paths_eo(x: f64, y: f64, paths: &Paths) -> bool {
    let mut inside_count = 0_usize;
    for path in paths.iter() {
        let result = clipper2::point_in_polygon(clipper2::Point::new(x, y), path);
        if matches!(
            result,
            clipper2::PointInPolygonResult::IsInside | clipper2::PointInPolygonResult::IsOn
        ) {
            inside_count += 1;
        }
    }
    inside_count % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipper2::{Path, Paths};

    /// A wall path entirely inside the unsupported region must be reclassified
    /// as `OverhangPerimeter`.
    #[test]
    fn test_classify_overhang_perimeters_in_air() {
        // 5×5 wall path centred at (5, 5).
        let wall: Path = vec![(2.5, 2.5), (7.5, 2.5), (7.5, 7.5), (2.5, 7.5)].into();
        let mut layer = SliceLayer::new(0.4);
        layer.paths.push(wall);
        layer.path_roles.push(ExtrusionRole::OuterWall);

        // Unsupported region: the entire 10×10 layer footprint is in air.
        let air: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.unsupported_regions = Paths::new(vec![air]);

        let mut layers = vec![layer];
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[0].path_roles[0],
            ExtrusionRole::OverhangPerimeter,
            "Wall fully in air must be reclassified to OverhangPerimeter"
        );
    }

    /// A wall path entirely outside the unsupported region must keep its
    /// original role.
    #[test]
    fn test_classify_overhang_perimeters_keeps_supported_walls() {
        // Wall path at (0..2, 0..2)
        let wall: Path = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)].into();
        let mut layer = SliceLayer::new(0.4);
        layer.paths.push(wall);
        layer.path_roles.push(ExtrusionRole::InnerWall);

        // Unsupported region is far away (5..10, 5..10) — wall is fully supported.
        let air: Path = vec![(5.0, 5.0), (10.0, 5.0), (10.0, 10.0), (5.0, 10.0)].into();
        layer.unsupported_regions = Paths::new(vec![air]);

        let mut layers = vec![layer];
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[0].path_roles[0],
            ExtrusionRole::InnerWall,
            "Supported wall must keep its InnerWall role"
        );
    }

    /// Non-wall roles (Infill, Bridge, …) must never be reclassified, even when
    /// they happen to lie inside `unsupported_regions`.
    #[test]
    fn test_classify_overhang_perimeters_skips_non_wall_roles() {
        let path: Path = vec![(2.5, 2.5), (7.5, 2.5), (7.5, 7.5), (2.5, 7.5)].into();
        let mut layer = SliceLayer::new(0.4);
        layer.paths.push(path);
        layer.path_roles.push(ExtrusionRole::Infill);

        let air: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.unsupported_regions = Paths::new(vec![air]);

        let mut layers = vec![layer];
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[0].path_roles[0],
            ExtrusionRole::Infill,
            "Infill paths must never be reclassified as OverhangPerimeter"
        );
    }

    /// **Regression** — slightly outward-leaning hulls (typical Benchy hull,
    /// step `S < d/2`) put the OuterWall centerline strictly inside the
    /// inflated previous-layer support envelope, so it must NOT be flagged
    /// as an overhang.  Synthetic test: pass a raw annular even-odd strip
    /// where the wall is strictly inside both rings — parity 0, never
    /// flagged regardless of the boundary policy.
    ///
    /// (The full production guard lives in
    /// `generate_top_bottom_surfaces_with_interior` which inflates the
    /// previous perimeter by `d/2` before differencing — see
    /// `test_classify_overhang_e2e_outward_lean_no_false_positive`.)
    #[test]
    fn test_classify_overhang_outward_lean_no_false_positive() {
        // OuterWall centerline of layer i: 4.8×4.8 square at (2.6..7.4).
        let wall: Path = vec![(2.6, 2.6), (7.4, 2.6), (7.4, 7.4), (2.6, 7.4)].into();
        // perimeters[i-1] (previous outer), 5×5 at (2.5..7.5).
        let prev_outer: Path = vec![(2.5, 2.5), (7.5, 2.5), (7.5, 7.5), (2.5, 7.5)].into();
        // perimeters[i] (current outer), 5.2×5.2 at (2.4..7.6).
        let cur_outer: Path = vec![(2.4, 2.4), (7.6, 2.4), (7.6, 7.6), (2.4, 7.6)].into();

        let mut layer = SliceLayer::new(0.4);
        layer.paths.push(wall);
        layer.path_roles.push(ExtrusionRole::OuterWall);
        // unsupported_regions = perimeters[i] − perimeters[i-1] (annular
        // even-odd strip). Centerline is strictly inside both rings →
        // parity 0 → not flagged.
        layer.unsupported_regions = Paths::new(vec![cur_outer, prev_outer]);

        let mut layers = vec![layer];
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[0].path_roles[0],
            ExtrusionRole::OuterWall,
            "Outward-leaning hull walls (step < d/2) must NOT be flagged as \
             OverhangPerimeter"
        );
    }

    /// **Regression** — a real overhang (step `S > d/2`) must be flagged.
    /// Synthetic test passing a raw annular strip; wall vertices land
    /// strictly inside the air strip so the parity test fires regardless
    /// of `IsOn` policy.  See
    /// `test_classify_overhang_e2e_real_overhang_is_flagged` for the
    /// production-geometry test where wall vertices lie on the strip's
    /// outer boundary.
    #[test]
    fn test_classify_overhang_real_overhang_is_flagged() {
        // OuterWall centerline of layer i: 5.6×5.6 square at (2.2..7.8).
        let wall: Path = vec![(2.2, 2.2), (7.8, 2.2), (7.8, 7.8), (2.2, 7.8)].into();
        // perimeters[i-1]: small inner 5×5 at (2.5..7.5).
        let prev_outer: Path = vec![(2.5, 2.5), (7.5, 2.5), (7.5, 7.5), (2.5, 7.5)].into();
        // perimeters[i]: outer 6×6 at (2.0..8.0). Centerline at d/2 = 0.2
        // inside that = 5.6×5.6.
        let cur_outer: Path = vec![(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)].into();

        let mut layer = SliceLayer::new(0.4);
        layer.paths.push(wall);
        layer.path_roles.push(ExtrusionRole::OuterWall);
        layer.unsupported_regions = Paths::new(vec![cur_outer, prev_outer]);

        let mut layers = vec![layer];
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[0].path_roles[0],
            ExtrusionRole::OverhangPerimeter,
            "Real overhang (step > d/2) must be flagged as OverhangPerimeter"
        );
    }

    /// **End-to-end regression** — production geometry where the wall path
    /// IS `perimeters[i]`.  Two layers, layer 1's perimeter shifted 0.05 mm
    /// outward (≈ 14° lean for 0.2 mm layer / 0.4 mm nozzle) — well below
    /// the `d/2 = 0.2 mm` support threshold.  Production pipeline
    /// (`generate_top_bottom_surfaces` then `classify_overhang_perimeters`)
    /// must NOT flag the wall.
    #[test]
    fn test_classify_overhang_e2e_outward_lean_no_false_positive() {
        use crate::core::surfaces::generate_top_bottom_surfaces;
        use clipper2::Path;

        let mut layer0 = SliceLayer::new(0.2);
        let prev: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer0.paths.push(prev);
        layer0.path_roles.push(ExtrusionRole::OuterWall);

        let mut layer1 = SliceLayer::new(0.2);
        // 0.05 mm outward step — sub-threshold lean.
        let cur: Path = vec![
            (-0.05, -0.05),
            (10.05, -0.05),
            (10.05, 10.05),
            (-0.05, 10.05),
        ]
        .into();
        layer1.paths.push(cur);
        layer1.path_roles.push(ExtrusionRole::OuterWall);

        let mut layers = vec![layer0, layer1];
        generate_top_bottom_surfaces(&mut layers, 0, 1, 0.2, 45.0);
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[1].path_roles[0],
            ExtrusionRole::OuterWall,
            "Sub-d/2 outward lean must not be flagged in the production pipeline"
        );
    }

    /// **End-to-end regression** — the bug the user reported: NO overhangs
    /// were detected on the Benchy because the wall path coincides with
    /// the outer boundary of `unsupported_regions`.  Production pipeline
    /// must flag a real overhang where the current perimeter sits a full
    /// nozzle width outside the previous perimeter.
    #[test]
    fn test_classify_overhang_e2e_real_overhang_is_flagged() {
        use crate::core::surfaces::generate_top_bottom_surfaces;
        use clipper2::Path;

        let mut layer0 = SliceLayer::new(0.2);
        let prev: Path = vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)].into();
        layer0.paths.push(prev);
        layer0.path_roles.push(ExtrusionRole::OuterWall);

        let mut layer1 = SliceLayer::new(0.2);
        // 0.5 mm outward step on every side — well above d/2 = 0.2 mm.
        let cur: Path = vec![(-0.5, -0.5), (5.5, -0.5), (5.5, 5.5), (-0.5, 5.5)].into();
        layer1.paths.push(cur);
        layer1.path_roles.push(ExtrusionRole::OuterWall);

        let mut layers = vec![layer0, layer1];
        generate_top_bottom_surfaces(&mut layers, 0, 1, 0.2, 45.0);
        classify_overhang_perimeters(&mut layers, 0.4);

        assert_eq!(
            layers[1].path_roles[0],
            ExtrusionRole::OverhangPerimeter,
            "Real overhang (step > d/2) must be flagged in the production pipeline"
        );
    }
}
