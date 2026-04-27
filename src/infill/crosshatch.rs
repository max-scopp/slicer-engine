//! Cross hatch infill pattern implementation.
//!
//! Generates a simple cross-hatch pattern similar to rectilinear but only
//! one direction per layer (simpler than grid which has both directions per layer).
//! This is the most material-efficient pattern while still providing good strength.

use clipper2::*;
use super::rectilinear::generate_rectilinear;

/// Generate cross hatch infill pattern.
///
/// Cross hatch is similar to rectilinear - it generates parallel lines
/// at the specified angle. The difference is that it's meant to be used
/// with alternating angles per layer to create a cross-hatched appearance
/// across multiple layers. Each individual layer only has lines in one direction.
///
/// This pattern is very material-efficient and prints fast while still
/// providing reasonable strength due to the cross-hatching between layers.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths containing parallel line segments
pub fn generate_crosshatch(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    // Cross hatch is just rectilinear lines in one direction
    // The alternating angle is handled by the caller (add_infill_to_layers)
    generate_rectilinear(region, density, angle_offset)
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
    fn test_crosshatch_different_angles() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result1 = generate_crosshatch(&region, 0.2, 0.0);
        let result2 = generate_crosshatch(&region, 0.2, std::f64::consts::FRAC_PI_2);
        
        // Different angles should produce different line orientations
        assert!(!result1.is_empty());
        assert!(!result2.is_empty());
        
        // The patterns should be geometrically different
        // (we expect the line coordinates to be significantly different)
        let has_difference = result1.iter().zip(result2.iter()).any(|(p1, p2)| {
            p1.iter().zip(p2.iter()).any(|(pt1, pt2)| {
                (pt1.x() - pt2.x()).abs() > 1.0 || (pt1.y() - pt2.y()).abs() > 1.0
            })
        });
        
        assert!(has_difference, "Different angles should produce different patterns");
    }
}
