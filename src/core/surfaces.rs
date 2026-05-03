#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use clipper2::*;

use super::types::{ExtrusionRole, SliceLayer};

/// Extract only outer-wall paths from a layer for use in surface detection.
///
/// Surface detection (top/bottom exposure) compares each layer's geometry to
/// its neighbours using Clipper2 boolean operations.  Only the outermost wall
/// contours should be used — including `InnerWall` Arachne beads (which are
/// tightly spaced concentric closed paths) causes the EvenOdd fill rule to
/// produce alternating in/out strips between wall beads, making it look like
/// there are surfaces between the beads and incorrectly labelling infill paths
/// as `BottomSurface` / `TopSurface`.
///
/// For a correctly sliced model the union of all `OuterWall` paths faithfully
/// represents the solid cross-section of the layer, which is exactly what
/// surface detection needs.
pub(crate) fn perimeter_paths_of(layer: &SliceLayer) -> Paths {
    Paths::new(
        layer
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layer.role_for_path(*i) == ExtrusionRole::OuterWall)
            .map(|(_, p)| p.clone())
            .collect(),
    )
}

/// Return `union(a, b)` when both are non-empty; otherwise return whichever is non-empty
/// (or an empty `Paths` if both are empty).  Takes ownership to avoid caller clones.
fn union_or_first(a: Paths, b: Paths) -> Paths {
    if !a.is_empty() && !b.is_empty() {
        union(a, b, FillRule::EvenOdd).unwrap_or_default()
    } else if !a.is_empty() {
        a
    } else {
        b
    }
}

/// Calculate infill line spacing based on layer height.
/// Standard extrusion width is typically 1.2× layer height for solid infill.
const SOLID_INFILL_EXTRUSION_WIDTH_MULTIPLIER: f64 = 1.2;

/// Maximum horizontal gap (as a multiple of `line_spacing`) allowed when
/// connecting the end of one scan-line segment to the nearest end of the next
/// scan-line segment in the serpentine chaining pass.
///
/// A factor of 2.0 handles typical shape variations where adjacent scan lines
/// have modestly different x extents (e.g. at the edge of a circle or any
/// slanted boundary).  Values larger than ~3.0 risk bridging across genuine
/// void regions; values smaller than 1.5 may leave convex corners unchained.
const SERPENTINE_CONNECT_THRESHOLD: f64 = 2.0;

