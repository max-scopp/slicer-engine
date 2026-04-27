//! Arachne variable-width perimeter (wall) generator.
//!
//! This module implements the Arachne algorithm for variable-width extrusion
//! (VWE) toolpath generation.  Instead of fixed-width perimeter lines, Arachne
//! approximates the medial axis of each shell polygon using successive inward
//! Clipper2 offsets and emits paths whose extrusion width varies per bead to
//! match the local wall thickness.
//!
//! ## Algorithm overview
//!
//! For each closed shell contour produced by [`crate::core::slice_mesh`]:
//!
//! 1. **Collapse-depth search** — binary-search the largest inward offset `D`
//!    at which the polygon is still non-empty.  `D` equals the polygon's
//!    inradius (half the minimum local wall thickness).
//!
//! 2. **Standard beads** — place up to `wall_count` full-width beads at
//!    centerline depths `d/2, 3d/2, …` (where `d = nozzle_diameter_mm`).
//!    Each bead has width `d`.  Beads whose centerline would fall outside the
//!    polygon (depth ≥ D) are skipped.
//!
//! 3. **Thin-wall residual** — if the remaining inner space after the standard
//!    beads has width ≥ `wall_line_width_min × d`, a single variable-width bead
//!    is added at the centroid of that space with width = remaining width
//!    (clamped to `wall_line_width_max × d`).
//!
//! ## Reference
//!
//! Kuipers et al. (2020) — *Arachne: Arc-based Toolpath Generation for FDM 3D
//! Printing*.  See also Cura `SkeletalTrapezoidation` and OrcaSlicer
//! `libslic3r/Arachne/`.

use clipper2::*;

use crate::core::{ExtrusionRole, SliceLayer};
use crate::settings::params::SlicingParams;

// ── Public types ──────────────────────────────────────────────────────────────

/// Resolved Arachne parameters with all values in absolute mm.
///
/// Constructed from [`SlicingParams`] via [`ArachneParams::from_slicing_params`].
pub struct ArachneParams {
    /// Nozzle diameter in mm.
    pub nozzle_diameter_mm: f64,
    /// Maximum number of perimeter beads per shell.
    pub wall_count: usize,
    /// Minimum bead width in mm (= `wall_line_width_min × nozzle_diameter_mm`).
    pub wall_line_width_min_mm: f64,
    /// Maximum bead width in mm (= `wall_line_width_max × nozzle_diameter_mm`).
    pub wall_line_width_max_mm: f64,
    /// Number of innermost beads that may absorb residual width variation.
    pub wall_distribution_count: usize,
}

impl ArachneParams {
    /// Build [`ArachneParams`] from the slicing-parameter bag.
    pub fn from_slicing_params(params: &SlicingParams) -> Self {
        let d = params.nozzle_diameter_mm;
        Self {
            nozzle_diameter_mm: d,
            wall_count: params.wall_count,
            wall_line_width_min_mm: params.wall_line_width_min * d,
            wall_line_width_max_mm: params.wall_line_width_max * d,
            wall_distribution_count: params.wall_distribution_count,
        }
    }
}

