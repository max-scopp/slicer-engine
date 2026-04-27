//! Core slicing operations and data structures

use clipper2::*;

use crate::logging::{phases, PhaseTimer, ProcessLogger};
use crate::mesh::types::{Mesh, Vertex};
use crate::settings::params::SlicingParams;

/// The role of an extrusion path, used to annotate G-code with `;TYPE:` comments
/// and enable firmware features like Klipper adaptive acceleration by role.
///
/// Each variant maps to a named type that is emitted in the G-code output and
/// carries a default extrusion width for that role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtrusionRole {
    /// Outermost perimeter / wall contour (default role).
    #[default]
    OuterWall,
    /// Inner perimeter / wall contours.
    InnerWall,
    /// Sparse infill pattern (low-density interior fill).
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
    ///
    /// Strings match the OrcaSlicer convention exactly so that G-code previews
    /// colour and classify paths correctly.  Any unrecognised string would be
    /// shown as *Undefined* in OrcaSlicer's G-code viewer.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::OuterWall => "Outer wall",
            Self::InnerWall => "Inner wall",
            Self::Infill => "Sparse infill",
            Self::Bridge => "Bridge",
            Self::TopSurface => "Top surface",
            Self::BottomSurface => "Bottom surface",
            Self::Support => "Support material",
            Self::Skirt => "Skirt",
        }
    }

    /// Default extrusion width in mm for this role.
    ///
    /// Used to populate the `;WIDTH:` annotation in the G-code output.
    pub fn default_width_mm(self) -> f64 {
        match self {
            Self::OuterWall
            | Self::InnerWall
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
    /// the remaining paths default to [`ExtrusionRole::OuterWall`].
    pub path_roles: Vec<ExtrusionRole>,
    /// Per-path extrusion width override in mm.
    ///
    /// `path_widths[i]` is the extrusion width for `paths[i]`.  `None` means
    /// use the role's default width ([`ExtrusionRole::default_width_mm`]).
    /// This is set by the Arachne variable-width perimeter generator.
    pub path_widths: Vec<Option<f64>>,
    /// The union of top and bottom solid-surface regions on this layer.
    ///
    /// Populated by [`generate_top_bottom_surfaces`] and used by
    /// [`add_infill_to_layers`] to prevent sparse infill from being placed on
    /// areas already filled with solid top/bottom surface infill.
    pub solid_regions: Paths,
}

impl SliceLayer {
    /// Create a new slice layer at the given Z coordinate
    pub fn new(z: f64) -> Self {
        Self {
            z,
            paths: Paths::default(),
            path_roles: Vec::new(),
            path_widths: Vec::new(),
            solid_regions: Paths::default(),
        }
    }

    /// Return the extrusion role for path index `i`.
    ///
    /// Falls back to [`ExtrusionRole::OuterWall`] when `path_roles` has no
    /// entry for the given index.
    pub fn role_for_path(&self, i: usize) -> ExtrusionRole {
        self.path_roles.get(i).copied().unwrap_or_default()
    }

    /// Return the extrusion width in mm for path index `i`.
    ///
    /// Returns the per-path override when set, otherwise falls back to the
    /// role's default width via [`ExtrusionRole::default_width_mm`].
    pub fn width_for_path(&self, i: usize) -> Option<f64> {
        self.path_widths.get(i).copied().flatten()
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
/// This function only generates perimeter paths. Use [`add_infill_to_layers`]
/// to add infill patterns to the layers after slicing.
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
                layer.path_roles.push(ExtrusionRole::OuterWall);
            }
        }

        layers.push(layer);
        z += layer_height;
    }

    layers
}

/// Add infill paths to layers based on slicing parameters.
///
/// Takes a set of layers with perimeter paths and adds infill patterns within
/// the perimeter boundaries. Infill paths are assigned the [`ExtrusionRole::Infill`]
/// role for proper G-code annotation.
///
/// # Arguments
/// * `layers` - Slice layers with perimeter paths (will be modified in place)
/// * `infill_density` - Infill density as a fraction (0.0 = no infill, 1.0 = solid)
/// * `infill_pattern` - The pattern type to generate (rectilinear, grid, etc.)
/// * `infill_base_angle` - Base angle in degrees (alternating layers rotate +90° on top of this)
///
/// # Example
/// ```rust,no_run
/// use slicer_engine::core::{slice_mesh, add_infill_to_layers};
/// use slicer_engine::infill::InfillPattern;
/// # use slicer_engine::mesh::types::Mesh;
/// # let mesh = Mesh::new();
///
/// let mut layers = slice_mesh(&mesh, 0.2);
/// add_infill_to_layers(&mut layers, 0.2, InfillPattern::Rectilinear, 45.0);
/// ```
pub fn add_infill_to_layers(
    layers: &mut [SliceLayer],
    infill_density: f64,
    infill_pattern: crate::infill::InfillPattern,
    infill_base_angle: f64,
) {
    use crate::infill::generate_infill;
    
    if infill_density <= 0.0 {
        return; // No infill requested
    }

    for (layer_idx, layer) in layers.iter_mut().enumerate() {
        // Skip layers with no perimeters
        if layer.paths.is_empty() {
            continue;
        }

        // Calculate the infill region: start with the perimeter-only paths,
        // then subtract any area already covered by solid top/bottom surfaces.
        let perimeter_paths = Paths::new(
            layer
                .paths
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    let role = layer.role_for_path(*i);
                    role == ExtrusionRole::OuterWall || role == ExtrusionRole::InnerWall
                })
                .map(|(_, p)| p.clone())
                .collect(),
        );

        let infill_area = if !layer.solid_regions.is_empty() {
            // Subtract solid surface regions so sparse infill is never placed
            // on top of existing solid top/bottom surface infill.
            let remaining = difference(
                perimeter_paths,
                layer.solid_regions.clone(),
                FillRule::EvenOdd,
            )
            .unwrap_or_default();
            if remaining.is_empty() {
                // Entire layer is covered by solid surfaces — no sparse infill needed.
                continue;
            }
            remaining
        } else {
            perimeter_paths
        };

        // Calculate angle offset for alternating patterns
        // Rectilinear infill alternates base_angle and base_angle+90° each layer
        let base_angle_rad = infill_base_angle.to_radians();
        let angle_offset = if layer_idx % 2 == 0 {
            base_angle_rad
        } else {
            base_angle_rad + std::f64::consts::FRAC_PI_2 // +90 degrees
        };

        // Generate infill paths within the computed area
        // Pass layer Z height for patterns like gyroid that need it
        let infill_paths = generate_infill(&infill_area, infill_pattern, infill_density, angle_offset, layer.z);

        // Add infill paths to the layer with proper role annotation
        for infill_path in infill_paths.iter() {
            layer.paths.push(infill_path.clone());
            layer.path_roles.push(ExtrusionRole::Infill);
        }
    }
}

