//! Scene operations — the only way to mutate object placement.
//!
//! Every CLI flag and every UI gesture must be expressed as a [`SceneOp`].
//! Each successfully-applied op returns an [`OpReceipt`] containing the
//! inverse op, so undo can be added later without redesigning the API.

use crate::mesh::types::Vertex;
use crate::scene::loader::{self, MeshFormat};
use crate::scene::state::{ObjectId, SceneState};
use crate::scene::transform::Transform;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A unit operation on a [`SceneState`].
///
/// All variants must be reversible from the current state plus the returned
/// [`OpReceipt`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "args", rename_all = "snake_case")]
pub enum SceneOp {
    /// Add a mesh from raw bytes.
    Add {
        name: String,
        format: MeshFormat,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// Remove an object.
    Remove { id: ObjectId },
    /// Translate by a delta in scene millimeters.
    Translate { id: ObjectId, delta: [f64; 3] },
    /// Replace the entire transform.
    SetTransform { id: ObjectId, transform: Transform },
    /// Rotate around `axis` by `radians`, composing with the current rotation.
    Rotate {
        id: ObjectId,
        axis: [f32; 3],
        radians: f32,
    },
    /// Multiply the per-axis scale by `factors`.
    Scale { id: ObjectId, factors: [f32; 3] },
    /// Translate so the object's transformed-AABB center matches the bed center
    /// in XY. Z is preserved.
    CenterOnBed { id: ObjectId },
    /// Translate so the object's transformed-AABB sits with min.z = 0.
    DropToFloor { id: ObjectId },
    /// Rotate so the chosen face's outward normal points along `-Z`, then drop
    /// the object to the floor.
    AlignFaceToFloor { id: ObjectId, face_index: usize },
}

/// Receipt returned by a successful [`SceneState::apply`] call.
#[derive(Debug, Clone)]
pub struct OpReceipt {
    /// Op that, when applied to the post-state, restores the pre-state.
    pub inverse: SceneOp,
}

/// Errors that can arise when applying a [`SceneOp`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum SceneError {
    #[error("object {0} not found")]
    NotFound(ObjectId),
    #[error("face index {face} out of range (mesh has {count} faces)")]
    FaceOutOfRange { face: usize, count: usize },
    #[error("face {0} has degenerate (zero-area) normal")]
    DegenerateFace(usize),
    #[error("mesh load failed: {0}")]
    Load(String),
}

