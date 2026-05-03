//! Auto-orient: find the rotation that minimises overhangs and maximises
//! flat bed-contact area for a given mesh.
//!
//! ## Algorithm
//!
//! 1. **Candidate generation** — collect unique "floor direction" candidates:
//!    - One area-weighted representative normal per coplanar face group
//!      (covers all flat regions with O(faces) work).
//!    - If [`AutoOrientOptions::allow_rotations`] is `true`, additionally
//!      sample ~128 directions on a Fibonacci sphere (covers organic shapes
//!      with no prominent flat regions).
//!
//! 2. **Scoring** — for each candidate direction `d` (the direction that will
//!    be rotated to face down / align with `−Z`):
//!    - Compute `q = from_rotation_arc(d, −Z)`.
//!    - Apply `q` to every face normal.
//!    - Score = `OVERHANG_W × overhang_area − CONTACT_W × contact_area + HEIGHT_W × height`
//!      (lower is better).
//!
//! 3. **Result** — return the quaternion for the best candidate, optionally
//!    composed with a preferred Z-rotation (e.g. 45° for CoreXY printers).

use crate::mesh::analysis::{calculate_aabb, compute_coplanar_groups};
use crate::mesh::types::{Face, Mesh, Vertex};
use crate::scene::transform::{transformed_aabb, Transform};
use glam::{Quat, Vec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Scoring weights — intentionally not exposed; tune here if needed.
// ---------------------------------------------------------------------------

/// Weight applied to overhang area (dominant term — matches Cura's approach).
const OVERHANG_W: f64 = 1.0;
/// Reward for large flat contact area with the bed (OrcaSlicer heuristic).
const CONTACT_W: f64 = 0.5;
/// Small height penalty so identical-overhang candidates prefer shorter prints.
const HEIGHT_W: f64 = 0.01;
/// Half-angle (degrees) for "essentially flat on bed" contact detection.
const CONTACT_ANGLE_DEG: f64 = 10.0;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options controlling the auto-orient algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AutoOrientOptions {
    /// When `true`, additionally sample a Fibonacci-sphere grid (~128
    /// candidates) in addition to the flat-face candidates.  Recommended for
    /// organic shapes with no large flat faces, e.g. figurines.  When
    /// `false` (default), only unique flat-face-normal directions are tested —
    /// fast and correct for box-like objects.
    pub allow_rotations: bool,

    /// After finding the best face-down orientation, additionally rotate the
    /// object around Z by this many degrees.  Set to `45.0` for CoreXY
    /// printers to align the seam line with the stepper axes.  `0.0` =
    /// disabled (default).
    pub preferred_z_rotation_deg: f64,

    /// Faces whose outward normal points more than this many degrees below
    /// horizontal are counted as overhanging (and penalised).  Should match
    /// the printer's support angle threshold.  **Default: 45°.**
    pub overhang_threshold_deg: f64,
}

impl Default for AutoOrientOptions {
    fn default() -> Self {
        Self {
            allow_rotations: false,
            preferred_z_rotation_deg: 0.0,
            overhang_threshold_deg: 45.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Compute the rotation quaternion that best orients `mesh` for FDM printing.
///
/// The returned quaternion, when applied to the mesh, minimises unsupported
/// overhangs, maximises flat bed-contact area, and — as a tiebreaker — prefers
/// shorter print heights.
///
/// The caller is responsible for:
/// - Applying the quaternion (e.g. via [`crate::scene::ops::SceneOp::AutoOrient`]).
/// - Dropping the oriented mesh to the floor (`DropToFloor`).
pub fn auto_orient(mesh: &Mesh, options: &AutoOrientOptions) -> Quat {
    if mesh.faces.is_empty() {
        return Quat::IDENTITY;
    }

    // Pre-compute per-face normals (unit vectors) and areas.
    let normals: Vec<Vec3> = mesh
        .faces
        .iter()
        .map(|f| face_normal_vec3(f).unwrap_or(Vec3::Z))
        .collect();

    let areas: Vec<f64> = mesh.faces.iter().map(|f| f.area()).collect();
    let total_area: f64 = areas.iter().sum();
    if total_area < 1e-10 {
        return Quat::IDENTITY;
    }

    // Build candidate floor-normal directions.
    let candidates = build_candidates(mesh, options);
    if candidates.is_empty() {
        return Quat::IDENTITY;
    }

    // Convert threshold angles to float comparands used in the inner loop.
    // A face is "overhang" if its rotated normal Z-component < -sin(threshold):
    //   angle_below_horizontal > threshold
    //   ↔ arcsin(|rotated_z|) > threshold   (for rotated_z < 0)
    //   ↔ |rotated_z| > sin(threshold)
    //   ↔ rotated_z < -sin(threshold)
    let overhang_sin = options.overhang_threshold_deg.to_radians().sin() as f32;
    // A face is "contact" if its rotated normal is within CONTACT_ANGLE_DEG of −Z.
    // cos(angle_from_neg_z) = -rotated_z → contact when -rotated_z > cos(contact_angle)
    // ↔ rotated_z < -cos(contact_angle)
    let contact_cos = CONTACT_ANGLE_DEG.to_radians().cos() as f32;

    // AABB used for height scoring.
    let local_aabb = calculate_aabb(mesh);

    let mut best_score = f64::MAX;
    let mut best_quat = Quat::IDENTITY;

    for candidate in &candidates {
        let q = Quat::from_rotation_arc(*candidate, Vec3::NEG_Z);

        let mut overhang_area = 0.0_f64;
        let mut contact_area = 0.0_f64;

        for (i, n) in normals.iter().enumerate() {
            let rz = (q * *n).z;
            if rz < -overhang_sin {
                overhang_area += areas[i];
            }
            if rz < -contact_cos {
                contact_area += areas[i];
            }
        }

        // Height of the AABB after applying this rotation.
        let height = {
            let t = Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [q.x, q.y, q.z, q.w],
                scale: [1.0, 1.0, 1.0],
            };
            let w = transformed_aabb(&local_aabb, &t);
            w.max.z - w.min.z
        };

        // Bed-contact faces (rotated_z ≈ -1) are supported and must NOT be
        // counted as overhangs. The net unsupported overhang area is the
        // downward-facing area minus the bed-contact area.
        let net_overhang = overhang_area - contact_area;
        let score =
            OVERHANG_W * net_overhang - CONTACT_W * contact_area + HEIGHT_W * height;

        if score < best_score {
            best_score = score;
            best_quat = q;
        }
    }

    // Optionally compose with a Z-rotation preference (CoreXY 45°, etc.).
    if options.preferred_z_rotation_deg.abs() > 1e-6 {
        let z_rot = Quat::from_axis_angle(
            Vec3::Z,
            (options.preferred_z_rotation_deg as f32).to_radians(),
        );
        best_quat = z_rot * best_quat;
    }

    best_quat
}

// ---------------------------------------------------------------------------
// Candidate generation
// ---------------------------------------------------------------------------

/// Collect the "floor direction" candidates to evaluate.
///
/// Always returns one representative direction per coplanar face group (normal
/// direction of that group, area-weighted average).  If
/// [`AutoOrientOptions::allow_rotations`] is `true`, also returns ~128 points
/// uniformly distributed on the sphere (Fibonacci spiral).
fn build_candidates(mesh: &Mesh, options: &AutoOrientOptions) -> Vec<Vec3> {
    let mut candidates: Vec<Vec3> = Vec::new();

    // --- 1. One representative per coplanar face group -----------------------
    let groups = compute_coplanar_groups(mesh, 1.0, 0.001);
    // Accumulate area-weighted normal sums per group.
    let mut group_accum: HashMap<u32, (Vec3, f32)> = HashMap::new();
    for (face_idx, face) in mesh.faces.iter().enumerate() {
        if let Some(n) = face_normal_vec3(face) {
            let area = face.area() as f32;
            let entry = group_accum
                .entry(groups[face_idx])
                .or_insert((Vec3::ZERO, 0.0));
            entry.0 += n * area;
            entry.1 += area;
        }
    }
    for (n_sum, area_sum) in group_accum.values() {
        if *area_sum > 1e-10 {
            let n = (*n_sum / *area_sum).normalize_or_zero();
            if n.length_squared() > 0.25 {
                candidates.push(n);
            }
        }
    }

    // --- 2. Fibonacci sphere (allow_rotations) --------------------------------
    if options.allow_rotations {
        const N: usize = 128;
        let golden = (1.0 + 5.0_f64.sqrt()) / 2.0;
        for i in 0..N {
            let cos_theta = 1.0 - 2.0 * (i as f64 + 0.5) / N as f64;
            let theta = cos_theta.acos() as f32;
            let phi = (2.0 * PI * i as f64 / golden) as f32;
            candidates.push(Vec3::new(
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            ));
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Compute the geometric unit normal of a triangular face.
/// Returns `None` for degenerate (zero-area) triangles.
fn face_normal_vec3(face: &Face) -> Option<Vec3> {
    let a = vertex_to_vec3(&face.vertices[0]);
    let b = vertex_to_vec3(&face.vertices[1]);
    let c = vertex_to_vec3(&face.vertices[2]);
    let n = (b - a).cross(c - a);
    let len_sq = n.length_squared();
    if len_sq < 1e-12 {
        None
    } else {
        Some(n / len_sq.sqrt())
    }
}

#[inline]
fn vertex_to_vec3(v: &Vertex) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::types::{Face, Mesh, Vertex};

    /// 10 × 10 × 10 mm axis-aligned cube with outward-facing normals.
    fn cube_mesh() -> Mesh {
        let v = [
            Vertex::new(0.0, 0.0, 0.0),   // 0 bottom corners
            Vertex::new(10.0, 0.0, 0.0),  // 1
            Vertex::new(10.0, 10.0, 0.0), // 2
            Vertex::new(0.0, 10.0, 0.0),  // 3
            Vertex::new(0.0, 0.0, 10.0),  // 4 top corners
            Vertex::new(10.0, 0.0, 10.0), // 5
            Vertex::new(10.0, 10.0, 10.0),// 6
            Vertex::new(0.0, 10.0, 10.0), // 7
        ];
        let idx: [[usize; 3]; 12] = [
            [0, 2, 1], [0, 3, 2], // bottom −Z
            [4, 5, 6], [4, 6, 7], // top +Z
            [0, 1, 5], [0, 5, 4], // front −Y
            [2, 3, 7], [2, 7, 6], // back +Y
            [0, 4, 7], [0, 7, 3], // left −X
            [1, 2, 6], [1, 6, 5], // right +X
        ];
        Mesh {
            vertices: v.to_vec(),
            faces: idx
                .iter()
                .map(|i| Face::new([v[i[0]], v[i[1]], v[i[2]]]))
                .collect(),
            aabb: None,
        }
    }

    /// A tall thin box: 5 × 5 × 50 mm standing upright.
    fn tall_box_mesh() -> Mesh {
        let v = [
            Vertex::new(0.0, 0.0, 0.0),  // 0
            Vertex::new(5.0, 0.0, 0.0),  // 1
            Vertex::new(5.0, 5.0, 0.0),  // 2
            Vertex::new(0.0, 5.0, 0.0),  // 3
            Vertex::new(0.0, 0.0, 50.0), // 4
            Vertex::new(5.0, 0.0, 50.0), // 5
            Vertex::new(5.0, 5.0, 50.0), // 6
            Vertex::new(0.0, 5.0, 50.0), // 7
        ];
        let idx: [[usize; 3]; 12] = [
            [0, 2, 1], [0, 3, 2], // bottom
            [4, 5, 6], [4, 6, 7], // top
            [0, 1, 5], [0, 5, 4], // front
            [2, 3, 7], [2, 7, 6], // back
            [0, 4, 7], [0, 7, 3], // left
            [1, 2, 6], [1, 6, 5], // right
        ];
        Mesh {
            vertices: v.to_vec(),
            faces: idx
                .iter()
                .map(|i| Face::new([v[i[0]], v[i[1]], v[i[2]]]))
                .collect(),
            aabb: None,
        }
    }

    /// A triangular wedge prism: cross-section is a right triangle.
    /// Vertices: two triangular end caps and three rectangular faces.
    /// The slanted face has a 45° angle relative to horizontal.
    fn wedge_mesh() -> Mesh {
        // Right-triangle cross-section in XZ plane, extruded along Y.
        // Base: (0,0,0)→(10,0,0), Height: 10 at x=0.
        // Slanted face: normal = normalise([10,0,10]) = [1/√2, 0, 1/√2]
        let v = [
            Vertex::new(0.0, 0.0, 0.0),  // 0
            Vertex::new(10.0, 0.0, 0.0), // 1
            Vertex::new(0.0, 0.0, 10.0), // 2  ← apex (x=0, z=10)
            Vertex::new(0.0, 5.0, 0.0),  // 3
            Vertex::new(10.0, 5.0, 0.0), // 4
            Vertex::new(0.0, 5.0, 10.0), // 5
        ];
        // Front cap (y=0): v0,v2,v1 → outward normal −Y
        // Back cap  (y=5): v3,v4,v5 → outward normal +Y
        // Bottom face (z=0): v0,v1,v4,v3 → normal −Z (two triangles)
        // Left face (x=0): v0,v3,v5,v2 → normal −X (two triangles)
        // Slanted face: v1,v2,v5,v4 → normal [1,0,1]/√2 (two triangles)
        let faces: Vec<Face> = vec![
            // front cap
            Face::new([v[0], v[2], v[1]]),
            // back cap
            Face::new([v[3], v[4], v[5]]),
            // bottom (z=0, outward = -Z)
            Face::new([v[0], v[1], v[4]]),
            Face::new([v[0], v[4], v[3]]),
            // left (x=0, outward = -X)
            Face::new([v[0], v[3], v[5]]),
            Face::new([v[0], v[5], v[2]]),
            // slanted face (outward ≈ [1,0,1]/√2, i.e. pointing up-right)
            Face::new([v[1], v[2], v[5]]),
            Face::new([v[1], v[5], v[4]]),
        ];
        Mesh {
            vertices: v.to_vec(),
            faces,
            aabb: None,
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Height of the bounding box after applying `q` to `mesh`.
    fn oriented_height(mesh: &Mesh, q: Quat) -> f64 {
        let t = Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [q.x, q.y, q.z, q.w],
            scale: [1.0, 1.0, 1.0],
        };
        let aabb = crate::mesh::analysis::calculate_aabb(mesh);
        let world = transformed_aabb(&aabb, &t);
        world.max.z - world.min.z
    }

    /// Net unsupported overhang area after rotation `q`:
    /// faces pointing >threshold below horizontal, minus faces essentially flat
    /// on the bed (within `CONTACT_ANGLE_DEG` of -Z). The bed-contact face
    /// is always supported, so it must not be counted as an overhang.
    fn net_overhang_area(mesh: &Mesh, q: Quat, threshold_deg: f64) -> f64 {
        let sin_t = threshold_deg.to_radians().sin() as f32;
        let contact_cos = CONTACT_ANGLE_DEG.to_radians().cos() as f32;
        let mut overhang = 0.0f64;
        let mut contact = 0.0f64;
        for f in &mesh.faces {
            let n = face_normal_vec3(f).unwrap_or(Vec3::Z);
            let rz = (q * n).z;
            let area = f.area();
            if rz < -sin_t {
                overhang += area;
            }
            if rz < -contact_cos {
                contact += area;
            }
        }
        overhang - contact
    }

    // -----------------------------------------------------------------------
    // Test: cube
    // -----------------------------------------------------------------------
    #[test]
    fn cube_already_flat() {
        let mesh = cube_mesh();
        let opts = AutoOrientOptions::default();
        let q = auto_orient(&mesh, &opts);
        // The cube is already flat. After applying q, the height should be ~10
        // (the cube's side length) — any axis-aligned face can be chosen.
        let h = oriented_height(&mesh, q);
        assert!(
            (h - 10.0).abs() < 0.5,
            "cube height after orient should be ~10, got {h}"
        );
        // Net unsupported overhangs should be zero: the bottom face is
        // supported by the bed (it's the bed-contact face), so it cancels out.
        let oa = net_overhang_area(&mesh, q, 45.0);
        assert!(
            oa < 1e-6,
            "cube should have zero net overhang after orient, got {oa}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: tall thin box with allow_rotations=true → should lay flat
    // -----------------------------------------------------------------------
    #[test]
    fn tall_box_lays_flat_with_rotations() {
        let mesh = tall_box_mesh();
        let opts = AutoOrientOptions {
            allow_rotations: true,
            ..Default::default()
        };
        let q = auto_orient(&mesh, &opts);
        let h = oriented_height(&mesh, q);
        // Laying flat: height ≈ 5 (the short side). Upright: height = 50.
        // Accept anything ≤ 10 as "laid flat".
        assert!(
            h <= 10.0 + 0.5,
            "tall box should lay flat (height ≤ 10), got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: wedge — auto-orient without allow_rotations should choose the
    // orientation with zero net overhangs (slanted face down wins because it
    // has the largest flat contact area among zero-net-overhang candidates).
    // -----------------------------------------------------------------------
    #[test]
    fn wedge_no_overhang() {
        let mesh = wedge_mesh();
        let opts = AutoOrientOptions::default();
        let q = auto_orient(&mesh, &opts);
        let oa = net_overhang_area(&mesh, q, 45.0);
        assert!(
            oa < 1e-6,
            "wedge should have zero net overhang after orient (threshold=45°), got {oa}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: preferred_z_rotation_deg is composed into the result
    // -----------------------------------------------------------------------
    #[test]
    fn preferred_z_rotation_applied() {
        let mesh = cube_mesh();
        let opts_no_rot = AutoOrientOptions::default();
        let opts_z45 = AutoOrientOptions {
            preferred_z_rotation_deg: 45.0,
            ..Default::default()
        };
        let q_no = auto_orient(&mesh, &opts_no_rot);
        let q_z45 = auto_orient(&mesh, &opts_z45);
        // The two results should differ (the Z rotation changes the quaternion).
        let dot = (q_no.x * q_z45.x + q_no.y * q_z45.y + q_no.z * q_z45.z + q_no.w * q_z45.w)
            .abs();
        assert!(
            dot < 0.999,
            "preferred_z_rotation should change the quaternion (dot={dot})"
        );
    }
}
