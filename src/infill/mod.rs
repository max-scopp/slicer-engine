//! Infill pattern generation for 3D printing.
//!
//! This module provides functions to generate various infill patterns within closed
//! perimeter regions. Infill provides internal structure and strength while minimizing
//! material usage.
//!
//! # Pattern Types
//!
//! - **Rectilinear**: Parallel lines alternating direction per layer (fastest)
//! - **Grid**: Perpendicular lines forming a grid pattern (stronger)
//! - **Honeycomb**: Hexagonal cells (good strength-to-weight ratio)
//! - **Gyroid**: 3D mathematical pattern (best strength, isotropic)
//! - **TpmsD**: Triply Periodic Minimal Surface - Diamond (organic, isotropic structure)
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
//!     InfillPattern::TpmsD,
//!     0.2,  // 20% density
//!     0.0,  // layer rotation angle
//!     0.2,  // Z height in mm
//! );
//! ```

use clipper2::*;

mod rectilinear;
mod grid;
mod honeycomb;
mod gyroid;
mod tpms_d;
mod utils;

use rectilinear::generate_rectilinear;
use grid::generate_grid;
use honeycomb::generate_honeycomb;
use gyroid::generate_gyroid;
use tpms_d::generate_tpms_d;
use utils::clip_lines_to_region;

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
    /// Triply Periodic Minimal Surface - Diamond (organic, isotropic structure).
    TpmsD,
}

impl InfillPattern {
    /// Parse pattern name from string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rectilinear" | "linear" => Some(Self::Rectilinear),
            "grid" => Some(Self::Grid),
            "honeycomb" | "hexagonal" => Some(Self::Honeycomb),
            "gyroid" => Some(Self::Gyroid),
            "tpms-d" | "tpmsd" | "tpms_d" => Some(Self::TpmsD),
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
            Self::TpmsD => "tpms-d",
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
/// * `z_height` - Z coordinate of the current layer (for 3D patterns like gyroid)
///
/// # Returns
/// A `Paths` collection containing the infill line segments clipped to the perimeter regions.
/// Returns empty paths if density is zero or perimeters are empty.
pub fn generate_infill(
    perimeters: &Paths,
    pattern: InfillPattern,
    density: f64,
    angle_offset: f64,
    z_height: f64,
) -> Paths {
    // Early exit for no infill or invalid density
    if density <= 0.0 || perimeters.is_empty() {
        return Paths::default();
    }

    // Clamp density to valid range [0, 1]
    let density = density.clamp(0.0, 1.0);

    // `perimeters` is already the correctly-bounded interior region produced by
    // `calculate_interior_region` in `add_infill_to_layers`.  Do NOT apply an
    // additional inward offset here: the caller has already placed the boundary
    // at the inner edge of the innermost wall (accounting for all wall beads
    // and the configured infill-overlap percentage).  A second inward deflation
    // was causing a double-inset that collapsed the infill region entirely on
    // features narrower than ~2× the extra offset, producing the "missing
    // infill on many layers" artifact visible on complex geometry (e.g. the
    // 3DBenchy cabin and chimney transition layers).
    let raw_lines = match pattern {
        InfillPattern::Rectilinear => generate_rectilinear(perimeters, density, angle_offset),
        InfillPattern::Grid => generate_grid(perimeters, density, angle_offset),
        InfillPattern::Honeycomb => generate_honeycomb(perimeters, density, angle_offset),
        InfillPattern::Gyroid => generate_gyroid(perimeters, density, z_height),
        InfillPattern::TpmsD => generate_tpms_d(perimeters, density, z_height),
    };

    // Clip the generated lines to the infill region boundaries
    clip_lines_to_region(&raw_lines, perimeters)
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
        assert_eq!(InfillPattern::parse("tpms-d"), Some(InfillPattern::TpmsD));
        assert_eq!(InfillPattern::parse("tpmsd"), Some(InfillPattern::TpmsD));
        assert_eq!(InfillPattern::parse("invalid"), None);
    }

    #[test]
    fn test_infill_pattern_name() {
        assert_eq!(InfillPattern::Rectilinear.name(), "rectilinear");
        assert_eq!(InfillPattern::Grid.name(), "grid");
        assert_eq!(InfillPattern::Honeycomb.name(), "honeycomb");
        assert_eq!(InfillPattern::Gyroid.name(), "gyroid");
        assert_eq!(InfillPattern::TpmsD.name(), "tpms-d");
    }

    #[test]
    fn test_generate_infill_empty_perimeters() {
        let perimeters = Paths::default();
        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.2, 0.0, 0.2);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_infill_zero_density() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.0, 0.0, 0.2);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_infill_rectilinear_basic() {
        let mut perimeters = Paths::default();
        // Use a larger square to ensure there's space for infill after offset
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Rectilinear, 0.2, 0.0, 0.2);
        
        // Should generate some infill lines (non-empty)
        assert!(!infill.is_empty(), "Expected infill lines to be generated");
    }

    #[test]
    fn test_generate_infill_honeycomb_basic() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Honeycomb, 0.2, 0.0, 0.2);
        
        // Should generate honeycomb pattern
        assert!(!infill.is_empty(), "Expected honeycomb infill to be generated");
    }

    #[test]
    fn test_generate_infill_gyroid_basic() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::Gyroid, 0.2, 0.0, 0.2);
        
        // Should generate gyroid pattern
        assert!(!infill.is_empty(), "Expected gyroid infill to be generated");
    }

    #[test]
    fn test_generate_infill_tpms_d_basic() {
        let mut perimeters = Paths::default();
        let square: Path = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)].into();
        perimeters.push(square);

        let infill = generate_infill(&perimeters, InfillPattern::TpmsD, 0.2, 0.0, 0.2);
        
        // Should generate tpms-d pattern
        assert!(!infill.is_empty(), "Expected tpms-d infill to be generated");
    }
}