/// Central entry point for the complete slicing pipeline.
///
/// This function processes a mesh through the entire slicing pipeline, including
/// basic slicing, top/bottom surface generation, and Arachne variable-width
/// perimeter generation.  All pipeline progress is reported through `logger`
/// so that CLI and WebSocket callers receive the same verbosity and information.
///
/// # Arguments
/// * `mesh` - The triangle mesh to process
/// * `params` - Slicing parameters controlling all aspects of the slicing process
/// * `logger` - Pipeline logger; use [`NullLogger`] when logging is not needed
///
/// # Returns
/// A `Vec<SliceLayer>` with all processing applied (Arachne walls, surfaces, etc.).
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

    let t_slicing = PhaseTimer::start(phases::SLICING, logger);
    logger.log_debug("slicing mesh…");
    let mut layers = slice_mesh(mesh, params.layer_height);
    logger.log_info(&format!("sliced into {} layers", layers.len()));
    t_slicing.finish();

    // Generate Arachne walls FIRST from the raw mesh contours
    logger.log_debug(&format!(
        "generating Arachne walls (wall_count: {}, nozzle: {}mm)",
        params.wall_count, params.nozzle_diameter_mm
    ));
    let arachne_params = crate::arachne::ArachneParams::from_slicing_params(params);
    crate::arachne::generate_arachne_walls(&mut layers, &arachne_params);
    logger.log_debug("Arachne wall generation complete");

    // Apply single-wall restrictions to first/last layers if configured.
    //
    // The "last layer of each top surface run" detection MUST run before
    // calculate_interior_region so that the interior of single-wall layers
    // collapses to "everything inside the outer wall" (deflate by 1×nozzle)
    // instead of a tiny disk inside the would-be innermost wall.  We derive
    // top-surface layers geometrically from perimeters here because surface
    // generation hasn't run yet, so path_roles don't carry TopSurface.
    if params.only_one_wall_first_layer || params.only_one_wall_top {
        logger.log_debug("applying single-wall restrictions");
        let has_top_surface = if params.only_one_wall_top {
            compute_layers_with_top_surface(&layers, params.top_layers)
        } else {
            vec![false; layers.len()]
        };
        apply_single_wall_restrictions(&mut layers, params, &has_top_surface);
    }

    // Calculate interior regions (inside walls) for each layer where surfaces will go
    let interior_regions: Vec<Paths> = if params.top_layers > 0 || params.bottom_layers > 0 {
        logger.log_debug("calculating interior regions for surfaces");
        layers
            .iter()
            .map(|layer| {
                calculate_interior_region(
                    layer,
                    params.infill_overlap_percent,
                    params.nozzle_diameter_mm,
                )
            })
            .collect()
    } else {
        vec![]
    };

    // Now generate top/bottom surfaces INSIDE the walls
    if params.top_layers > 0 || params.bottom_layers > 0 {
        let t_surfaces = PhaseTimer::start(phases::SURFACES, logger);
        logger.log_debug(&format!(
            "generating surfaces (top: {}, bottom: {}, angle: {}°)",
            params.top_layers, params.bottom_layers, params.surface_infill_angle
        ));
        generate_top_bottom_surfaces_with_interior(
            &mut layers,
            params.top_layers,
            params.bottom_layers,
            params.layer_height,
            params.surface_infill_angle,
            Some(&interior_regions),
        );
        logger.log_debug("surface generation complete");
        t_surfaces.finish();
    }

    // Add infill
    if params.infill_density > 0.0 {
        use crate::infill::InfillPattern;
        
        let infill_pattern = InfillPattern::parse(&params.infill_pattern)
            .unwrap_or(InfillPattern::Rectilinear);
        
        logger.log_debug(&format!(
            "generating {} infill at {:.0}% density, {}° base angle…",
            infill_pattern.name(),
            params.infill_density * 100.0,
            params.infill_base_angle
        ));
        
        add_infill_to_layers(&mut layers, params.infill_density, infill_pattern, params.infill_base_angle);
        logger.log_debug("infill generation complete");
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
/// - Holes (debossed text, portholes, etc.): the `chain_segments` function
///   does not guarantee a specific winding order for inner contours produced
///   by the mesh slicer.  All Clipper2 boolean operations therefore use
///   [`FillRule::EvenOdd`] which is winding-order–independent: a point is
///   "inside" when surrounded by an **odd** number of boundaries, naturally
///   treating nested contours as holes without relying on CW vs CCW direction.
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
    generate_top_bottom_surfaces_with_interior(
        layers,
        top_layers,
        bottom_layers,
        layer_height,
        infill_angle,
        None, // No interior regions - use full perimeters
    );
}

