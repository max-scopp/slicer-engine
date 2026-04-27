//! Core slicing operations and data structures

use clipper2::*;

use crate::logging::ProcessLogger;
use crate::mesh::types::{Mesh, Vertex};
use crate::settings::params::SlicingParams;

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
    /// Solid bottom-surface infill.
    BottomSurface,
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
            Self::BottomSurface => "Bottom surface",
            Self::Support => "Support",
            Self::Skirt => "Skirt",
        }
    }

    /// Default extrusion width in mm for this role.
    ///
    /// Used to populate the `;WIDTH:` annotation in the G-code output.
    pub fn default_width_mm(self) -> f64 {
        match self {
            Self::Perimeter
            | Self::Infill
            | Self::Bridge
            | Self::TopSurface
            | Self::BottomSurface => 0.4,
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

/// Central entry point for the complete slicing pipeline.
///
/// This function processes a mesh through the entire slicing pipeline, including
/// basic slicing, top/bottom surface generation, and any other processing steps.
/// All pipeline progress is reported through `logger` so that CLI and WebSocket
/// callers receive the same verbosity and information.
///
/// This is the main API function that should be extended with additional features
/// like infill generation, support structures, etc.
///
/// # Arguments
/// * `mesh` - The triangle mesh to process
/// * `params` - Slicing parameters controlling all aspects of the slicing process
/// * `logger` - Pipeline logger; use [`NullLogger`] when logging is not needed
///
/// # Returns
/// A `Vec<SliceLayer>` with all processing applied (perimeters, surfaces, etc.).
///
/// # Example
/// ```
/// use slicer_engine::logging::NullLogger;
/// use slicer_engine::mesh::types::Mesh;
/// use slicer_engine::settings::params::SlicingParams;
/// use slicer_engine::core::process_mesh;
///
/// let mesh = Mesh::new(); // Load your mesh
/// let params = SlicingParams::default();
/// let layers = process_mesh(&mesh, &params, &NullLogger);
/// ```
pub fn process_mesh(
    mesh: &Mesh,
    params: &SlicingParams,
    logger: &dyn ProcessLogger,
) -> Vec<SliceLayer> {
    logger.log_info(&format!("processing mesh: {} triangles", mesh.faces.len()));
    logger.log_debug("slicing mesh…");

    let mut layers = slice_mesh(mesh, params.layer_height);

    logger.log_info(&format!("sliced into {} layers", layers.len()));

    // Add top/bottom surfaces
    if params.top_layers > 0 || params.bottom_layers > 0 {
        logger.log_debug(&format!(
            "generating surfaces (top: {}, bottom: {}, angle: {}°)",
            params.top_layers, params.bottom_layers, params.surface_infill_angle
        ));
        generate_top_bottom_surfaces(
            &mut layers,
            params.top_layers,
            params.bottom_layers,
            params.layer_height,
            params.surface_infill_angle,
        );
        logger.log_debug("surface generation complete");
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

/// Generate solid infill patterns for top and bottom surfaces.
///
/// For each layer the function computes the region that needs solid infill by
/// asking: *"what area of this layer is NOT covered by all N layers
/// above/below it simultaneously?"*
///
/// Formally, the top-surface region at layer `i` is:
///
/// ```text
/// top_region[i] = layer[i]  −  ∩(layer[i+1], layer[i+2], …, layer[i+N])
/// ```
///
/// The intersection of the N successor layers represents the area that has
/// continuous solid support for every one of those layers.  Any part of
/// `layer[i]` that is **not** in that intersection is exposed within the next
/// N layers and therefore needs solid top infill.
///
/// This correctly handles:
/// - Absolute top/bottom of the model (no layers above/below → intersection
///   is empty → entire layer is a surface).
/// - Small features sitting on a larger body (e.g. the Benchy cabin on the
///   boat deck): a wall layer of the cabin is *not* falsely marked as a
///   surface, because the intermediate cabin layers above it still cover it.
///   Only the cabin **roof** layers (the topmost N) are correctly marked as
///   top surfaces, since above them there are no more cabin layers.
/// - Mid-model surfaces: ledges, internal floors, porthole rims, etc.
///
/// # Arguments
/// * `layers`        – Mutable reference to the slice layers
/// * `top_layers`    – Number of solid layers above any exposed top surface
/// * `bottom_layers` – Number of solid layers below any exposed bottom surface
/// * `layer_height`  – Layer height in mm, used to derive infill spacing
/// * `infill_angle`  – Angle in degrees for solid infill lines (e.g. 45)
pub fn generate_top_bottom_surfaces(
    layers: &mut [SliceLayer],
    top_layers: usize,
    bottom_layers: usize,
    layer_height: f64,
    infill_angle: f64,
) {
    if layers.is_empty() || (top_layers == 0 && bottom_layers == 0) {
        return;
    }

    let total = layers.len();

    // Snapshot the perimeter contours of every layer *before* we begin adding
    // infill paths. Surface detection must operate on sliced geometry only;
    // comparing against previously added infill would give wrong results.
    let perimeters: Vec<Paths> = layers.iter().map(perimeter_paths_of).collect();

    for i in 0..total {
        // ── Bottom surfaces ──────────────────────────────────────────────────
        let bottom_region = if bottom_layers > 0 {
            // Compute the area of layer[i] covered by ALL bottom_layers layers
            // below it via progressive intersection.  Any part of layer[i] not
            // in that intersection is a bottom surface.
            let mut covered = perimeters[i].clone();
            for j in 1..=bottom_layers {
                if i < j {
                    // Ran off the model bottom — no coverage from here on.
                    covered = Paths::new(vec![]);
                    break;
                }
                let neighbor = &perimeters[i - j];
                if neighbor.is_empty() {
                    // Empty layer below counts as no coverage.
                    covered = Paths::new(vec![]);
                    break;
                }
                covered =
                    intersect(covered, neighbor.clone(), FillRule::NonZero).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }

            let region =
                difference(perimeters[i].clone(), covered, FillRule::NonZero).unwrap_or_default();
            if !region.is_empty() {
                add_solid_infill_for_region(
                    &mut layers[i],
                    &region,
                    ExtrusionRole::BottomSurface,
                    layer_height,
                    infill_angle,
                );
            }
            region
        } else {
            Paths::new(vec![])
        };

        // ── Top surfaces ─────────────────────────────────────────────────────
        if top_layers > 0 {
            // Symmetric to the bottom-surface logic above, but looking upward.
            // IMPORTANT: Exclude the bottom_region to prevent overlapping top
            // and bottom surfaces on the same layer.
            let mut covered = perimeters[i].clone();
            for j in 1..=top_layers {
                if i + j >= total {
                    // Ran off the model top — no coverage from here on.
                    covered = Paths::new(vec![]);
                    break;
                }
                let neighbor = &perimeters[i + j];
                if neighbor.is_empty() {
                    covered = Paths::new(vec![]);
                    break;
                }
                covered =
                    intersect(covered, neighbor.clone(), FillRule::NonZero).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }

            let mut top_region =
                difference(perimeters[i].clone(), covered, FillRule::NonZero).unwrap_or_default();

            // Subtract bottom_region to avoid overlap
            if !bottom_region.is_empty() && !top_region.is_empty() {
                top_region = difference(top_region, bottom_region, FillRule::NonZero)
                    .unwrap_or_default();
            }

            if !top_region.is_empty() {
                add_solid_infill_for_region(
                    &mut layers[i],
                    &top_region,
                    ExtrusionRole::TopSurface,
                    layer_height,
                    infill_angle,
                );
            }
        }
    }
}

/// Extract only [`ExtrusionRole::Perimeter`] paths from a layer.
///
/// Used to snapshot slice contours before infill is added, so that surface
/// detection compares geometry, not previously-generated infill.
fn perimeter_paths_of(layer: &SliceLayer) -> Paths {
    Paths::new(
        layer
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layer.role_for_path(*i) == ExtrusionRole::Perimeter)
            .map(|(_, p)| p.clone())
            .collect(),
    )
}

/// Calculate infill line spacing based on layer height
/// Standard extrusion width is typically 1.2× layer height for solid infill
const SOLID_INFILL_EXTRUSION_WIDTH_MULTIPLIER: f64 = 1.2;

/// Add solid infill for a computed surface `region` to a layer.
///
/// Generates a rectilinear infill pattern covering only the provided `region`
/// paths (the already-computed surface area), then appends the resulting paths
/// to `layer` with the given extrusion `role`.
fn add_solid_infill_for_region(
    layer: &mut SliceLayer,
    region: &Paths,
    role: ExtrusionRole,
    layer_height: f64,
    infill_angle: f64,
) {
    if region.is_empty() {
        return;
    }

    let line_spacing = layer_height * SOLID_INFILL_EXTRUSION_WIDTH_MULTIPLIER;
    let infill_paths = generate_rectilinear_infill(region, line_spacing, infill_angle);

    for path in infill_paths {
        layer.paths.push(path);
        layer.path_roles.push(role);
    }
}

/// Generate rectilinear infill pattern within the given contours.
///
/// Creates a series of parallel lines at the specified angle that fill the
/// interior of the contours and are **clipped exactly to the contour shape**
/// using a scanline intersection algorithm. Lines never extend outside the
/// perimeter boundary.
///
/// # Algorithm
/// 1. Rotate all contour vertices by `-angle` so scan lines become horizontal.
/// 2. For each horizontal scan line (spaced by `line_spacing`), find where it
///    crosses each polygon edge and collect the X-intersection coordinates.
/// 3. Sort intersections and emit segments between paired entry/exit points.
/// 4. Rotate the resulting segment endpoints back by `+angle`.
///
/// # Arguments
/// * `contours`      – Boundary paths (the surface region) to fill
/// * `line_spacing`  – Distance between infill lines in mm
/// * `angle_degrees` – Angle of infill lines (0° = horizontal, 45° = diagonal)
///
/// # Returns
/// Paths representing the infill lines, clipped to `contours`.
fn generate_rectilinear_infill(contours: &Paths, line_spacing: f64, angle_degrees: f64) -> Paths {
    if contours.is_empty() || line_spacing <= 0.0 {
        return Paths::new(vec![]);
    }

    let angle_rad = angle_degrees.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    // Rotate point (x, y) by -angle so infill direction aligns with the X axis
    let rotate_neg = |x: f64, y: f64| -> (f64, f64) {
        (x * cos_a + y * sin_a, -x * sin_a + y * cos_a)
    };
    // Rotate point (x, y) by +angle to recover the original coordinate system
    let rotate_pos = |x: f64, y: f64| -> (f64, f64) {
        (x * cos_a - y * sin_a, x * sin_a + y * cos_a)
    };

    // Collect rotated polygon vertices for every contour path
    let rotated_polys: Vec<Vec<(f64, f64)>> = contours
        .iter()
        .filter_map(|path| {
            let pts: Vec<(f64, f64)> = path.iter().map(|pt| rotate_neg(pt.x(), pt.y())).collect();
            if pts.len() >= 2 { Some(pts) } else { None }
        })
        .collect();

    if rotated_polys.is_empty() {
        return Paths::new(vec![]);
    }

    // Bounding Y range in the rotated coordinate system
    let y_min = rotated_polys
        .iter()
        .flat_map(|p| p.iter().map(|&(_, y)| y))
        .fold(f64::INFINITY, f64::min);
    let y_max = rotated_polys
        .iter()
        .flat_map(|p| p.iter().map(|&(_, y)| y))
        .fold(f64::NEG_INFINITY, f64::max);

    if y_min >= y_max {
        return Paths::new(vec![]);
    }

    let mut result_paths = Paths::new(vec![]);

    // First scan line aligned to the grid, spanning [y_min, y_max]
    let start_y = (y_min / line_spacing).floor() * line_spacing;
    let mut scan_y = start_y;

    // Half a line_spacing is added so the final scan line is not missed when
    // y_max falls exactly on a grid position (avoids an off-by-one at the top).
    while scan_y <= y_max + line_spacing * 0.5 {
        // Collect all X-coordinates where the scan line crosses polygon edges
        let mut xs: Vec<f64> = Vec::new();

        for poly in &rotated_polys {
            let n = poly.len();
            for i in 0..n {
                let (x0, y0) = poly[i];
                let (x1, y1) = poly[(i + 1) % n];

                // Edge straddle check using strict inequality on both sides gives
                // the standard even-odd scanline rule: each edge is counted exactly
                // once even when the scan line passes through a shared vertex.
                if (y0 < scan_y) != (y1 < scan_y) {
                    let t = (scan_y - y0) / (y1 - y0);
                    xs.push(x0 + t * (x1 - x0));
                }
            }
        }

        xs.sort_by(|a, b| a.total_cmp(b));

        // Emit a line segment for each inside pair (even/odd winding)
        let mut k = 0;
        while k + 1 < xs.len() {
            let x_start = xs[k];
            let x_end = xs[k + 1];
            if x_end > x_start + 1e-9 {
                let (sx, sy) = rotate_pos(x_start, scan_y);
                let (ex, ey) = rotate_pos(x_end, scan_y);
                let path: Path = vec![(sx, sy), (ex, ey)].into();
                result_paths.push(path);
            }
            k += 2;
        }

        scan_y += line_spacing;
    }

    result_paths
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

    #[test]
    fn test_generate_top_bottom_surfaces_empty_layers() {
        let mut layers: Vec<SliceLayer> = vec![];
        generate_top_bottom_surfaces(&mut layers, 3, 3, 0.2, 45.0);
        // Should handle empty input gracefully
        assert!(layers.is_empty());
    }

    #[test]
    fn test_generate_top_bottom_surfaces_zero_count() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let original_count = layers.len();

        generate_top_bottom_surfaces(&mut layers, 0, 0, 2.0, 45.0);

        // Layers should remain unchanged when both counts are 0
        assert_eq!(layers.len(), original_count);
    }

    #[test]
    fn test_generate_top_bottom_surfaces_adds_infill() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let original_paths_first = layers[0].paths.len();

        // Generate bottom surfaces for first 2 layers, top for last 2
        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // First layer should have more paths (original perimeters + infill)
        assert!(
            layers[0].paths.len() > original_paths_first,
            "Expected infill to be added to bottom layer"
        );
    }

    #[test]
    fn test_generate_top_bottom_surfaces_roles_assigned() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let total = layers.len();

        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // Check that bottom layers have BottomSurface role
        for (i, layer) in layers.iter().take(2).enumerate() {
            let has_bottom_role = layer.path_roles.contains(&ExtrusionRole::BottomSurface);
            assert!(
                has_bottom_role,
                "Layer {} should have BottomSurface role",
                i
            );
        }

        // Check that top layers have TopSurface role
        for (i, layer) in layers.iter().enumerate().skip(total - 2).take(2) {
            let has_top_role = layer.path_roles.contains(&ExtrusionRole::TopSurface);
            assert!(has_top_role, "Layer {} should have TopSurface role", i);
        }
    }

    #[test]
    fn test_generate_top_bottom_surfaces_mid_model_detection() {
        // Build a stacked two-cube mesh: a 10×10×4 base with a 6×6×4 column on top.
        // When sliced at layer_height=2 we get:
        //   layer 0  z=1 – base (10×10)
        //   layer 1  z=3 – base (10×10)
        //   layer 2  z=5 – column (6×6)
        //   layer 3  z=7 – column (6×6)
        //
        // With top_layers=1, bottom_layers=1 (intersection-based algorithm):
        //
        //   TopSurface on layer 1 (z=3):
        //     covered = intersect(10×10, layer_above=6×6) = 6×6
        //     top_region = diff(10×10, 6×6) = annular region → non-empty
        //     → layer 1 must have TopSurface infill
        //
        //   No TopSurface on layer 2 (z=5):
        //     covered = intersect(6×6, layer_above=6×6) = 6×6
        //     top_region = diff(6×6, 6×6) = empty
        //     → layer 2 is fully covered by layer 3 and must NOT have TopSurface infill
        //
        //   BottomSurface on layer 0 (z=1):
        //     i < j → covered = empty → bottom_region = perimeters[0] (first layer)
        //
        //   No BottomSurface on layer 2 (z=5):
        //     covered = intersect(6×6, layer_below=10×10) = 6×6 (column inside base)
        //     bottom_region = diff(6×6, 6×6) = empty
        //     → the column is fully supported; it must NOT get spurious BottomSurface infill

        let v: Vec<Vertex> = vec![
            // Base cube 10×10×4 (z 0..4)
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
            Vertex::new(0.0, 0.0, 4.0),
            Vertex::new(10.0, 0.0, 4.0),
            Vertex::new(10.0, 10.0, 4.0),
            Vertex::new(0.0, 10.0, 4.0),
            // Upper column 6×6×4 (z 4..8), centred at (2,2)..(8,8)
            Vertex::new(2.0, 2.0, 4.0),
            Vertex::new(8.0, 2.0, 4.0),
            Vertex::new(8.0, 8.0, 4.0),
            Vertex::new(2.0, 8.0, 4.0),
            Vertex::new(2.0, 2.0, 8.0),
            Vertex::new(8.0, 2.0, 8.0),
            Vertex::new(8.0, 8.0, 8.0),
            Vertex::new(2.0, 8.0, 8.0),
        ];
        let face_indices: &[[usize; 3]] = &[
            // Base cube faces
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
            // Column cube faces
            [8, 10, 9],
            [8, 11, 10],
            [12, 13, 14],
            [12, 14, 15],
            [8, 9, 13],
            [8, 13, 12],
            [10, 11, 15],
            [10, 15, 14],
            [8, 12, 15],
            [8, 15, 11],
            [9, 10, 14],
            [9, 14, 13],
        ];

        let faces = face_indices
            .iter()
            .map(|idx| Face::new([v[idx[0]], v[idx[1]], v[idx[2]]]))
            .collect();

        let mesh = Mesh {
            vertices: v,
            faces,
            aabb: None,
        };

        let mut layers = slice_mesh(&mesh, 2.0);
        assert_eq!(layers.len(), 4, "Expected 4 layers for the step mesh");

        generate_top_bottom_surfaces(&mut layers, 1, 1, 2.0, 45.0);

        // Layer 0 (z=1) is the absolute bottom → BottomSurface
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 0 (z=1) should be a bottom surface (first layer)"
        );

        // Layer 1 (z=3) is below the column; the annular 10×10 minus 6×6 region
        // is exposed above → TopSurface infill must be added.
        assert!(
            layers[1].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 1 (z=3) should detect the step-down as a top surface"
        );

        // Layer 2 (z=5) is fully covered by layer 3 above → no TopSurface here.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 2 (z=5) is fully covered above and must NOT have TopSurface infill"
        );

        // Layer 2 (z=5) is fully supported by layer 1 below → no BottomSurface.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 2 (z=5) is fully supported and must NOT have spurious BottomSurface infill"
        );

        // Layer 3 (z=7) is the absolute top → TopSurface
        assert!(
            layers[3].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 3 (z=7) should be a top surface (last layer)"
        );
    }

    #[test]
    fn test_infill_clipped_to_contour() {
        // Verify that infill lines are clipped to the contour and don't extend
        // beyond the bounding box of the given paths.
        let square: Path =
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 1.0, 0.0);

        assert!(!infill.is_empty(), "Expected infill lines to be generated");

        // All clipped endpoints should lie within the contour bounding box
        // (with a small epsilon for floating-point rounding by Clipper2).
        let eps = 0.01;
        for path in infill.iter() {
            for pt in path.iter() {
                let x = pt.x();
                let y = pt.y();
                assert!(
                    x >= -eps && x <= 10.0 + eps,
                    "Infill x={x} is outside contour bounds [0, 10]"
                );
                assert!(
                    y >= -eps && y <= 10.0 + eps,
                    "Infill y={y} is outside contour bounds [0, 10]"
                );
            }
        }
    }

    #[test]
    fn test_add_solid_infill_for_region_empty_region() {
        let mut layer = SliceLayer::new(1.0);
        let empty: Paths = Paths::new(vec![]);
        add_solid_infill_for_region(&mut layer, &empty, ExtrusionRole::TopSurface, 0.2, 45.0);
        // Should handle empty region gracefully – no paths added
        assert!(layer.paths.is_empty());
    }

    #[test]
    fn test_extrusion_role_bottom_surface() {
        assert_eq!(ExtrusionRole::BottomSurface.type_name(), "Bottom surface");
        assert!(ExtrusionRole::BottomSurface.default_width_mm() > 0.0);
    }

    #[test]
    fn test_process_mesh() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 45.0,
            ..SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);

        // Should have layers
        assert!(!layers.is_empty());

        // First layer should have BottomSurface paths
        assert!(layers[0].path_roles.contains(&ExtrusionRole::BottomSurface));

        // Last layer should have TopSurface paths
        let last_idx = layers.len() - 1;
        assert!(layers[last_idx]
            .path_roles
            .contains(&ExtrusionRole::TopSurface));
    }

    #[test]
    fn test_generate_rectilinear_infill_basic() {
        // Create a simple square path
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 1.0, 45.0);

        // Should generate some infill lines
        assert!(!infill.is_empty(), "Expected infill lines to be generated");
    }

    #[test]
    fn test_generate_rectilinear_infill_empty_contours() {
        let paths = Paths::new(vec![]);
        let infill = generate_rectilinear_infill(&paths, 1.0, 45.0);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_rectilinear_infill_zero_spacing() {
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 0.0, 45.0);
        assert!(infill.is_empty());
    }

    /// Regression test: the surface detection algorithm must use progressive
    /// intersection, not a single comparison against the N-th neighbour.
    ///
    /// The "hourglass" scenario has a wide layer, a narrow intermediate layer,
    /// and then wide layers again.  The old `difference(layer[i], layer[i+N])`
    /// approach compared a wide layer against a later wide layer and returned
    /// empty → no top surface, silently missing the narrow gap in between.
    /// With the intersection-based approach the coverage is narrowed by the
    /// intermediate narrow layer, so the annular region is correctly flagged.
    #[test]
    fn test_surface_detection_non_monotonic_shape() {
        // Layers (manual construction, not mesh-derived):
        //   layer 0: 10×10 wide
        //   layer 1: 10×10 wide
        //   layer 2:  4×4  narrow  ← the "waist"
        //   layer 3: 10×10 wide
        //   layer 4: 10×10 wide
        //   layer 5: 10×10 wide
        //
        // With top_layers=3:
        //
        //   layer 2 (narrow, 4×4): covered by layers 3,4,5 (all 10×10 ⊇ 4×4)
        //     → NOT a top surface ✓
        //
        //   layer 0 (wide, 10×10):
        //     NEW: j=1 → intersect(10×10, 10×10) = 10×10
        //          j=2 → intersect(10×10, 4×4)   = 4×4   ← narrows
        //          j=3 → intersect(4×4,  10×10)  = 4×4
        //          top_region = diff(10×10, 4×4) = annular  ← TOP SURFACE ✓
        //     OLD: diff(layer[0], layer[3]) = diff(10×10, 10×10) = empty  ✗
        let make_rect_layer = |z: f64, w: f64, h: f64| -> SliceLayer {
            let mut layer = SliceLayer::new(z);
            let path: Path = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].into();
            layer.paths.push(path);
            layer.path_roles.push(ExtrusionRole::Perimeter);
            layer
        };

        let mut layers = vec![
            make_rect_layer(1.0, 10.0, 10.0), // 0 – wide
            make_rect_layer(2.0, 10.0, 10.0), // 1 – wide
            make_rect_layer(3.0, 4.0, 4.0),   // 2 – narrow
            make_rect_layer(4.0, 10.0, 10.0), // 3 – wide
            make_rect_layer(5.0, 10.0, 10.0), // 4 – wide
            make_rect_layer(6.0, 10.0, 10.0), // 5 – wide
        ];

        generate_top_bottom_surfaces(&mut layers, 3, 0, 1.0, 45.0);

        // Layer 2 (narrow): fully covered by the three wide layers above it.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::TopSurface),
            "Narrow layer 2 is fully covered above and must NOT have TopSurface infill"
        );

        // Layer 0: the 10×10 annular area is NOT covered at layer 2 (only 4×4)
        // → must be flagged as a top surface even though layer[0+3]=layer3 is
        //   also 10×10 (the gap at layer 2 is in between).
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 0 should have TopSurface infill: the annular region is exposed at layer 2"
        );

        // Layers 3, 4, 5 are the top-3 wide layers → must all be top surfaces.
        for idx in [3, 4, 5] {
            assert!(
                layers[idx].path_roles.contains(&ExtrusionRole::TopSurface),
                "Layer {idx} is within top_layers=3 of the model top and must have TopSurface"
            );
        }
    }

    /// Test that top and bottom surfaces don't overlap on the same layer.
    /// This was a bug where regions could be marked as both top AND bottom,
    /// causing incorrect G-code output.
    #[test]
    fn test_no_overlapping_top_bottom_surfaces() {
        // Create a simple layer stack where the first layer could potentially
        // be marked as both top and bottom if the algorithm is broken.
        let make_rect_layer = |z: f64, w: f64, h: f64| -> SliceLayer {
            let mut layer = SliceLayer::new(z);
            let path: Path = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].into();
            layer.paths.push(path);
            layer.path_roles.push(ExtrusionRole::Perimeter);
            layer
        };

        let mut layers = vec![
            make_rect_layer(1.0, 10.0, 10.0), // 0 – base layer
            make_rect_layer(2.0, 10.0, 10.0), // 1
            make_rect_layer(3.0, 10.0, 10.0), // 2
            make_rect_layer(4.0, 10.0, 10.0), // 3 – top layer
        ];

        // Generate with both top_layers and bottom_layers enabled
        generate_top_bottom_surfaces(&mut layers, 2, 2, 1.0, 45.0);

        // Check each layer to ensure no path is in BOTH top and bottom regions
        for (layer_idx, layer) in layers.iter().enumerate() {
            let has_top = layer.path_roles.contains(&ExtrusionRole::TopSurface);
            let has_bottom = layer.path_roles.contains(&ExtrusionRole::BottomSurface);

            // Count the actual number of each type
            let top_count = layer
                .path_roles
                .iter()
                .filter(|&&r| r == ExtrusionRole::TopSurface)
                .count();
            let bottom_count = layer
                .path_roles
                .iter()
                .filter(|&&r| r == ExtrusionRole::BottomSurface)
                .count();

            if has_top && has_bottom {
                panic!(
                    "Layer {} has BOTH top ({}) and bottom ({}) surface paths - they should not overlap!",
                    layer_idx, top_count, bottom_count
                );
            }
        }

        // Layer 0 should be bottom only (first two layers)
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 0 should have bottom surface"
        );
        assert!(
            !layers[0].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 0 should NOT have top surface"
        );

        // Layer 3 (top) should be top only
        assert!(
            layers[3].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 3 should have top surface"
        );
        assert!(
            !layers[3].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 3 should NOT have bottom surface"
        );
    }
}
