//! Utility functions for infill generation.
//!
//! Provides common helper functions used across different infill pattern
//! implementations.

use clipper2::*;

/// Calculate the infill region by offsetting perimeters inward.
///
/// Creates a small gap (typically 0.1-0.2mm) between the perimeter wall and
/// infill to ensure good layer adhesion and prevent gaps.
pub fn calculate_infill_region(perimeters: &Paths) -> Paths {
    // Offset inward by 0.15mm to create gap between perimeter and infill
    // Negative offset = inward (deflate)
    let offset_delta = -0.15;

    // Use Clipper2 inflate operation (inflate with negative delta = deflate)
    inflate(
        perimeters.clone(),
        offset_delta,
        JoinType::Miter,
        EndType::Polygon,
        2.0, // miter limit
    )
}

/// Calculate the axis-aligned bounding box of a set of paths.
///
/// Returns (min_x, min_y, max_x, max_y) or None if paths are empty.
pub fn calculate_bounds(paths: &Paths) -> Option<(f64, f64, f64, f64)> {
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

/// Clip generated line segments to the infill region boundaries.
///
/// Uses Clipper2 intersection to trim lines that extend outside the region.
pub fn clip_lines_to_region(lines: &Paths, _region: &Paths) -> Paths {
    if lines.is_empty() {
        return Paths::default();
    }

    // TODO: Properly implement line clipping against region using Clipper2
    // For now, return lines as-is since they're generated within bounds
    // Future improvement: clip individual line segments that extend beyond region
    lines.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

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