/// Generate top and bottom solid surface infill for layers.
///
/// Detects which parts of each layer are exposed (unsupported from below for
/// bottom surfaces, or exposed from above for top surfaces) by comparing each
/// layer's geometry to its neighbors. Exposed regions are then filled with
/// solid rectilinear infill.
///
/// The detection algorithm uses **progressive intersection** to handle complex
/// geometry: for top surfaces, a layer's region is intersected with ALL
/// `top_layers` layers above it; any part not in that full intersection is a
/// top surface.  Bottom surfaces use the symmetric logic below.
///
/// Clipper2's **EvenOdd fill rule** is used for all boolean operations,
/// treating any closed contour boundary as defining an interior/exterior toggle.
/// Regions are considered "outside" when surrounded by an **even** number of
/// boundaries (0, 2, 4…) and "inside" when surrounded by an **odd** number,
/// naturally treating nested contours as holes without relying on winding order.
///
/// # Arguments
/// * `layers` - Mutable reference to the slice layers
/// * `top_layers` - Number of solid layers above any exposed top surface
/// * `bottom_layers` - Number of solid layers below any exposed bottom surface
/// * `layer_height` - Layer height in mm, used to derive infill spacing
/// * `infill_angle` - Angle in degrees for solid infill lines (e.g. 45)
/// * `interior_regions` - Optional interior regions for each layer (inside walls).
///   If provided, surface infill is clipped to these regions, ensuring walls
///   have priority over surfaces.
pub fn generate_top_bottom_surfaces_with_interior(
    layers: &mut [SliceLayer],
    top_layers: usize,
    bottom_layers: usize,
    layer_height: f64,
    infill_angle: f64,
    interior_regions: Option<&[Paths]>,
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
            //
            // EvenOdd fill rule is used throughout so that the operations are
            // winding-order–independent.  The mesh slicer (chain_segments) does
            // not guarantee a consistent winding direction for inner contours
            // (holes such as debossed text); NonZero would misidentify a hole
            // whose winding happens to match the outer contour as solid material.
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
                    intersect(covered, neighbor.clone(), FillRule::EvenOdd).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }

            let mut region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();

            // If interior regions are provided, clip surface to interior (inside walls)
            // Skip surface if interior region is empty or too small
            if let Some(interior_regions) = interior_regions {
                if interior_regions[i].is_empty() {
                    // No interior space - walls fill the entire area
                    // Skip surface generation entirely
                    region = Paths::new(vec![]);
                } else if !region.is_empty() {
                    region = intersect(region, interior_regions[i].clone(), FillRule::EvenOdd)
                        .unwrap_or_default();
                }
            }

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
        // Use a symmetric approach to the bottom-surface logic above, looking
        // upward.  The result is captured so we can later union it with the
        // bottom region to compute the layer's total solid-surface footprint.
        let top_region = if top_layers > 0 {
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
                    intersect(covered, neighbor.clone(), FillRule::EvenOdd).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }

            let mut top_region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();

            // Subtract bottom_region to avoid overlap (clone so bottom_region
            // is still available for the solid_regions union below).
            if !bottom_region.is_empty() && !top_region.is_empty() {
                top_region =
                    difference(top_region, bottom_region.clone(), FillRule::EvenOdd).unwrap_or_default();
            }

            // If interior regions are provided, clip surface to interior (inside walls)
            // Skip surface if interior region is empty or too small
            if let Some(interior_regions) = interior_regions {
                if interior_regions[i].is_empty() {
                    // No interior space - walls fill the entire area
                    // Skip surface generation entirely
                    top_region = Paths::new(vec![]);
                } else if !top_region.is_empty() {
                    top_region =
                        intersect(top_region, interior_regions[i].clone(), FillRule::EvenOdd)
                            .unwrap_or_default();
                }
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

            top_region
        } else {
            Paths::new(vec![])
        };

        // Record the union of all solid-surface regions on this layer so that
        // add_infill_to_layers can exclude them from sparse infill.
        let combined_solid = if !bottom_region.is_empty() && !top_region.is_empty() {
            union(bottom_region, top_region, FillRule::EvenOdd).unwrap_or_default()
        } else if !bottom_region.is_empty() {
            bottom_region
        } else {
            top_region // may be empty, handled below
        };
        if !combined_solid.is_empty() {
            layers[i].solid_regions = combined_solid;
        }
    }
}

