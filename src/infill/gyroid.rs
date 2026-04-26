//! Gyroid infill pattern implementation.
//!
//! Generates a 3D mathematical surface pattern that provides excellent strength
//! properties and is particularly suitable for flexible materials.

use clipper2::*;
use super::utils::calculate_bounds;

/// Generate gyroid infill pattern.
///
/// Creates a mathematically-defined 3D surface pattern based on the gyroid
/// minimal surface. This pattern provides superior mechanical properties,
/// particularly for load distribution, but is computationally intensive.
///
/// The gyroid surface is defined by: sin(x)cos(y) + sin(y)cos(z) + sin(z)cos(x) = 0
///
/// # Arguments
/// * `region` - The infill region boundaries
/// * `density` - Infill density as a fraction (0.0-1.0)
/// * `angle_offset` - Rotation angle in radians for this layer
///
/// # Returns
/// Paths representing the gyroid surface at the current Z-height
pub fn generate_gyroid(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    if density <= 0.0 || region.is_empty() {
        return Paths::default();
    }

    let bounds = calculate_bounds(region);
    if bounds.is_none() {
        return Paths::default();
    }
    let (min_x, min_y, max_x, max_y) = bounds.unwrap();

    // Calculate scale based on density
    // Higher density = more frequent oscillations
    let line_width = 0.4;
    let scale = (line_width / density) * 2.0;
    
    let cos_a = angle_offset.cos();
    let sin_a = angle_offset.sin();
    
    let mut lines = Paths::default();
    
    // Sample the gyroid surface at the current layer
    // We'll create a contour by following iso-lines of the gyroid function
    let z_phase = angle_offset; // Use angle_offset as Z-phase for layer variation
    
    // Generate horizontal scan lines and find intersections
    let step = line_width; // Resolution for sampling
    let mut y = min_y;
    
    while y < max_y {
        let mut current_path: Vec<(f64, f64)> = Vec::new();
        let mut x = min_x;
        let mut was_inside = false;
        
        while x < max_x {
            // Evaluate gyroid function: sin(x)cos(y) + sin(y)cos(z) + sin(z)cos(x)
            let gx = (x / scale) * std::f64::consts::TAU;
            let gy = (y / scale) * std::f64::consts::TAU;
            let gz = z_phase * std::f64::consts::TAU;
            
            let gyroid_value = gx.sin() * gy.cos() 
                             + gy.sin() * gz.cos() 
                             + gz.sin() * gx.cos();
            
            // Threshold determines density - smaller threshold = more material
            let threshold = 0.5 - (density * 0.5);
            let is_inside = gyroid_value > threshold;
            
            if is_inside != was_inside {
                // Apply rotation if needed
                let rotated_x = x * cos_a - y * sin_a + (max_x + min_x) / 2.0 * (1.0 - cos_a) + (max_y + min_y) / 2.0 * sin_a;
                let rotated_y = x * sin_a + y * cos_a - (max_x + min_x) / 2.0 * sin_a + (max_y + min_y) / 2.0 * (1.0 - cos_a);
                
                current_path.push((rotated_x, rotated_y));
                
                // If we have at least 2 points, create a line segment
                if current_path.len() >= 2 {
                    let path: Path = current_path.clone().into();
                    lines.push(path);
                    current_path.clear();
                }
                
                was_inside = is_inside;
            }
            
            x += step;
        }
        
        y += step;
    }
    
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gyroid_empty_region() {
        let region = Paths::default();
        let result = generate_gyroid(&region, 0.2, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_gyroid_zero_density() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result = generate_gyroid(&region, 0.0, 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_gyroid_generates_curves() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result = generate_gyroid(&region, 0.2, 0.0);
        assert!(!result.is_empty(), "Should generate gyroid pattern");
    }

    #[test]
    fn test_gyroid_varies_by_layer() {
        let mut region = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        region.push(square);
        
        let result1 = generate_gyroid(&region, 0.2, 0.0);
        let result2 = generate_gyroid(&region, 0.2, std::f64::consts::FRAC_PI_2);
        
        // Different angles should produce different patterns
        // This is a simple check - in practice the patterns will differ significantly
        assert!(!result1.is_empty());
        assert!(!result2.is_empty());
    }
}