/// A single computed extrusion bead produced by the Arachne generator.
pub struct Bead {
    /// Centerline path (a closed polygon offset inward from the shell boundary).
    pub path: Path,
    /// Extrusion width in mm for this bead.
    pub width_mm: f64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate Arachne variable-width wall paths for every layer.
///
/// Replaces the raw mesh-contour [`ExtrusionRole::Perimeter`] paths in each
/// layer with properly generated variable-width perimeter beads.  All
/// non-perimeter paths (top/bottom surface infill, sparse infill, etc.) are
/// preserved in their original order after the new wall paths.
///
/// # Arguments
/// * `layers` – mutable slice layers produced by [`crate::core::slice_mesh`]
///   (after surface generation).
/// * `params` – resolved Arachne parameters.
pub fn generate_arachne_walls(layers: &mut [SliceLayer], params: &ArachneParams) {
    for layer in layers.iter_mut() {
        generate_arachne_walls_for_layer(layer, params);
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Replace the perimeter paths in a single layer with Arachne beads.
fn generate_arachne_walls_for_layer(layer: &mut SliceLayer, params: &ArachneParams) {
    // Collect raw perimeter contours (closed mesh cross-section loops).
    let raw_perimeters: Vec<Path> = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| layer.role_for_path(*i) == ExtrusionRole::Perimeter)
        .map(|(_, p)| p.clone())
        .collect();

    if raw_perimeters.is_empty() {
        return;
    }

    // Preserve non-perimeter paths with their roles and widths.
    let non_perimeter: Vec<(Path, ExtrusionRole, Option<f64>)> = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| layer.role_for_path(*i) != ExtrusionRole::Perimeter)
        .map(|(i, p)| (p.clone(), layer.role_for_path(i), layer.width_for_path(i)))
        .collect();

    // Compute Arachne beads from the raw contours.
    let input = Paths::new(raw_perimeters);
    let beads = compute_arachne_beads(&input, params);

    // Rebuild the layer: Arachne wall beads first, then non-perimeter paths.
    layer.paths = Paths::new(vec![]);
    layer.path_roles = Vec::new();
    layer.path_widths = Vec::new();

    for bead in beads {
        layer.paths.push(bead.path);
        layer.path_roles.push(ExtrusionRole::Perimeter);
        layer.path_widths.push(Some(bead.width_mm));
    }

    for (path, role, width) in non_perimeter {
        layer.paths.push(path);
        layer.path_roles.push(role);
        layer.path_widths.push(width);
    }
}

/// Estimate the bounding-box half-width of a path set.
///
/// Used as the upper bound for the collapse-depth binary search.
fn estimate_max_depth(input: &Paths) -> f64 {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for path in input.iter() {
        for pt in path.iter() {
            x_min = x_min.min(pt.x());
            x_max = x_max.max(pt.x());
            y_min = y_min.min(pt.y());
            y_max = y_max.max(pt.y());
        }
    }

    if x_max <= x_min || y_max <= y_min {
        return 0.0;
    }
    ((x_max - x_min).min(y_max - y_min) / 2.0).max(0.0)
}

/// Binary-search the collapse depth of `input`.
///
/// The collapse depth `D` is the largest inward offset at which the polygon
/// is still non-empty.  It equals the polygon's *inradius* — half the
/// minimum local wall thickness.
fn find_collapse_depth(input: &Paths) -> f64 {
    let max_d = estimate_max_depth(input);
    if max_d <= 0.0 {
        return 0.0;
    }

    // Quick check: can we offset by any amount at all?
    let tiny = simplify(
        inflate(input.clone(), -1e-6, JoinType::Round, EndType::Polygon, 2.0),
        1e-6,
        false,
    );
    if tiny.is_empty() {
        return 0.0;
    }

    let mut lo = 0.0_f64;
    let mut hi = max_d;

    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let shrunk = simplify(
            inflate(input.clone(), -mid, JoinType::Round, EndType::Polygon, 2.0),
            1e-6,
            false,
        );
        if shrunk.is_empty() {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    lo // largest depth where inflate returns a non-empty result
}

/// Inward-offset helper: shrink `input` by `depth` and simplify.
fn shrink(input: &Paths, depth: f64, tol: f64) -> Paths {
    simplify(
        inflate(input.clone(), -depth, JoinType::Round, EndType::Polygon, 2.0),
        tol,
        false,
    )
}

/// Compute Arachne beads for the given polygon set.
///
/// Returns beads ordered from the outermost wall inward.
pub fn compute_arachne_beads(input: &Paths, params: &ArachneParams) -> Vec<Bead> {
    let d = params.nozzle_diameter_mm;
    let min_w = params.wall_line_width_min_mm;
    let max_w = params.wall_line_width_max_mm;
    let tol = 1e-4 * d.max(0.01);

    // Find the polygon's minimum inradius (= collapse depth D).
    let big_d = find_collapse_depth(input);

    // Nothing to print if the total wall width is below the minimum bead width.
    if 2.0 * big_d < min_w {
        return vec![];
    }

    let mut beads = Vec::new();

    // ── Standard full-width beads ─────────────────────────────────────────────
    //
    // Place beads at centerline depths d/2, 3d/2, 5d/2, … until:
    //   (a) the centerline depth ≥ D (polygon collapses), or
    //   (b) wall_count beads have been placed.
    let n_fit: usize = (0..params.wall_count)
        .take_while(|&k| (k as f64 + 0.5) * d < big_d)
        .count();

    for k in 0..n_fit {
        let depth = (k as f64 + 0.5) * d;
        let paths = shrink(input, depth, tol);
        for p in paths.iter() {
            beads.push(Bead {
                path: p.clone(),
                width_mm: d,
            });
        }
    }

    // ── Thin-wall residual bead ───────────────────────────────────────────────
    //
    // After the standard beads the remaining inner space extends from depth
    // `n_fit × d` to `D` (on each side of the wall).
    //
    // • remaining_width = 2 × (D − n_fit × d)
    //
    // If remaining_width ≥ min_w we add one variable-width bead whose
    // centerline sits at the midpoint of the remaining space.
    //
    // If remaining_width is positive but < min_w the gap is too thin for a
    // separate bead.  In this case we widen the innermost standard bead(s) by
    // distributing the gap across up to `wall_distribution_count` beads.
    let inner_edge_depth = n_fit as f64 * d;
    let remaining_half = big_d - inner_edge_depth;
    let remaining_width = 2.0 * remaining_half;

    if remaining_width >= min_w {
        // Add a variable-width residual bead at the center of the remaining space.
        let center_depth = inner_edge_depth + remaining_half / 2.0;
        let width = remaining_width.min(max_w);
        let paths = shrink(input, center_depth, tol);
        for p in paths.iter() {
            beads.push(Bead {
                path: p.clone(),
                width_mm: width,
            });
        }
    } else if remaining_width > 0.0 && !beads.is_empty() {
        // The gap is too thin for a new bead.  Widen the innermost
        // wall_distribution_count beads by spreading the remaining width
        // evenly among them.
        let n_absorb = params.wall_distribution_count.min(n_fit).max(1);
        let extra_per_bead = remaining_width / n_absorb as f64;

        // Identify the last n_absorb beads in the bead list (they are the
        // innermost standard beads) and re-generate them with a wider profile.
        // We only track beads by their position in the list; for the common
        // case of simple polygons each standard bead produces exactly one
        // path, but complex polygons may produce multiple paths per bead.
        //
        // Strategy: remove the last n_absorb path entries and regenerate.
        // We count from the end, only touching beads with width == d
        // (i.e. standard beads, not a previous residual bead).
        let total_beads = beads.len();
        // Determine the cutoff index: the start of the last n_absorb standard
        // bead groups.  We do a simple heuristic: re-generate beads at the
        // last n_absorb centerline depths with an adjusted width.
        let absorb_start_k = n_fit.saturating_sub(n_absorb);

        // Remove the beads that will be regenerated.
        // We don't know exactly how many path entries each bead produced,
        // so we regenerate from scratch for the affected depths.
        let _ = total_beads; // suppress lint

        // Remove the innermost n_absorb standard beads from the list.
        // They start at some index in `beads`; compute by counting paths.
        let mut paths_to_remove = 0usize;
        for k in absorb_start_k..n_fit {
            let depth = (k as f64 + 0.5) * d;
            paths_to_remove += shrink(input, depth, tol).len();
        }
        let keep = beads.len().saturating_sub(paths_to_remove);
        beads.truncate(keep);

        // Re-add the affected beads with adjusted widths.
        for k in absorb_start_k..n_fit {
            let new_width = (d + extra_per_bead).min(max_w);
            // Shift the centerline outward by half the extra width so the
            // bead's inner edge aligns with the polygon center.
            let outer_edge = k as f64 * d;
            let new_depth = outer_edge + new_width / 2.0;
            let paths = shrink(input, new_depth, tol);
            for p in paths.iter() {
                beads.push(Bead {
                    path: p.clone(),
                    width_mm: new_width,
                });
            }
        }
    }

    beads
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SliceLayer;
    use crate::settings::params::SlicingParams;

    fn default_params() -> ArachneParams {
        ArachneParams::from_slicing_params(&SlicingParams::default())
    }

    fn square_paths(side: f64) -> Paths {
        let half = side / 2.0;
        let sq: Path = vec![
            (-half, -half),
            (half, -half),
            (half, half),
            (-half, half),
        ]
        .into();
        Paths::new(vec![sq])
    }

    // ── find_collapse_depth ───────────────────────────────────────────────────

    #[test]
    fn test_collapse_depth_square() {
        // A 10×10 square has inradius = 5 mm.
        let paths = square_paths(10.0);
        let d = find_collapse_depth(&paths);
        assert!(
            (d - 5.0).abs() < 0.02,
            "collapse depth should be ≈5 mm, got {d}"
        );
    }

    #[test]
    fn test_collapse_depth_thin_rectangle() {
        // A 1×100 rectangle has inradius ≈ 0.5 mm.
        let rect: Path = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 1.0), (0.0, 1.0)].into();
        let paths = Paths::new(vec![rect]);
        let d = find_collapse_depth(&paths);
        assert!(
            (d - 0.5).abs() < 0.02,
            "collapse depth for 1mm wide rect should be ≈0.5 mm, got {d}"
        );
    }

