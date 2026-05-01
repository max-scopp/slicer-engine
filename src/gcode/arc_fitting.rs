//! Arc-fitting (G2/G3) post-processor for polylines.
//!
//! Inspired by [ArcWelder](https://github.com/FormerLurker/ArcWelderLib) and the
//! `ArcFitter` used by OrcaSlicer / PrusaSlicer.
//!
//! Given a sequence of 2D points, [`fit_arcs`] greedily welds runs of points
//! that lie within `tolerance` of a common circle into [`PathSegment::Arc`]
//! variants and emits the rest as [`PathSegment::Line`] segments.  The result
//! is a stream the G-code generator can render directly into G1/G2/G3
//! commands.
//!
//! ## Algorithm
//!
//! For every starting index `i`, we extend a window `[i, j]` while:
//!
//! 1. The three "anchor" points (`p_i`, `p_mid`, `p_j`) are not collinear and
//!    define a circle with radius below `max_radius`.
//! 2. Every interior point's perpendicular distance from that circle is
//!    `≤ tolerance`.
//! 3. The arc is monotonic — angles around the centre advance in a single
//!    direction without wrapping past the endpoints.
//!
//! When `j` can no longer be extended, the run `[i, j]` is emitted as an arc
//! (if it covers at least `min_run` segments) and we restart from `j`.

/// Output of [`fit_arcs`] — a single line or arc segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    /// Straight move from `start` to `end`.
    Line { start: (f64, f64), end: (f64, f64) },
    /// Circular arc from `start` to `end` around `center` (absolute coords).
    /// `is_cw` selects G2 (clockwise) when true and G3 (counter-clockwise) when false.
    Arc {
        start: (f64, f64),
        end: (f64, f64),
        center: (f64, f64),
        radius: f64,
        is_cw: bool,
    },
}

impl PathSegment {
    /// Geometric length of the segment along its path.
    pub fn length(&self) -> f64 {
        match *self {
            PathSegment::Line { start, end } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                (dx * dx + dy * dy).sqrt()
            }
            PathSegment::Arc {
                start,
                end,
                center,
                radius,
                is_cw,
            } => {
                let a0 = (start.1 - center.1).atan2(start.0 - center.0);
                let a1 = (end.1 - center.1).atan2(end.0 - center.0);
                let sweep = arc_sweep(a0, a1, is_cw);
                radius * sweep
            }
        }
    }

    /// End point of this segment (handy for chaining).
    pub fn end(&self) -> (f64, f64) {
        match *self {
            PathSegment::Line { end, .. } => end,
            PathSegment::Arc { end, .. } => end,
        }
    }
}

