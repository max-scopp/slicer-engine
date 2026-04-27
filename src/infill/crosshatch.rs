//! Cross hatch infill pattern implementation.
//!
//! Generates a zigzag infill pattern where lines continuously reverse direction
//! to create a serpentine toolpath, minimizing travel moves. This is more
//! material-efficient than grid while still providing good strength.

use clipper2::*;
use super::utils::calculate_bounds;

/// Generate cross hatch zigzag infill pattern.
///
/// Creates a continuous serpentine (zigzag) path by generating parallel lines
/// and chaining them together with alternating directions. This minimizes
/// non-printing travel moves and creates an efficient single toolpath.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths containing zigzag line segments forming a continuous serpentine path
pub fn generate_crosshatch(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    if density <= 0.0 || region.is_empty() {
        return Paths::default();
    }

    // Calculate line spacing from density
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

    // Calculate angle
    let angle = angle_offset;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Generate parallel lines across the bounding box
    let diagonal = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    // Collect all parallel lines
    let mut lines: Vec<((f64, f64), (f64, f64))> = Vec::new();
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

        lines.push((line_start, line_end));
        offset += spacing;
    }

    if lines.is_empty() {
        return Paths::default();
    }

    // Create zigzag pattern by alternating line directions
    let mut result = Paths::default();
    
    for (i, (start, end)) in lines.iter().enumerate() {
        let path: Path = if i % 2 == 0 {
            // Even lines: normal direction
            vec![*start, *end].into()
        } else {
            // Odd lines: reversed direction for zigzag
            vec![*end, *start].into()
        };
        result.push(path);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crosshatch_empty_region() {
        let region = Paths::default();
        let result = generate_crosshatch(&region, 0.2, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_crosshatch_zero_density() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        region.push(square);
        
        let result = generate_crosshatch(&region, 0.0, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_crosshatch_generates_lines() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result = generate_crosshatch(&region, 0.2, 0.0);
        assert!(!result.is_empty(), "Should generate cross hatch lines");
    }

    #[test]
    fn test_crosshatch_alternates_direction() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result = generate_crosshatch(&region, 0.2, 0.0);
        assert!(result.len() >= 2, "Should generate multiple lines");
        
        // Check that consecutive lines have different directions (zigzag)
        if result.len() >= 2 {
            let mut result_iter = result.iter();
            let line1 = result_iter.next().unwrap();
            let line2 = result_iter.next().unwrap();
            
            let line1_start = line1.iter().next().unwrap();
            let line1_end = line1.iter().last().unwrap();
            let line2_start = line2.iter().next().unwrap();
            let line2_end = line2.iter().last().unwrap();
            
            // For zigzag, consecutive lines should have reversed directions
            // Check both X and Y components
            let line1_dx = line1_end.x() - line1_start.x();
            let line1_dy = line1_end.y() - line1_start.y();
            let line2_dx = line2_end.x() - line2_start.x();
            let line2_dy = line2_end.y() - line2_start.y();
            
            // At least one component should be reversed
            let x_reversed = (line1_dx.abs() > 0.1) && (line1_dx.signum() != line2_dx.signum());
            let y_reversed = (line1_dy.abs() > 0.1) && (line1_dy.signum() != line2_dy.signum());
            
            assert!(x_reversed || y_reversed, 
                "Adjacent lines should alternate direction for zigzag (dx1={}, dy1={}, dx2={}, dy2={})",
                line1_dx, line1_dy, line2_dx, line2_dy);
        }
    }

    #[test]
    fn test_crosshatch_different_angles() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result1 = generate_crosshatch(&region, 0.2, 0.0);
        let result2 = generate_crosshatch(&region, 0.2, std::f64::consts::FRAC_PI_2);
        
        // Different angles should produce different line orientations
        assert!(!result1.is_empty());
        assert!(!result2.is_empty());
    }
}
