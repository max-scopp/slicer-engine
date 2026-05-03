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
    // We maintain a set of "open chains" — paths whose last point has not yet
    // been connected to a segment on the next scan line.  For each new scan
    // line we greedily match its segments to the best open chain, connecting
    // them via a short U-turn (the distance between adjacent scan lines is
    // exactly `line_spacing`).  When no open chain is close enough, a new
    // chain is started.
    //
    // Threshold: accept a U-turn connection when the horizontal gap between
    // the chain's last X and the closest segment endpoint is ≤ 2× line_spacing.
    // This handles small shape variations between consecutive scan lines while
    // refusing to bridge large gaps that would print across void areas.
    let connect_threshold = line_spacing * SERPENTINE_CONNECT_THRESHOLD;

    // Each chain is a list of points in the *rotated* coordinate system.
    // `chain_last_x[i]` caches the X coordinate of the final point of chain i.
    let mut chains: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut chain_last_x: Vec<f64> = Vec::new();

    for (sy, segments) in &scan_line_data {
        // For each segment on this scan line, find the best chain to attach to.
        // Track which segments have been consumed so we don't double-attach.
        let mut consumed = vec![false; segments.len()];

        // Match each open chain to its best segment on this scan line.
        // We iterate over chains in order; each chain is matched at most once.
        for ci in 0..chains.len() {
            let lx = chain_last_x[ci];

            // Find the closest endpoint among unconsumed segments
            let mut best_seg = None;
            let mut best_dist = connect_threshold;
            for (si, &(xs, xe)) in segments.iter().enumerate() {
                if consumed[si] {
                    continue;
                }
                let dist_start = (lx - xs).abs();
                let dist_end = (lx - xe).abs();
                let (dist, reversed) = if dist_start <= dist_end {
                    (dist_start, false)
                } else {
                    (dist_end, true)
                };
                if dist < best_dist {
                    best_dist = dist;
                    best_seg = Some((si, reversed));
                }
            }

            if let Some((si, reversed)) = best_seg {
                consumed[si] = true;
                let (xs, xe) = segments[si];
                // Extend the chain: first point lands on the scan line (U-turn),
                // second point is the far end of the segment.
                if reversed {
                    // Approach from the right end, extrude leftward
                    chains[ci].push((xe, *sy));
                    chains[ci].push((xs, *sy));
                    chain_last_x[ci] = xs;
                } else {
                    // Approach from the left end, extrude rightward
                    chains[ci].push((xs, *sy));
                    chains[ci].push((xe, *sy));
                    chain_last_x[ci] = xe;
                }
            }
        }

        // Any unmatched segment on this scan line starts a new chain.
        for (si, &(xs, xe)) in segments.iter().enumerate() {
            if consumed[si] {
                continue;
            }
            chains.push(vec![(xs, *sy), (xe, *sy)]);
            chain_last_x.push(xe);
        }
    }

    // ── Phase 3: convert chains back to original coordinates ─────────────────
    let mut result_paths = Paths::new(vec![]);
    for chain in chains {
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
        top_layers,
        bottom_layers,
        layer_height,
        infill_angle,
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
) -> SurfaceSubTimings {
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
    // embarrassingly parallel.  We collect `(bottom_region, top_region)` pairs
    // and then apply them to `layers` in a serial pass to avoid shared mutable
    // state.
    let detect_region = |i: usize| -> (Paths, Paths) {
        let bottom_region = if bottom_layers > 0 {
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
            region
        } else {
            Paths::new(vec![])
        };

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

            if !bottom_region.is_empty() && !top_region.is_empty() {
                top_region = difference(top_region, bottom_region.clone(), FillRule::EvenOdd)
                    .unwrap_or_default();
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

        (bottom_region, top_region)
    };

    #[cfg(not(target_arch = "wasm32"))]
    let t_detect = Instant::now();
    #[cfg(not(target_arch = "wasm32"))]
    let regions: Vec<(Paths, Paths)> = {
        use rayon::prelude::*;
        (0..total).into_par_iter().map(detect_region).collect()
    };
    #[cfg(target_arch = "wasm32")]
    let regions: Vec<(Paths, Paths)> = (0..total).map(detect_region).collect();
    #[cfg(not(target_arch = "wasm32"))]
    let detection_ns = t_detect.elapsed().as_nanos();
    #[cfg(target_arch = "wasm32")]
    let detection_ns = 0u128;

    // ── Serial apply pass ─────────────────────────────────────────────────────
    for (i, (bottom_region, top_region)) in regions.into_iter().enumerate() {
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
        let combined_solid = if !bottom_region.is_empty() && !top_region.is_empty() {
            union(bottom_region, top_region, FillRule::EvenOdd).unwrap_or_default()
        } else if !bottom_region.is_empty() {
            bottom_region
        } else {
            top_region
        };
        if !combined_solid.is_empty() {
            layers[i].solid_regions = combined_solid;
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
