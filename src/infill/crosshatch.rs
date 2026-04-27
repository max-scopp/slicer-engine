//! Cross hatch infill pattern implementation.
//!
//! Generates a crosshatch (✗) pattern: two sets of parallel lines per layer,
//! one at `angle_offset` and one at `angle_offset + 90°`. With the default
//! `infill_base_angle` of 45°, this produces diagonal lines at 45° and 135°,
//! visually distinct from rectilinear (single direction) and grid (axis-aligned).
//!
//! Adjacent layers share the same ✗ orientation because rotating a ✗ by 90°
//! produces the same ✗, so the pattern is stable across layers while still
//! providing cross-layer strength.
//!
//! Density is split evenly between the two directions so the total material
//! usage matches the requested density.

use clipper2::*;
use super::utils::calculate_bounds;
use std::f64::consts::PI;

/// Generate a crosshatch (✗) infill pattern: two perpendicular sets of
/// parallel lines per layer at `angle_offset` and `angle_offset + 90°`.
///
/// With the default `infill_base_angle` of 45°, this produces diagonal lines
/// at 45° and 135°.  The pattern is visually distinct from rectilinear
/// (1 direction) and grid (0°/90° axis-aligned).
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer (defaults to
///   45° = PI/4 from `infill_base_angle`; the two line sets are at this angle
///   and `angle_offset + PI/2`).
///
/// # Returns
/// Paths containing line segments in both directions, unclipped.
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

    // The two directions are 90° apart, starting at `angle_offset`.
    // With the default base angle of 45°, this produces diagonal ✗ lines
    // (45° and 135°). The previous ±PI/4 approach rotated the whole pattern
    // by 45°, turning 45° base → 0°/90° axis-aligned lines indistinguishable
    // from Grid.
    let mut result = Paths::default();
    generate_oriented_lines(region, spacing, angle_offset, &mut result);
    generate_oriented_lines(region, spacing, angle_offset + PI / 2.0, &mut result);
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
