# Arachne — Variable-Width Perimeter Generator

This module implements [Arachne][arachne-paper] (Kuipers et al. 2020) for the
slicer engine — variable-width extrusion (VWE) toolpaths that approximate the
medial axis of each shell polygon and emit beads whose extrusion width varies
to match the local wall thickness.

It replaces fixed-width concentric perimeter offsets with a small set of beads
that better fill thin features, sharp corners, and near-medial regions, the
way OrcaSlicer / Cura's `SkeletalTrapezoidation` does.

[arachne-paper]: https://dl.acm.org/doi/10.1145/3386569.3392408

---

## Algorithm Overview

For each closed shell contour produced by [`crate::core::slice_mesh`]:

1. **Collapse-depth detection.** Find the largest inward offset `D` at which
   the polygon is still non-empty. `D` equals the polygon's inradius
   (half the minimum local wall thickness).
2. **Standard beads.** Place up to `wall_count` full-width beads at centerline
   depths `d/2, 3d/2, …` (where `d = nozzle_diameter_mm`). Each bead has
   width `d`. Beads whose centerline would fall outside the polygon
   (depth ≥ `D`) are skipped.
3. **Thin-wall residual.** If the remaining inner space after the standard
   beads has width ≥ `wall_line_width_min × d`, a single variable-width bead
   is added at the centroid of that space, with width = remaining width
   (clamped to `wall_line_width_max × d`).
4. **Width distribution.** If the residual is positive but smaller than
   `wall_line_width_min × d`, the leftover width is absorbed by the innermost
   `wall_distribution_count` standard beads instead, slightly widening them.

The Clipper2 negative-`inflate` op is the fundamental primitive used in steps
1–3; one call per bead gives the centerline path.

---

## Public API

| Item | Purpose |
| --- | --- |
| [`ArachneParams`](mod.rs) | Resolved per-run config (mm-absolute) built from [`SlicingParams`](../settings/params.rs) |
| [`Bead`](mod.rs) | One emitted bead: closed centerline `Path` + extrusion `width_mm` + `is_outer` flag |
| [`generate_arachne_walls`](mod.rs) | Replace OuterWall contours in every layer with bead paths (parallel via rayon on native targets) |
| [`generate_arachne_walls_for_layer`](mod.rs) | Per-layer entry; preserves all non-perimeter paths in their original order after the new walls |
| [`compute_arachne_beads`](mod.rs) | Pure function: `Paths → Vec<Bead>` for a single shell |
| [`ArachneSubTimings`](mod.rs) | CPU-time breakdown (collapse search vs. bead shrinks) returned from a full run |

---

## Output Topology

Arachne emits **centerline paths**, not filled polygons. Every emitted path is
a closed polygon whose vertices are the *center* of the extrusion bead, not
its edge.

- `OuterWall` paths sit at inward depth `d/2` from the raw mesh contour.
- `InnerWall` paths sit at `3d/2`, `5d/2`, …
- `path_widths[i]` carries the actual extrusion width for variable-width beads
  (used by the G-code generator to compute the correct E values per move).
- For a mesh with holes (donut, hollow cylinder) the **hole boundary** also
  receives an `OuterWall`-tagged bead — this is correct because, for that
  contour's shrink sequence, the hole boundary *is* the outermost bead.
  There is no separate "hole wall" tag.

**Consequence:** you cannot tell a solid-island contour from a hole contour by
role alone. Use signed area:

| `path.signed_area()` sign | Topology |
| --- | --- |
| Positive (CCW) | Solid island |
| Negative (CW) | Hole |

---

## Pipeline Integration

Arachne sits between raw slicing and surface/infill generation:

```
slice_mesh()                                  — raw mesh → OuterWall contours per layer
generate_arachne_walls()                      — replaces those contours with bead paths
pre_strip_infill_regions snapshot             — taken before wall stripping
apply_single_wall_restrictions()              — strips inner walls from first/top layers
interior_regions computed                     — post-strip, used by surfaces
generate_top_bottom_surfaces_with_interior()  — top/bottom solid infill within interior
add_infill_to_layers()                        — sparse infill = pre_strip region − solid_regions
```

Order matters: [`crate::core::surfaces`](../core/surfaces.rs) is computed
**after** Arachne so that [`calculate_interior_region`](../core/infill.rs)
sees the correct bead geometry. See the per-pipeline notes in
[`AGENTS.md`](../../AGENTS.md) for additional invariants.

---

## Configuration

| `SlicingParams` field | Used as | Default |
| --- | --- | --- |
| `nozzle_diameter_mm` | Bead spacing `d` and standard-bead width | 0.4 mm |
| `wall_count` | Maximum standard beads per shell | 3 |
| `wall_line_width_min` | × `d` = minimum bead width (residual cutoff) | 0.85 |
| `wall_line_width_max` | × `d` = maximum bead width (clamp on residual / absorb) | 1.5 |
| `wall_distribution_count` | Innermost bead count that may absorb sub-min residuals | 1 |

These are resolved into mm-absolute values by
[`ArachneParams::from_slicing_params`](mod.rs).

