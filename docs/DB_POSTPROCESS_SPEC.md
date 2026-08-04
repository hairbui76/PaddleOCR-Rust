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

Everything in [Reported pyclipper behaviour](#reported-pyclipper-behaviour)
came from a separate research pass and is **not verified in this environment**:
`pyclipper` is not installed here, so its constant values, truncation
behaviour, and offsetting internals have not been executed or re-read locally.
Treat that section as a lead to confirm, not as established fact. Confirming it
is a prerequisite for implementing `unclip`.

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

## Reported pyclipper behaviour

**Unverified in this environment.** `pyclipper` is not installed here.

- Reported constants: `JT_SQUARE = 0`, `JT_ROUND = 1`, `JT_MITER = 2`;
  `ET_CLOSEDPOLYGON = 0`.
- Reported: no coordinate scaling is applied by PaddleOCR before `AddPath`, and
  `pyclipper` converts each coordinate to `int64` by truncation toward zero. If
  true, the `float32` corner coordinates are silently truncated, which would
  make the offset input differ from the corners used for scoring.
- Reported defaults: `miter_limit = 2.0`, `arc_tolerance = 0.25`.
- Reported round-join construction: step count from
  `steps = pi / acos(1 - y / |delta|)` with `y` clamped by `arc_tolerance` and
  `|delta| * 0.25`, capped at `|delta| * pi`; vertices generated by an
  incremental rotation using fixed `sin`/`cos` increments rather than
  recomputing each angle; final union performed by Clipper with positive fill.

Each of these must be confirmed against the actual `pyclipper` and Clipper
sources before any Rust implementation depends on it.

## Fidelity hazards to settle before implementing

1. Three different roundings coexist: `floor`/`ceil` in the score bounding box,
   truncation in `fillPoly`'s polygon, and half-to-even in the final rescale.
2. The final rescale is `float32`, not `float64`.
3. `min(bounding_box[1])` comes from `minAreaRect`'s own size, not from the
   emitted corners; recomputing it from corners can differ in the last ulp.
4. The corner sort is stable and keyed only on `x`; ties keep `boxPoints` order.
5. `cv2.fillPoly`'s scanline coverage rule decides which pixels enter the mean,
   and `cv2.mean` accumulates in double.
6. If the reported truncation in `AddPath` is real, the polygon offset operates
   on different coordinates than the score did.

## Required oracles

`tools/capture_min_area_box_oracle.py` already records sixteen
`minAreaRect`/`boxPoints` cases. Still missing: a `fillPoly` plus `cv2.mean`
oracle for the score, and an `unclip` oracle, which cannot be captured until
`pyclipper` is available in a disposable environment.
