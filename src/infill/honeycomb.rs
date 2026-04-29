//! Honeycomb (hexagonal) infill pattern implementation.
//!
//! Generates hexagonal cells that provide excellent strength-to-weight ratio,
//! mimicking natural honeycomb structures.

use super::utils::calculate_bounds;
use clipper2::*;

/// Generate honeycomb (hexagonal) infill pattern.
///
/// Creates a tessellation of hexagonal cells. This pattern provides excellent
/// strength-to-weight ratio but is more complex to generate and slower to print
/// than rectilinear or grid patterns.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths containing hexagonal cell edges
pub fn generate_honeycomb(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    if density <= 0.0 || region.is_empty() {
        return Paths::default();
    }

    let bounds = calculate_bounds(region);
    if bounds.is_none() {
        return Paths::default();
    }
    let (min_x, min_y, max_x, max_y) = bounds.unwrap();

    // Calculate hexagon size based on density
    // Higher density = smaller hexagons
    let line_width = 0.4;
    let hex_size = (line_width / density) * 1.5; // Slightly larger spacing for honeycomb

    let cos_a = angle_offset.cos();
    let sin_a = angle_offset.sin();

    let mut lines = Paths::default();

    // Hexagon geometry constants
    let hex_width = hex_size * 2.0;
    let hex_height = hex_size * 1.732; // sqrt(3)
    let hex_vert_spacing = hex_height * 0.75;

    // Generate hexagonal grid
    let mut row = 0;
    let mut y = min_y - hex_height;

    while y < max_y + hex_height {
        let offset_x = if row % 2 == 0 { 0.0 } else { hex_width / 2.0 };
        let mut x = min_x - hex_width + offset_x;

        while x < max_x + hex_width {
            // Generate hexagon centered at (x, y)
            let hexagon = generate_hexagon(x, y, hex_size, cos_a, sin_a);

            // Add hexagon edges as separate line segments
            for i in 0..6 {
                let start = hexagon[i];
                let end = hexagon[(i + 1) % 6];
                let path: Path = vec![start, end].into();
                lines.push(path);
            }

            x += hex_width;
        }

        y += hex_vert_spacing;
        row += 1;
    }

    lines
}

/// Generate vertices of a hexagon centered at (cx, cy) with given size and rotation.
fn generate_hexagon(cx: f64, cy: f64, size: f64, cos_a: f64, sin_a: f64) -> [(f64, f64); 6] {
    let mut vertices = [(0.0, 0.0); 6];

    for (i, vertex) in vertices.iter_mut().enumerate() {
        let angle = (i as f64) * std::f64::consts::PI / 3.0; // 60 degrees apart
        let local_x = size * angle.cos();
        let local_y = size * angle.sin();

        // Apply rotation and translation
        *vertex = (
            cx + local_x * cos_a - local_y * sin_a,
            cy + local_x * sin_a + local_y * cos_a,
        );
    }

    vertices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_honeycomb_empty_region() {
        let region = Paths::default();
        let result = generate_honeycomb(&region, 0.2, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_honeycomb_zero_density() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);

        let result = generate_honeycomb(&region, 0.0, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_honeycomb_generates_hexagons() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);

        let result = generate_honeycomb(&region, 0.2, 0.0);
        assert!(!result.is_empty(), "Should generate honeycomb pattern");

        // Each hexagon has 6 edges, so should be divisible by 6
        assert_eq!(result.len() % 6, 0, "Should generate complete hexagons");
    }

    #[test]
    fn test_generate_hexagon_has_six_vertices() {
        let hexagon = generate_hexagon(10.0, 10.0, 2.0, 1.0, 0.0);
        assert_eq!(hexagon.len(), 6);
    }
}
