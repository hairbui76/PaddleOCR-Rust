# Orientation Classification Contract

Roadmap item: `DOCORI-001` (contract half)
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: contract frozen from the pinned source **and** from the provisioned
artifact configs; **no implementation** yet

This freezes the observable behaviour of orientation classification before any
Rust is written, the way `CLASSIC_OCR_CONTRACT.md` did for the classic path. Every
value below was read from the pinned checkout and is cited to the file it came
from. Nothing here is inferred from documentation or from a model card.

## 1. Correction to an earlier claim in this document

An earlier revision of this file stated that the pinned checkout "does not
contain" a document-orientation classifier, and characterised its mentions as
test files, a TypeScript model list, and a JavaScript pipeline default. **That was
wrong**, and the roadmap entry citing it was wrong with it.

The pinned checkout contains a full document-orientation implementation in the
C++ deployment tree:

- `deploy/cpp_infer/src/api/models/doc_img_orientation_classification.{h,cc}`
- `deploy/cpp_infer/src/pipelines/doc_preprocessor/pipeline.cc`, which predicts an
  angle, parses it as an integer label, and rotates the whole image
- `deploy/cpp_infer/src/common/processors.cc:557`, `ComponentsProcessor::RotateImage`
- `deploy/cpp_infer/src/configs/OCR.yaml`, which pins the model name
  `PP-LCNet_x1_0_doc_ori`

What is true, and what the earlier claim garbled, is narrower: there is **no
Python predictor** for document orientation in this checkout. Searching the
Python tree is what produced the mistaken conclusion, because every previous
capability in this port had its authority in `tools/infer/*.py` and I searched
where the answer had always been before.

## 2. There are still two different capabilities

| | Text-line orientation | Document orientation |
|---|---|---|
| Authority in the checkout | `tools/infer/predict_cls.py` (Python) **and** `deploy/cpp_infer/.../textline_orientation_classification.cc` | C++ only |
| Input | one cropped text line, after detection | the whole page, before detection |
| Classes | `["0", "180"]` | an integer angle in `[0, 360)` |
| Effect on geometry | none — the crop is rotated, detected polygons are untouched | every coordinate changes; the page itself is rotated and resized |
| Pinned model | `PP-LCNet_x1_0_textline_ori` | `PP-LCNet_x1_0_doc_ori` |

`DOCORI-001`'s "inverse geometry semantics" belong to the second column.

## 3. What cannot be frozen from source, and why

The C++ classifier is **config-driven**. `ClasPredictor` builds its preprocessing
from keys it reads at run time out of the artifact's own `inference.yml`:

```
ResizeImage.size or ResizeImage.resize_short
CropImage.size            (optional)
NormalizeImage.scale, .mean, .std
PostProcess.Topk.label_list
```

So the resize mode, the crop size, the normalization constants, and the label set
for **both** orientation models live in the artifact, not in the source. They
cannot be frozen by reading the repository. Provisioning the artifact is a
prerequisite for the contract, not merely for the implementation — which is the
opposite of the classic path, where every constant was in `predict_det.py` and
`utility.py` and the artifact only supplied weights.

A second consequence: the Python text-line classifier **hard-codes** `3,48,192`
and `cls_thresh 0.9`, while the C++ one reads its shape from config. The two
implementations of the same capability can therefore disagree, and a port must
say which one it reproduces. This document freezes the Python one, because that
is the path `predict_system.py` uses and the one every M2 contract in this
project was read from.

## 4. Document-level rotation — `RotateImage`

Frozen from `processors.cc:557`, since this part *is* in the source:

- The angle must be in `[0, 360)`; anything else is an error, not a clamp.
- An angle within `1e-7` of zero returns a clone, not a resampled copy.
- Rotation is about the image centre with `getRotationMatrix2D`, then the output
  size is expanded to `new_w = int(h·|sin| + w·|cos|)`,
  `new_h = int(h·|cos| + w·|sin|)` — truncated, not rounded — and the translation
  is adjusted by `(new − old) / 2`.
- Resampling is `INTER_CUBIC`, not the `INTER_LINEAR` used by the detector
  resize. Two different interpolations in one pipeline, again.

## 5. Second correction: the Python path is not this baseline's path

Section 3 said this document freezes the Python `predict_cls.py` contract
"because that is the path `predict_system.py` uses". **That reasoning was wrong
for the pinned model**, and provisioning the artifact is what showed it.