/// Trim solid surface regions to fit inside walls, with configurable overlap.
///
/// **NOTE**: This function is currently not fully working because surfaces are
/// generated as open line segments (infill lines), not closed regions. Boolean
/// operations like intersect() don't work reliably with open paths. A better
/// approach would be to generate surfaces AFTER walls, directly in the interior
/// region, rather than trying to trim them post-hoc.
///
/// After Arachne wall generation, the solid top/bottom surface infill paths may
/// overlap with the generated walls. This function attempts to ensure surfaces
/// are printed in the interior region defined by the innermost walls, while
/// maintaining a small configurable overlap for bonding.
///
/// # Arguments
/// * `layers` - Mutable reference to all layers
/// * `overlap_percent` - How much surfaces overlap into walls (0.0-1.0, e.g., 0.25 = 25%)
/// * `nozzle_diameter` - Nozzle diameter in mm, used to calculate overlap distance
#[allow(dead_code)] // Currently disabled, but kept for future implementation
fn trim_surfaces_to_walls(layers: &mut [SliceLayer], overlap_percent: f64, nozzle_diameter: f64) {
    // Calculate overlap as a distance in mm
    let overlap_distance = nozzle_diameter * overlap_percent;

    for layer in layers.iter_mut() {
        // Collect all wall paths (OuterWall and InnerWall).
        let wall_paths: Vec<Path> = layer
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                let role = layer.role_for_path(*i);
                role == ExtrusionRole::OuterWall || role == ExtrusionRole::InnerWall
            })
            .map(|(_, p)| p.clone())
            .collect();

        if wall_paths.is_empty() {
            // No walls, leave surfaces as-is
            continue;
        }

        // Create interior region by shrinking walls inward
        // The interior is where surfaces should be printed
        let walls = Paths::new(wall_paths);

        // Deflate (shrink) walls to create interior region
        // Use negative inflation to shrink inward
        // Shrink by (nozzle_diameter/2 - overlap_distance) to leave the overlap
        let shrink_amount = (nozzle_diameter / 2.0) - overlap_distance;
        let interior_region = if shrink_amount > 0.01 {
            // Shrink walls to define interior
            clipper2::inflate(
                walls,
                -shrink_amount * 100.0, // Negative = deflate, convert to Centi
                JoinType::Round,
                EndType::Polygon,
                2.0,
            )
        } else {
            // If shrink amount is too small, just use the walls as-is
            walls
        };

        if interior_region.is_empty() {
            // Walls collapsed completely, remove all surfaces
            let mut new_paths = Paths::new(vec![]);
            let mut new_roles = Vec::new();
            let mut new_widths = Vec::new();

            for (i, path) in layer.paths.iter().enumerate() {
                let role = layer.role_for_path(i);
                if role != ExtrusionRole::TopSurface && role != ExtrusionRole::BottomSurface {
                    // Keep non-surface paths
                    new_paths.push(path.clone());
                    new_roles.push(role);
                    new_widths.push(layer.width_for_path(i));
                }
            }

            layer.paths = new_paths;
            layer.path_roles = new_roles;
            layer.path_widths = new_widths;
            continue;
        }

        // Now intersect surface paths with the interior region
        let mut new_paths = Paths::new(vec![]);
        let mut new_roles = Vec::new();
        let mut new_widths = Vec::new();

        for (i, path) in layer.paths.iter().enumerate() {
            let role = layer.role_for_path(i);
            if role == ExtrusionRole::TopSurface || role == ExtrusionRole::BottomSurface {
                // Intersect this surface path with the interior region
                let path_as_paths = Paths::new(vec![path.clone()]);
                let trimmed = intersect(path_as_paths, interior_region.clone(), FillRule::EvenOdd)
                    .unwrap_or_default();

                // Add all resulting paths (may be split into multiple pieces).
                for p in trimmed.iter() {
                    new_paths.push(p.clone());
                    new_roles.push(role);
                    new_widths.push(layer.width_for_path(i));
                }
            } else {
                // Keep non-surface paths as-is (including walls).
                new_paths.push(path.clone());
                new_roles.push(role);
                new_widths.push(layer.width_for_path(i));
            }
        }

        layer.paths = new_paths;
        layer.path_roles = new_roles;
        layer.path_widths = new_widths;
    }
}

/// Apply single-wall restrictions to specific layers based on parameters.
///
/// This function modifies layers to use only the outer wall in specific cases:
/// 1. First layer (layer 0) if `only_one_wall_first_layer` is true.
/// 2. Last layer of every "top surface run" if `only_one_wall_top` is true,
///    where a run is a contiguous range of layers that have an exposed top
///    surface (i.e. are not fully covered by perimeters within `top_layers`
///    above them).
///
/// Inner walls are removed from these layers, leaving only outer walls.
///
/// # `has_top_surface`
///
/// Caller must pre-compute, per layer, whether a top surface will be drawn
/// there. This **must** be derived from perimeter geometry (see
/// [`compute_layers_with_top_surface`]), not from `path_roles`, because the
/// `TopSurface` role is only assigned later, after surfaces are generated.
/// Earlier versions used `path_roles.contains(&TopSurface)` and were a
/// permanent no-op for `only_one_wall_top`, leaving the topmost layer with
/// every wall and producing a visible inter-wall gap between the top surface
/// and the outer wall.
fn apply_single_wall_restrictions(
    layers: &mut [SliceLayer],
    params: &SlicingParams,
    has_top_surface: &[bool],
) {
    if layers.is_empty() {
        return;
    }

    // Process first layer restriction
    if params.only_one_wall_first_layer {
        remove_inner_walls_from_layer(&mut layers[0]);
    }

    // Process last-layer-of-each-top-surface-run restriction.
    if params.only_one_wall_top {
        debug_assert_eq!(
            has_top_surface.len(),
            layers.len(),
            "has_top_surface mask must align with layers"
        );

        let mut in_top_surface_run = false;
        let mut last_top_surface_idx = None;

        for (i, &has_top) in has_top_surface.iter().enumerate() {
            if has_top {
                in_top_surface_run = true;
                last_top_surface_idx = Some(i);
            } else if in_top_surface_run {
                if let Some(idx) = last_top_surface_idx {
                    remove_inner_walls_from_layer(&mut layers[idx]);
                }
                in_top_surface_run = false;
                last_top_surface_idx = None;
            }
        }

        // Handle the case where a top-surface run extends to the very last layer.
        if let Some(idx) = last_top_surface_idx {
            remove_inner_walls_from_layer(&mut layers[idx]);
        }
    }
}

/// Pre-compute, for every layer, whether it will receive a top surface.
///
/// Mirrors the geometric "covered above" test in
/// [`generate_top_bottom_surfaces_with_interior`] so that callers running
/// **before** surface generation (e.g. [`apply_single_wall_restrictions`])
/// can identify top-surface layers without inspecting `path_roles`.
///
/// A layer has a top surface iff its perimeter region is not fully covered
/// by the EvenOdd intersection of perimeters of the next `top_layers` layers
/// above it (or it sits within `top_layers` of the model top).
fn compute_layers_with_top_surface(layers: &[SliceLayer], top_layers: usize) -> Vec<bool> {
    if top_layers == 0 || layers.is_empty() {
        return vec![false; layers.len()];
    }

    let perimeters: Vec<Paths> = layers.iter().map(perimeter_paths_of).collect();
    let total = perimeters.len();

    (0..total)
        .map(|i| {
            // Same loop shape as the top branch of
            // generate_top_bottom_surfaces_with_interior so the two stay in
            // lock-step.
            let mut covered = perimeters[i].clone();
            for j in 1..=top_layers {
                if i + j >= total {
                    covered = Paths::new(vec![]);
                    break;
                }
                let neighbor = &perimeters[i + j];
                if neighbor.is_empty() {
                    covered = Paths::new(vec![]);
                    break;
                }
                covered =
                    intersect(covered, neighbor.clone(), FillRule::EvenOdd).unwrap_or_default();
                if covered.is_empty() {
                    break;
                }
            }
            let region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();
            !region.is_empty()
        })
        .collect()
}

