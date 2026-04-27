//! Cross hatch infill pattern implementation.
//!
//! Generates an artistic crosshatch (✗) pattern: two sets of parallel lines
//! per layer at 45° and 135° relative to the layer's base angle. This creates
//! a visibly distinct cross-hatched appearance on every layer, unlike
//! rectilinear (single direction) or grid (axis-aligned 0°/90°).
//!
//! The base angle alternates between layers (set by the caller via
//! `angle_offset`), so adjacent layers rotate the entire ✗ pattern, providing
//! cross-layer strength while keeping the per-layer crosshatching visible.
//!
//! Density is split evenly between the two directions so the total material
//! usage matches the requested density: each direction uses spacing of
//! `2 * line_width / density`, producing combined density equivalent to a
//! single-direction pattern at the requested density.
//!
//! # Earlier implementations
//!
//! A previous version ported OrcaSlicer's FillCrossHatch which alternates
//! between straight "repeat layers" and hex-wave "transform layers" in 3D.
//! That algorithm produces output bit-identical to rectilinear for ~60% of
//! Z values (the repeat layers) and only barely-visible waves for the rest,
//! making it look like a buggy version of rectilinear when viewed layer by
//! layer. The current implementation favors visual distinctiveness over the
//! 3D-honeycomb interlock geometry.

use clipper2::*;
use super::utils::calculate_bounds;
use std::f64::consts::PI;

/// Generate a crosshatch (✗) infill pattern: two perpendicular sets of
/// parallel lines per layer, oriented at 45° and 135° from `angle_offset`.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer (the whole ✗
///   pattern rotates with this; caller alternates per layer for strength).
///
/// # Returns
/// Paths containing line segments in both diagonal directions, unclipped.
/// Caller is expected to clip against the actual region boundary.
pub fn generate_crosshatch(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    if density <= 0.0 || region.is_empty() {
        return Paths::default();
    }

    // Each direction carries half the requested density, so their union
    // matches the total density of a single-direction pattern.
    let line_width = 0.4;
    let half_density = (density * 0.5).max(1e-6);
    let spacing = line_width / half_density;

    // The two sets are oriented at +45° and -45° (i.e. 135°) relative to the
    // layer's base angle. This produces a visible ✗ pattern.
    let mut result = Paths::default();
    generate_oriented_lines(region, spacing, angle_offset + PI / 4.0, &mut result);
    generate_oriented_lines(region, spacing, angle_offset - PI / 4.0, &mut result);
    result
}

/// Generate parallel lines at the given absolute angle covering `region`'s
/// bounding box. Lines are unclipped — the caller's `clip_lines_to_region`
/// pass will trim them to the actual polygon.
fn generate_oriented_lines(region: &Paths, spacing: f64, angle: f64, out: &mut Paths) {
    let bounds = calculate_bounds(region);
    if bounds.is_none() {
        return;
    }
    let (min_x, min_y, max_x, max_y) = bounds.unwrap();

    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;

    // Use a generously oversized box so rotated lines fully cover the region.
    // Diagonal of the bbox is the longest possible cross-section.
    let dx = max_x - min_x;
    let dy = max_y - min_y;
    let half_size = (dx * dx + dy * dy).sqrt() * 0.5 + spacing;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Snap line offset to a multiple of spacing for stable layer-to-layer alignment.
    let mut v = (-half_size / spacing).floor() * spacing;
    let v_end = half_size;

    while v <= v_end {
        let u_start = -half_size;
        let u_end = half_size;

        let p1 = (cx + u_start * cos_a - v * sin_a, cy + u_start * sin_a + v * cos_a);
        let p2 = (cx + u_end * cos_a - v * sin_a, cy + u_end * sin_a + v * cos_a);

        let line: Path = vec![p1, p2].into();
        out.push(line);

        v += spacing;
    }
}
