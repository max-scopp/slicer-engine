//! Spatial analysis functions: AABB, volume, surface area.

use crate::mesh::types::{Mesh, AABB};

/// Compute the Axis-Aligned Bounding Box for the given mesh.
///
/// Returns an `AABB` covering all vertices. Panics if the mesh has no vertices.
pub fn calculate_aabb(mesh: &Mesh) -> AABB {
    AABB::new_from_vertices(&mesh.vertices)
        .expect("Mesh must have at least one vertex to calculate AABB")
}

/// Compute the signed volume of a closed mesh using the divergence theorem (mm³).
///
/// Each triangle contributes `(v0 · (v1 × v2)) / 6` to the total, which equals
/// the volume of the signed tetrahedron between the triangle and the origin.
/// Summing over all faces and taking the absolute value gives the mesh volume.
///
/// Returns an error string if the mesh appears to be open (the raw sum is
/// suspiciously small relative to the surface area, which would not be the case
/// for a geometrically valid closed shell). Note: the current implementation
/// simply checks if the mesh has any faces; full open-mesh detection is deferred.
pub fn calculate_volume(mesh: &Mesh) -> Result<f64, String> {
    if mesh.faces.is_empty() {
        return Err("Mesh has no faces; cannot calculate volume".to_string());
    }

    let mut signed_volume = 0.0_f64;
    for face in &mesh.faces {
        let v0 = face.vertices[0];
        let v1 = face.vertices[1];
        let v2 = face.vertices[2];

        // Scalar triple product: v0 · (v1 × v2)
        let cross_x = v1.y * v2.z - v1.z * v2.y;
        let cross_y = v1.z * v2.x - v1.x * v2.z;
        let cross_z = v1.x * v2.y - v1.y * v2.x;
        signed_volume += v0.x * cross_x + v0.y * cross_y + v0.z * cross_z;
    }

    Ok((signed_volume / 6.0).abs())
}

/// Compute the total surface area of the mesh (mm²) by summing the area of
/// every triangle face.
pub fn calculate_surface_area(mesh: &Mesh) -> f64 {
    mesh.faces.iter().map(|f| f.area()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::types::{Face, Mesh, Vertex};

    /// Build a 10×10×10 axis-aligned unit cube mesh (12 triangles).
    fn make_cube_mesh() -> Mesh {
        let v = [
            Vertex::new(0.0, 0.0, 0.0),    // 0
            Vertex::new(10.0, 0.0, 0.0),   // 1
            Vertex::new(10.0, 10.0, 0.0),  // 2
            Vertex::new(0.0, 10.0, 0.0),   // 3
            Vertex::new(0.0, 0.0, 10.0),   // 4
            Vertex::new(10.0, 0.0, 10.0),  // 5
            Vertex::new(10.0, 10.0, 10.0), // 6
            Vertex::new(0.0, 10.0, 10.0),  // 7
        ];

        // 12 faces (2 triangles per side × 6 sides), wound outward
        let face_indices: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2], // bottom  (-Z)
            [4, 5, 6],
            [4, 6, 7], // top     (+Z)
            [0, 1, 5],
            [0, 5, 4], // front   (-Y)
            [2, 3, 7],
            [2, 7, 6], // back    (+Y)
            [0, 4, 7],
            [0, 7, 3], // left    (-X)
            [1, 2, 6],
            [1, 6, 5], // right   (+X)
        ];

        let faces: Vec<Face> = face_indices
            .iter()
            .map(|idx| Face::new([v[idx[0]], v[idx[1]], v[idx[2]]]))
            .collect();

        Mesh {
            vertices: v.to_vec(),
            faces,
            aabb: None,
        }
    }

    #[test]
    fn test_aabb_on_cube() {
        let mesh = make_cube_mesh();
        let aabb = calculate_aabb(&mesh);
        assert_eq!(aabb.min.x, 0.0);
        assert_eq!(aabb.min.y, 0.0);
        assert_eq!(aabb.min.z, 0.0);
        assert_eq!(aabb.max.x, 10.0);
        assert_eq!(aabb.max.y, 10.0);
        assert_eq!(aabb.max.z, 10.0);
    }

    #[test]
    fn test_volume_on_cube() {
        let mesh = make_cube_mesh();
        let vol = calculate_volume(&mesh).unwrap();
        // 10^3 = 1000 mm³, allow 1% tolerance
        assert!((vol - 1000.0).abs() < 10.0, "Volume was {vol}");
    }

    #[test]
    fn test_surface_area_on_cube() {
        let mesh = make_cube_mesh();
        let area = calculate_surface_area(&mesh);
        // 6 sides × 10×10 = 600 mm², allow 1% tolerance
        assert!((area - 600.0).abs() < 6.0, "Surface area was {area}");
    }

    #[test]
    fn test_volume_empty_mesh_returns_error() {
        let mesh = Mesh::new();
        assert!(calculate_volume(&mesh).is_err());
    }
}
