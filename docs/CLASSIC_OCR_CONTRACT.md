# Classic OCR Contract

Roadmap item: CTR-001
Status: Frozen M2 classic contract; model ABI and runtime remain P3 work
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854
Selected model family: `PP-OCRv6_medium_det` + `PP-OCRv6_medium_rec`

## Scope and source boundary

This contract covers the M2 native classic OCR sequence only: one DB detector,
quadrilateral sort/crop, one CTC recognizer, score filtering, typed result
ordering, and a later Rust API/CLI. It is derived from the legacy classic
implementation rather than modern PaddleX wrappers.

Primary inspected source paths:

- `tools/infer/predict_system.py` (`TextSystem`, `sorted_boxes`);
- `tools/infer/predict_det.py` (`TextDetector`);
- `tools/infer/predict_rec.py` (`TextRecognizer`);
- `tools/infer/utility.py` (`get_rotate_crop_image`);
- `ppocr/data/imaug/operators.py` (`DetResizeForTest`, `NormalizeImage`,
  `ToCHWImage`);
- `ppocr/postprocess/db_postprocess.py` (`DBPostProcess`);
- `ppocr/postprocess/rec_postprocess.py` (`CTCLabelDecode`);
- `deploy/cpp_infer/src/configs/OCR.yaml` (current C++ v6 deployment
  contrast);
- `paddleocr-js/packages/core/src/models/det.ts` and `rec.ts` (separate
  ONNX-oriented DB/CTC reference).

No exact model archive, `inference.yml`, tensor name, tensor dtype, output
format, hash, or runtime is fixed by this document. Those are preconditions of
`MOD-001` and `RT-001`–`RT-004`.

## M2 profile

The selected model family aligns with current default model names. The M2
orchestration profile intentionally follows the legacy classic source values,
not the modern C++ pipeline values. This is an explicit, documented initial
semantic choice.

| Setting | M2 value | Legacy source evidence | C++ v6 configuration contrast |
|---|---:|---|---|
| Detector algorithm | `DB` | `utility.py:init_args` | `OCR.yaml` text detection module |
| Detector side limit | `960` | `utility.py:init_args` | `64` |
| Detector limit type | `max` | `utility.py:init_args` | `min` |
| Detector maximum resized side | `4000` | `DetResizeForTest` default | `4000` |
| DB map threshold | `0.3` | `utility.py:init_args` | `0.3` |
| DB box threshold | `0.6` | `utility.py:init_args` | `0.6` |
| DB unclip ratio | `1.5` | `utility.py:init_args` | `1.5` |
| DB candidates | `1000` | `predict_det.py` DB setup | not declared in `OCR.yaml` |
| DB score mode | `fast` | `utility.py:init_args` | not declared in `OCR.yaml` |
| DB dilation | disabled | `utility.py:init_args` | not declared in `OCR.yaml` |
| Detector box type | `quad` | `utility.py:init_args` | module result contract not expanded in this YAML |
| Recognizer decode path | CTC decoding; exact architecture label is artifact-gated | `rec_postprocess.py:CTCLabelDecode`; candidate ABI in `MODEL_CANDIDATES.md` | Legacy CLI default is `SVTR_LCNet`, but it is not accepted as a v6-medium artifact ABI without P3 evidence |
| Recognizer image shape | `[3, 48, 320]` | `utility.py:init_args` | not declared in this YAML |
| Recognizer batch size | `6` | `utility.py:init_args` | `6` |
| Recognition score filter | `0.5`, inclusive | `utility.py:init_args`, `TextSystem` | C++ `score_thresh` is `0.0` |
| Orientation classifier | disabled | legacy default is disabled | C++ OCR config enables it |
| Document unwarping | unsupported in M2 | M2 scope decision | C++ OCR config enables it |

The model manifest may not silently override this public profile. If the exact
artifact requires a materially different preprocess/postprocess contract, the
change requires a contract amendment, compatibility ledger update, and fixture
evidence before release. `MODEL_CANDIDATES.md` records a current example: the
candidate v6 detector manifest has DB thresholds that differ from M2, and the
candidate recognizer has a CTC+NRTR multi-head architecture while M2 exposes
only the verified CTC fetch path.

The browser implementation is an independent source of DB/CTC evidence, not
the M2 default contract. Its detector parser fallback uses an `unclipRatio` of
`2.0`, unlike this classic profile's `1.5`; its model archive configuration is
therefore never silently substituted for the selected M2 model manifest.

## Input and resource contract

M2 accepts only one explicitly supplied local PNG/JPEG image through the later
Rust API/CLI. The decoder is P4 work, but all paths must enforce
`QUALITY_PROFILE.md` limits before unbounded allocation:

- encoded bytes at most `64 MiB`;
- decoded dimensions at most `16,384` per side and `40,000,000` pixels;
- at most `1,000` detected regions;
- finite coordinates and strictly convex quadrilaterals at the public geometry
  boundary.

An empty/zero-dimension image, malformed bytes, invalid geometry, more than
the allowed regions, wrong model, incompatible tensor shape/dtype, or missing
local artifact returns a structured Rust error. It must not panic, terminate
the process, silently truncate a result, fetch a model, or return fabricated
text.