`deploy/cpp_infer/src/configs/OCR.yaml` pins `PP-LCNet_x1_0_textline_ori`. That
model's own `inference.yml` declares a contract that does not match
`predict_cls.py` in any of its significant values:

| | `predict_cls.py` (hard-coded) | `PP-LCNet_x1_0_textline_ori` (artifact config) |
|---|---|---|
| Input shape | `3, 48, 192` | `3, 80, 160` |
| Resize | aspect-preserving, then zero-pad to `192` | plain `ResizeImage` to `160×80`, **no padding** |
| Normalization | `(x/255 − 0.5) / 0.5` | scale `1/255`, mean `[0.485, 0.456, 0.406]`, std `[0.229, 0.224, 0.225]` |
| Labels | `["0", "180"]` | `["0_degree", "180_degree"]` |

`predict_cls.py` describes the **legacy** `ch_ppocr_mobile_v2.0_cls` classifier.
The baseline's configuration selects a different model with a different input
contract, and a port that implemented the Python path would preprocess correctly
for a model this baseline does not use.

This also explains the substring test flagged below as a curiosity. `"180" in
label` is not sloppiness: with real label lists it is load-bearing, because
`"180" in "180_degree"` is true and `"180" in "0_degree"` is false, while an
equality test against `"180"` would never fire.

## 6. The provisioned artifacts

Both are Apache-2.0 and are stored outside version control, as
`MODEL-DEC-001` requires.

| Model | Revision | `inference.onnx` SHA-256 / bytes | `inference.yml` SHA-256 / bytes |
|---|---|---|---|
| `PP-LCNet_x1_0_textline_ori` | `7fdcf3cf7061163eda7183b224aa334bd33068f7` | `38aa97cd4be591e0ad304e659f07ba30d946f27a63315433f6659c69c8778345` / `6,777,816` | `8d5120d0e1a30a9df7ed46aa9119da3796ed066777089d1c1d705f132d5e90f9` / `735` |
| `PP-LCNet_x1_0_doc_ori` | `7330ab7039123e46af2dc03154b9969aa412c61d` | `af9a0a4f317ff0709ce752067807f819cb15d883f8ecad89f28df1c6ee2d9c92` / `6,788,069` | `9e195eb729a8173588cd0e8a852c8b373aa606e79e77b4ac7d8346f5426caf26` / `766` |

Both are roughly `6.8 MB`, an order of magnitude smaller than the detector and
recognizer.

## 7. Text-line orientation — the real contract

From `PP-LCNet_x1_0_textline_ori/inference.yml`:

```
PreProcess:
  ResizeImage: size [160, 80]          # width, height
  NormalizeImage: scale 1/255,
                  mean [0.485, 0.456, 0.406],
                  std  [0.229, 0.224, 0.225],
                  channel_num 3, order ''
  ToCHWImage
PostProcess:
  Topk: topk 1, label_list ["0_degree", "180_degree"]
```

Input tensor `x` is `[N, 3, 80, 160]`, batch bounded to `8` by the declared
dynamic shapes.

Two consequences worth stating plainly:

- **The normalization is the detector's, not the recognizer's.** This project
  already has that exact evaluation order in `src/tensor.rs`
  (`classic_detector_input`), verified bit-identical against a captured upstream
  tensor. The classifier can reuse it; the recognizer's `(x − 0.5) / 0.5` path
  would be wrong.
- **The resize is unconditional.** No aspect ratio, no padding, no per-batch
  width. Every crop becomes `160×80` regardless of shape, which makes batching
  trivial and makes the `max_wh_ratio` dead code in `predict_cls.py` doubly
  irrelevant.

## 8. Document orientation — the real contract

From `PP-LCNet_x1_0_doc_ori/inference.yml`:

```
PreProcess:
  ResizeImage: resize_short 256        # shorter side to 256, aspect preserved
  CropImage: size 224                  # centre crop
  NormalizeImage: same constants as above
  ToCHWImage
PostProcess:
  Topk: topk 1, label_list ["0", "90", "180", "270"]
```

Input tensor `x` is `[N, 3, 224, 224]`.

**Four classes, not two**, and the labels are bare numbers here while the
text-line model uses the `_degree` suffix. Two models in one pipeline with
different label conventions is exactly the kind of detail that a port gets wrong
by generalising from the first one it implements.

The `resize_short` plus centre-crop pair is a preprocessing shape this project
has never implemented: the classic path only ever resizes, never crops to a
centre window. It needs its own oracle capture.

## 9. Text-line classification with the legacy Python model

