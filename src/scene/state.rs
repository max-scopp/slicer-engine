//! Scene state: objects, transforms, and bed.

use crate::mesh::analysis::calculate_aabb;
use crate::mesh::types::{Mesh, AABB};
use crate::scene::bed::BedConfig;
use crate::scene::transform::{transformed_aabb, Transform};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Monotonically-allocated identifier for a scene object.
///
/// Stable for the lifetime of a [`SceneState`]; not reused after removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "obj#{}", self.0)
    }
}

/// One transformable mesh placed in the scene.
#[derive(Debug, Clone)]
pub struct SceneObject {
    /// Stable identifier within the owning [`SceneState`].
    pub id: ObjectId,
    /// Display name (typically the source file name).
    pub name: String,
    /// Underlying triangle mesh — shared via `Arc` so transforms are cheap.
    pub mesh: Arc<Mesh>,
    /// Affine transform applied at slice time.
    pub transform: Transform,
}

impl SceneObject {
    /// AABB of the object's mesh in its **local** (untransformed) frame.
    pub fn local_aabb(&self) -> AABB {
        calculate_aabb(self.mesh.as_ref())
    }

    /// AABB of the object after applying its current transform.
    pub fn world_aabb(&self) -> AABB {
        transformed_aabb(&self.local_aabb(), &self.transform)
    }
}

/// Top-level scene state owned by the CLI / WS server / WASM handle.
#[derive(Debug, Clone)]
pub struct SceneState {
    /// Objects in insertion order.
    pub objects: Vec<SceneObject>,
    /// Print bed configuration.
    pub bed: BedConfig,
    next_id: u64,
}

impl SceneState {
    /// Create an empty scene with the given bed configuration.
    pub fn new(bed: BedConfig) -> Self {
        Self {
            objects: Vec::new(),
            bed,
            next_id: 1,
        }
    }

    /// Add a mesh to the scene. Returns the assigned [`ObjectId`].
    pub fn add_mesh(&mut self, name: impl Into<String>, mesh: Arc<Mesh>) -> ObjectId {
        let id = ObjectId(self.next_id);
        self.next_id += 1;
        self.objects.push(SceneObject {
            id,
            name: name.into(),
            mesh,
            transform: Transform::IDENTITY,
        });
        id
    }

    /// Remove an object by id. Returns `true` if removed.
    pub fn remove(&mut self, id: ObjectId) -> bool {
        let len = self.objects.len();
        self.objects.retain(|o| o.id != id);
        self.objects.len() != len
    }

    /// Get a reference to an object by id.
    pub fn get(&self, id: ObjectId) -> Option<&SceneObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Get a mutable reference to an object by id.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut SceneObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::types::Vertex;

    fn unit_cube_mesh() -> Arc<Mesh> {
        let mut m = Mesh::new();
        m.vertices = vec![Vertex::new(0.0, 0.0, 0.0), Vertex::new(1.0, 1.0, 1.0)];
        Arc::new(m)
    }

    #[test]
    fn add_and_remove() {
        let mut s = SceneState::new(BedConfig::default());
        let id = s.add_mesh("cube", unit_cube_mesh());
        assert_eq!(s.objects.len(), 1);
        assert!(s.get(id).is_some());
        assert!(s.remove(id));
        assert_eq!(s.objects.len(), 0);
    }

    #[test]
    fn ids_are_monotonic_and_not_reused() {
        let mut s = SceneState::new(BedConfig::default());
        let a = s.add_mesh("a", unit_cube_mesh());
        let b = s.add_mesh("b", unit_cube_mesh());
        s.remove(a);
        let c = s.add_mesh("c", unit_cube_mesh());
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert!(c.0 > b.0);
    }
}
