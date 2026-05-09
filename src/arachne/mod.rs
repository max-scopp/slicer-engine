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

mod beads;
mod types;

use std::sync::atomic::Ordering;

use clipper2::*;

use crate::core::{ExtrusionRole, SliceLayer};

// Re-export public types
pub use beads::compute_arachne_beads;
#[cfg(not(target_arch = "wasm32"))]
pub use beads::compute_arachne_beads_debug;
pub use types::{ArachneParams, ArachneSubTimings, Bead};

// ── Per-run timing accumulators (CPU time Σ across all worker threads) ────────
use beads::{ARACHNE_BEAD_SHRINK_NS, ARACHNE_COLLAPSE_NS};

/// Generate Arachne variable-width wall paths for every layer.
///
/// Replaces the raw mesh-contour [`ExtrusionRole::OuterWall`] paths in each
/// layer with properly generated variable-width perimeter beads.  All
/// non-perimeter paths (top/bottom surface infill, sparse infill, etc.) are
/// preserved in their original order after the new wall paths.
///
/// # Arguments
///
/// * `layers` – mutable slice layers produced by [`crate::core::slice_mesh`]
///   (after surface generation).
/// * `params` – resolved Arachne parameters.
pub fn generate_arachne_walls(
    layers: &mut [SliceLayer],
    params: &ArachneParams,
) -> ArachneSubTimings {
    ARACHNE_COLLAPSE_NS.store(0, Ordering::Relaxed);
    ARACHNE_BEAD_SHRINK_NS.store(0, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        layers
            .par_iter_mut()
            .for_each(|layer| generate_arachne_walls_for_layer(layer, params));
    }
    #[cfg(target_arch = "wasm32")]
    for layer in layers.iter_mut() {
        generate_arachne_walls_for_layer(layer, params);
    }
    ArachneSubTimings {
        collapse_depth_ms: ARACHNE_COLLAPSE_NS.load(Ordering::Relaxed) / 1_000_000,
        bead_shrink_ms: ARACHNE_BEAD_SHRINK_NS.load(Ordering::Relaxed) / 1_000_000,
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Generate Arachne walls for every layer, capturing intermediate geometry for
/// visual debugging.
///
/// This is the debug-mode counterpart to [`generate_arachne_walls`].  It runs
/// **sequentially** (not in parallel) so that intermediate Clipper2 results can
/// be pushed into `debug` without any synchronisation overhead.
///
/// For each layer the following snapshots are recorded in `debug`:
///
/// * [`DebugStage::ArachneNormalisedInput`] — the EvenOdd-union-normalised
///   perimeter contours fed to the bead algorithm.
/// * [`DebugStage::ArachneInflateStep`] — every intermediate `shrink` result
///   at each bead depth, keyed by `bead_k`.
/// * [`DebugStage::ArachneBeads`] — the final bead centerline paths.
#[cfg(not(target_arch = "wasm32"))]
pub fn generate_arachne_walls_debug(
    layers: &mut [SliceLayer],
    params: &ArachneParams,
    debug: &mut crate::debug::DebugGeometry,
) -> ArachneSubTimings {
    ARACHNE_COLLAPSE_NS.store(0, Ordering::Relaxed);
    ARACHNE_BEAD_SHRINK_NS.store(0, Ordering::Relaxed);

    for (layer_index, layer) in layers.iter_mut().enumerate() {
        generate_arachne_walls_for_layer_debug(layer, params, layer_index, debug);
    }

    ArachneSubTimings {
        collapse_depth_ms: ARACHNE_COLLAPSE_NS.load(Ordering::Relaxed) / 1_000_000,
        bead_shrink_ms: ARACHNE_BEAD_SHRINK_NS.load(Ordering::Relaxed) / 1_000_000,
    }
}

/// Replace the perimeter paths in a single layer with Arachne beads.
fn generate_arachne_walls_for_layer(layer: &mut SliceLayer, params: &ArachneParams) {
    // Collect raw perimeter contours (closed mesh cross-section loops).
    let raw_perimeters: Vec<Path> = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let role = layer.role_for_path(*i);
            role == ExtrusionRole::OuterWall || role == ExtrusionRole::InnerWall
        })
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
        .filter(|(i, _)| {
            let role = layer.role_for_path(*i);
            role != ExtrusionRole::OuterWall && role != ExtrusionRole::InnerWall
        })
        .map(|(i, p)| (p.clone(), layer.role_for_path(i), layer.width_for_path(i)))
        .collect();

    // Compute Arachne beads from the raw contours.
    //
    // The raw contours produced by `chain_segments` have **arbitrary winding**
    // (CCW or CW depending on triangle orientation in the input mesh) and may
    // overlap, duplicate, or be nested (e.g. when slicing through engraved
    // text or near-degenerate triangles).  Passing such a `Paths` directly to
    // `inflate(-d, ...)` produces fragmented, self-intersecting output:
    // hundreds of tiny "bead" loops per layer that swamp the G-code with
    // useless retract/travel pairs and visually appear as missing/skipped
    // perimeters when rendered.  See bug investigation in `examples/diag_arachne.rs`.
    //
    // Fix: normalise the input topology with a Clipper2 EvenOdd union before
    // running Arachne.  EvenOdd is winding-independent and resolves overlaps,
    // yielding the canonical Clipper2 representation (CCW outer rings, CW
    // holes).  All subsequent `inflate` calls then behave correctly.
    let normalised = union(
        Paths::new(raw_perimeters),
        Paths::new(vec![]),
        FillRule::EvenOdd,
    )
    .unwrap_or_default();
    let beads = compute_arachne_beads(&normalised, params);

    // Rebuild the layer: Arachne wall beads first, then non-perimeter paths.
    layer.paths = Paths::new(vec![]);
    layer.path_roles = Vec::new();
    layer.path_widths = Vec::new();

    for bead in beads {
        layer.paths.push(bead.path);
        let role = if bead.is_outer {
            ExtrusionRole::OuterWall
        } else {
            ExtrusionRole::InnerWall
        };
        layer.path_roles.push(role);
        layer.path_widths.push(Some(bead.width_mm));
    }

    for (path, role, width) in non_perimeter {
        layer.paths.push(path);
        layer.path_roles.push(role);
        layer.path_widths.push(width);
    }
}