/// Greedily weld a polyline of `points` into [`PathSegment`]s.
///
/// `points` is expected to have `≥ 2` entries.  Returns one segment per input
/// edge when arc fitting is impossible.
pub fn fit_arcs(
    points: &[(f64, f64)],
    tolerance: f64,
    min_run: usize,
    max_radius: f64,
) -> Vec<PathSegment> {
    let n = points.len();
    let mut out = Vec::with_capacity(n);
    if n < 2 {
        return out;
    }
    if n < min_run.max(3) || tolerance <= 0.0 {
        for w in points.windows(2) {
            out.push(PathSegment::Line {
                start: w[0],
                end: w[1],
            });
        }
        return out;
    }

    let mut i = 0usize;
    while i + 1 < n {
        // Try to find the largest j such that points[i..=j] fit a circle.
        let mut best_j = 0usize;
        let mut best_circle: Option<(f64, f64, f64, bool)> = None; // (cx, cy, r, is_cw)

        // Need at least 3 points to define a circle — start at i + 2.
        let mut j = i + 2;
        while j < n {
            let p_start = points[i];
            let p_end = points[j];

            // Reject when the chord between start and end is essentially zero.
            // This guards against closed-loop perimeters where extending j wraps
            // back near p_start and the algorithm would otherwise emit a "full
            // circle" arc.
            let chord = ((p_end.0 - p_start.0).powi(2) + (p_end.1 - p_start.1).powi(2)).sqrt();
            if chord < 1e-4 {
                break;
            }

            // Use the point with maximum perpendicular deviation from the chord
            // as the third anchor for the circumcircle.  Using a naive index
            // midpoint `(i+j)/2` would shift the anchor on every step, changing
            // the circle definition mid-extension and potentially flipping the
            // CW/CCW direction or producing a degenerate (collinear) triple —
            // both of which cause a premature `break` that cuts short valid arcs.
            // The max-deviation point is the most distant from the chord, making
            // it the most numerically stable anchor: it is always clearly on the
            // dominant side of the arc and is closest to what ArcWelder uses.
            let mid_idx = max_deviation_idx(points, i, j);
            let p_mid = points[mid_idx];

            let Some((cx, cy, r)) = circle_through(p_start, p_mid, p_end) else {
                break;
            };
            if r > max_radius || !r.is_finite() {
                break;
            }

            // Determine direction from the three anchors.
            let is_cw = cross2(p_start, p_mid, p_end) < 0.0;

            // Verify every interior point is within tolerance of the circle and
            // monotonically progressing.
            if !window_fits_arc(&points[i..=j], (cx, cy), r, is_cw, tolerance) {
                break;
            }

            best_j = j;
            best_circle = Some((cx, cy, r, is_cw));
            j += 1;
        }

        let run_len = best_j.saturating_sub(i);
        // Hard floor independent of user setting: very short fitted runs are
        // visually unstable and prone to overfitting into unwanted bows.
        // Keep this moderate so tight-radius holes can still form arcs.
        const HARD_MIN_RUN_SEGMENTS: usize = 5;
        if run_len + 1 >= min_run.max(3).max(HARD_MIN_RUN_SEGMENTS) {
            if let Some((cx, cy, r, is_cw)) = best_circle {
                // Final-arc sanity gate.  Prevents weak/ambiguous arcs from
                // being emitted, where a wrong CW/CCW flag from numerical
                // noise would cause printers and viewers to render the long
                // way around the circle.  Three independent checks:
                //
                //   1. Sweep ≥ ~6° — below this the polyline is effectively
                //      straight; emit lines instead of risking a flipped arc.
                //   2. Sagitta ≥ 2 × tolerance — the arc must actually bulge
                //      enough that a straight line would not also fit.
                //   3. Chord ≥ 0.5 mm AND chord ≥ r × 0.1 — rejects tiny
                //      crescents whose start ≈ end (closed-loop tail) and
                //      also tight U-turns where the slicer's intended direction
                //      is fragile.  These were the source of the "diagonal
                //      lines criss-crossing the model" artefact in the viewer.
                let a_start = (points[i].1 - cy).atan2(points[i].0 - cx);
                let a_end = (points[best_j].1 - cy).atan2(points[best_j].0 - cx);
                let final_sweep = arc_sweep(a_start, a_end, is_cw);
                let final_sagitta = r * (1.0 - (final_sweep * 0.5).cos());
                let chord = ((points[best_j].0 - points[i].0).powi(2)
                    + (points[best_j].1 - points[i].1).powi(2))
                .sqrt();
                // For practical print quality we intentionally keep the fitter
                // conservative: very shallow arcs are better emitted as lines.
                const FINAL_MIN_SWEEP: f64 = std::f64::consts::PI / 9.0; // 20°
                const FINAL_MIN_CHORD_MM: f64 = 0.4;
                const FINAL_MIN_CHORD_RATIO: f64 = 0.1;
                const FINAL_MIN_SAGITTA_MM: f64 = 0.08;
                const FINAL_MIN_CURVATURE_RATIO: f64 = 0.06; // sagitta/chord
                let required_sagitta = (2.0 * tolerance).max(FINAL_MIN_SAGITTA_MM);
                let max_chord_deviation = max_distance_to_chord(&points[i..=best_j]);
                let curvature_ratio = if chord > 1e-9 {
                    final_sagitta / chord
                } else {
                    0.0
                };
                if final_sweep >= FINAL_MIN_SWEEP
                    && final_sagitta >= required_sagitta
                    && chord >= FINAL_MIN_CHORD_MM
                    && chord >= r * FINAL_MIN_CHORD_RATIO
                    && max_chord_deviation >= required_sagitta
                    && curvature_ratio >= FINAL_MIN_CURVATURE_RATIO
                {
                    out.push(PathSegment::Arc {
                        start: points[i],
                        end: points[best_j],
                        center: (cx, cy),
                        radius: r,
                        is_cw,
                    });
                    i = best_j;
                    continue;
                }
            }
        }

        // Emit a single line segment and advance one point.
        out.push(PathSegment::Line {
            start: points[i],
            end: points[i + 1],
        });
        i += 1;
    }

    out
}

