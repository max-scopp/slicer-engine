//! Core slicing operations and data structures

use clipper2::*;

use crate::mesh::types::{Mesh, Vertex};

/// The role of an extrusion path, used to annotate G-code with `;TYPE:` comments
/// and enable firmware features like Klipper adaptive acceleration by role.
///
/// Each variant maps to a named type that is emitted in the G-code output and
/// carries a default extrusion width for that role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtrusionRole {
    /// Outer or inner perimeter / wall contour (default role).
    #[default]
    Perimeter,
    /// Sparse infill pattern.
    Infill,
    /// Bridge extrusion spanning a gap with no support below.
    Bridge,
    /// Solid top-surface infill.
    TopSurface,
    /// Support structure material.
    Support,
    /// Skirt or brim line.
    Skirt,
}

impl ExtrusionRole {
    /// The `;TYPE:` label emitted in G-code comments for this role.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Perimeter => "Perimeter",
            Self::Infill => "Infill",
            Self::Bridge => "Bridge",
            Self::TopSurface => "Top surface",
            Self::Support => "Support",
            Self::Skirt => "Skirt",
        }
    }

    /// Default extrusion width in mm for this role.
    ///
    /// Used to populate the `;WIDTH:` annotation in the G-code output.
    pub fn default_width_mm(self) -> f64 {
        match self {
            Self::Perimeter | Self::Infill | Self::Bridge | Self::TopSurface => 0.4,
            Self::Support => 0.4,
            Self::Skirt => 0.4,
        }
    }
}

/// Represents a slice layer in the 3D model
#[derive(Debug, Clone)]
pub struct SliceLayer {
    /// Z-coordinate of this layer
    pub z: f64,
    /// Paths that make up this layer (closed contours in XY)
    pub paths: Paths,
    /// Extrusion role for each path in [`SliceLayer::paths`].
    ///
    /// `path_roles[i]` is the role of `paths[i]`.  If shorter than `paths`,
    /// the remaining paths default to [`ExtrusionRole::Perimeter`].
    pub path_roles: Vec<ExtrusionRole>,
}

impl SliceLayer {
    /// Create a new slice layer at the given Z coordinate
    pub fn new(z: f64) -> Self {
        Self {
            z,
            paths: Paths::default(),
            path_roles: Vec::new(),
        }
    }

    /// Return the extrusion role for path index `i`.
    ///
    /// Falls back to [`ExtrusionRole::Perimeter`] when `path_roles` has no
    /// entry for the given index.
    pub fn role_for_path(&self, i: usize) -> ExtrusionRole {
        self.path_roles.get(i).copied().unwrap_or_default()
    }
}