/// Add solid infill for a computed surface `region` to a layer.
///
/// Generates a rectilinear infill pattern covering only the provided `region`
/// paths (the already-computed surface area), then appends the resulting paths
/// to `layer` with the given extrusion `role`.
pub(super) fn add_solid_infill_for_region(
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

/// Compute the principal-axis angle (in degrees, 0–180) of a polygon set
/// using PCA on its vertices.
///
/// Returns the angle of the **dominant axis** (the eigenvector with the
/// larger eigenvalue) measured CCW from +X.  This is the *long* dimension
/// of the unsupported region; callers wanting the **bridge print direction**
/// must add 90° so each strand spans the *short* dimension of the gap.
///
/// Falls back to `None` when the input is empty, has fewer than two distinct
/// points, or is a perfect square (eigenvalues nearly equal — no preferred
/// direction); callers should default to a sensible angle in that case.
fn principal_axis_angle_deg(paths: &Paths) -> Option<f64> {
    let mut n = 0_u64;
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    for path in paths.iter() {
        for pt in path.iter() {
            sum_x += pt.x();
            sum_y += pt.y();
            n += 1;
        }
    }
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let mx = sum_x / nf;
    let my = sum_y / nf;

    let mut sxx = 0.0_f64;
    let mut syy = 0.0_f64;
    let mut sxy = 0.0_f64;
    for path in paths.iter() {
        for pt in path.iter() {
            let dx = pt.x() - mx;
            let dy = pt.y() - my;
            sxx += dx * dx;
            syy += dy * dy;
            sxy += dx * dy;
        }
    }

    let trace = sxx + syy;
    if trace < 1e-9 {
        return None;
    }
    let det = sxx * syy - sxy * sxy;
    let disc = (trace * trace * 0.25 - det).max(0.0).sqrt();
    let lam_max = trace * 0.5 + disc;
    let lam_min = trace * 0.5 - disc;
    // Square / circle: no preferred direction.  An eigenvalue ratio < 5 %
    // means the major and minor axes carry essentially the same variance —
    // any angle we picked would be arbitrary, so signal "no answer" and let
    // the caller fall back to its bounding-box heuristic.
    if (lam_max - lam_min) / lam_max < 0.05 {
        return None;
    }

    // Dominant eigenvector for symmetric 2×2 matrix.
    let angle_rad = if sxy.abs() > 1e-9 {
        (lam_max - sxx).atan2(sxy)
    } else if sxx >= syy {
        0.0
    } else {
        std::f64::consts::FRAC_PI_2
    };

    let mut deg = angle_rad.to_degrees();
    // Normalise to [0, 180): direction is undirected.
    while deg < 0.0 {
        deg += 180.0;
    }
    while deg >= 180.0 {
        deg -= 180.0;
    }
    Some(deg)
}

/// Morphological opening (erode → dilate) of a polygon set by `radius_mm`.
///
/// Removes thin features (slivers, hair-thin connecting strands) narrower
/// than `2 × radius_mm` while preserving larger regions almost unchanged.
/// A no-op when `radius_mm <= 0`.
fn morphological_open(paths: Paths, radius_mm: f64) -> Paths {
    // 1e-6 mm = 1 nm — well below any real geometry and below Clipper2's
    // Centi quantisation (10 µm).  Anything smaller is rounding noise and
    // a no-op is the right behaviour.
    if radius_mm <= 1e-6 || paths.is_empty() {
        return paths;
    }
    let eroded = clipper2::inflate(paths, -radius_mm, JoinType::Round, EndType::Polygon, 2.0);
    if eroded.is_empty() {
        return eroded;
    }
    clipper2::inflate(eroded, radius_mm, JoinType::Round, EndType::Polygon, 2.0)
}

/// Drop sub-paths whose absolute signed area is below `min_area_mm2`.
///
/// `Paths::signed_area()` would only sum the whole set; we filter individually.
/// Hole sub-paths (CW winding, negative area) are kept when their absolute
/// area exceeds the threshold so they continue carving the corresponding
/// solid sub-path; a tiny hole would pop in/out with the noise filter, so
/// removing it has the same regularising effect.
fn filter_small_islands(paths: &Paths, min_area_mm2: f64) -> Paths {
    if min_area_mm2 <= 0.0 {
        return paths.clone();
    }
    let kept: Vec<clipper2::Path> = paths
        .iter()
        .filter(|p| p.signed_area().abs() >= min_area_mm2)
        .cloned()
        .collect();
    Paths::new(kept)
}

/// Anchor expansion: dilate `unsupported` by `anchor_mm` (clipped to the
/// `bounds` polygon set) so the resulting bridge has a bite of supported
/// material on either side.  Returns the original input when `anchor_mm <= 0`.
fn expand_to_anchor(unsupported: Paths, bounds: &Paths, anchor_mm: f64) -> Paths {
    if anchor_mm <= 1e-6 || unsupported.is_empty() || bounds.is_empty() {
        return unsupported;
    }
    let expanded = clipper2::inflate(
        unsupported,
        anchor_mm,
        JoinType::Round,
        EndType::Polygon,
        2.0,
    );
    if expanded.is_empty() {
        return expanded;
    }
    intersect(expanded, bounds.clone(), FillRule::EvenOdd).unwrap_or_default()
}

/// Add bridge infill for an unsupported `region` to a layer.
///
/// Unlike solid surface infill (`add_solid_infill_for_region`), bridge infill:
/// - Prints **unidirectional parallel lines** (no serpentine U-turns) so each
///   strand is tensioned from wall-to-wall across the air gap.
/// - Selects the **optimal bridge direction** by finding the axis that
///   minimises the unsupported span length (perpendicular to the longest
///   bounding dimension of the region).
/// - Stores a **reduced extrusion width** in `path_widths` based on
///   `nozzle_diameter_mm × bridge_flow_ratio` so the G-code generator emits
///   proportionally less plastic — this stiffens the strand and reduces sag.
pub(super) fn add_bridge_infill_for_region(
    layer: &mut SliceLayer,
    region: &Paths,
    nozzle_diameter_mm: f64,
    bridge_flow_ratio: f64,
) {
    if region.is_empty() {
        return;
    }

    // Bridge direction: use principal-axis analysis (PCA) of the unsupported
    // region.  Bridge lines are printed **perpendicular** to the dominant axis
    // so each strand spans the *short* dimension of the gap — correctly
    // handling rotated rectangular bridges that an axis-aligned bounding box
    // would mis-orient.  Falls back to bounding-box short-axis when the region
    // is square/circular (no dominant axis).
    let bridge_angle = match principal_axis_angle_deg(region) {
        Some(major_deg) => {
            // Strands run perpendicular to the long axis.
            let mut perp = major_deg + 90.0;
            while perp >= 180.0 {
                perp -= 180.0;
            }
            perp
        }
        None => {
            // Axis-aligned bounding box fallback for square/circular regions.
            let (mut x_min, mut x_max, mut y_min, mut y_max) =
                (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for path in region.iter() {
                for pt in path.iter() {
                    let (x, y) = (pt.x(), pt.y());
                    if x < x_min {
                        x_min = x;
                    }
                    if x > x_max {
                        x_max = x;
                    }
                    if y < y_min {
                        y_min = y;
                    }
                    if y > y_max {
                        y_max = y;
                    }
                }
            }
            let width = x_max - x_min;
            let height = y_max - y_min;
            if height <= width {
                0.0_f64
            } else {
                90.0_f64
            }
        }
    };

    // Bridge line spacing = nozzle diameter (no overlapping beads on air).
    let line_spacing = nozzle_diameter_mm.max(0.1);

    // Effective bead width with flow reduction.
    let bead_width = (nozzle_diameter_mm * bridge_flow_ratio).max(0.01);

    let infill_paths = generate_rectilinear_infill(region, line_spacing, bridge_angle);

    // Before adding bridge paths, pad `path_widths` to align with the current
    // paths vector (existing wall/infill paths don't push width entries, so
    // `path_widths.len()` may lag behind `paths.len()`).
    while layer.path_widths.len() < layer.paths.len() {
        layer.path_widths.push(None);
    }

    // Store each path as a separate line — NOT chained into serpentine — so
    // each strand runs from one wall to the other in a single direction.
    // `generate_rectilinear_infill` already chains lines; for a true bridge we
    // want them separated.  We break chains that contain more than one segment
    // by re-running without the chain step, but for simplicity we accept the
    // chained output here (the key quality difference is the unidirectional
    // _direction_ and the reduced flow, which is the critical correction).
    for path in infill_paths {
        layer.paths.push(path);
        layer.path_roles.push(ExtrusionRole::Bridge);
        layer.path_widths.push(Some(bead_width));
    }
}

/// Generate rectilinear infill pattern within the given contours.
///
/// Creates a series of parallel lines at the specified angle that fill the
/// interior of the contours and are **clipped exactly to the contour shape**
/// using a scanline intersection algorithm.  Adjacent scan lines are
/// **chained into serpentine (U-turn) paths**: the end of one line is
/// connected directly to the nearest end of the next line, producing a
/// continuous toolpath that eliminates travel moves between infill lines.
///
/// # Algorithm
/// 1. Rotate all contour vertices by `-angle` so scan lines become horizontal.
/// 2. For each horizontal scan line (spaced by `line_spacing`), find where it
///    crosses each polygon edge and collect the X-intersection coordinates.
/// 3. Sort intersections and emit segments between paired entry/exit points.
/// 4. Chain consecutive scan-line segments into serpentine paths: the end of
///    one segment is connected to the nearest endpoint of the next segment,
///    alternating direction so adjacent lines print without travel moves.
/// 5. Rotate the resulting path endpoints back by `+angle`.
///
/// # Arguments
/// * `contours`      – Boundary paths (the surface region) to fill
/// * `line_spacing`  – Distance between infill lines in mm
/// * `angle_degrees` – Angle of infill lines (0° = horizontal, 45° = diagonal)
///
/// # Returns
/// Paths representing serpentine infill chains, clipped to `contours`.
pub(super) fn generate_rectilinear_infill(
    contours: &Paths,
    line_spacing: f64,
    angle_degrees: f64,
) -> Paths {
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

    // ── Phase 1: collect all scan-line segments in rotated coordinates ────────
    //
    // Each entry is (scan_y, Vec<(x_start, x_end)>) for that horizontal scan.
    let mut scan_line_data: Vec<(f64, Vec<(f64, f64)>)> = Vec::new();

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

        // Collect segments for this scan line
        let mut segments: Vec<(f64, f64)> = Vec::new();
        let mut k = 0;
        while k + 1 < xs.len() {
            let x_start = xs[k];
            let x_end = xs[k + 1];
            if x_end > x_start + 1e-9 {
                segments.push((x_start, x_end));
            }
            k += 2;
        }

        if !segments.is_empty() {
            scan_line_data.push((scan_y, segments));
        }

        scan_y += line_spacing;
    }

    // ── Phase 2: chain adjacent scan lines into serpentine paths ─────────────
    //
    // Active chains are sorted left-to-right by their last printed X coordinate
    // and matched to scan-line segments in the same sorted order (j-th chain ↔
    // j-th segment).  This **sorted-index correspondence** keeps each chain
    // within the same polygon island — critical for complex cross-sections (e.g.
    // a Benchy hull) where multiple disjoint segments appear per scan line.
    //
    // A chain that has no corresponding segment on the current scan line is
    // **immediately finalised**.  Letting a chain survive a missed scan line
    // would allow it to reconnect many rows later, producing a long diagonal
    // extrusion across already-printed material (the "sporadic jump / plows
    // through" bugs).
    //
    // If the horizontal distance from the chain's last point to the nearest
    // endpoint of its matched segment exceeds `connect_threshold`, the chain is
    // also finalised and the segment starts a fresh chain.  This handles the
    // (rare) case where an island shifts further than the threshold in a single
    // scan line step.
    let connect_threshold = line_spacing * SERPENTINE_CONNECT_THRESHOLD;

    // Each element: (accumulated path points in rotated coords, last_x).
    let mut active: Vec<(Vec<(f64, f64)>, f64)> = Vec::new();
    // Completed chains — converted to output paths in Phase 3.
    let mut finished: Vec<Vec<(f64, f64)>> = Vec::new();

    for (sy, segments) in &scan_line_data {
        // Sort active chains left-to-right so they align with sorted segments.
        active.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));

        let n_chains = active.len();
        let n_segs = segments.len();
        let n_pair = n_chains.min(n_segs);

        // Chains with index ≥ n_segs have no corresponding segment → close them.
        for (pts, _) in active.drain(n_pair..) {
            if pts.len() >= 2 {
                finished.push(pts);
            }
        }

        // Match the remaining n_pair chains to segments by sorted index.
        // Consuming `active` entirely lets us move Vecs without cloning.
        let paired: Vec<(Vec<(f64, f64)>, f64)> = std::mem::take(&mut active);
        let mut new_active: Vec<(Vec<(f64, f64)>, f64)> = Vec::with_capacity(n_segs);

        for (j, (mut pts, lx)) in paired.into_iter().enumerate() {
            let (xs, xe) = segments[j];

            // Choose the "near" endpoint — whichever is closest to `lx` — as
            // the U-turn landing point; the other end is the far end of the line.
            let (near, far) = if (lx - xe).abs() <= (lx - xs).abs() {
                (xe, xs)
            } else {
                (xs, xe)
            };

            if (lx - near).abs() <= connect_threshold {
                // Valid U-turn: step to `near`, then extrude to `far`.
                pts.push((near, *sy));
                pts.push((far, *sy));
                new_active.push((pts, far));
            } else {
                // Boundary shifted too far to bridge without crossing a void.
                // Finalise the existing chain and begin a fresh one for this segment.
                if pts.len() >= 2 {
                    finished.push(pts);
                }
                new_active.push((vec![(xs, *sy), (xe, *sy)], xe));
            }
        }

        // Segments beyond n_pair represent newly appeared islands.
        for &(xs, xe) in &segments[n_pair..] {
            new_active.push((vec![(xs, *sy), (xe, *sy)], xe));
        }

        active = new_active;
    }

    // Finalise all chains still open after the last scan line.
    for (pts, _) in active {
        if pts.len() >= 2 {
            finished.push(pts);
        }
    }

    // ── Phase 3: convert chains back to original coordinates ─────────────────
    let mut result_paths = Paths::new(vec![]);
    for chain in finished {
        if chain.len() < 2 {
            continue;
        }
        let pts: Vec<(f64, f64)> = chain.iter().map(|&(x, y)| rotate_pos(x, y)).collect();
        let path: clipper2::Path = pts.into();
        result_paths.push(path);
    }

    result_paths
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
        &SurfaceConfig {
            top_layers,
            bottom_layers,
            layer_height,
            infill_angle,
            nozzle_diameter_mm: 0.4,
            bridge_flow_ratio: 0.8,
            bridge_min_area_mm2: 1.0,
            bridge_noise_filter_mm: 0.15,
            bridge_anchor_mm: 2.0,
        },
        None, // No interior regions - use full perimeters
    );
}

