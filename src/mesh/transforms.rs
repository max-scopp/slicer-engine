//! Coordinate transformations for meshes.
//!
//! All transforms are immutable: they return a new `Mesh` and leave the
//! original unchanged. The cached `aabb` field is cleared on the new instance
//! so it is recomputed lazily on next access.

use crate::mesh::analysis::calculate_aabb;
use crate::mesh::types::{Face, Mesh, Vertex};

/// Translate every vertex of the mesh by the given offset.
///
/// Returns a new `Mesh`; the original is unchanged.
pub fn translate_mesh(mesh: &Mesh, offset: Vertex) -> Mesh {
    let vertices: Vec<Vertex> = mesh
        .vertices
        .iter()
        .map(|v| Vertex::new(v.x + offset.x, v.y + offset.y, v.z + offset.z))
        .collect();

    // Rebuild faces using the translated vertex positions
    let faces: Vec<Face> = mesh
        .faces
        .iter()
        .map(|f| Face {
            vertices: [
                Vertex::new(
                    f.vertices[0].x + offset.x,
                    f.vertices[0].y + offset.y,
                    f.vertices[0].z + offset.z,
                ),
                Vertex::new(
                    f.vertices[1].x + offset.x,
                    f.vertices[1].y + offset.y,
                    f.vertices[1].z + offset.z,
                ),
                Vertex::new(
                    f.vertices[2].x + offset.x,
                    f.vertices[2].y + offset.y,
                    f.vertices[2].z + offset.z,
                ),
            ],
            normal: f.normal,
        })
        .collect();

    Mesh {
        vertices,
        faces,
        aabb: None,
    }
}

/// Center the mesh horizontally so that the AABB center lies at (0, 0, z_center).
///
/// Only the X and Y axes are affected; the Z position is unchanged.
/// Returns a new `Mesh`; the original is unchanged.
pub fn center_mesh(mesh: &Mesh) -> Mesh {
    let aabb = calculate_aabb(mesh);
    let center = aabb.center();
    // Shift XY so the center is at the origin; Z is not altered.
    let offset = Vertex::new(-center.x, -center.y, 0.0);
    translate_mesh(mesh, offset)
}

/// Translate the mesh so that its lowest Z vertex sits exactly on Z = 0.
///
/// Returns a new `Mesh`; the original is unchanged.
pub fn drop_to_floor(mesh: &Mesh) -> Mesh {
    let aabb = calculate_aabb(mesh);
    let offset = Vertex::new(0.0, 0.0, -aabb.min.z);
    translate_mesh(mesh, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::analysis::calculate_aabb;
    use crate::mesh::types::{Face, Mesh, Vertex};

    /// Build a simple cube displaced from the origin.
    fn make_displaced_cube(x_off: f64, y_off: f64, z_off: f64) -> Mesh {
        let v = [
            Vertex::new(x_off, y_off, z_off),
            Vertex::new(x_off + 10.0, y_off, z_off),
            Vertex::new(x_off + 10.0, y_off + 10.0, z_off),
            Vertex::new(x_off, y_off + 10.0, z_off),
            Vertex::new(x_off, y_off, z_off + 10.0),
            Vertex::new(x_off + 10.0, y_off, z_off + 10.0),
            Vertex::new(x_off + 10.0, y_off + 10.0, z_off + 10.0),
            Vertex::new(x_off, y_off + 10.0, z_off + 10.0),
        ];
        let face_indices: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
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
    fn test_center_mesh() {
        // Cube from (10,10,5) to (20,20,15) — elevated so we verify Z is NOT shifted
        let mesh = make_displaced_cube(10.0, 10.0, 5.0);
        let centered = center_mesh(&mesh);
        let aabb = calculate_aabb(&centered);
        // AABB center in XY should be at (0, 0)
        assert!(
            (aabb.center().x).abs() < 1e-9,
            "center.x={}",
            aabb.center().x
        );
        assert!(
            (aabb.center().y).abs() < 1e-9,
            "center.y={}",
            aabb.center().y
        );
        // Z should be unchanged: floor stays at 5
        assert!((aabb.min.z - 5.0).abs() < 1e-9, "z_min={}", aabb.min.z);
    }

    #[test]
    fn test_drop_to_floor() {
        // Cube from (5,5,5) to (15,15,15)
        let mesh = make_displaced_cube(5.0, 5.0, 5.0);
        let dropped = drop_to_floor(&mesh);
        let aabb = calculate_aabb(&dropped);
        assert!((aabb.min.z).abs() < 1e-9, "z_min={}", aabb.min.z);
        // XY should be unchanged
        assert!((aabb.min.x - 5.0).abs() < 1e-9);
        assert!((aabb.min.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_translate_mesh() {
        let mesh = make_displaced_cube(0.0, 0.0, 0.0);
        let offset = Vertex::new(5.0, 5.0, 5.0);
        let translated = translate_mesh(&mesh, offset);
        let aabb = calculate_aabb(&translated);
        assert!((aabb.min.x - 5.0).abs() < 1e-9);
        assert!((aabb.min.y - 5.0).abs() < 1e-9);
        assert!((aabb.min.z - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_original_mesh_unchanged_after_translate() {
        let mesh = make_displaced_cube(0.0, 0.0, 0.0);
        let _translated = translate_mesh(&mesh, Vertex::new(100.0, 100.0, 100.0));
        // Original mesh vertices should be untouched
        assert_eq!(mesh.vertices[0].x, 0.0);
        assert_eq!(mesh.vertices[0].y, 0.0);
        assert_eq!(mesh.vertices[0].z, 0.0);
    }
}
