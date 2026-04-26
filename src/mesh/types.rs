//! Core mesh data types: Vertex, Face, AABB, Mesh
//!
//! All coordinate values are assumed to be in millimeters.
//! The Z axis is vertical (up) per slicing convention.

use serde::{Deserialize, Serialize};

/// A point in 3D space, expressed in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    /// X coordinate in mm
    pub x: f64,
    /// Y coordinate in mm
    pub y: f64,
    /// Z coordinate (vertical) in mm
    pub z: f64,
}

impl Vertex {
    /// Create a new vertex at the given coordinates.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Euclidean distance to another vertex.
    pub fn distance_to(&self, other: &Vertex) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// A triangular face defined by three vertices with an optional surface normal.
///
/// Normals are stored as they appear in the source STL file and may be `None`
/// when the file does not provide them (e.g. ASCII files with zero normals).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    /// The three corner vertices of the triangle (in mm).
    pub vertices: [Vertex; 3],
    /// Optional surface normal (unit vector pointing outward).
    pub normal: Option<Vertex>,
}

impl Face {
    /// Create a new face from three vertices. Normal defaults to `None`.
    pub fn new(vertices: [Vertex; 3]) -> Self {
        Self {
            vertices,
            normal: None,
        }
    }

    /// Surface area of the triangle using Heron's formula (mm²).
    pub fn area(&self) -> f64 {
        let [a, b, c] = &self.vertices;
        let ab = a.distance_to(b);
        let bc = b.distance_to(c);
        let ca = c.distance_to(a);
        let s = (ab + bc + ca) / 2.0;
        let area_sq = s * (s - ab) * (s - bc) * (s - ca);
        if area_sq > 0.0 {
            area_sq.sqrt()
        } else {
            0.0
        }
    }
}

/// Axis-Aligned Bounding Box for a mesh.
///
/// `min` and `max` corners are in millimeters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AABB {
    /// Corner with the smallest x, y, z values.
    pub min: Vertex,
    /// Corner with the largest x, y, z values.
    pub max: Vertex,
}

impl AABB {
    /// Compute the AABB from a slice of vertices.
    ///
    /// Returns `None` if the slice is empty.
    pub fn new_from_vertices(vertices: &[Vertex]) -> Option<Self> {
        let first = vertices.first()?;
        let mut min = *first;
        let mut max = *first;
        for v in vertices.iter().skip(1) {
            if v.x < min.x {
                min.x = v.x;
            }
            if v.y < min.y {
                min.y = v.y;
            }
            if v.z < min.z {
                min.z = v.z;
            }
            if v.x > max.x {
                max.x = v.x;
            }
            if v.y > max.y {
                max.y = v.y;
            }
            if v.z > max.z {
                max.z = v.z;
            }
        }
        Some(Self { min, max })
    }

    /// Width of the bounding box along the X axis (mm).
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    /// Depth of the bounding box along the Y axis (mm).
    pub fn depth(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// Height of the bounding box along the Z axis (mm).
    pub fn height(&self) -> f64 {
        self.max.z - self.min.z
    }

    /// Geometric center of the bounding box.
    pub fn center(&self) -> Vertex {
        Vertex::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
            (self.min.z + self.max.z) / 2.0,
        )
    }

    /// Returns `true` if the point lies within (or on the surface of) the AABB.
    pub fn contains_point(&self, p: &Vertex) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
}

/// A triangle-mesh 3D model.
///
/// Coordinates are in millimeters, loaded in native STL coordinates
/// (no automatic transforms applied on import).
///
/// `aabb` is cached after the first call to [`Mesh::calculate_aabb`] and
/// should be considered read-only by callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    /// Unique vertices of the mesh.
    pub vertices: Vec<Vertex>,
    /// Triangle faces referencing vertices.
    pub faces: Vec<Face>,
    /// Cached Axis-Aligned Bounding Box. `None` until first calculated.
    pub aabb: Option<AABB>,
}

impl Mesh {
    /// Create an empty mesh with no vertices or faces.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
            aabb: None,
        }
    }

    /// Calculate and cache the AABB for this mesh.
    ///
    /// Subsequent calls return the cached result without recomputing.
    /// Returns `None` if the mesh has no vertices.
    pub fn calculate_aabb(&mut self) -> Option<&AABB> {
        if self.aabb.is_none() {
            self.aabb = AABB::new_from_vertices(&self.vertices);
        }
        self.aabb.as_ref()
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_construction() {
        let v = Vertex::new(1.0, 2.0, 3.0);
        assert_eq!(v.x, 1.0);
        assert_eq!(v.y, 2.0);
        assert_eq!(v.z, 3.0);
    }

    #[test]
    fn test_vertex_distance_to() {
        let a = Vertex::new(0.0, 0.0, 0.0);
        let b = Vertex::new(3.0, 4.0, 0.0);
        assert!((a.distance_to(&b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_face_area_right_triangle() {
        // Right triangle with legs 3 and 4 → area = 6
        let a = Vertex::new(0.0, 0.0, 0.0);
        let b = Vertex::new(3.0, 0.0, 0.0);
        let c = Vertex::new(0.0, 4.0, 0.0);
        let face = Face::new([a, b, c]);
        assert!((face.area() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_aabb_from_vertices() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 10.0),
            Vertex::new(5.0, 5.0, 5.0),
        ];
        let aabb = AABB::new_from_vertices(&verts).unwrap();
        assert_eq!(aabb.min.x, 0.0);
        assert_eq!(aabb.max.x, 10.0);
        assert_eq!(aabb.width(), 10.0);
        assert_eq!(aabb.depth(), 10.0);
        assert_eq!(aabb.height(), 10.0);
    }

    #[test]
    fn test_aabb_center() {
        let aabb = AABB {
            min: Vertex::new(0.0, 0.0, 0.0),
            max: Vertex::new(10.0, 10.0, 10.0),
        };
        let center = aabb.center();
        assert_eq!(center.x, 5.0);
        assert_eq!(center.y, 5.0);
        assert_eq!(center.z, 5.0);
    }

    #[test]
    fn test_aabb_contains_point() {
        let aabb = AABB {
            min: Vertex::new(0.0, 0.0, 0.0),
            max: Vertex::new(10.0, 10.0, 10.0),
        };
        assert!(aabb.contains_point(&Vertex::new(5.0, 5.0, 5.0)));
        assert!(!aabb.contains_point(&Vertex::new(11.0, 5.0, 5.0)));
    }

    #[test]
    fn test_mesh_creation_and_aabb_cache() {
        let mut mesh = Mesh::new();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.faces.is_empty());
        assert!(mesh.aabb.is_none());

        mesh.vertices = vec![Vertex::new(0.0, 0.0, 0.0), Vertex::new(1.0, 1.0, 1.0)];
        let aabb = mesh.calculate_aabb().unwrap();
        assert_eq!(aabb.max.x, 1.0);
        // Second call uses cache
        assert!(mesh.aabb.is_some());
    }
}