    // ── compute_arachne_beads ─────────────────────────────────────────────────

    #[test]
    fn test_thick_wall_produces_standard_beads() {
        // 20×20 square, wall_count=3, nozzle=0.4 → 3 full-width beads of 0.4 mm
        let paths = square_paths(20.0);
        let params = default_params();
        let beads = compute_arachne_beads(&paths, &params);
        assert!(
            !beads.is_empty(),
            "thick wall should produce at least one bead"
        );
        let std_beads: Vec<_> = beads.iter().filter(|b| (b.width_mm - 0.4).abs() < 1e-6).collect();
        assert_eq!(
            std_beads.len(),
            3,
            "expected 3 standard-width beads, got {}",
            std_beads.len()
        );
    }

    #[test]
    fn test_thin_wall_produces_variable_width_bead() {
        // 0.5×10 rectangle → inradius ≈ 0.25 mm.
        // With nozzle=0.4, no standard bead fits (depth 0.2 < 0.25, so actually fits?).
        // Let's use a 0.3×10 rect: inradius ≈ 0.15 mm < d/2=0.2 → no standard bead.
        // 2*D=0.3 mm ≥ min_w=0.34mm? No: 0.3 < 0.34.  So let's use 0.4×10.
        // 0.4×10 rect: inradius ≈ 0.2 mm. Standard bead at depth 0.2 exactly hits D → count=0.
        // remaining_width=2*0.2=0.4 ≥ min_w=0.34 → one variable bead.
        let rect: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 0.4), (0.0, 0.4)].into();
        let paths = Paths::new(vec![rect]);
        let params = default_params();
        let beads = compute_arachne_beads(&paths, &params);

        assert!(
            !beads.is_empty(),
            "0.4mm wall should produce at least one bead"
        );
        // All beads should have width ≤ nozzle_diameter (variable-width or standard).
        for bead in &beads {
            assert!(
                bead.width_mm > 0.0 && bead.width_mm <= params.wall_line_width_max_mm,
                "bead width {} should be in (0, max_w={}]",
                bead.width_mm,
                params.wall_line_width_max_mm
            );
        }
    }

    #[test]
    fn test_very_thin_wall_produces_no_beads() {
        // A wall thinner than min_width should yield no beads.
        // min_w = 0.85 * 0.4 = 0.34 mm → wall must be < 0.34 mm.
        let rect: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 0.2), (0.0, 0.2)].into();
        let paths = Paths::new(vec![rect]);
        let params = default_params();
        let beads = compute_arachne_beads(&paths, &params);
        assert!(
            beads.is_empty(),
            "0.2mm wall (< min_w=0.34mm) should produce no beads, got {}",
            beads.len()
        );
    }

    // ── generate_arachne_walls_for_layer ─────────────────────────────────────

    #[test]
    fn test_arachne_replaces_raw_perimeter_paths() {
        let mut layer = SliceLayer::new(0.2);
        // Add a 10×10 square as a raw perimeter.
        let sq: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(sq);
        layer.path_roles.push(ExtrusionRole::Perimeter);
        layer.path_widths.push(None);

        let params = ArachneParams::from_slicing_params(&SlicingParams::default());
        generate_arachne_walls_for_layer(&mut layer, &params);

        assert!(
            !layer.paths.is_empty(),
            "layer should have paths after Arachne"
        );
        // All resulting paths should be Perimeter role.
        for i in 0..layer.paths.len() {
            assert_eq!(
                layer.role_for_path(i),
                ExtrusionRole::Perimeter,
                "path {i} should have Perimeter role"
            );
        }
        // path_widths should be set for all paths.
        assert_eq!(
            layer.path_widths.len(),
            layer.paths.len(),
            "path_widths should have one entry per path"
        );
        for w in &layer.path_widths {
            assert!(
                w.is_some(),
                "Arachne paths must have an explicit width set"
            );
        }
    }

    #[test]
    fn test_arachne_preserves_non_perimeter_paths() {
        let mut layer = SliceLayer::new(0.2);

        // A perimeter path.
        let sq: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(sq.clone());
        layer.path_roles.push(ExtrusionRole::Perimeter);
        layer.path_widths.push(None);

        // A top-surface path that must survive.
        layer.paths.push(sq);
        layer.path_roles.push(ExtrusionRole::TopSurface);
        layer.path_widths.push(None);

        let params = ArachneParams::from_slicing_params(&SlicingParams::default());
        generate_arachne_walls_for_layer(&mut layer, &params);

        let top_count = (0..layer.paths.len())
            .filter(|&i| layer.role_for_path(i) == ExtrusionRole::TopSurface)
            .count();
        assert_eq!(top_count, 1, "the TopSurface path must be preserved");
    }

    #[test]
    fn test_generate_arachne_walls_all_layers() {
        let params = SlicingParams::default();
        let arachne_params = ArachneParams::from_slicing_params(&params);

        // Build two layers with a simple square perimeter each.
        let mut layers: Vec<SliceLayer> = (0..2)
            .map(|i| {
                let mut layer = SliceLayer::new(0.2 * (i as f64 + 1.0));
                let sq: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
                layer.paths.push(sq);
                layer.path_roles.push(ExtrusionRole::Perimeter);
                layer.path_widths.push(None);
                layer
            })
            .collect();

        generate_arachne_walls(&mut layers, &arachne_params);

        for layer in &layers {
            assert!(
                !layer.paths.is_empty(),
                "every layer should have at least one path after Arachne"
            );
        }
    }
}
