//! Grid infill pattern implementation.
//!
//! Generates perpendicular lines in both directions, creating a grid pattern
//! for improved strength in all directions.

use clipper2::*;
use super::rectilinear::generate_rectilinear;

/// Generate grid infill pattern (perpendicular lines).
///
/// Creates two sets of parallel lines at 90° to each other, forming a grid.
/// This pattern provides better strength than rectilinear but uses more material.
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths containing perpendicular line segments forming a grid
pub fn generate_grid(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    // Grid is two sets of rectilinear lines at 90° to each other
    let mut lines = generate_rectilinear(region, density, angle_offset);
    let perpendicular_lines = generate_rectilinear(region, density, angle_offset + std::f64::consts::FRAC_PI_2);
    
    // Merge the two sets of lines
    lines.push(perpendicular_lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_empty_region() {
        let region = Paths::default();
        let result = generate_grid(&region, 0.2, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_grid_generates_more_than_rectilinear() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let rectilinear_result = generate_rectilinear(&region, 0.2, 0.0);
        let grid_result = generate_grid(&region, 0.2, 0.0);
        
        // Grid should have approximately double the paths (two directions)
        assert!(grid_result.len() > rectilinear_result.len(), 
            "Grid should generate more paths than rectilinear");
    }
}
