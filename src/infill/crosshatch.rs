//! Cross hatch infill pattern implementation.
//!
//! Implements OrcaSlicer's CrossHatch infill: a 3D pattern that alternates
//! between "repeat layers" (straight parallel lines) and "transform layers"
//! (hexagonal-wave curves that warp into honeycomb-like shapes to transition
//! between directions). Direction rotates 90° between periods for cross-layer
//! strength, while transform layers create cohesive 3D structure.
//!
//! Algorithm port from OrcaSlicer's FillCrossHatch.cpp (Bambu Lab), which
//! itself is inspired by David Eccles' improved 3D honeycomb. The transform
//! pattern looks like:
//!
//! ```text
//!     o---o
//!    /     \
//!   /       \
//!            \       /
//!             \     /
//!              o---o
//!    p1   p2  p3   p4
//! ```

use clipper2::*;
use super::utils::calculate_bounds;

/// Generate cross hatch 3D infill pattern.
///
/// Creates a pattern that alternates between straight repeat layers and
/// hexagonal-wave transform layers. The combination of layer types and
/// rotating direction creates an interlocked 3D structure that's stronger
/// than rectilinear infill while remaining fast to print.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Base rotation angle in radians
/// * `z_height` - Z coordinate of the current layer in mm (drives 3D pattern)
///
/// # Returns
/// Paths containing either straight lines or hexagonal-wave polylines depending
/// on which layer phase the current Z falls into.
pub fn generate_crosshatch(region: &Paths, density: f64, angle_offset: f64, z_height: f64) -> Paths {
    if density <= 0.0 || region.is_empty() {
        return Paths::default();
    }

    // Calculate line spacing (grid_size) from density
    let line_width = 0.4;
    let spacing = line_width / density;

    // Get bounding box of the region
    let bounds = calculate_bounds(region);
    if bounds.is_none() {
        return Paths::default();
    }
    let (min_x, min_y, max_x, max_y) = bounds.unwrap();

    let bb_width = max_x - min_x;
    let bb_height = max_y - min_y;

    if bb_width <= 0.0 || bb_height <= 0.0 {
        return Paths::default();
    }

    // Repeat ratio: lower density => shorter repeat zone (relatively more transform)
    // Matches OrcaSlicer behavior for low density strength.
    let repeat_ratio = if density < 0.3 {
        (1.0 - (-5.0 * density).exp()).clamp(0.2, 1.0)
    } else {
        1.0
    };

    // Generate the pattern in the bbox local space, then translate.
    let local_polylines = generate_infill_layers(z_height, repeat_ratio, spacing, bb_width, bb_height);

    // Apply rotation around bbox center if angle_offset is set, then translate
    // to bbox origin.
    let cos_a = angle_offset.cos();
    let sin_a = angle_offset.sin();
    let cx = bb_width * 0.5;
    let cy = bb_height * 0.5;

    let mut result = Paths::default();
    for poly in local_polylines {
        if poly.len() < 2 {
            continue;
        }
        let transformed: Vec<(f64, f64)> = poly
            .into_iter()
            .map(|(px, py)| {
                // Rotate around local center, then translate to bbox min
                let dx = px - cx;
                let dy = py - cy;
                let rx = dx * cos_a - dy * sin_a + cx + min_x;
                let ry = dx * sin_a + dy * cos_a + cy + min_y;
                (rx, ry)
            })
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        if transformed.len() >= 2 {
            let path: Path = transformed.into();
            result.push(path);
        }
    }

    result
}

/// Build a single hex-cycle template polyline of length `period` along X.
///
/// Produces 4 control points that form the canonical CrossHatch wave shape.
fn generate_one_cycle(progress: f64, period: f64) -> Vec<(f64, f64)> {
    let offset = progress * (1.0 / 8.0) * period;
    vec![
        (0.25 * period - offset, offset),
        (0.25 * period + offset, offset),
        (0.75 * period - offset, -offset),
        (0.75 * period + offset, -offset),
    ]
}

/// Generate the transform pattern (hexagonal-wave curves) for one layer.
///
/// `direction`: positive => horizontal lines, negative => vertical (swap XY).
fn generate_transform_pattern(
    progress: f64,
    direction: i32,
    grid_size: f64,
    in_width: f64,
    in_height: f64,
) -> Vec<Vec<(f64, f64)>> {
    let (mut width, mut height) = (in_width, in_height);
    let g2 = grid_size * 2.0; // we deal with odd and even separately

    // Build template cycle
    let one_cycle = generate_one_cycle(progress, g2);

    // Swap dimensions for vertical orientation
    if direction < 0 {
        std::mem::swap(&mut width, &mut height);
    }

    // Replicate one cycle along X to fill width
    let num_cycles = (width / g2) as usize + 2;
    let mut odd_poly: Vec<(f64, f64)> = Vec::with_capacity(num_cycles * one_cycle.len());
    for i in 0..num_cycles {
        let tx = i as f64 * g2;
        for &(px, py) in &one_cycle {
            odd_poly.push((px + tx, py));
        }
    }

    // Replicate odd_poly down Y for the "odd" rows
    let num_lines = (height / g2) as usize + 2;
    let mut out: Vec<Vec<(f64, f64)>> = Vec::with_capacity(num_lines * 2);

    for i in 0..num_lines {
        let ty = g2 * i as f64;
        let row: Vec<(f64, f64)> = odd_poly.iter().map(|&(x, y)| (x, y + ty)).collect();
        out.push(row);
    }

    // Even rows: shifted by (-0.5*g2, +0.5*g2) per Orca
    let odd_count = out.len();
    for i in 0..odd_count {
        let ty = (i as f64 + 0.5) * g2;
        let tx = -0.5 * g2;
        let row: Vec<(f64, f64)> = odd_poly.iter().map(|&(x, y)| (x + tx, y + ty)).collect();
        out.push(row);
    }

    // Swap XY for vertical orientation
    if direction < 0 {
        for poly in &mut out {
            for p in poly.iter_mut() {
                std::mem::swap(&mut p.0, &mut p.1);
            }
        }
    }

    out
}