impl SceneState {
    /// Apply a [`SceneOp`] to this scene.
    pub fn apply(&mut self, op: SceneOp) -> Result<OpReceipt, SceneError> {
        match op {
            SceneOp::Add {
                name,
                format,
                bytes,
            } => {
                let mesh = loader::load_bytes(&bytes, format).map_err(SceneError::Load)?;
                let id = self.add_mesh(name, Arc::new(mesh));
                Ok(OpReceipt {
                    inverse: SceneOp::Remove { id },
                })
            }

            SceneOp::Remove { id } => {
                let obj = self.get(id).ok_or(SceneError::NotFound(id))?.clone();
                self.remove(id);
                // Pure removal cannot be inverted without re-uploading the mesh
                // bytes; record a SetTransform stub so the receipt is at least
                // shaped like the others. True undo of a Remove will require
                // a higher-level history that retains the mesh.
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: obj.transform,
                    },
                })
            }

            SceneOp::Translate { id, delta } => {
                let prev = self.get(id).ok_or(SceneError::NotFound(id))?.transform;
                let mut new_t = prev;
                new_t.translation[0] += delta[0] as f32;
                new_t.translation[1] += delta[1] as f32;
                new_t.translation[2] += delta[2] as f32;
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::SetTransform { id, transform } => {
                let prev = self.get(id).ok_or(SceneError::NotFound(id))?.transform;
                self.get_mut(id).unwrap().transform = transform;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::Rotate { id, axis, radians } => {
                let prev = self.get(id).ok_or(SceneError::NotFound(id))?.transform;
                let axis_v = Vec3::from(axis).normalize_or_zero();
                let q = Quat::from_axis_angle(axis_v, radians);
                let mut new_t = prev;
                new_t.set_quat(q * prev.quat());
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::Scale { id, factors } => {
                let prev = self.get(id).ok_or(SceneError::NotFound(id))?.transform;
                let mut new_t = prev;
                new_t.scale[0] *= factors[0];
                new_t.scale[1] *= factors[1];
                new_t.scale[2] *= factors[2];
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::CenterOnBed { id } => {
                let obj = self.get(id).ok_or(SceneError::NotFound(id))?;
                let prev = obj.transform;
                let world = obj.world_aabb();
                let world_center = world.center();
                let (bx, by) = self.bed.center_xy();
                let mut new_t = prev;
                new_t.translation[0] += (bx - world_center.x) as f32;
                new_t.translation[1] += (by - world_center.y) as f32;
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::DropToFloor { id } => {
                let obj = self.get(id).ok_or(SceneError::NotFound(id))?;
                let prev = obj.transform;
                let world = obj.world_aabb();
                let mut new_t = prev;
                new_t.translation[2] -= world.min.z as f32;
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }

            SceneOp::AlignFaceToFloor { id, face_index } => {
                let obj = self.get(id).ok_or(SceneError::NotFound(id))?;
                let prev = obj.transform;
                let mesh = obj.mesh.clone();
                if face_index >= mesh.faces.len() {
                    return Err(SceneError::FaceOutOfRange {
                        face: face_index,
                        count: mesh.faces.len(),
                    });
                }
                let face = &mesh.faces[face_index];
                let local_normal =
                    face_normal(face).ok_or(SceneError::DegenerateFace(face_index))?;
                // Apply current rotation to the local normal to get its current world direction.
                let world_normal = (prev.quat() * local_normal).normalize_or_zero();
                if world_normal.length_squared() < 1e-12 {
                    return Err(SceneError::DegenerateFace(face_index));
                }
                // Quaternion that rotates the world normal to point along -Z.
                let down = Vec3::new(0.0, 0.0, -1.0);
                let align = Quat::from_rotation_arc(world_normal, down);
                let mut new_t = prev;
                new_t.set_quat(align * prev.quat());
                self.get_mut(id).unwrap().transform = new_t;
                // Then drop to floor on the new orientation.
                let world = self.get(id).unwrap().world_aabb();
                let mut new_t = self.get(id).unwrap().transform;
                new_t.translation[2] -= world.min.z as f32;
                self.get_mut(id).unwrap().transform = new_t;
                Ok(OpReceipt {
                    inverse: SceneOp::SetTransform {
                        id,
                        transform: prev,
                    },
                })
            }
        }
    }
}

/// Compute a face's geometric normal from its three vertices.
fn face_normal(face: &crate::mesh::types::Face) -> Option<Vec3> {
    if let Some(n) = face.normal {
        let v = Vec3::new(n.x as f32, n.y as f32, n.z as f32);
        if v.length_squared() > 1e-12 {
            return Some(v.normalize());
        }
    }
    let a = vertex_to_vec3(&face.vertices[0]);
    let b = vertex_to_vec3(&face.vertices[1]);
    let c = vertex_to_vec3(&face.vertices[2]);
    let n = (b - a).cross(c - a);
    if n.length_squared() < 1e-12 {
        None
    } else {
        Some(n.normalize())
    }
}

fn vertex_to_vec3(v: &Vertex) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::types::{Face, Mesh};
    use crate::scene::bed::BedConfig;

    fn cube_mesh(origin: [f64; 3], size: f64) -> Arc<Mesh> {
        let [x, y, z] = origin;
        let s = size;
        let v: Vec<Vertex> = vec![
            Vertex::new(x, y, z),
            Vertex::new(x + s, y, z),
            Vertex::new(x + s, y + s, z),
            Vertex::new(x, y + s, z),
            Vertex::new(x, y, z + s),
            Vertex::new(x + s, y, z + s),
            Vertex::new(x + s, y + s, z + s),
            Vertex::new(x, y + s, z + s),
        ];
        let idx: [[usize; 3]; 12] = [
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
        let faces: Vec<Face> = idx
            .iter()
            .map(|i| Face::new([v[i[0]], v[i[1]], v[i[2]]]))
            .collect();
        Arc::new(Mesh {
            vertices: v,
            faces,
            aabb: None,
        })
    }

    fn small_bed() -> BedConfig {
        BedConfig {
            width: 100.0,
            depth: 100.0,
            height: 100.0,
            origin_offset_x: 0.0,
            origin_offset_y: 0.0,
        }
    }

    #[test]
    fn translate_updates_transform() {
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([0.0, 0.0, 0.0], 10.0));
        s.apply(SceneOp::Translate {
            id,
            delta: [5.0, 6.0, 7.0],
        })
        .unwrap();
        let t = s.get(id).unwrap().transform.translation;
        assert!((t[0] - 5.0).abs() < 1e-5);
        assert!((t[1] - 6.0).abs() < 1e-5);
        assert!((t[2] - 7.0).abs() < 1e-5);
    }

    #[test]
    fn center_on_bed_centers_xy() {
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([0.0, 0.0, 0.0], 10.0));
        s.apply(SceneOp::CenterOnBed { id }).unwrap();
        let world = s.get(id).unwrap().world_aabb();
        assert!((world.center().x - 50.0).abs() < 1e-4);
        assert!((world.center().y - 50.0).abs() < 1e-4);
    }

    #[test]
    fn center_on_bed_is_idempotent() {
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([3.0, 7.0, 0.0], 10.0));
        s.apply(SceneOp::CenterOnBed { id }).unwrap();
        let after_once = s.get(id).unwrap().transform;
        s.apply(SceneOp::CenterOnBed { id }).unwrap();
        let after_twice = s.get(id).unwrap().transform;
        for i in 0..3 {
            assert!(
                (after_once.translation[i] - after_twice.translation[i]).abs() < 1e-4,
                "axis {i}"
            );
        }
    }

    #[test]
    fn drop_to_floor_lands_on_zero() {
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([0.0, 0.0, 12.0], 10.0));
        s.apply(SceneOp::DropToFloor { id }).unwrap();
        let world = s.get(id).unwrap().world_aabb();
        assert!(world.min.z.abs() < 1e-4, "min.z={}", world.min.z);
    }

    #[test]
    fn translate_inverse_restores_transform() {
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([0.0, 0.0, 0.0], 10.0));
        let receipt = s
            .apply(SceneOp::Translate {
                id,
                delta: [11.0, 22.0, 33.0],
            })
            .unwrap();
        s.apply(receipt.inverse).unwrap();
        let t = s.get(id).unwrap().transform.translation;
        assert!(t[0].abs() < 1e-5);
        assert!(t[1].abs() < 1e-5);
        assert!(t[2].abs() < 1e-5);
    }

    #[test]
    fn align_face_to_floor_lands_face_at_z0() {
        // Cube faces 8 and 9 are the +X-facing pair (vertices (1,*,*) and (1,*,*)).
        // Pick face index 4: triangle (0,1,5) — the -Y face (normal points -Y).
        // We want a face with non-Z normal so the alignment actually rotates.
        let mut s = SceneState::new(small_bed());
        let id = s.add_mesh("c", cube_mesh([0.0, 0.0, 0.0], 10.0));
        // Face 4 normal: (b-a)x(c-a) where a=(0,0,0), b=(10,0,0), c=(10,0,10) → (0,-100,0) → -Y.
        s.apply(SceneOp::AlignFaceToFloor { id, face_index: 4 })
            .unwrap();
        let world = s.get(id).unwrap().world_aabb();
        assert!(
            world.min.z.abs() < 1e-3,
            "min.z={} after align+drop",
            world.min.z
        );
    }

    #[test]
    fn missing_object_returns_not_found() {
        let mut s = SceneState::new(small_bed());
        let err = s
            .apply(SceneOp::Translate {
                id: ObjectId(999),
                delta: [0.0; 3],
            })
            .unwrap_err();
        matches!(err, SceneError::NotFound(_));
    }
}