/// Interpolate the XY intersection point of a triangle edge with a Z plane.
///
/// Given two vertices `a` and `b` that straddle the plane `z`, returns the XY
/// point where the edge crosses that plane.
fn edge_intersect(a: Vertex, b: Vertex, z: f64) -> (f64, f64) {
    let t = (z - a.z) / (b.z - a.z);
    (a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
}

/// Slice a mesh into layers separated by `layer_height` millimeters.
///
/// For each layer plane the function intersects every triangle with the plane
/// and chains the resulting line segments into closed contour paths.  The
/// contours are stored in a [`SliceLayer`] using Clipper2's [`Paths`] type so
/// they can be used directly with Boolean or offset operations.
///
/// # Arguments
/// * `mesh`         – triangle mesh in millimetres
/// * `layer_height` – distance between layer planes in mm (must be > 0)
///
/// # Returns
/// A `Vec<SliceLayer>` ordered from bottom to top.  Empty if the mesh has no
/// faces or `layer_height` is not positive.
///
/// # Example
/// ```
/// use slicer_engine::mesh::types::{Face, Mesh, Vertex};
/// use slicer_engine::core::slice_mesh;
///
/// let v = [
///     Vertex::new(0.0, 0.0, 0.0),
///     Vertex::new(10.0, 0.0, 0.0),
///     Vertex::new(0.0, 10.0, 0.0),
///     Vertex::new(0.0, 0.0, 10.0),
/// ];
/// let mesh = Mesh {
///     vertices: v.to_vec(),
///     faces: vec![Face::new([v[0], v[1], v[3]]), Face::new([v[0], v[2], v[3]])],
///     aabb: None,
/// };
/// let layers = slice_mesh(&mesh, 2.0);
/// assert!(!layers.is_empty());
/// ```
pub fn slice_mesh(mesh: &Mesh, layer_height: f64) -> Vec<SliceLayer> {
    if mesh.faces.is_empty() || layer_height <= 0.0 {
        return Vec::new();
    }

    // Determine Z extent from vertices
    let z_min = mesh
        .vertices
        .iter()
        .map(|v| v.z)
        .fold(f64::INFINITY, f64::min);
    let z_max = mesh
        .vertices
        .iter()
        .map(|v| v.z)
        .fold(f64::NEG_INFINITY, f64::max);

    if z_min >= z_max {
        return Vec::new();
    }

    // Layer planes start half a layer above the mesh bottom
    let first_z = z_min + layer_height * 0.5;
    let layer_count = ((z_max - first_z) / layer_height).ceil() as usize + 1;

    let mut layers = Vec::with_capacity(layer_count);

    let mut z = first_z;
    while z < z_max {
        let segments = collect_segments(mesh, z);
        let contours = chain_segments(segments);

        let mut layer = SliceLayer::new(z);
        for contour in contours {
            if contour.len() >= 3 {
                let path: Path = contour.into();
                layer.paths.push(path);
                layer.path_roles.push(ExtrusionRole::Perimeter);
            }
        }

        layers.push(layer);
        z += layer_height;
    }

    layers
}

/// Collect all XY line segments produced by intersecting `mesh` with the
/// horizontal plane at height `z`.
fn collect_segments(mesh: &Mesh, z: f64) -> Vec<[(f64, f64); 2]> {
    let mut segments = Vec::new();

    for face in &mesh.faces {
        let [v0, v1, v2] = face.vertices;

        // Signed heights relative to the slice plane
        let h0 = v0.z - z;
        let h1 = v1.z - z;
        let h2 = v2.z - z;

        // Classify each vertex: true = above or on plane (≥ 0), false = below
        let a0 = h0 >= 0.0;
        let a1 = h1 >= 0.0;
        let a2 = h2 >= 0.0;

        // Skip triangles entirely on one side of the plane
        if a0 == a1 && a1 == a2 {
            continue;
        }

        // Find the two edges that cross the plane
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(2);

        if a0 != a1 {
            pts.push(edge_intersect(v0, v1, z));
        }
        if a1 != a2 {
            pts.push(edge_intersect(v1, v2, z));
        }
        if a2 != a0 {
            pts.push(edge_intersect(v2, v0, z));
        }

        if pts.len() == 2 {
            segments.push([pts[0], pts[1]]);
        }
    }

    segments
}

/// Chain unordered line segments into closed contour polygons.
///
/// Each segment endpoint is rounded to 4 decimal places (≈ 0.1 mm / 100 µm)
/// before being used as a map key, which handles the small floating-point
/// differences that can arise between adjacent triangles.
fn chain_segments(segments: Vec<[(f64, f64); 2]>) -> Vec<Vec<(f64, f64)>> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Represent coordinates as (i64, i64) keyed at 10 000× precision (0.1 mm)
    const SCALE: f64 = 10_000.0;
    let key = |p: (f64, f64)| -> (i64, i64) {
        ((p.0 * SCALE).round() as i64, (p.1 * SCALE).round() as i64)
    };

    // Build adjacency: endpoint → segment index
    let mut endpoint_map: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, seg) in segments.iter().enumerate() {
        endpoint_map.entry(key(seg[0])).or_default().push(i);
        endpoint_map.entry(key(seg[1])).or_default().push(i);
    }

    let mut used = vec![false; segments.len()];
    let mut contours: Vec<Vec<(f64, f64)>> = Vec::new();

    for start in 0..segments.len() {
        if used[start] {
            continue;
        }

        let mut chain: Vec<(f64, f64)> = Vec::new();
        let mut current_seg = start;
        let mut last_point = segments[start][0];

        loop {
            if used[current_seg] {
                break;
            }
            used[current_seg] = true;

            let [p0, p1] = segments[current_seg];
            // Determine which endpoint continues from `last_point`.
            // If neither matches (degenerate topology), use p1 as a fallback.
            let next_point = if key(p0) == key(last_point) {
                p1
            } else if key(p1) == key(last_point) {
                p0
            } else {
                // Disconnected segment: start a new sub-chain from p0
                p1
            };
            chain.push(last_point);
            last_point = next_point;

            // Follow the chain: find an adjacent unused segment
            let candidates = endpoint_map
                .get(&key(last_point))
                .cloned()
                .unwrap_or_default();
            let mut found = false;
            for &cand in &candidates {
                if !used[cand] {
                    current_seg = cand;
                    found = true;
                    break;
                }
            }
            if !found {
                // Close or terminate the chain
                chain.push(last_point);
                break;
            }
        }

        if chain.len() >= 3 {
            contours.push(chain);
        }
    }

    contours
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::types::{Face, Mesh, Vertex};

    /// Build a simple 10×10×10 mm axis-aligned box mesh (12 triangles).
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

        let faces = face_indices
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
    fn test_slice_layer_creation() {
        let layer = SliceLayer::new(1.0);
        assert_eq!(layer.z, 1.0);
        assert!(layer.paths.is_empty());
        assert!(layer.path_roles.is_empty());
    }

    #[test]
    fn test_slice_layer_role_for_path_default() {
        let layer = SliceLayer::new(1.0);
        // No roles set → should fall back to Perimeter
        assert_eq!(layer.role_for_path(0), ExtrusionRole::Perimeter);
        assert_eq!(layer.role_for_path(99), ExtrusionRole::Perimeter);
    }

    #[test]
    fn test_slice_layer_role_for_path_explicit() {
        let mut layer = SliceLayer::new(1.0);
        layer.path_roles.push(ExtrusionRole::Skirt);
        layer.path_roles.push(ExtrusionRole::Infill);
        assert_eq!(layer.role_for_path(0), ExtrusionRole::Skirt);
        assert_eq!(layer.role_for_path(1), ExtrusionRole::Infill);
        // Out of bounds → Perimeter default
        assert_eq!(layer.role_for_path(2), ExtrusionRole::Perimeter);
    }

    #[test]
    fn test_extrusion_role_type_names() {
        assert_eq!(ExtrusionRole::Perimeter.type_name(), "Perimeter");
        assert_eq!(ExtrusionRole::Infill.type_name(), "Infill");
        assert_eq!(ExtrusionRole::Bridge.type_name(), "Bridge");
        assert_eq!(ExtrusionRole::TopSurface.type_name(), "Top surface");
        assert_eq!(ExtrusionRole::Support.type_name(), "Support");
        assert_eq!(ExtrusionRole::Skirt.type_name(), "Skirt");
    }

    #[test]
    fn test_extrusion_role_widths_positive() {
        for role in [
            ExtrusionRole::Perimeter,
            ExtrusionRole::Infill,
            ExtrusionRole::Bridge,
            ExtrusionRole::TopSurface,
            ExtrusionRole::Support,
            ExtrusionRole::Skirt,
        ] {
            assert!(
                role.default_width_mm() > 0.0,
                "{:?} width must be positive",
                role
            );
        }
    }

    #[test]
    fn test_slice_mesh_path_roles_match_paths() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        for layer in &layers {
            assert_eq!(
                layer.paths.len(),
                layer.path_roles.len(),
                "path_roles length must match paths length at z={}",
                layer.z
            );
            for role in &layer.path_roles {
                assert_eq!(
                    *role,
                    ExtrusionRole::Perimeter,
                    "slice_mesh assigns Perimeter"
                );
            }
        }
    }

    #[test]
    fn test_slice_mesh_empty_mesh() {
        let mesh = Mesh::new();
        let layers = slice_mesh(&mesh, 0.2);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_zero_layer_height() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 0.0);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_negative_layer_height() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, -0.2);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_cube_layer_count() {
        let mesh = make_cube_mesh();
        // 10 mm cube sliced at 2 mm → 5 layers at z=1,3,5,7,9
        let layers = slice_mesh(&mesh, 2.0);
        assert_eq!(layers.len(), 5, "Expected 5 layers, got {}", layers.len());
    }

    #[test]
    fn test_slice_mesh_cube_z_values() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        let zs: Vec<f64> = layers.iter().map(|l| l.z).collect();
        // First layer at z_min + layer_height/2 = 0 + 1 = 1.0
        assert!((zs[0] - 1.0).abs() < 1e-10, "First layer z={}", zs[0]);
        assert!((zs[1] - 3.0).abs() < 1e-10, "Second layer z={}", zs[1]);
    }

    #[test]
    fn test_slice_mesh_cube_has_contours() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        // Every layer through the cube should have at least one contour
        for layer in &layers {
            assert!(
                !layer.paths.is_empty(),
                "Layer at z={} has no contours",
                layer.z
            );
        }
    }
}