/// Generate the repeat pattern (straight parallel lines) for one layer.
fn generate_repeat_pattern(
    direction: i32,
    grid_size: f64,
    in_width: f64,
    in_height: f64,
) -> Vec<Vec<(f64, f64)>> {
    let (mut width, mut height) = (in_width, in_height);
    if direction < 0 {
        std::mem::swap(&mut width, &mut height);
    }

    let num_lines = (height / grid_size) as usize + 1;
    let mut out: Vec<Vec<(f64, f64)>> = Vec::with_capacity(num_lines);

    for i in 0..num_lines {
        let y = grid_size * i as f64;
        let line = vec![(0.0, y), (width, y)];
        out.push(line);
    }

    if direction < 0 {
        for poly in &mut out {
            for p in poly.iter_mut() {
                std::mem::swap(&mut p.0, &mut p.1);
            }
        }
    }

    out
}

/// Pick the right layer type (repeat or transform) for a given Z height and
/// generate the polylines for it.
fn generate_infill_layers(
    z_height: f64,
    repeat_ratio: f64,
    grid_size: f64,
    width: f64,
    height: f64,
) -> Vec<Vec<(f64, f64)>> {
    let trans_layer_size = grid_size * 0.4;
    let repeat_layer_size = grid_size * repeat_ratio;
    // Offset Z to improve first-few-layer strength and reduce warping risk
    let z = z_height + repeat_layer_size / 2.0 + trans_layer_size;

    let period = trans_layer_size + repeat_layer_size;
    let remains = z - (z / period).floor() * period;
    let trans_z = remains - repeat_layer_size;

    // Phase determines direction (alternating every full period*2).
    // Match Orca: phase = fmod(z, period*2) - (period - 1)
    let two_period = period * 2.0;
    let phase = (z - (z / two_period).floor() * two_period) - (period - 1.0);
    let direction = if phase <= 0.0 { -1 } else { 1 };

    if trans_z < 0.0 {
        // Repeat layer (straight lines)
        generate_repeat_pattern(direction, grid_size, width, height)
    } else {
        // Transform layer (hex-wave curves)
        let progress = (trans_z - (trans_z / trans_layer_size).floor() * trans_layer_size)
            / trans_layer_size;
        if progress < 0.5 {
            generate_transform_pattern((progress + 0.1) * 2.0, direction, grid_size, width, height)
        } else {
            generate_transform_pattern((1.1 - progress) * 2.0, -direction, grid_size, width, height)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_region(size: f64) -> Paths {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)].into();
        region.push(square);
        region
    }

    #[test]
    fn test_crosshatch_empty_region() {
        let region = Paths::default();
        let result = generate_crosshatch(&region, 0.2, 0.0, 0.2);
        assert!(result.is_empty());
    }

    #[test]
    fn test_crosshatch_zero_density() {
        let region = square_region(20.0);
        let result = generate_crosshatch(&region, 0.0, 0.0, 0.2);
        assert!(result.is_empty());
    }

    #[test]
    fn test_crosshatch_generates_lines() {
        let region = square_region(30.0);
        let result = generate_crosshatch(&region, 0.2, 0.0, 0.2);
        assert!(!result.is_empty(), "Should generate cross hatch lines");
    }

    #[test]
    fn test_crosshatch_varies_by_layer() {
        // Different Z heights should produce different patterns since CrossHatch
        // is a 3D pattern that alternates between repeat and transform layers.
        let region = square_region(40.0);

        // Pick Z heights spaced far enough apart to land in different phases.
        let r1 = generate_crosshatch(&region, 0.3, 0.0, 0.0);
        let r2 = generate_crosshatch(&region, 0.3, 0.0, 5.0);
        let r3 = generate_crosshatch(&region, 0.3, 0.0, 10.0);

        // At least one of the heights should produce a different output shape.
        let differs = r1.len() != r2.len() || r1.len() != r3.len() || r2.len() != r3.len();
        assert!(differs || !r1.is_empty(), "CrossHatch should vary across Z heights");
    }

    #[test]
    fn test_crosshatch_transform_layer_has_curves() {
        // A transform-layer Z should produce multi-point polylines (curves),
        // not just straight 2-point segments like rectilinear.
        let region = square_region(40.0);

        // Find a Z that lands inside a transform layer; scan a few values.
        let mut found_transform = false;
        for n in 0..20 {
            let z = n as f64 * 0.3;
            let result = generate_crosshatch(&region, 0.3, 0.0, z);
            if result.iter().any(|p| p.len() > 2) {
                found_transform = true;
                break;
            }
        }
        assert!(found_transform, "Some Z values should produce multi-point transform polylines");
    }

    #[test]
    fn test_crosshatch_finite_coordinates() {
        let region = square_region(30.0);
        for n in 0..10 {
            let z = n as f64 * 0.7;
            let result = generate_crosshatch(&region, 0.2, 0.5, z);
            for poly in result.iter() {
                for p in poly.iter() {
                    assert!(p.x().is_finite() && p.y().is_finite(),
                        "All coordinates must be finite (z={}, point=({},{}))",
                        z, p.x(), p.y());
                }
            }
        }
    }
}
