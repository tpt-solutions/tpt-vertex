# Spec: Slicer geometry kernels (Phase 10)

- Status: Accepted
- Date: 2026-07-20

This document captures the **formal invariants** that the highest-risk geometric
kernels in `tpt-vertex-slicer` must satisfy. These properties were intended to be
machine-checked with `tpt-telos`; that tool is not available in this build
environment, so the invariants below are additionally encoded as property-style
tests (`#[test]` functions with randomized/edge inputs) in the corresponding
modules. Treat this file as the source of truth for what each kernel guarantees.

## 1. Plane intersection — `layers::intersect_triangle`

Given a triangle `T = {a, b, c}` and a horizontal plane `z = h`:

- **P1.1 (crossing existence):** `intersect_triangle` returns `Some(seg)` **iff**
  the triangle has at least one vertex strictly above `h` and at least one
  strictly below `h`. A triangle entirely on one side (including flat-on-plane)
  returns `None`.
- **P1.2 (endpoints on plane):** every returned segment endpoint lies on the
  plane in Z (`|p.z - h| <= eps`) — enforced structurally since endpoints are
  linear interpolations between an above and a below vertex.
- **P1.3 (endpoints on edges):** each endpoint lies on a triangle edge; the
  interpolation parameter `t = (h - a.z)/(b.z - a.z)` is in `[0, 1]`.
- **P1.4 (no NaN):** the denominator `b.z - a.z` is non-zero because `a` is
  strictly above and `b` strictly below `h`.

## 2. Contour stitching — `layers::stitch_segments`

Given a bag of unordered crossing segments produced by P1 for one plane:

- **P2.1 (closure):** every emitted contour with ≥ 3 points is a closed loop:
  consecutive points (cyclically) are within `tol` of a shared segment endpoint.
- **P2.2 (conservation):** each input segment is consumed at most once
  (`used[i]` is monotonic), so contours never share an edge.
- **P2.3 (termination):** stitching visits a finite number of segments; each
  outer iteration marks the start segment used, and the inner walk only appends
  unused segments, so the algorithm terminates in `O(n^2)`.
- **P2.4 (manifold slice):** for a watertight, consistently-oriented input mesh,
  the union of emitted contours is the exact set of boundary loops of the solid's
  cross-section at that plane (up to `tol`).

## 3. Polygon offset — `offset::offset_contour`

Given a simple polygon `P` (counter-clockwise) and signed distance `d`:

- **P3.1 (parallel edges):** each edge of the offset polygon is parallel to the
  corresponding source edge and displaced outward by `d` (inward for `d < 0`),
  using the miter vertex `v' = v + d·(n1 + n2)/(1 + n1·n2)` with unit outward
  edge normals `n1, n2`.
- **P3.2 (no blow-up at collinear vertices):** when `n1 == n2` (collinear
  points), the miter reduces to `v' = v + d·n1`; coordinates stay bounded. This
  is the invariant that eliminated the earlier coordinate-explosion bug.
- **P3.3 (spike clamp):** for near-180° reversals the denominator `1 + n1·n2` is
  clamped to `>= 0.2`, bounding the miter length to `5·d`.
- **P3.4 (monotone inset area):** for convex `P` and `d < 0`, `|area(offset(P))|
  <= |area(P)|`; nested walls strictly shrink until they collapse (guarded by a
  minimum-area cutoff in `walls_for`).

## Verification status

| Kernel | Property tests | `tpt-telos` |
|--------|----------------|-------------|
| `intersect_triangle` | `layers::tests::*`, `spec_tests::plane_*` | deferred (tool unavailable) |
| `stitch_segments` | `layers::tests::*`, `spec_tests::stitch_*` | deferred (tool unavailable) |
| `offset_contour` | `offset::tests::*`, `spec_tests::offset_*` | deferred (tool unavailable) |

When `tpt-telos` becomes available, encode P1–P3 as its specifications and wire
the check into CI; the property tests should remain as fast regression guards.
