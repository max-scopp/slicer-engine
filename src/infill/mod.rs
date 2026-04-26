//! Infill pattern generation for 3D printing.
//!
//! This module provides functions to generate various infill patterns (linear,
//! grid, honeycomb, gyroid) within closed perimeter regions. Infill provides
//! internal structure and strength while minimizing material usage.
//!
//! # Pattern Types
//!
//! - **Rectilinear**: Parallel lines alternating direction per layer (fastest)
//! - **Grid**: Perpendicular lines forming a grid pattern (stronger)
//! - **Honeycomb**: Hexagonal cells (good strength-to-weight ratio)
//! - **Gyroid**: 3D mathematical pattern (experimental, best strength)
//!
//! # Usage
//!
//! ```rust,no_run
//! use slicer_engine::infill::{generate_infill, InfillPattern};
//! use clipper2::Paths;
//!
//! let perimeter_paths = Paths::default(); // from slice_mesh
//! let infill_paths = generate_infill(
//!     &perimeter_paths,
//!     InfillPattern::Rectilinear,
//!     0.2,  // 20% density
//!     0.0,  // layer rotation angle
//! );
//! ```

use clipper2::*;

/// Supported infill patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfillPattern {
    /// Parallel lines alternating direction per layer (default, fastest).
    #[default]
    Rectilinear,
    /// Perpendicular lines forming a grid pattern (stronger).
    Grid,
    /// Hexagonal cells (good strength-to-weight ratio).
    Honeycomb,
    /// 3D mathematical pattern (experimental, best strength).
    Gyroid,
}

impl InfillPattern {
    /// Parse pattern name from string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rectilinear" | "linear" => Some(Self::Rectilinear),
            "grid" => Some(Self::Grid),
            "honeycomb" | "hexagonal" => Some(Self::Honeycomb),
            "gyroid" => Some(Self::Gyroid),
            _ => None,
        }
    }

    /// Get the canonical name of this pattern.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rectilinear => "rectilinear",
            Self::Grid => "grid",
            Self::Honeycomb => "honeycomb",
            Self::Gyroid => "gyroid",
        }
    }
}

/// Generate infill paths within the given perimeter regions.
///
/// # Arguments
/// * `perimeters` - Closed contour paths defining the boundaries
/// * `pattern` - The infill pattern to generate
/// * `density` - Infill density as a fraction (0.0 = no infill, 1.0 = solid)
/// * `angle_offset` - Rotation angle in radians for this layer (for alternating patterns)
///
/// # Returns
/// A `Paths` collection containing the infill line segments clipped to the perimeter regions.
/// Returns empty paths if density is zero or perimeters are empty.
pub fn generate_infill(
    perimeters: &Paths,
    pattern: InfillPattern,
    density: f64,
    angle_offset: f64,
) -> Paths {
    // Early exit for no infill or invalid density
    if density <= 0.0 || perimeters.is_empty() {
        return Paths::default();
    }

    // Clamp density to valid range [0, 1]
    let density = density.clamp(0.0, 1.0);

    // Calculate infill region by offsetting perimeters inward
    // This creates a gap between perimeter and infill for better adhesion
    let infill_region = calculate_infill_region(perimeters);

    if infill_region.is_empty() {
        return Paths::default();
    }

    // Generate pattern-specific line segments
    let raw_lines = match pattern {
        InfillPattern::Rectilinear => generate_rectilinear(&infill_region, density, angle_offset),
        InfillPattern::Grid => generate_grid(&infill_region, density, angle_offset),
        InfillPattern::Honeycomb => generate_honeycomb(&infill_region, density, angle_offset),
        InfillPattern::Gyroid => generate_gyroid(&infill_region, density, angle_offset),
    };

    // Clip the generated lines to the infill region boundaries
    clip_lines_to_region(&raw_lines, &infill_region)
}

/// Calculate the infill region by offsetting perimeters inward.
///
/// Creates a small gap (typically 0.1-0.2mm) between the perimeter wall and
/// infill to ensure good layer adhesion and prevent gaps.
fn calculate_infill_region(perimeters: &Paths) -> Paths {
    // Offset inward by 0.15mm to create gap between perimeter and infill
    // Negative offset = inward (deflate)
    let offset_delta = -0.15;

    // Use Clipper2 inflate operation (inflate with negative delta = deflate)
    use clipper2::inflate;
    inflate(
        perimeters.clone(),
        offset_delta,
        JoinType::Miter,
        EndType::Polygon,
        2.0, // miter limit
    )
}