---

## Performance Notes

For the common count-limited case (geometry has plenty of material to host
all `wall_count` beads), the total Clipper call count per shell is just
**`wall_count`** — one negative `inflate` per bead, no separate collapse-depth
search.

For the geometry-limited case (polygon collapses before `wall_count` beads
fit), we do `wall_count` fit tests + 4 narrow Miter probes + 1 residual
`shrink` ≈ 8 calls.

The narrow search uses `JoinType::Miter` (no arc-approximation vertices)
because we only test for emptiness; bead emission uses `JoinType::Round` for
smooth centerline corners.

Per-run timing is split into [`ArachneSubTimings`](mod.rs) — `collapse_depth_ms`
and `bead_shrink_ms` — both summed across all rayon worker threads (so the
numbers will exceed the wall-clock duration of the phase on multi-core
machines).

On native targets the per-layer work is parallelised via `rayon`; the wasm32
target falls back to sequential iteration.

---

## Critical Invariants

These have all been hit as bugs at least once. **Read before changing the
module.**

### 1. Input must be normalised before running Arachne

The raw contours produced by [`chain_segments`](../core/slicer.rs) have
**arbitrary winding** (CCW or CW depending on triangle orientation in the
input mesh) and may overlap, duplicate, or be nested (engraved text on the
3DBenchy hull, near-degenerate triangles on hand-modeled assets, etc.).

Passing such a `Paths` directly to `inflate(-d, …)` produces fragmented,
self-intersecting output — sometimes hundreds of micro-loops per layer,
sometimes a near-total geometric collapse where legitimate features
disappear entirely.

**Fix in place:** [`generate_arachne_walls_for_layer`](mod.rs) runs a
Clipper2 `union(…, FillRule::EvenOdd)` over the raw contours before calling
[`compute_arachne_beads`](mod.rs). EvenOdd is winding-independent and
resolves overlaps, yielding the canonical Clipper2 representation
(CCW outer rings, CW holes) that `inflate` handles correctly.

3DBenchy.stl @ 0.2 mm layer height, before vs. after the union:

| Layer | Before | After | Orca reference |
| ----- | -----: | ----: | -------------: |
| 0 (hull bottom) | 772 loops | 10 | 22 |
| 150 (cabin windows) | **4** loops (collapsed!) | 18 | 18 |
| 200 (smokestack) | 84 loops | 5 | 3 |
| Total wall loops | 7,859 | 2,872 | 2,568 |

Bonus side effect: [`apply_single_wall_restrictions`](../core/walls.rs) drops
from ~1 s to ~100 ms because there are far fewer noise paths to chew through.

### 2. Degenerate beads must be filtered

Even after input normalisation, very thin slivers can survive as zero-area
"back-and-forth" line stubs after the negative offset. [`drop_degenerate_beads`](mod.rs)
removes any centerline whose enclosed area is below `0.01 × d²` (~1 % of a
bead-square). This is intentionally generous and drops only pure noise.

The collapse-detection branch (`first_miss_depth = depth; break;`) must use the
**raw** shrink result, not the filtered one — otherwise mesh noise that should
have stopped bead emission would instead be silently dropped, and we would
keep emitting beads in regions where the polygon has actually collapsed.

### 3. Do not normalise wall paths to CCW elsewhere

[`calculate_interior_region`](../core/infill.rs) consumes `OuterWall` paths
directly and **must preserve their winding**. Hole boundary beads are
legitimately CW; flipping them to CCW makes Clipper2 treat hole interiors as
solid material, and infill is then generated through the void.

The `−0.5 × d` correction inside `calculate_interior_region` exists precisely
because Arachne `OuterWall` centerlines are already inset `d/2` from the
model surface — without it, the interior region is over-shrunk by half a
bead width.

### 4. Bead union with `EvenOdd` is wrong

(Listed for completeness; the engine no longer does this.) Tightly nested
concentric closed paths under EvenOdd produce alternating in/out bands instead
of a single solid region. If you ever need to union the bead set itself, use
`NonZero` — and only after you have made every input path CCW.

---

## Related Files

- [src/core/slicer.rs](../core/slicer.rs) — produces the raw contours that
  feed Arachne
- [src/core/walls.rs](../core/walls.rs) — per-island first/top-layer
  single-wall restriction (runs *after* Arachne)
- [src/core/infill.rs](../core/infill.rs) — infill boundary derived from
  Arachne `OuterWall` centerlines
- [src/gcode/generator.rs](../gcode/generator.rs) — consumes
  `(path, role, width)` triples; variable widths come from `path_widths[i]`
- [src/settings/params.rs](../settings/params.rs) — `SlicingParams` source for
  `ArachneParams`
- [AGENTS.md](../../AGENTS.md) — pipeline-wide invariants and Clipper2
  fill-rule guidance
- [examples/diag_arachne.rs](../../examples/diag_arachne.rs) — per-contour vs.
  bundled bead-count diagnostic (used to find the input-normalisation bug)

---

**Last Updated:** 2026-04-28 (input-normalisation fix for unioned raw contours)
