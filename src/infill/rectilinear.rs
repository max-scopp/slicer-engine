//! Rectilinear (linear) infill pattern implementation.
//!
//! Generates parallel lines that alternate direction by 90° each layer for
//! optimal mechanical strength and minimal material usage.

use clipper2::*;
use super::utils::{calculate_bounds};

/// Generate rectilinear (parallel line) infill pattern.
///
/// Lines alternate direction by 90° each layer using the angle_offset.
/// This pattern is the fastest to generate and print.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths containing parallel line segments
pub fn generate_rectilinear(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    // Calculate line spacing from density
    // Typical line width is 0.4mm, spacing inversely proportional to density
    let line_width = 0.4;
    let spacing = if density > 0.0 {
        line_width / density
    } else {
        return Paths::default();
    };

    // Get bounding box of the region
    let bounds = calculate_bounds(region);
    if bounds.is_none() {
        return Paths::default();
    }
    let (min_x, min_y, max_x, max_y) = bounds.unwrap();

    // Calculate angle (0° or 90° alternating, plus offset)
    let angle = angle_offset;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Generate parallel lines across the bounding box
    let mut lines = Paths::default();
    let diagonal = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    let mut offset = -diagonal / 2.0;
    while offset <= diagonal / 2.0 {
        // Create a line perpendicular to the angle
        let line_start = (
            center_x - diagonal * sin_a + offset * cos_a,
            center_y + diagonal * cos_a + offset * sin_a,
        );
        let line_end = (
            center_x + diagonal * sin_a + offset * cos_a,
            center_y - diagonal * cos_a + offset * sin_a,
        );

        let path: Path = vec![line_start, line_end].into();
        lines.push(path);

        offset += spacing;
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectilinear_empty_region() {
        let region = Paths::default();
        let result = generate_rectilinear(&region, 0.2, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rectilinear_zero_density() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        region.push(square);
        
        let result = generate_rectilinear(&region, 0.0, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rectilinear_generates_lines() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result = generate_rectilinear(&region, 0.2, 0.0);
        assert!(!result.is_empty(), "Should generate infill lines");
    }
}