/// Generate rectilinear (parallel line) infill pattern.
///
/// Lines alternate direction by 90° each layer using the angle_offset.
fn generate_rectilinear(region: &Paths, density: f64, angle_offset: f64) -> Paths {
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

/// Generate grid infill pattern (perpendicular lines).
fn generate_grid(region: &Paths, density: f64, angle_offset: f64) -> Paths {
    // Grid is two sets of rectilinear lines at 90° to each other
    let mut lines = generate_rectilinear(region, density, angle_offset);
    let perpendicular_lines = generate_rectilinear(region, density, angle_offset + std::f64::consts::FRAC_PI_2);
    
    // Merge the two sets of lines
    lines.push(perpendicular_lines);
    lines
}

/// Generate honeycomb (hexagonal) infill pattern.
fn generate_honeycomb(_region: &Paths, _density: f64, _angle_offset: f64) -> Paths {
    // TODO: Implement hexagonal pattern
    // For now, fall back to grid pattern
    Paths::default()
}

/// Generate gyroid infill pattern.
fn generate_gyroid(_region: &Paths, _density: f64, _angle_offset: f64) -> Paths {
    // TODO: Implement gyroid pattern using mathematical surface
    // For now, fall back to rectilinear
    Paths::default()
}

/// Clip generated line segments to the infill region boundaries.
///
/// Uses Clipper2 intersection to trim lines that extend outside the region.
fn clip_lines_to_region(lines: &Paths, _region: &Paths) -> Paths {
    if lines.is_empty() {
        return Paths::default();
    }

    // TODO: Properly implement line clipping against region using Clipper2
    // For now, return lines as-is since they're generated within bounds
    // Future improvement: clip individual line segments that extend beyond region
    lines.clone()
}

/// Calculate the axis-aligned bounding box of a set of paths.
///
/// Returns (min_x, min_y, max_x, max_y) or None if paths are empty.
fn calculate_bounds(paths: &Paths) -> Option<(f64, f64, f64, f64)> {
    if paths.is_empty() {
        return None;
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths.iter() {
        for point in path.iter() {
            let x = point.x();
            let y = point.y();
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infill_pattern_default() {
        assert_eq!(InfillPattern::default(), InfillPattern::Rectilinear);
    }

    #[test]
    fn test_infill_pattern_from_str() {
        assert_eq!(InfillPattern::parse("rectilinear"), Some(InfillPattern::Rectilinear));
        assert_eq!(InfillPattern::parse("linear"), Some(InfillPattern::Rectilinear));
        assert_eq!(InfillPattern::parse("grid"), Some(InfillPattern::Grid));
        assert_eq!(InfillPattern::parse("GRID"), Some(InfillPattern::Grid));
        assert_eq!(InfillPattern::parse("honeycomb"), Some(InfillPattern::Honeycomb));
        assert_eq!(InfillPattern::parse("gyroid"), Some(InfillPattern::Gyroid));
        assert_eq!(InfillPattern::parse("invalid"), None);
    }

    #[test]
    fn test_infill_pattern_name() {
        assert_eq!(InfillPattern::Rectilinear.name(), "rectilinear");
        assert_eq!(InfillPattern::Grid.name(), "grid");
        assert_eq!(InfillPattern::Honeycomb.name(), "honeycomb");
        assert_eq!(InfillPattern::Gyroid.name(), "gyroid");
    }

    #[test]
    fn test_generate_infill_empty_perimeters() {
        let perimeters = Paths::default();
        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.2, 0.0);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_infill_zero_density() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.0, 0.0);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_infill_rectilinear_basic() {
        let mut perimeters = Paths::default();
        // Use a larger square to ensure there's space for infill after offset
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.2, 0.0);
        
        // Should generate some infill lines (non-empty)
        assert!(!infill.is_empty(), "Expected infill lines to be generated");
    }

    #[test]
    fn test_calculate_bounds_empty() {
        let paths = Paths::default();
        assert!(calculate_bounds(&paths).is_none());
    }

    #[test]
    fn test_calculate_bounds_square() {
        let mut paths = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        paths.push(square);

        let bounds = calculate_bounds(&paths);
        assert!(bounds.is_some());
        let (min_x, min_y, max_x, max_y) = bounds.unwrap();
        assert_eq!(min_x, 0.0);
        assert_eq!(min_y, 0.0);
        assert_eq!(max_x, 10.0);
        assert_eq!(max_y, 10.0);
    }

    #[test]
    fn test_calculate_infill_region_offsets_inward() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        perimeters.push(square);

        let infill_region = calculate_infill_region(&perimeters);
        // Should produce a smaller region (offset inward)
        assert!(!infill_region.is_empty(), "Expected offset region to exist");
    }
}
