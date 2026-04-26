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

mod rectilinear;
mod grid;
mod honeycomb;
mod gyroid;
mod utils;

use rectilinear::generate_rectilinear;
use grid::generate_grid;
use honeycomb::generate_honeycomb;
use gyroid::generate_gyroid;
use utils::{calculate_infill_region, clip_lines_to_region};

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
    fn test_generate_infill_honeycomb_basic() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Honeycomb, 0.2, 0.0);
        
        // Should generate honeycomb pattern
        assert!(!infill.is_empty(), "Expected honeycomb infill to be generated");
    }

    #[test]
    fn test_generate_infill_gyroid_basic() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Gyroid, 0.2, 0.0);
        
        // Should generate gyroid pattern
        assert!(!infill.is_empty(), "Expected gyroid infill to be generated");
    }
}