/// Maximum perpendicular distance of interior points from the start-end chord.
///
/// Values near zero indicate an almost straight run, which should stay as line
/// segments even if a large-radius circle can be numerically fitted.
fn max_distance_to_chord(pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }

    let a = pts[0];
    let b = pts[pts.len() - 1];
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let len = (vx * vx + vy * vy).sqrt();
    if len < 1e-12 {
        return 0.0;
    }

    let mut max_d = 0.0;
    for &p in &pts[1..pts.len() - 1] {
        let wx = p.0 - a.0;
        let wy = p.1 - a.1;
        let cross = (vx * wy - vy * wx).abs();
        let d = cross / len;
        if d > max_d {
            max_d = d;
        }
    }

    max_d
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Return the index in `pts[first..last]` (exclusive of first and last) whose
/// point has the greatest perpendicular distance from the chord
/// `(pts[first], pts[last])`.  Falls back to the naive index midpoint when
/// the chord length is near zero or there are no interior points.
fn max_deviation_idx(pts: &[(f64, f64)], first: usize, last: usize) -> usize {
    if last <= first + 1 {
        return first;
    }

    let a = pts[first];
    let b = pts[last];
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let len_sq = vx * vx + vy * vy;

    if len_sq < 1e-24 {
        return (first + last) / 2;
    }

    let mut max_cross_sq = -1.0f64;
    let mut max_idx = (first + last) / 2;

    for idx in (first + 1)..last {
        let wx = pts[idx].0 - a.0;
        let wy = pts[idx].1 - a.1;
        let cross = vx * wy - vy * wx;
        let cross_sq = cross * cross;
        if cross_sq > max_cross_sq {
            max_cross_sq = cross_sq;
            max_idx = idx;
        }
    }

    max_idx
}

/// Signed cross product `(b-a) × (c-b)` — sign reveals orientation (CCW > 0).
fn cross2(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
}