## Detector preprocessing and postprocessing

### Preprocessing

For the M2 legacy profile, the detector path must:

1. Begin with the decoded OpenCV-style BGR HWC image semantics used by the
   inspected classic source. The Rust decoder contract must make its color
   order explicit before normalization.
2. For inputs whose width plus height is below `64`, pad the image with zeroes
   to at least `32×32` before resizing, matching `DetResizeForTest`.
3. Apply `limit_type=max` resizing only when the longer source side exceeds
   `960`; if the resized longer side exceeds `4000`, scale it down again.
4. Round each resized side to the nearest multiple of `32`, with a minimum of
   `32`; retain separate `ratio_h` and `ratio_w` after rounding.
5. Apply `(channel / 255 - mean) / std` in HWC channel order, with
   `mean=[0.485, 0.456, 0.406]` and `std=[0.229, 0.224, 0.225]`; transpose to
   CHW and add a batch dimension.
6. Preserve the original dimensions and exact resize ratios for inverse
   geometry. A future exact artifact contract may establish a different tensor
   profile only through an approved amendment.

### DB map contract

The P3-selected backend must expose a validated detector map with one map per
image. For the selected DB contract, the expected logical shape is
`[batch, 1, map_height, map_width]`; any other shape/dtype is a model tensor
contract error until explicitly supported.

For each map, M2 mirrors the classic DB semantics:

1. Make a binary segmentation where `map_value > 0.3`; equality is excluded.
2. Do not dilate the segmentation.
3. Retrieve up to the first `1000` contours using the selected equivalent of
   OpenCV `RETR_LIST` plus `CHAIN_APPROX_SIMPLE`.
4. Derive a minimum-area quadrilateral; discard a contour with initial minimum
   side below `3` pixels.
5. Score the polygon with the `fast` path: the masked mean over its clipped
   bounding rectangle. Keep scores equal to `0.6`; discard only scores below
   it.
6. Unclip by `polygon_area * 1.5 / polygon_perimeter` with round joins and a
   closed polygon. Multiple unclip paths are rejected rather than selected
   arbitrarily.
7. Compute a second minimum-area quadrilateral; discard it when its minimum
   side is below `5` pixels.
8. Scale points to original image dimensions, round each coordinate, clip x to
   `[0, source_width]` and y to `[0, source_height]`, then order points as
   `top-left, top-right, bottom-right, bottom-left` using the classic
   sum/difference rule.
9. Clip detector output again to `[0, width - 1] × [0, height - 1]`, convert
   to integer coordinates, and discard boxes with width or height at most `3`.

`DB-001` may implement step 1 alone as a private checked borrowed-map kernel.
It must preserve the strict threshold and row-major map order, reject malformed
or non-finite model-map data, and bound the generated mask. It does not
validate runtime tensor rank/batching, implement any later step, or constitute
a detector output/compatibility claim.

The contour library, polygon offset implementation, rounding, and exact
inverse-transform behavior must be chosen in P4/P5 only after differential
fixtures prove the `QUALITY_PROFILE.md` geometry budget.

`GEO-001` may provide checked polygon area/perimeter arithmetic and a bounded,
private minimum-area quadrilateral candidate over already checked polygon
vertices. Such a candidate is not an OpenCV `minAreaRect` equivalence claim and
must not emit detector output. Contour extraction, exact OpenCV rectangle
semantics, offset path generation, and detector-map output remain `DET-003`
work and are not implied by that arithmetic.

The same early geometry layer may scale detector-map points with the classic
round-then-inclusive-clamp rule in step 8. Ordering, final exclusive image
clipping, and minimum-side rejection remain a distinct subsequent pass; no
contour/model implementation is implied by the scale primitive.

## Reading order and crop contract

The detector's order is not public output order. Before crop/recognition, sort
detected quadrilaterals exactly as `sorted_boxes`:

1. Stable-sort by the first point's y coordinate, then its x coordinate.
2. Traverse adjacent positions with a backwards inner loop. If their first
   point y coordinates differ by less than `10` pixels and the later box has a
   smaller first-point x coordinate, swap them; otherwise stop the backwards
   loop.

For each sorted quadrilateral:

1. Compute crop width as `int(max(distance(p0,p1), distance(p2,p3)))` and
   height as `int(max(distance(p0,p3), distance(p1,p2)))`.
2. Map `[p0,p1,p2,p3]` to `[0,0], [width,0], [width,height], [0,height]` with
   a perspective transform.
3. Warp using cubic interpolation and replicated borders.
4. Rotate the crop by 90 degrees counter-clockwise when `height / width >=
   1.5`.

The upstream helper uses assertions/implicit backend behavior for invalid crop
dimensions. The Rust contract intentionally returns an `InvalidInput` or
`ResourceLimit` error instead of panicking or attempting a zero-sized warp.