Retained for reference, because `predict_system.py` still implements it and a
caller pointing at the legacy artifact would get this behaviour. It is **not**
the contract for the pinned model.

Read from `tools/infer/utility.py:init_args`: `use_angle_cls` `False`,
`cls_image_shape` `3, 48, 192`, `label_list` `["0", "180"]`, `cls_batch_num` `6`,
`cls_thresh` `0.9`.

`resize_norm_img` preserves aspect ratio, caps the width at `192`, normalizes
`(x/255 − 0.5) / 0.5`, and zero-pads left-aligned. `__call__` sorts by aspect
ratio, batches by six, restores by index, and computes a `max_wh_ratio` it never
uses. `ClsPostProcess` takes `argmax`, which resolves ties to the lowest index.

The rotation rule and its asymmetry, which apply to both paths:

```python
if "180" in label and score > self.cls_thresh:
    img_list[...] = cv2.rotate(img_list[...], 1)
```

- `score > cls_thresh` is **strict**: exactly `0.9` does **not** rotate. The
  detector's box-score rule is the opposite convention, where equality is
  retained. Two thresholds in one pipeline with opposite boundaries.
- `"180" in label` is a substring test, which §5 explains is load-bearing.
- `cv2.rotate(img, 1)` is `ROTATE_180`, applied in place, so recognition sees the
  rotated crop.

## 10. Geometry semantics

Rotation happens **after** cropping, on the crop, so it does not alter the
detected polygon. The returned box coordinates continue to describe the region in
the source image, and a caller who rotates a crop by 180° does not need an
inverse transform to map results back — the quadrilateral is unchanged.

That is why this capability is cheap to add to the classic path and why it is
*not* what `DOCORI-001` means by "inverse geometry semantics". Those semantics
belong to document-level orientation, which rotates the page before detection and
therefore does change every coordinate.

## 11. What an implementation would need

Recorded now so it constrains the work rather than being written to fit it:

1. **An artifact**, with its terms reviewed under `LIC-001` and its digest,
   revision, byte count, and source recorded per `MODEL-DEC-001`.
2. **A committed fixture** with an upside-down text line and its expected label
   and score, reproduced by a gate the way `G1` reproduces the classic fixtures —
   the bar `LANG-001` set for adding any artifact.
3. **A captured input-tensor comparison** under `m2-tensor-v1`, since `PRE-001`
   found two real divergences in exactly this kind of preprocessing.
4. **The threshold boundary tested at exactly `0.9`**, because the strict
   comparison is the detail most likely to be got wrong.
5. **A default of off**, matching upstream, so enabling it is a caller's choice.

## 12. Oracle results

`tools/capture_orientation_oracle.py` ran the provisioned artifact over six
cases: four deterministic synthetic crops and the committed reading-order page
both upright and rotated 180 degrees.

| Case | Label | Score |
|---|---|---|
| `reading-order-upright` | `0_degree` | `0.998999` |
| `reading-order-rotated` | `180_degree` | `0.999229` |
| four synthetic crops | `0_degree` | `0.520`–`0.979` |

The model answers correctly on the pair that matters, and both answers clear the
`0.9` threshold, so the rotated page would be corrected and the upright one left
alone. The synthetic crops are text-free noise and the model is unconfident about
them, which is the expected shape of that answer rather than a problem.

`tests/fixtures/classic-v1-orientation` records the input tensors, the model
outputs, and the verdicts. This port reproduces every captured input tensor
**bit-identically**, and separately reaches the same verdict from each recorded
output. The two halves are tested apart so a failure in one does not implicate
the other.

One useful by-product: the rotated case's source digest is the one the capture
recorded *after* calling `cv2.rotate(img, cv2.ROTATE_180)`, so matching it proves
`rotate_180` agrees with OpenCV rather than merely looking plausible.

## 13. Document orientation oracle results

`tools/capture_document_orientation_oracle.py` ran the provisioned
`PP-LCNet_x1_0_doc_ori` over eight cases: four deterministic synthetic pages and
the committed benchmark page at each right angle.

| Case | Label | Score |
|---|---|---|
| `benchmark-page` | `0` | `0.927196` |
| `benchmark-page-90` | `90` | `0.926185` |
| `benchmark-page-180` | `180` | `0.924321` |
| `benchmark-page-270` | `270` | `0.925406` |

**All four right angles identified**, each above `0.92`. The four synthetic pages
are text-free noise and the model is unconfident about them, which is the shape
that answer should have.