/// Compute the unique circle through three points; returns `None` when they are
/// collinear.
fn circle_through(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Option<(f64, f64, f64)> {
    let ax = a.0;
    let ay = a.1;
    let bx = b.0;
    let by = b.1;
    let cx_p = c.0;
    let cy_p = c.1;

    let d = 2.0 * (ax * (by - cy_p) + bx * (cy_p - ay) + cx_p * (ay - by));
    if d.abs() < 1e-12 {
        return None;
    }

    let ax2_ay2 = ax * ax + ay * ay;
    let bx2_by2 = bx * bx + by * by;
    let cx2_cy2 = cx_p * cx_p + cy_p * cy_p;

    let ux = (ax2_ay2 * (by - cy_p) + bx2_by2 * (cy_p - ay) + cx2_cy2 * (ay - by)) / d;
    let uy = (ax2_ay2 * (cx_p - bx) + bx2_by2 * (ax - cx_p) + cx2_cy2 * (bx - ax)) / d;
    let r = ((ax - ux).powi(2) + (ay - uy).powi(2)).sqrt();
    Some((ux, uy, r))
}

/// Sweep angle (always positive) from `a0` to `a1` taking the direction
/// implied by `is_cw`.
fn arc_sweep(a0: f64, a1: f64, is_cw: bool) -> f64 {
    let two_pi = std::f64::consts::TAU;
    if is_cw {
        let mut s = a0 - a1;
        while s < 0.0 {
            s += two_pi;
        }
        s
    } else {
        let mut s = a1 - a0;
        while s < 0.0 {
            s += two_pi;
        }
        s
    }
}

/// Verify that every point in `pts` lies within `tol` of the circle and that
/// angles around the centre advance monotonically in the direction implied by
/// `is_cw`.
fn window_fits_arc(pts: &[(f64, f64)], center: (f64, f64), r: f64, is_cw: bool, tol: f64) -> bool {
    if pts.len() < 3 {
        return false;
    }

    let angle_of = |p: (f64, f64)| -> f64 { (p.1 - center.1).atan2(p.0 - center.0) };

    let a_start = angle_of(pts[0]);
    let a_end = angle_of(pts[pts.len() - 1]);
    let total_sweep = arc_sweep(a_start, a_end, is_cw);

    // Cap arc sweep at ~100°.  This remains conservative versus ArcWelder-like
    // defaults while allowing tighter-radius features (small holes) to retain
    // meaningful arc coverage.
    // Larger sweeps are where users most often report
    // visually odd "half-circle" arcs on wall paths; keeping arcs shorter makes
    // fitted paths track the original polyline more faithfully.
    // because the chord-to-arc deviation grows with the squared sweep angle,
    // and a single G2/G3 carrying half-or-more of a perimeter is brittle on
    // many firmwares.  Anything larger is split into multiple shorter arcs.
    const MAX_SWEEP: f64 = std::f64::consts::PI * 5.0 / 9.0;
    if total_sweep < 1e-6 || total_sweep > MAX_SWEEP {
        return false;
    }

    let mut prev_progress = 0.0;
    for (idx, &p) in pts.iter().enumerate() {
        let dx = p.0 - center.0;
        let dy = p.1 - center.1;
        let d = (dx * dx + dy * dy).sqrt();
        if (d - r).abs() > tol {
            return false;
        }

        if idx == 0 || idx == pts.len() - 1 {
            continue;
        }

        let a = angle_of(p);
        let progress = arc_sweep(a_start, a, is_cw);
        if progress < prev_progress - 1e-9 || progress > total_sweep + 1e-9 {
            return false;
        }
        prev_progress = progress;
    }

    true
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_circle(cx: f64, cy: f64, r: f64, n: usize, ccw: bool) -> Vec<(f64, f64)> {
        let mut pts = Vec::with_capacity(n);
        // Sample a 70° arc — comfortably within the 100° MAX_SWEEP cap.
        let total = 7.0 * std::f64::consts::PI / 18.0;
        for k in 0..n {
            let t = (k as f64) / ((n - 1) as f64);
            let theta = if ccw { t * total } else { -t * total };
            pts.push((cx + r * theta.cos(), cy + r * theta.sin()));
        }
        pts
    }

    #[test]
    fn fits_a_clean_arc_into_one_arc() {
        let pts = sample_circle(0.0, 0.0, 10.0, 40, true);
        let segs = fit_arcs(&pts, 0.01, 4, 1000.0);
        assert_eq!(segs.len(), 1);
        match segs[0] {
            PathSegment::Arc { radius, is_cw, .. } => {
                assert!((radius - 10.0).abs() < 0.01, "radius {}", radius);
                assert!(!is_cw, "CCW arc should produce G3");
            }
            _ => panic!("expected an arc"),
        }
    }

    #[test]
    fn straight_line_emits_only_lines() {
        let pts: Vec<(f64, f64)> = (0..10).map(|k| (k as f64, 0.0)).collect();
        let segs = fit_arcs(&pts, 0.01, 4, 1000.0);
        assert!(segs.iter().all(|s| matches!(s, PathSegment::Line { .. })));
        assert_eq!(segs.len(), 9);
    }

    #[test]
    fn shallow_bow_stays_lines() {
        // Very shallow 8° bow across a long chord should remain line segments.
        let mut pts = Vec::new();
        let cx = 0.0;
        let cy = -143.0;
        let r = 145.0;
        let total = 8.0_f64.to_radians();
        let n = 20usize;
        for k in 0..n {
            let t = (k as f64) / ((n - 1) as f64);
            let theta = -total * 0.5 + total * t;
            pts.push((cx + r * theta.sin(), cy + r * theta.cos()));
        }

        let segs = fit_arcs(&pts, 0.025, 4, 1000.0);
        assert!(
            segs.iter().all(|s| matches!(s, PathSegment::Line { .. })),
            "shallow bow should not be emitted as arc"
        );
    }

    #[test]
    fn cw_arc_becomes_g2() {
        let pts = sample_circle(0.0, 0.0, 5.0, 25, false);
        let segs = fit_arcs(&pts, 0.01, 4, 1000.0);
        assert!(!segs.is_empty());
        let arcs: Vec<_> = segs
            .iter()
            .filter(|s| matches!(s, PathSegment::Arc { .. }))
            .collect();
        assert!(!arcs.is_empty(), "expected at least one arc");
        if let PathSegment::Arc { is_cw, .. } = arcs[0] {
            assert!(*is_cw, "CW path should produce G2");
        }
    }

    #[test]
    fn closed_loop_does_not_produce_full_circle() {
        // Sample a complete circle so the polyline returns to its start; the
        // fitter must NOT collapse this into a single full-revolution G3.
        let mut pts = Vec::new();
        let n = 60;
        for k in 0..=n {
            let t = (k as f64) / (n as f64);
            let theta = t * std::f64::consts::TAU;
            pts.push((10.0 * theta.cos(), 10.0 * theta.sin()));
        }
        let segs = fit_arcs(&pts, 0.01, 4, 1000.0);
        for seg in &segs {
            if let PathSegment::Arc {
                start, end, center, ..
            } = *seg
            {
                let chord = ((end.0 - start.0).powi(2) + (end.1 - start.1).powi(2)).sqrt();
                let r = ((start.0 - center.0).powi(2) + (start.1 - center.1).powi(2)).sqrt();
                // Sweep is bounded ≤120° so the chord must be a meaningful span,
                // not a near-zero start-end coincidence.
                // a real chord, not a near-zero start-end coincidence.
                assert!(chord > r * 0.1, "arc chord {} too small for r {}", chord, r);
            }
        }
    }

    #[test]
    fn empty_and_short_inputs_are_safe() {
        assert!(fit_arcs(&[], 0.01, 4, 1000.0).is_empty());
        let two = vec![(0.0, 0.0), (1.0, 0.0)];
        let segs = fit_arcs(&two, 0.01, 4, 1000.0);
        assert_eq!(segs.len(), 1);
    }
}