/// Remove all inner walls from a layer, keeping only outer walls.
fn remove_inner_walls_from_layer(layer: &mut SliceLayer) {
    let mut new_paths = Paths::new(vec![]);
    let mut new_roles = Vec::new();
    let mut new_widths = Vec::new();

    for (i, path) in layer.paths.iter().enumerate() {
        let role = layer.role_for_path(i);
        // Keep everything except InnerWall
        if role != ExtrusionRole::InnerWall {
            new_paths.push(path.clone());
            new_roles.push(role);
            new_widths.push(layer.width_for_path(i));
        }
    }

    layer.paths = new_paths;
    layer.path_roles = new_roles;
    layer.path_widths = new_widths;
}

/// Calculate the interior region of a layer where solid surfaces should be
/// printed (i.e. the area enclosed by the **innermost** wall of every island,
/// shrunk slightly to leave the configured overlap between surface and wall).
///
/// # Why this is non-trivial with Arachne walls
///
/// Arachne emits each wall as a **centerline path** (not a filled polygon),
/// often with variable width per bead.  Inflating each centerline by `width/2`
/// with `EndType::Polygon` produces a *filled* polygon (Clipper2 treats the
/// closed centerline as the polygon boundary), and N concentric walls become
/// N nested filled polygons.
///
/// Two correctness pitfalls have to be addressed:
///
/// 1. **Fill rule.**  Unioning N nested polygons with `FillRule::EvenOdd`
///    produces an alternating in/out pattern: with 3 walls the band between
///    the outer and middle walls counts as "inside" (1 boundary crossing),
///    the band between middle and inner counts as "outside" (2), and the
///    central hole counts as "inside" (3).  Using that as the surface mask
///    therefore generates surfaces **between concentric walls** – exactly the
///    bug we are fixing.  We use `FillRule::NonZero` instead, where nested
///    polygons of the same winding combine into a single solid extent.
///
/// 2. **Winding consistency.**  The mesh slicer (`chain_segments`) does not
///    enforce a consistent winding direction for inner contours, and Arachne
///    inherits that.  With `NonZero` plus mixed CW/CCW windings, opposite
///    windings cancel and the union develops phantom holes between walls
///    again.  We therefore **normalise every wall path to CCW** (positive
///    signed area in Clipper2's convention) before unioning.
///
/// # Strategy
///
/// 1. Normalise every wall centerline to CCW.
/// 2. Inflate each by its own half-width with `JoinType::Round` /
///    `EndType::Polygon`, yielding a filled polygon per wall.
/// 3. Union them all with `FillRule::NonZero` so nested polygons combine into
///    a single solid outer extent (≈ the original sliced contour grown by
///    half the outer wall's width).
/// 4. Deflate that outer extent inward by the **total wall-band thickness**
///    (`walls_per_island × nozzle_diameter`) minus the configured overlap,
///    yielding the true interior hole inside the innermost wall.
///
/// `walls_per_island` is estimated as `ceil(total_walls / outer_wall_count)`
/// so it correctly reflects single-wall layers (e.g. when
/// `only_one_wall_first_layer` removes inner walls) and multi-island parts.
///
/// Returns an empty `Paths` when the interior collapses, signalling that
/// walls alone fill the cross-section and no surfaces are needed (the
/// "smart-skip" outcome).
fn calculate_interior_region(
    layer: &SliceLayer,
    overlap_percent: f64,
    nozzle_diameter: f64,
) -> Paths {
    // Collect (path, width, is_outer) for every wall in the layer, after
    // normalising winding to CCW (positive signed area).
    let walls: Vec<(Path, f64, bool)> = layer
        .paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let role = layer.role_for_path(i);
            match role {
                ExtrusionRole::OuterWall | ExtrusionRole::InnerWall => {
                    let width = layer.width_for_path(i).unwrap_or(nozzle_diameter);
                    let normalised = if p.signed_area() < 0.0 {
                        // Reverse to make CCW so subsequent NonZero union is
                        // consistent across all walls regardless of source
                        // winding.
                        Path::new(p.iter().copied().rev().collect())
                    } else {
                        p.clone()
                    };
                    Some((normalised, width, role == ExtrusionRole::OuterWall))
                }
                _ => None,
            }
        })
        .collect();

    if walls.is_empty() {
        return Paths::new(vec![]);
    }

    // Step 2+3: build the outer extent = NonZero union of every wall inflated
    // by half its bead width.  NonZero (with consistent CCW winding) makes
    // nested polygons combine into a single solid region instead of producing
    // alternating in/out bands.
    let mut outer_extent = Paths::new(vec![]);
    for (path, width, _) in &walls {
        let inflated = clipper2::inflate(
            Paths::new(vec![path.clone()]),
            width / 2.0,
            JoinType::Round,
            EndType::Polygon,
            2.0,
        );
        if inflated.is_empty() {
            continue;
        }
        outer_extent = if outer_extent.is_empty() {
            inflated
        } else {
            clipper2::union(outer_extent.clone(), inflated, FillRule::NonZero)
                .unwrap_or(outer_extent)
        };
    }

    if outer_extent.is_empty() {
        return Paths::new(vec![]);
    }

    // Estimate walls per island so we know how far inward to deflate.  Using
    // ceil(total / outer_count) handles parts with multiple OuterWalls
    // (multiple islands) and layers where inner walls were stripped (e.g. the
    // first layer with only_one_wall_first_layer = true) without overshoot.
    let outer_count = walls
        .iter()
        .filter(|(_, _, is_outer)| *is_outer)
        .count()
        .max(1);
    let walls_per_island = walls.len().div_ceil(outer_count);

    // Total inward distance from the outer extent to the inside of the
    // innermost wall ≈ walls_per_island × nozzle_diameter.  Subtract the
    // configured overlap so surfaces still bond to the innermost wall.
    let overlap_distance = nozzle_diameter * overlap_percent;
    let total_inward = (walls_per_island as f64) * nozzle_diameter - overlap_distance;

    if total_inward < 0.01 {
        // Pathological: nothing to deflate – return the outer extent as-is.
        return outer_extent;
    }

    // Empty result here is the correct "smart-skip" signal: walls fill the
    // entire cross-section and no surface should be generated.
    clipper2::inflate(
        outer_extent,
        -total_inward,
        JoinType::Round,
        EndType::Polygon,
        2.0,
    )
}