`tests/fixtures/classic-v1-document-orientation` records the input tensors and
outputs; `src/document_orientation.rs` reproduces every captured tensor
**bit-identically**, which is the only evidence that both new roundings — the
resize's round-half-away and the crop origin's integer division — are right.

## 14. The one-pixel finding in upstream's page rotation

`RotateImage` builds its matrix with `getRotationMatrix2D(center, angle, 1)`
where `center = (w / 2, h / 2)`. That is **not** the centre of the pixel grid,
which is `((w - 1) / 2, (h - 1) / 2)`, so the rotation carries a half-pixel
offset in each axis.

This was measured rather than reasoned about. At `180` degrees on the `1280x720`
benchmark page, upstream's `warpAffine` output equals `cv2.rotate(ROTATE_180)`
**shifted by exactly one pixel in both axes**:

| Shift applied to the exact rotation | Mismatching pixels |
|---|---|
| `(0, 0)` | `15,795` |
| `(1, 0)` | `10,760` |
| `(0, 1)` | `11,687` |
| **`(1, 1)`** | **`0`** |

The matrix is `x' = -x + 1280`, `y' = -y + 720`, where an exact rotation gives
`1279 - x` and `719 - y`.

So the obvious implementation — a transpose-and-flip right-angle rotation, which
is exact, lossless, and what any reviewer would expect — **displaces every
coordinate on the page by one pixel** against upstream. `DocumentRotation`
reproduces upstream's matrix rather than the tidy one, and a test asserts that
matching the exact rotation at that corner would be the bug.

The same measurement also rules out reusing a right-angle rotation for the
pixels: upstream resamples with `INTER_CUBIC` through this offset matrix, so the
rotated page differs from a transposed one everywhere, not only at the border.

## 15. The border mode, and why the crop warp is not reusable

`src/crop.rs` implements cubic sampling verified against `72` captured OpenCV
cases, and it is **not** reusable for page rotation. It replicates the source
border, because `get_rotate_crop_image` calls `warpPerspective` with
`BORDER_REPLICATE`. `RotateImage` calls `warpAffine` with the **default** border,
which is `BORDER_CONSTANT` at zero.

For a page rotation that difference is not marginal: the expanded canvas samples
outside the source at every corner, which is exactly where the border rule
decides the answer. Reusing the crop path would fill those corners with edge
pixels smeared outward instead of black.

The cubic *weights* are shared, since those are the same OpenCV construction and
`crop.rs` already preserves its exact evaluation order.

`rotate_page` reproduces all `12` captured cases — three sizes including odd
`9x7` and `23x11`, at each of the four angles — **byte for byte**. The odd sizes
matter: they are where the output-size truncation and the half-pixel centre
offset both bite.

## 16. Status

Contract frozen for both models. Artifacts provisioned and hashed. The text-line
classifier is implemented in `src/orientation.rs`, compared against a captured
oracle, and available as an optional pipeline stage that defaults to off.

Text-line orientation is delivered end to end: implemented, oracle-matched,
gate-verified against the real model, and exposed through the API and CLI with
the default off.

Document orientation is **half** delivered. Its preprocessing and decision are
implemented and verified bit-identically against a capture, and all four right
angles are identified. Nothing calls it, deliberately: acting on an angle means
rotating the page before detection and mapping every returned coordinate back
through the inverse transform, and that geometry is not written. Wiring it in
without the inverse would return polygons that are internally consistent and
silently wrong against the image the caller supplied, which is worse than not
wiring it in.

The rotation **geometry** is now implemented: `DocumentRotation` reproduces
upstream's affine matrix including its one-pixel offset — see §14, which is where
the obvious implementation would have gone wrong — computes the expanded output
size with upstream's truncation, and provides the inverse map that returns a
coordinate to the caller's image.

Document orientation is now complete as a capability: preprocessing, decision,
rotation geometry with its one-pixel offset, the inverse map, and cubic
resampling with the correct border mode — every part matched against a capture.

What remains is not implementation but **integration**: deciding where a page
rotation belongs in the pipeline, and mapping detected polygons back through
`DocumentRotation::inverse` so a caller receives coordinates in the image they
supplied. `DOCPIPE-001` owns composing the document preprocessing stages, and
that is where this belongs rather than being bolted onto the classic path.

Two corrections are recorded above rather than silently fixed, because both were
wrong in ways that would have produced working code against the wrong contract:
the claim that document orientation is absent from the checkout, and the choice
to freeze the Python text-line path for a model that does not use it.