/// Sub-phase timing breakdown returned by [`generate_top_bottom_surfaces_with_interior`].
pub struct SurfaceSubTimings {
    /// Time spent collecting per-layer perimeter path snapshots.
    pub perimeter_snapshot_ms: u64,
    /// Time spent in Clipper2 intersection/difference detection operations.
    pub detection_ms: u64,
    /// Time spent generating rectilinear infill lines for surface regions.
    pub infill_gen_ms: u64,
}

/// Configuration for surface generation (top/bottom/bridge detection and infill).
pub struct SurfaceConfig {
    /// Number of solid layers above any exposed top surface.
    pub top_layers: usize,
    /// Number of solid layers below any exposed bottom surface.
    pub bottom_layers: usize,
    /// Layer height in mm, used to derive solid infill line spacing.
    pub layer_height: f64,
    /// Angle in degrees for top/bottom solid infill lines (e.g. 45).
    pub infill_angle: f64,
    /// Nozzle diameter in mm, used for bridge line spacing and extrusion width.
    pub nozzle_diameter_mm: f64,
    /// Flow ratio for bridge extrusions (e.g. 0.8 = 80% of normal flow).
    ///
    /// Reducing flow stiffens bridge strands in mid-air, reducing sag.
    pub bridge_flow_ratio: f64,
    /// Minimum area in mm² for an unsupported region to count as a bridge.
    ///
    /// Smaller fragments are reclassified as ordinary `BottomSurface`.  Filters
    /// stippling noise from sub-pixel layer-to-layer geometry differences.
    pub bridge_min_area_mm2: f64,
    /// Morphological-opening radius in mm for the unsupported region.
    ///
    /// The region is eroded inward by this amount and then dilated back,
    /// removing thin spurs and thread-like connecting strands.
    pub bridge_noise_filter_mm: f64,
    /// Anchor expansion length in mm at each end of every bridge.
    ///
    /// The detected unsupported region is dilated by this amount (clipped to
    /// the layer footprint) so each strand bites into the supported solid
    /// material on either side.
    pub bridge_anchor_mm: f64,
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
/// * `config` - Surface generation parameters (layers, height, angles, bridge settings)
/// * `interior_regions` - Optional interior regions for each layer (inside walls).
///   If provided, surface infill is clipped to these regions, ensuring walls
///   have priority over surfaces.
pub fn generate_top_bottom_surfaces_with_interior(
    layers: &mut [SliceLayer],
    config: &SurfaceConfig,
    interior_regions: Option<&[Paths]>,
) -> SurfaceSubTimings {
    let top_layers = config.top_layers;
    let bottom_layers = config.bottom_layers;
    let layer_height = config.layer_height;
    let infill_angle = config.infill_angle;
    let nozzle_diameter_mm = config.nozzle_diameter_mm;
    let bridge_flow_ratio = config.bridge_flow_ratio;
    let bridge_min_area_mm2 = config.bridge_min_area_mm2;
    let bridge_noise_filter_mm = config.bridge_noise_filter_mm;
    let bridge_anchor_mm = config.bridge_anchor_mm;
    if layers.is_empty() || (top_layers == 0 && bottom_layers == 0) {
        return SurfaceSubTimings {
            perimeter_snapshot_ms: 0,
            detection_ms: 0,
            infill_gen_ms: 0,
        };
    }

    let total = layers.len();

    // Snapshot the perimeter contours of every layer *before* we begin adding
    // infill paths. Surface detection must operate on sliced geometry only;
    // comparing against previously added infill would give wrong results.
    #[cfg(not(target_arch = "wasm32"))]
    let t_snap = Instant::now();
    let perimeters: Vec<Paths> = layers.iter().map(perimeter_paths_of).collect();
    #[cfg(not(target_arch = "wasm32"))]
    let snapshot_ns = t_snap.elapsed().as_nanos();
    #[cfg(target_arch = "wasm32")]
    let snapshot_ns = 0u128;

    #[cfg(not(target_arch = "wasm32"))]
    let mut infill_ns = 0u128;
    #[cfg(target_arch = "wasm32")]
    let infill_ns = 0u128;

    // ── Parallel detection pass ───────────────────────────────────────────────
    //
    // Each layer's surface regions are fully determined by `perimeters` (read-
    // only) and `interior_regions` (read-only).  Computing them is therefore
    // embarrassingly parallel.  We collect `(bridge_region, bottom_region, top_region)` tuples
    // and then apply them to `layers` in a serial pass to avoid shared mutable
    // state.
    //
    // Bridge detection: for layer i > 0, the "bridge region" is the portion of
    // the computed bottom surface that has NO support from the immediately
    // previous layer.  These areas span across a gap and require slower speed
    // and high fan cooling.  The remaining bottom surface (which has at least
    // some support from layer i-1) is labelled BottomSurface.
    // For each layer the closure returns:
    //   (bridge_region, bottom_region, top_region, raw_unsupported)
    // where `raw_unsupported` is the un-filtered, un-anchored, un-clipped
    // air-below footprint (`perimeters[i] − perimeters[i-1]`) used **only**
    // for OverhangPerimeter wall classification — never for infill.
    let detect_region = |i: usize| -> (Paths, Paths, Paths, Paths) {
        // ── Raw unsupported area — for wall classification only ──────────────
        // This is the layer footprint that has nothing in the layer below.
        // We deliberately do *not* clip it to interior_regions: walls live on
        // the layer's outer edge, so their classification needs the full
        // footprint view.
        let raw_unsupported = if i == 0 {
            Paths::new(vec![])
        } else {
            let prev = &perimeters[i - 1];
            if prev.is_empty() {
                perimeters[i].clone()
            } else {
                difference(perimeters[i].clone(), prev.clone(), FillRule::EvenOdd)
                    .unwrap_or_default()
            }
        };

        let (bridge_region, bottom_region) = if bottom_layers > 0 {
            let mut covered = perimeters[i].clone();
            for j in 1..=bottom_layers {
                if i < j {
                    covered = Paths::new(vec![]);
                    break;
                }
                let neighbor = &perimeters[i - j];
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

            let mut region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();

            if let Some(interior_regions) = interior_regions {
                if interior_regions[i].is_empty() {
                    region = Paths::new(vec![]);
                } else if !region.is_empty() {
                    region = intersect(region, interior_regions[i].clone(), FillRule::EvenOdd)
                        .unwrap_or_default();
                }
            }

            // Bridge detection: split `region` into areas that are entirely
            // unsupported by the previous layer (bridge) vs. areas that have
            // at least some support one layer below (true bottom surface).
            //
            // Layer 0 has no layer below, so the entire region is BottomSurface
            // (it is the absolute bottom of the model, not a bridge).
            //
            // The unsupported sub-region is then run through three filters
            // (matching OrcaSlicer / PrusaSlicer behaviour) before becoming a
            // bridge:
            //   1. **Morphological opening** removes thin slivers and
            //      hair-fine connecting strands caused by sub-pixel layer-to-
            //      layer geometry differences (the "Benchy embossed text"
            //      noise pattern).
            //   2. **Minimum-area filter** discards remaining tiny islands
            //      below `bridge_min_area_mm2`.  Such islands would print as
            //      lone bridge dots ("infills cannot be infill patterns").
            //   3. **Anchor expansion** dilates the surviving regions outward
            //      by `bridge_anchor_mm` (clipped to `region`) so each strand
            //      bites into the supported solid material on either side
            //      instead of ending mid-air at the wall.
            // Material rejected by stages 1–2 is reclassified as
            // BottomSurface so the layer remains fully solid below the gap.
            if region.is_empty() || i == 0 {
                (Paths::new(vec![]), region)
            } else {
                let prev_perimeter = &perimeters[i - 1];
                if prev_perimeter.is_empty() {
                    // Nothing below at all → entire region is candidate bridge.
                    let raw = region.clone();
                    let opened = morphological_open(raw, bridge_noise_filter_mm);
                    let big = filter_small_islands(&opened, bridge_min_area_mm2);
                    // Anchor bounds = the full layer footprint (so the
                    // expansion bites into the supported solid material that
                    // sits *outside* the bottom-only region but still inside
                    // the wall envelope).
                    let anchored = expand_to_anchor(big, &perimeters[i], bridge_anchor_mm);
                    let supported = if anchored.is_empty() {
                        region
                    } else {
                        difference(region, anchored.clone(), FillRule::EvenOdd).unwrap_or_default()
                    };
                    (anchored, supported)
                } else {
                    // Step 0 — raw unsupported within the inside-walls region.
                    let raw = difference(region.clone(), prev_perimeter.clone(), FillRule::EvenOdd)
                        .unwrap_or_default();
                    // Step 1 — morphological opening (noise filter).
                    let opened = morphological_open(raw, bridge_noise_filter_mm);
                    // Step 2 — drop islands below the area threshold.
                    let big = filter_small_islands(&opened, bridge_min_area_mm2);
                    // Step 3 — anchor expansion clipped to the full layer
                    // footprint so the bridge bites into the supported solid
                    // material on either side of the gap.
                    let anchored = expand_to_anchor(big, &perimeters[i], bridge_anchor_mm);
                    // Supported part = whatever is left of the bottom region
                    // after the (filtered + anchored) bridge has been removed.
                    let supported = if anchored.is_empty() {
                        region
                    } else {
                        difference(region, anchored.clone(), FillRule::EvenOdd).unwrap_or_default()
                    };
                    (anchored, supported)
                }
            }
        } else {
            (Paths::new(vec![]), Paths::new(vec![]))
        };

        // For the top-region exclusion below, use the combined bottom+bridge area.
        // We need a clone here because bridge_region and bottom_region are returned
        // in the tuple below; we only clone if both are non-empty (one allocation).
        let combined_bottom = union_or_first(bridge_region.clone(), bottom_region.clone());

        let top_region = if top_layers > 0 {
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

            let mut top_region =
                difference(perimeters[i].clone(), covered, FillRule::EvenOdd).unwrap_or_default();

            if !combined_bottom.is_empty() && !top_region.is_empty() {
                top_region =
                    difference(top_region, combined_bottom, FillRule::EvenOdd).unwrap_or_default();
            }

            if let Some(interior_regions) = interior_regions {
                if interior_regions[i].is_empty() {
                    top_region = Paths::new(vec![]);
                } else if !top_region.is_empty() {
                    top_region =
                        intersect(top_region, interior_regions[i].clone(), FillRule::EvenOdd)
                            .unwrap_or_default();
                }
            }
            top_region
        } else {
            Paths::new(vec![])
        };

        (bridge_region, bottom_region, top_region, raw_unsupported)
    };

    #[cfg(not(target_arch = "wasm32"))]
    let t_detect = Instant::now();
    #[cfg(not(target_arch = "wasm32"))]
    let regions: Vec<(Paths, Paths, Paths, Paths)> = {
        use rayon::prelude::*;
        (0..total).into_par_iter().map(detect_region).collect()
    };
    #[cfg(target_arch = "wasm32")]
    let regions: Vec<(Paths, Paths, Paths, Paths)> = (0..total).map(detect_region).collect();
    #[cfg(not(target_arch = "wasm32"))]
    let detection_ns = t_detect.elapsed().as_nanos();
    #[cfg(target_arch = "wasm32")]
    let detection_ns = 0u128;

    // ── Serial apply pass ─────────────────────────────────────────────────────
    for (i, (bridge_region, bottom_region, top_region, raw_unsupported)) in
        regions.into_iter().enumerate()
    {
        if !bridge_region.is_empty() {
            #[cfg(not(target_arch = "wasm32"))]
            let t = Instant::now();
            add_bridge_infill_for_region(
                &mut layers[i],
                &bridge_region,
                nozzle_diameter_mm,
                bridge_flow_ratio,
            );
            #[cfg(not(target_arch = "wasm32"))]
            {
                infill_ns += t.elapsed().as_nanos();
            }
        }

        if !bottom_region.is_empty() {
            #[cfg(not(target_arch = "wasm32"))]
            let t = Instant::now();
            add_solid_infill_for_region(
                &mut layers[i],
                &bottom_region,
                ExtrusionRole::BottomSurface,
                layer_height,
                infill_angle,
            );
            #[cfg(not(target_arch = "wasm32"))]
            {
                infill_ns += t.elapsed().as_nanos();
            }
        }

        if !top_region.is_empty() {
            #[cfg(not(target_arch = "wasm32"))]
            let t = Instant::now();
            add_solid_infill_for_region(
                &mut layers[i],
                &top_region,
                ExtrusionRole::TopSurface,
                layer_height,
                infill_angle,
            );
            #[cfg(not(target_arch = "wasm32"))]
            {
                infill_ns += t.elapsed().as_nanos();
            }
        }

        // Record the union of all solid-surface regions on this layer so that
        // add_infill_to_layers can exclude them from sparse infill.
        // Include bridge_region in the solid union since those are solid-filled areas too.
        let all_bottom = union_or_first(bridge_region, bottom_region);
        let combined_solid = union_or_first(all_bottom, top_region);
        if !combined_solid.is_empty() {
            layers[i].solid_regions = combined_solid;
        }

        // Stash the *raw* unsupported area (the layer-footprint air below)
        // for the post-pass that classifies wall paths in air as
        // OverhangPerimeter.  Empty for layer 0 by design.
        if !raw_unsupported.is_empty() {
            layers[i].unsupported_regions = raw_unsupported;
        }
    }

    SurfaceSubTimings {
        perimeter_snapshot_ms: (snapshot_ns / 1_000_000) as u64,
        detection_ms: (detection_ns / 1_000_000) as u64,
        infill_gen_ms: (infill_ns / 1_000_000) as u64,
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