`GEO-001` calculates the checked, no-allocation dimensions and projective maps
required by these steps. The early `CROP-001` implementation in `src/crop.rs`
applies those maps to a private checked interleaved byte buffer, uses replicated
borders, enforces the final dimensions before allocation, and performs the
discrete counter-clockwise byte rotation. Its fixed `a = -0.75` cubic sampler
uses a private OpenCV-style `f32` sampling transform and checked interpolation
in `f32`. It has a reviewed self-authored BGR component fixture in
`tests/fixtures/classic-v1-crop-oracle/`. The fixture records exact output
bytes from OpenCV 5.0.0 / opencv-python-headless 5.0.0.93 for identity,
replicated-border, fractional-projective, tall-rotation, non-linear interior,
all-side-border, tall-projective, eighth-pixel, one-pixel, tall-thin, and a
high-variation cubic-rounding case plus a high-variation cubic-weight-order
case, a high-variation sampling-matrix case, and a high-variation
perspective-LU case. Its `f32` sampler preserves OpenCV 5.0.0's four-weight
construction order and its source-to-warp inversion plus row-evaluation
boundary; its private matrix construction preserves the reviewed float32
coefficient products, equation order, default LU solve, and 3-by-3 inverse for
the perspective-LU regression. It proves an exact
regression only for those inputs and recorded environment; additional approved
captures must
establish any broader OpenCV `INTER_CUBIC` rounding, fixed-point, or
upstream-environment equivalence before M2 claims upstream pixel output
compatibility.

The crop type remains private and carries no BGR/RGB, alpha, decoder, EXIF, or
public API decision. Those semantics remain `IMG-DEC-001`, `IMG-001`, and
`IMG-002` work.

## Recognition and CTC decoding contract

For each sorted crop:

1. Sort crops by `width / height` for batching, then restore the original
   sorted-crop order after decoding. Ordering must not depend on backend
   completion order.
2. Use `[3, 48, 320]` as the base recognition shape. For each batch, calculate
   `max_wh_ratio` as the maximum of `320 / 48` and its crop ratios. Resize each
   crop to height `48`, use `ceil(48 * crop_ratio)` width capped by the batch
   width, normalize BGR CHW values as `(value / 255 - 0.5) / 0.5`, and
   right-pad with zeroes.
3. Require a validated CTC output shape `[batch, time, classes]`. The selected
   artifact manifest determines whether its values are probabilities; Rust must
   not silently apply softmax to an unknown output contract.
4. At each timestep select the maximum class, preserving the lowest class index
   on an exact tie. Class index `0` is the CTC blank.
5. Remove immediately repeated indexes before removing blanks. Map remaining
   indexes through the exact artifact dictionary. An out-of-range class index
   is a model tensor-contract error, not a panic.
6. Concatenate decoded UTF-8 characters in timestep order. Compute the score
   as the arithmetic mean of selected timestep maxima; an empty selection has
   score `0.0`.
7. Pair recognized results with sorted quadrilaterals and retain only entries
   whose score is greater than or equal to `0.5`.

M2 does not apply Arabic reversal, word-box generation, classifier rotation,
or non-CTC decoding. These are Later behavior, not silently ignored options.

`CTC-001` implements the private numeric portion of steps 3–6 over one checked
borrowed unbatched score matrix: lowest-index argmax, raw-index repeat collapse,
blank removal, and selected-max `f32` mean/empty score. Its temporary private
resource bounds are `16,384` time steps, `65,536` classes, and `40,000,000`
borrowed values. It leaves class-to-UTF-8 dictionary mapping, batched tensor
ABI, output semantic validation, text normalization, and all recognizer claims
to `REC-001` through `REC-004`.

## Result and error behavior

The P2 schema will define the exact Rust types and JSON envelope. Its invariant
is already fixed: a successful no-text image returns an explicitly empty result
set, not Python's internal `None` sentinel; ordering is the reading order above;
each retained result carries its source-image quadrilateral, decoded text, and
score.

No partial success is returned when a model, tensor, geometry, decoder, or
resource error occurs. The error category is public; backend-specific details
stay private unless safe and actionable.

## Intentional M2 differences

| Difference | Source behavior / contrast | M2 rule |
|---|---|---|
| Modern pipeline and PaddleX facade | Delegated modern behavior cannot be uniquely pinned. | Absent; no parity claim. |
| C++ v6 preprocessing/defaults | C++ config uses min-side `64`, score threshold `0.0`, and orientation/unwarping. | Use the documented classic profile above. |
| Invalid input/crop/model behavior | Classic source can assert, exit, or return ambiguous internal values. | Return structured errors without panic/process exit. |
| No-text representation | Legacy source distinguishes some `None` and empty-array paths. | One explicit empty successful result set. |
| User-controlled slicing/multipage | Classic code exposes broader modes. | Not supported in M2. |

## Validation plan

`ORACLE_CAPTURE.md` defines the isolated capture procedure. Before M2 claims,
fixtures must cover no text, multi-line ordering, threshold equality, tall crop
rotation, non-ASCII text, malformed images, oversized dimensions, invalid
model/tensor data, and stable repeat order. `TOL-001` will bind each fixture to
the budgets in `QUALITY_PROFILE.md`.
