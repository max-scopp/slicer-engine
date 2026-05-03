use crate::mesh::types::{Face, Vertex};
use glam::Vec3;

/// Threshold for treating a degenerate (near-zero) triangle normal.
/// Below this squared-length value, the cross-product result is too small
/// to reliably normalise.
pub(super) const DEGENERATE_NORMAL_SQ: f32 = 1e-12;

/// Compute the geometric unit normal of a triangular face.
/// Returns `None` for degenerate (zero-area) triangles.
pub(super) fn face_normal_vec3(face: &Face) -> Option<Vec3> {
    let a = vertex_to_vec3(&face.vertices[0]);
    let b = vertex_to_vec3(&face.vertices[1]);
    let c = vertex_to_vec3(&face.vertices[2]);
    let n = (b - a).cross(c - a);
    let len_sq = n.length_squared();
    if len_sq < DEGENERATE_NORMAL_SQ {
        None
    } else {
        Some(n / len_sq.sqrt())
    }
}

#[inline]
pub(super) fn vertex_to_vec3(v: &Vertex) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}