/// Debug variant of [`generate_arachne_walls_for_layer`].
#[cfg(not(target_arch = "wasm32"))]
fn generate_arachne_walls_for_layer_debug(
    layer: &mut SliceLayer,
    params: &ArachneParams,
    layer_index: usize,
    debug: &mut crate::debug::DebugGeometry,
) {
    let raw_perimeters: Vec<Path> = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let role = layer.role_for_path(*i);
            role == ExtrusionRole::OuterWall || role == ExtrusionRole::InnerWall
        })
        .map(|(_, p)| p.clone())
        .collect();

    if raw_perimeters.is_empty() {
        return;
    }

    let non_perimeter: Vec<(Path, ExtrusionRole, Option<f64>)> = layer
        .paths
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let role = layer.role_for_path(*i);
            role != ExtrusionRole::OuterWall && role != ExtrusionRole::InnerWall
        })
        .map(|(i, p)| (p.clone(), layer.role_for_path(i), layer.width_for_path(i)))
        .collect();

    let normalised = union(
        Paths::new(raw_perimeters),
        Paths::new(vec![]),
        FillRule::EvenOdd,
    )
    .unwrap_or_default();

    debug.push(
        crate::debug::DebugStage::ArachneNormalisedInput,
        layer_index,
        layer.z,
        normalised.clone(),
    );

    let mut inflate_steps: Vec<(usize, Paths)> = Vec::new();
    let beads = compute_arachne_beads_debug(&normalised, params, &mut inflate_steps);

    for (bead_k, paths) in inflate_steps {
        debug.push(
            crate::debug::DebugStage::ArachneInflateStep { bead_k },
            layer_index,
            layer.z,
            paths,
        );
    }

    // Collect final bead paths for the ArachneBeads snapshot.
    let bead_paths: Vec<Path> = beads.iter().map(|b| b.path.clone()).collect();
    if !bead_paths.is_empty() {
        debug.push(
            crate::debug::DebugStage::ArachneBeads,
            layer_index,
            layer.z,
            Paths::new(bead_paths),
        );
    }

    layer.paths = Paths::new(vec![]);
    layer.path_roles = Vec::new();
    layer.path_widths = Vec::new();

    for bead in beads {
        layer.paths.push(bead.path);
        let role = if bead.is_outer {
            ExtrusionRole::OuterWall
        } else {
            ExtrusionRole::InnerWall
        };
        layer.path_roles.push(role);
        layer.path_widths.push(Some(bead.width_mm));
    }

    for (path, role, width) in non_perimeter {
        layer.paths.push(path);
        layer.path_roles.push(role);
        layer.path_widths.push(width);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::params::SlicingParams;

    // ── generate_arachne_walls_for_layer ─────────────────────────────────────

    #[test]
    fn test_arachne_replaces_raw_perimeter_paths() {
        let mut layer = SliceLayer::new(0.2);
        // Add a 10×10 square as a raw perimeter.
        let sq: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(sq);
        layer.path_roles.push(ExtrusionRole::OuterWall);
        layer.path_widths.push(None);

        let params = ArachneParams::from_slicing_params(&SlicingParams::default());
        generate_arachne_walls_for_layer(&mut layer, &params);

        assert!(
            !layer.paths.is_empty(),
            "layer should have paths after Arachne"
        );
        // First path should be OuterWall, rest should be InnerWall.
        assert_eq!(
            layer.role_for_path(0),
            ExtrusionRole::OuterWall,
            "first path should be OuterWall"
        );
        for i in 1..layer.paths.len() {
            assert_eq!(
                layer.role_for_path(i),
                ExtrusionRole::InnerWall,
                "path {i} should be InnerWall"
            );
        }
        // path_widths should be set for all paths.
        assert_eq!(
            layer.path_widths.len(),
            layer.paths.len(),
            "path_widths should have one entry per path"
        );
        for w in &layer.path_widths {
            assert!(w.is_some(), "Arachne paths must have an explicit width set");
        }
    }

    #[test]
    fn test_arachne_preserves_non_perimeter_paths() {
        let mut layer = SliceLayer::new(0.2);

        // A perimeter path.
        let sq: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(sq.clone());
        layer.path_roles.push(ExtrusionRole::OuterWall);
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
                layer.path_roles.push(ExtrusionRole::OuterWall);
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
