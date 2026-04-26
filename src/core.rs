//! Core slicing operations and data structures

use clipper2::*;

/// Represents a slice layer in the 3D model
#[derive(Debug, Clone)]
pub struct SliceLayer {
    /// Z-coordinate of this layer
    pub z: f64,
    /// Paths that make up this layer
    pub paths: Paths,
}

impl SliceLayer {
    /// Create a new slice layer at the given Z coordinate
    pub fn new(z: f64) -> Self {
        Self {
            z,
            paths: Paths::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_layer_creation() {
        let layer = SliceLayer::new(1.0);
        assert_eq!(layer.z, 1.0);
        assert!(layer.paths.is_empty());
    }
}