/// Extract only wall (perimeter) paths from a layer.
///
/// Used to snapshot slice contours before infill is added, so that surface
/// detection compares geometry, not previously-generated infill.
fn perimeter_paths_of(layer: &SliceLayer) -> Paths {
    Paths::new(
        layer
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                let role = layer.role_for_path(*i);
                role == ExtrusionRole::OuterWall || role == ExtrusionRole::InnerWall
            })
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
    let rotate_neg =
        |x: f64, y: f64| -> (f64, f64) { (x * cos_a + y * sin_a, -x * sin_a + y * cos_a) };
    // Rotate point (x, y) by +angle to recover the original coordinate system
    let rotate_pos =
        |x: f64, y: f64| -> (f64, f64) { (x * cos_a - y * sin_a, x * sin_a + y * cos_a) };

    // Collect rotated polygon vertices for every contour path
    let rotated_polys: Vec<Vec<(f64, f64)>> = contours
        .iter()
        .filter_map(|path| {
            let pts: Vec<(f64, f64)> = path.iter().map(|pt| rotate_neg(pt.x(), pt.y())).collect();
            if pts.len() >= 2 {
                Some(pts)
            } else {
                None
            }
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

    /// Regression test: with `only_one_wall_top = true`, the topmost layer
    /// of each top-surface run must be reduced to a single (outer) wall, and
    /// its TopSurface must extend out to the outer-wall edge — mirroring the
    /// behaviour of `only_one_wall_first_layer` on layer 0.
    ///
    /// The previous role-based detection in `apply_single_wall_restrictions`
    /// was a no-op (TopSurface roles are assigned later, after this runs),
    /// so the topmost layer kept all walls and the top surface was confined
    /// to a tiny disk inside the innermost wall — leaving a visible
    /// inter-wall gap that users perceived as the "between walls" bug
    /// persisting on top surfaces.
    #[test]
    fn test_only_one_wall_top_reduces_topmost_layer() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 0.0,
            only_one_wall_first_layer: true,
            only_one_wall_top: true,
            wall_count: 3,
            nozzle_diameter_mm: 0.4,
            infill_overlap_percent: 0.25,
            ..SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);
        assert!(!layers.is_empty(), "expected sliced layers");

        let last = layers.len() - 1;
        let n_outer_top = layers[last]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::OuterWall)
            .count();
        let n_inner_top = layers[last]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::InnerWall)
            .count();
        assert!(n_outer_top >= 1, "topmost layer must keep its outer wall");
        assert_eq!(
            n_inner_top, 0,
            "only_one_wall_top should strip all InnerWall paths from the topmost \
             layer of a top-surface run, but {n_inner_top} remain"
        );

        // The layer below the topmost is also part of the top-surface run
        // (top_layers = 2) but is NOT the last layer of the run — it must
        // keep its inner walls.
        let n_inner_below_top = layers[last - 1]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::InnerWall)
            .count();
        assert!(
            n_inner_below_top > 0,
            "only_one_wall_top must NOT strip inner walls from layers in the \
             middle of a top-surface run (only the very topmost)"
        );

        // With only the outer wall remaining on the topmost layer, the
        // TopSurface should now extend to the outer-wall edge (within the
        // configured overlap), exactly mirroring the BottomSurface AABB on
        // layer 0 where only_one_wall_first_layer has the same effect.
        let top_pts: Vec<(f64, f64)> = layers[last]
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layers[last].role_for_path(*i) == ExtrusionRole::TopSurface)
            .flat_map(|(_, p)| p.iter().map(|pt| (pt.x(), pt.y())).collect::<Vec<_>>())
            .collect();
        assert!(
            !top_pts.is_empty(),
            "topmost layer should have TopSurface paths"
        );
        let xmax = top_pts
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let xmin = top_pts
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::INFINITY, f64::min);
        // Cube spans [0, 10]. With 1 wall (centerline ~0.2mm in) plus 25%
        // overlap, the surface should reach within ~0.5mm of each edge.
        // The buggy 3-wall behaviour confined it to ~[1.1, 8.9] (≥1.1mm from
        // each edge), so this threshold reliably separates fixed vs broken.
        assert!(
            xmax >= 9.5 && xmin <= 0.5,
            "top surface should extend close to the outer wall edge \
             (got xmin={xmin:.2}, xmax={xmax:.2}); the buggy multi-wall behaviour \
             would confine it to ~[1.1, 8.9]"
        );
    }

    #[test]
    fn test_slice_layer_role_for_path_default() {
        let layer = SliceLayer::new(1.0);
        // No roles set → should fall back to OuterWall
        assert_eq!(layer.role_for_path(0), ExtrusionRole::OuterWall);
        assert_eq!(layer.role_for_path(99), ExtrusionRole::OuterWall);
    }

    #[test]
    fn test_slice_layer_role_for_path_explicit() {
        let mut layer = SliceLayer::new(1.0);
        layer.path_roles.push(ExtrusionRole::Skirt);
        layer.path_roles.push(ExtrusionRole::Infill);
        assert_eq!(layer.role_for_path(0), ExtrusionRole::Skirt);
        assert_eq!(layer.role_for_path(1), ExtrusionRole::Infill);
        // Out of bounds → OuterWall default
        assert_eq!(layer.role_for_path(2), ExtrusionRole::OuterWall);
    }

    #[test]
    fn test_extrusion_role_type_names() {
        assert_eq!(ExtrusionRole::OuterWall.type_name(), "Outer wall");
        assert_eq!(ExtrusionRole::InnerWall.type_name(), "Inner wall");
        assert_eq!(ExtrusionRole::Infill.type_name(), "Sparse infill");
        assert_eq!(ExtrusionRole::Bridge.type_name(), "Bridge");
        assert_eq!(ExtrusionRole::TopSurface.type_name(), "Top surface");
        assert_eq!(ExtrusionRole::BottomSurface.type_name(), "Bottom surface");
        assert_eq!(ExtrusionRole::Support.type_name(), "Support material");
        assert_eq!(ExtrusionRole::Skirt.type_name(), "Skirt");
    }

    #[test]
    fn test_extrusion_role_widths_positive() {
        for role in [
            ExtrusionRole::OuterWall,
            ExtrusionRole::InnerWall,
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
                    ExtrusionRole::OuterWall,
                    "slice_mesh assigns OuterWall"
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
    fn test_add_infill_to_layers_basic() {
        use crate::infill::InfillPattern;
        
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        
        // Before infill: only wall paths
        for layer in &layers {
            for role in &layer.path_roles {
                assert!(
                    *role == ExtrusionRole::OuterWall || *role == ExtrusionRole::InnerWall,
                    "Expected wall role, got {:?}",
                    role
                );
            }
        }
        
        // Add infill
        add_infill_to_layers(&mut layers, 0.2, InfillPattern::Rectilinear, 45.0);
        
        // After infill: should have both wall and infill paths
        for layer in &layers {
            let has_walls = layer
                .path_roles
                .iter()
                .any(|r| *r == ExtrusionRole::OuterWall || *r == ExtrusionRole::InnerWall);
            let has_infill = layer.path_roles.contains(&ExtrusionRole::Infill);
            assert!(has_walls, "Layer at z={} missing walls", layer.z);
            assert!(has_infill, "Layer at z={} missing infill", layer.z);
        }
    }

    #[test]
    fn test_add_infill_to_layers_zero_density() {
        use crate::infill::InfillPattern;
        
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let initial_path_count: usize = layers.iter().map(|l| l.paths.len()).sum();
        
        // Add zero-density infill (should do nothing)
        add_infill_to_layers(&mut layers, 0.0, InfillPattern::Rectilinear, 45.0);
        
        let final_path_count: usize = layers.iter().map(|l| l.paths.len()).sum();
        assert_eq!(initial_path_count, final_path_count, "Zero density should not add paths");
    }

    #[test]
    fn test_add_infill_to_layers_grid_pattern() {
        use crate::infill::InfillPattern;
        
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        
        // Add grid infill
        add_infill_to_layers(&mut layers, 0.3, InfillPattern::Grid, 45.0);
        
        // Grid pattern should produce more infill paths than rectilinear
        for layer in &layers {
            let infill_count = layer.path_roles.iter().filter(|r| **r == ExtrusionRole::Infill).count();
            assert!(infill_count > 0, "Layer at z={} has no infill paths", layer.z);
        }
    }

    #[test]
    fn test_infill_not_placed_on_fully_solid_surface_layers() {
        use crate::infill::InfillPattern;

        // A 2-layer cube: with 2 top + 2 bottom layers, every layer is a
        // surface layer — sparse infill should not be added on top of solid surfaces.
        let mesh = make_cube_mesh();
        // 5 layers at 2mm.  Use top=2/bottom=2 so the first two and last two
        // layers are fully solid surfaces; the middle layer is interior.
        let mut layers = slice_mesh(&mesh, 2.0);
        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // Confirm solid_regions are populated for the top/bottom layers.
        let n = layers.len();
        assert!(!layers[0].solid_regions.is_empty(), "Layer 0 should have solid_regions");
        assert!(!layers[n - 1].solid_regions.is_empty(), "Last layer should have solid_regions");

        // Count surface-only infill paths before adding sparse infill.
        let surface_counts: Vec<usize> = layers
            .iter()
            .map(|l| {
                l.path_roles
                    .iter()
                    .filter(|r| **r == ExtrusionRole::TopSurface || **r == ExtrusionRole::BottomSurface)
                    .count()
            })
            .collect();

        // Now add sparse infill.
        add_infill_to_layers(&mut layers, 0.3, InfillPattern::Rectilinear, 45.0);

        // For a layer that is entirely solid (solid_regions == perimeter area),
        // no new Infill paths should have been added.
        // Layers 0 and n-1 are entirely solid surfaces on a simple cube.
        for i in [0, n - 1] {
            let infill_added = layers[i]
                .path_roles
                .iter()
                .filter(|r| **r == ExtrusionRole::Infill)
                .count();
            assert_eq!(
                infill_added, 0,
                "Layer {} (fully solid surface) should not have sparse infill (got {})",
                i, infill_added
            );
            // Surface paths must remain unchanged.
            let surface_now = layers[i]
                .path_roles
                .iter()
                .filter(|r| **r == ExtrusionRole::TopSurface || **r == ExtrusionRole::BottomSurface)
                .count();
            assert_eq!(
                surface_now, surface_counts[i],
                "Surface path count on layer {} must not change",
                i
            );
        }
    }

    #[test]
    fn test_solid_regions_populated_by_surface_generation() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);

        // Initially no solid regions.
        for layer in &layers {
            assert!(layer.solid_regions.is_empty());
        }

        generate_top_bottom_surfaces(&mut layers, 3, 3, 2.0, 45.0);

        // After surface generation the topmost and bottommost layers must have
        // non-empty solid_regions.
        let n = layers.len();
        assert!(
            !layers[0].solid_regions.is_empty(),
            "Bottom layer should have solid_regions after surface generation"
        );
        assert!(
            !layers[n - 1].solid_regions.is_empty(),
            "Top layer should have solid_regions after surface generation"
        );
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
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
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
            // Use old defaults for this test to verify basic functionality
            only_one_wall_first_layer: false,
            only_one_wall_top: false,
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

    /// Regression test for the "surfaces between walls" bug.
    ///
    /// Slices a 10×10×10 mm cube with the default 3 walls and verifies that
    /// every BottomSurface coordinate on layer 0 is well inside the innermost
    /// wall, i.e. no surface line is drawn in the band between concentric
    /// walls.  The previous EvenOdd-based interior calculation produced
    /// surfaces in that band on multi-wall layers; this test guards against
    /// that regression.
    #[test]
    fn test_smart_surface_skipping_no_between_walls_artifacts() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 0.0,
            // Disable single-wall restrictions so all layers carry the full
            // multi-wall stack – this is the configuration that triggered the
            // original bug.
            only_one_wall_first_layer: false,
            only_one_wall_top: false,
            // Explicit defaults to make the geometry expectations precise.
            wall_count: 3,
            nozzle_diameter_mm: 0.4,
            infill_overlap_percent: 0.25,
            ..SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);
        assert!(!layers.is_empty(), "expected sliced layers");

        // Cube is at [0, 10]² in XY.  With 3 × 0.4 mm walls the innermost
        // wall centerline sits ~1.0 mm from each edge, so its inner bound is
        // ~1.2 mm.  The 25 % overlap (= 0.1 mm) lets surfaces extend back
        // out to ~1.1 mm.  Any surface point closer than 0.5 mm to an edge
        // would lie in the inter-wall band and is the bug we are guarding.
        const SAFE_MARGIN_MM: f64 = 0.5;

        let mut total_surface_points = 0;
        for layer in &layers {
            for (i, path) in layer.paths.iter().enumerate() {
                let role = layer.role_for_path(i);
                if role != ExtrusionRole::BottomSurface && role != ExtrusionRole::TopSurface {
                    continue;
                }
                for pt in path.iter() {
                    total_surface_points += 1;
                    let (x, y) = (pt.x(), pt.y());
                    assert!(
                        (SAFE_MARGIN_MM..=10.0 - SAFE_MARGIN_MM).contains(&x)
                            && (SAFE_MARGIN_MM..=10.0 - SAFE_MARGIN_MM).contains(&y),
                        "surface point ({x}, {y}) lies in the inter-wall band on \
                         layer z={} (role={:?}) – smart surface skipping regressed",
                        layer.z,
                        role,
                    );
                }
            }
        }

        assert!(
            total_surface_points > 0,
            "expected some surface paths; smart-skip should not skip cube top/bottom"
        );
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
            layer.path_roles.push(ExtrusionRole::OuterWall);
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
            layer.path_roles.push(ExtrusionRole::OuterWall);
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

    /// Test that surface generation correctly handles holes (inner contours).
    /// When a layer has a hole, the surface infill should not fill the hole.
    ///
    /// Critically, this test uses the **same winding order** for both outer
    /// and hole contours — the exact case that `FillRule::NonZero` gets wrong
    /// (it treats the hole as doubly-wound solid material).  `EvenOdd` handles
    /// it correctly regardless of winding direction.
    #[test]
    fn test_surface_generation_with_holes() {
        use clipper2::Path;

        // Create a layer with an outer square and an inner square (hole).
        // Both contours use the same winding order (right → up → left → down),
        // which is the problematic case for NonZero but handled correctly by EvenOdd.
        let mut layer = SliceLayer::new(1.0);

        // Outer square 10x10 (same winding as inner = the hard case)
        let outer: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();

        // Inner square 4x4 (same winding as outer — NonZero would treat this as solid)
        let hole: Path = vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)].into();

        layer.paths.push(outer);
        layer.path_roles.push(ExtrusionRole::OuterWall);
        layer.paths.push(hole);
        layer.path_roles.push(ExtrusionRole::OuterWall);

        // Create a simple 1-layer setup
        let mut layers = vec![layer];

        // Generate bottom surfaces (first layer, no layers below)
        generate_top_bottom_surfaces(&mut layers, 0, 1, 1.0, 45.0);

        // Count the surface infill paths
        let surface_path_count = layers[0]
            .path_roles
            .iter()
            .filter(|&&r| r == ExtrusionRole::BottomSurface)
            .count();

        // There should be surface paths
        assert!(
            surface_path_count > 0,
            "Should have generated bottom surface infill"
        );

        // Collect all bottom surface path segments
        let surface_paths: Vec<&Path> = layers[0]
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layers[0].role_for_path(*i) == ExtrusionRole::BottomSurface)
            .map(|(_, p)| p)
            .collect();

        println!("Generated {} bottom surface paths", surface_paths.len());

        // Check if any surface path segments pass through the hole region
        // The hole is at (3,3) to (7,7).
        // With EvenOdd fill rule, holes are correctly excluded regardless of
        // the winding order of the contours, so no infill should be inside.
        let mut paths_in_hole = 0;
        for path in &surface_paths {
            for pt in path.iter() {
                let x = pt.x();
                let y = pt.y();
                // Check if point is inside the hole region (with small margin)
                if x > 3.5 && x < 6.5 && y > 3.5 && y < 6.5 {
                    paths_in_hole += 1;
                    break; // Count each path only once
                }
            }
        }

        assert_eq!(
            paths_in_hole, 0,
            "Surface infill must not penetrate the hole region (found {} paths inside hole). \
             EvenOdd fill rule should handle this regardless of contour winding order.",
            paths_in_hole
        );
    }
}
