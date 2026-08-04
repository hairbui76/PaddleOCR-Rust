# DB postprocessing implementation specification

Roadmap item: `DET-003`
Status: Specification only. Contour retrieval is implemented; minimum-area
geometry, unclipping, scoring, filtering, and rescaling are not
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Source: `ppocr/postprocess/db_postprocess.py`

## Why this document exists

The contour-retrieval step cost three failed prototypes because the algorithm
was reconstructed from a paper and from a summary instead of read. Reading the
actual source fixed it on the first try. This file records the remaining steps'
specification **before** implementation, so that mistake is not repeated.

## Evidence status, stated separately

Everything in [Verified upstream behaviour](#verified-upstream-behaviour) was
read directly from the pinned checkout during this work and re-checked against
the file.

[Confirmed pyclipper behaviour](#confirmed-pyclipper-behaviour) was originally
recorded as an unverified research lead. It has since been executed in a
disposable virtual environment holding `pyclipper` 1.4.0 and `shapely` 2.1.2,
and the observable parts are now confirmed. The internal round-join step
construction remains unverified and is deliberately **not** something a Rust
implementation should reimplement from a description; capture an oracle
instead.

## Verified upstream behaviour

### `get_mini_boxes(contour)`

Calls `cv2.minAreaRect(contour)`, then `cv2.boxPoints(...)`, then sorts the
four corners by `x` alone with Python's stable sort. The reindexing is:

- if `points[1][1] > points[0][1]` then `index_1 = 0, index_4 = 1`, else
  `index_1 = 1, index_4 = 0`;
- if `points[3][1] > points[2][1]` then `index_2 = 2, index_3 = 3`, else
  `index_2 = 3, index_3 = 2`.

The returned order is `[points[index_1], points[index_2], points[index_3],
points[index_4]]`, and `sside` is `min(bounding_box[1])`, taken from the rotated
rectangle's own size rather than recomputed from the corners. Both comparisons
are strict, so equal `y` values take the `else` branch.

### `unclip(box, unclip_ratio)`

```text
poly     = Polygon(box)
distance = poly.area * unclip_ratio / poly.length
offset   = pyclipper.PyclipperOffset()
offset.AddPath(box, pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
expanded = offset.Execute(distance)
```

The evaluation order is `(area * unclip_ratio) / length`. `poly.area` is the
unsigned planar area and `poly.length` the exterior-ring perimeter.

### `box_score_fast(bitmap, _box)`

Clips `floor(min)` and `ceil(max)` of each axis into `[0, w - 1]` and
`[0, h - 1]` as `int32`, allocates a `(ymax - ymin + 1, xmax - xmin + 1)`
`uint8` mask, shifts the box by `-xmin` and `-ymin`, fills with
`cv2.fillPoly(mask, box.reshape(1, -1, 2).astype("int32"), 1)`, and returns
`cv2.mean(bitmap[ymin:ymax+1, xmin:xmax+1], mask)[0]`.

Note the three different roundings already present: `floor`/`ceil` for the
bounding box, and `astype("int32")` truncation toward zero for the polygon
handed to `fillPoly`.

### The `boxes_from_bitmap` loop

In order, for each contour up to `max_candidates`:

1. `points, sside = get_mini_boxes(contour)`; `if sside < self.min_size: continue`
   with `min_size = 3`.
2. `score = box_score_fast(pred, points.reshape(-1, 2))` when `score_mode` is
   `"fast"`, otherwise `box_score_slow(pred, contour)`.
3. `if self.box_thresh > score: continue`. The comparison is **strict**, so a
   score exactly equal to the threshold is **kept**.
4. `box = self.unclip(points, self.unclip_ratio)`; `if len(box) > 1: continue`.
   A zero-length result is **not** guarded and reaches `minAreaRect` on an empty
   array; any Rust implementation must decide that case deliberately and record
   the choice as an intentional difference.
5. `box, sside = get_mini_boxes(np.array(box).reshape(-1, 1, 2))`;
   `if sside < self.min_size + 2: continue`, that is `< 5`.
6. Rescale, verbatim:

   ```text
   box[:, 0] = np.clip(np.round(box[:, 0] / width  * dest_width ), 0, dest_width )
   box[:, 1] = np.clip(np.round(box[:, 1] / height * dest_height), 0, dest_height)
   boxes.append(box.astype("int32"))
   ```

   `box` is `float32` here, so the arithmetic is `float32` evaluated
   left-to-right. `np.round` is half-to-even. The clip bounds are **inclusive**
   `dest_width` and `dest_height`, which is one past the last valid pixel index.

## Confirmed pyclipper behaviour

Executed on 2026-08-04 in a disposable virtual environment with `pyclipper`
1.4.0 and `shapely` 2.1.2. The environment was outside this repository and is
not a build or test dependency.

**Constants, confirmed by reading them at runtime:**
`JT_SQUARE = 0`, `JT_ROUND = 1`, `JT_MITER = 2`, `ET_CLOSEDPOLYGON = 0`,
`ET_CLOSEDLINE = 1`.

**Defaults, confirmed:** `MiterLimit = 2.0`, `ArcTolerance = 0.25`.

**Coordinate conversion, confirmed as truncation toward zero.** A zero-distance
offset re-emits the input polygon, which exposes the conversion. Input corner
`(110.7, 99.4)` came back as `[110, 99]`, and `(110.5, 89.5)` came back as
`[110, 89]`. That rules out rounding half-up and half-to-even alike.

This matters: PaddleOCR applies no scaling before `AddPath`, so the `float32`
corner coordinates are silently truncated. The polygon that gets offset is
therefore **not** the polygon that was scored.

**One concrete end-to-end case, confirmed:** for the box
`(1,1), (9,1), (9,5), (1,5)` with `unclip_ratio = 1.5`, Shapely reports
`area = 32.0` and `length = 24.0`, giving `distance = 2.0`. `Execute(2.0)`
returns exactly one path of eight vertices:

```text
[11, 0], [11, 5], [10, 7], [1, 7], [-1, 6], [-1, 1], [0, -1], [9, -1]
```

Note the negative coordinates: the offset result is not clipped to the bitmap,
and the following `get_mini_boxes` call receives them as-is.

**Still unverified:** the internal round-join step construction. A Rust
implementation should not reimplement it from a prose description. Capture an
oracle over representative boxes and match the emitted vertices instead.

## `minAreaRect` prototype findings

An external prototype using a convex hull plus a per-edge rotating rectangle
matched 11 of the 16 recorded cases. The five failures isolate two specific
problems, recorded so the next attempt does not rediscover them.

**1. Tie-breaking, not geometry.** For the `triangle` case the three candidate
rectangles all have area exactly `144`; the minimum is a three-way tie. OpenCV
returned the rectangle with `sside = 11.9175` while the prototype returned
`sside = 10.1823`. Both are genuine minimum-area rectangles. The difference is
purely which candidate is visited first, so the implementation must reproduce
OpenCV's hull orientation (`convexHull` with `clockwise = true`) and its
rotating-calipers traversal order, and must keep a strictly-less comparison so
the first minimum in that order wins.

**2. Degenerate inputs must go through `boxPoints`, not through corners.** For
one point, two points, and collinear points the prototype produced no
rectangle, but OpenCV produces duplicated corners: the recorded `two-points`
case is `[[2,2], [12,7], [12,7], [2,2]]` with `sside = 0`. That shape falls out
of applying `cv2.boxPoints` to a rectangle whose height is `0`. The correct
structure is therefore to compute `(center, size, angle)` first, including the
`n == 1` and `n == 2` branches, and then derive the four corners with the
`boxPoints` formula, rather than emitting corners directly from the calipers
step.


**Second prototype: 14 of 16.** Deriving the corners from `(center, size,
angle)` with the `boxPoints` formula fixed all three degenerate cases. The two
remaining failures, `diamond` and `triangle`, collapse to **one** cause rather
than two.

Both are ties in edge selection. `triangle` has three candidate rectangles of
area exactly `144`. `diamond` produces the same four corners as OpenCV but
starting from a different one: its rectangle is square, so several edges tie on
area, the winning edge fixes the `angle`, the `angle` rotates the `boxPoints`
output, and the later sort by `x` is stable, so a tie between two corners at
equal `x` preserves that rotation into the final ordered box.

The single remaining prerequisite is therefore OpenCV's exact edge-visit order:
the vertex at which `convexHull` starts, its direction, and how
`rotatingCalipers` advances. No geometry question remains open.


**Resolved: 16 of 16.** A sweep over hull orientation, edge direction, and
tie-break comparison found exactly one configuration that reproduces every
recorded case:

- convex hull in **counter-clockwise** monotone-chain order;
- edges visited **forward** in that order;
- the minimum-area comparison **non-strict**, so a later tied candidate
  replaces an earlier one and the **last** tied edge wins.

The other seven configurations score 13, 14 or 15 of 16, and each failure is
one of `triangle`, `hexagon`, or `diamond` — exactly the tie cases. A strict
comparison keeps the first tied edge and fails `triangle` and `hexagon`; a
clockwise hull fails `diamond`. Nothing about `minAreaRect` is unresolved now:
the geometry, the degenerate branches, the corner derivation, and the
tie-break rule are all pinned by the recorded oracle.


## `box_score_fast` prototype findings

An external prototype reached 7 of the 8 recorded score cases. The scanline
rule that works is **inclusive on both ends**: for each row, gather edge
intersections with `min(y0, y1) <= y <= max(y0, y1)`, take the leftmost and
rightmost, and fill `ceil(left) ..= floor(right)`. A half-open rule
(`y0 <= y < y1`) drops the last row and fails every axis-aligned case: it
produced 36 pixels where OpenCV produced 45 for `axis-small`.

The one remaining failure is `slanted`, which fills 70 pixels where OpenCV
fills 84. All seven axis-aligned and degenerate cases match exactly, so the gap
is specific to non-axis-aligned edges: OpenCV's `fillPoly` rasterises edges in
fixed point with `XY_SHIFT` and includes boundary pixels a float scanline with
`ceil`/`floor` excludes. Reproducing it requires that fixed-point edge walk,
not a tighter tolerance.

A follow-up sweep over three fill rules narrows it further. With
`ceil(left) ..= floor(right)` the slanted case fills 70 pixels; with
`round(left) ..= round(right)` it fills 77; with
`floor(left) ..= ceil(right)` it fills **exactly 84**, matching OpenCV's count,
yet the resulting mean still differs. The count is therefore reproducible by a
symmetric widening but the **set** is not, which means OpenCV's boundary rule is
**asymmetric per edge** rather than a uniform widening of both ends: a left edge
and a right edge are not rounded the same way. All three rules score 7 of 8, and
in every rule the only failure is `slanted`.

**That hypothesis is now ruled out.** A follow-up sweep tried all nine
combinations of `floor`, `ceil`, and round-half-up applied independently to the
left and right endpoint of each row. Every one of the nine scores 7 of 8, and
every one fails on `slanted` alone. The per-row endpoint rounding is therefore
**not** where the difference lives.

What remains is structural: either the scanline set itself differs, for example
by sampling at `y + 0.5` rather than at integer rows, or `fillPoly` walks each
edge and marks every pixel the edge touches instead of intersecting rows
analytically. Both were tested. Sampling at `y + 0.5` scores **0 of 8**: it is wrong for even
the axis-aligned cases. The edge-walk hypothesis scores **8 of 8**.

**Resolved.** `fillPoly` is reproduced by:

1. walking every polygon edge with an integer Bresenham line and marking each
   pixel the edge passes through;
2. then, for each row that has any marked pixel, filling every pixel between
   the leftmost and rightmost marked pixel inclusive.

No endpoint rounding rule is involved at all, which is why all nine of those
variants failed identically. The mean is then taken over the marked pixels of
the ROI, matching `cv2.mean` with the mask.


## `unclip` prototype findings

An external prototype implemented Clipper's offset directly: `AddPath`
truncation, `GetUnitNormal` with the reciprocal-then-multiply form, the step
count `steps = pi / acos(1 - y / |delta|)` capped at `|delta| * pi`, the
incremental `(m_cos, m_sin)` rotation, and `Round(v) = trunc(v +/- 0.5)`.

It reproduces the **geometry** but not yet the emitted path. Two differences
remain, and both come from Clipper's final union pass rather than from the
offset arithmetic:

1. **Starting vertex.** For every case that already has the right vertex count,
   the prototype emits the same cycle rotated. For `axis-thin` at ratio `1.5`
   it produced `[(0,4), (2,2), (14,2), (16,4), (16,7), (14,9), (2,9), (0,7)]`
   where Clipper returns the same eight points beginning at `(16,4)`. The
   union re-emits each polygon from its own chosen start.
2. **Vertex count.** Where the prototype emits 12 points Clipper often returns
   8. For a right-angle corner at `delta = 2.0` the step formula gives
   `steps = 6.216`, `StepsPerRad = 0.9893`, and `Round(0.9893 * pi/2) = 2`, so
   the round join emits two arc points plus the final normal point, three per
   corner and twelve in total. Clipper's union then collapses duplicate and
   collinear vertices down to eight.

So the offset math appears correct and the missing component is the union with
positive fill, which decides both the vertex reduction and the starting point.
A Rust implementation should either reproduce that union or, more cheaply,
normalise both sides before comparing: deduplicate, drop collinear vertices,
and rotate to a canonical start. Which of those is acceptable is a contract
decision, because the emitted order reaches `get_mini_boxes` unchanged.

**Confirmed: normalisation closes it, 16 of 16.** Applying that normalisation
to both the prototype output and the recorded Clipper output makes every one of
the eight boxes match at both unclip ratios. The offset arithmetic is therefore
correct as implemented; only the union's vertex reduction and starting point
differ.

That makes the contract decision concrete and cheap. Because `get_mini_boxes`
immediately reduces the path to a rotated rectangle, and `minAreaRect` is
invariant to vertex order, to duplicate vertices, and to collinear vertices, the
union's two effects are **not observable** through the rest of the pipeline. A
Rust implementation may therefore emit the un-unioned path, provided the
difference is recorded as an intentional deviation and re-checked if any later
consumer ever reads the polygon directly.

## Fidelity hazards to settle before implementing

1. Three different roundings coexist: `floor`/`ceil` in the score bounding box,
   truncation in `fillPoly`'s polygon, and half-to-even in the final rescale.
2. The final rescale is `float32`, not `float64`.
3. `min(bounding_box[1])` comes from `minAreaRect`'s own size, not from the
   emitted corners; recomputing it from corners can differ in the last ulp.
4. The corner sort is stable and keyed only on `x`; ties keep `boxPoints` order.
5. `cv2.fillPoly`'s scanline coverage rule decides which pixels enter the mean,
   and `cv2.mean` accumulates in double.
6. The truncation in `AddPath` is confirmed, so the polygon offset operates on
   different coordinates than the score did. This is upstream behaviour to
   reproduce, not a bug to fix.

## Required oracles

`tools/capture_min_area_box_oracle.py` already records sixteen
`minAreaRect`/`boxPoints` cases. `tools/capture_unclip_score_oracle.py` records the
remaining two steps together over eight self-authored boxes at two unclip
ratios, deliberately from the same input so the truncated-versus-untruncated
corner difference stays visible. Both generators are complete; what remains is
the Rust implementation that consumes them.
