# Orientation Classification Contract

Roadmap item: `DOCORI-001` (contract half)
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: contract frozen from the pinned source; **no implementation**, no
artifact provisioned

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

## 5. Frozen values — text-line orientation

Read from `tools/infer/utility.py:init_args`:

| Setting | Value | Note |
|---|---|---|
| `use_angle_cls` | `False` | **Off by default upstream** |
| `cls_image_shape` | `3, 48, 192` | Width is **fixed** at `192`, unlike the recognizer's per-batch width |
| `label_list` | `["0", "180"]` | Two classes only; no `90` or `270` |
| `cls_batch_num` | `6` | Same as `rec_batch_num` |
| `cls_thresh` | `0.9` | |

## 6. Preprocessing — `predict_cls.py:resize_norm_img`

For each crop, with `imgC, imgH, imgW = 3, 48, 192`:

1. `ratio = w / h`.
2. `resized_w = imgW` if `ceil(imgH * ratio) > imgW`, else `ceil(imgH * ratio)`.
3. `cv2.resize(img, (resized_w, imgH))` — default `INTER_LINEAR`.
4. `float32`, transpose to CHW, divide by `255`.
5. Subtract `0.5`, divide by `0.5`.
6. Zero-pad into a `(3, 48, 192)` canvas, content at the left.

Three differences from the recognizer's `resize_norm_img`, each of which would
be a bug if carried across:

- **The width is a constant, not derived.** The recognizer computes
  `imgW = int(imgH * max_wh_ratio)` per batch; the classifier does not, and pads
  every crop to `192`.
- **`max_wh_ratio` is computed and never used.** `__call__` calculates it per
  batch and passes it nowhere. It is dead code upstream. A port that "uses" it
  would produce a different tensor.
- **A single-channel branch exists** (`cls_image_shape[0] == 1`) which the
  default `3`-channel shape never takes.

## 7. Batching — `predict_cls.py:__call__`

Identical in shape to recognition: sort all crops by `width / height`, process in
chunks of `cls_batch_num`, restore results to the caller's order by index.
`cls_res` is initialised to `[["", 0.0]] * img_num`, so a crop whose batch never
ran keeps an empty label rather than a fabricated one.

## 8. Postprocessing — `ppocr/postprocess/cls_postprocess.py:ClsPostProcess`

`pred_idxs = preds.argmax(axis=1)`, then `(label_list[idx], preds[i, idx])`.
NumPy's `argmax` returns the **first** maximum on a tie, so class `0` wins an
exact tie — the same lowest-index rule the CTC decoder uses.

## 9. The rotation rule, and its asymmetry

```python
if "180" in label and score > self.cls_thresh:
    img_list[...] = cv2.rotate(img_list[...], 1)
```

Three details that are all observable:

- The test is `score > cls_thresh`, **strict**. A score of exactly `0.9` does
  **not** rotate. This is the opposite of the detector's box-score rule, where
  `box_thresh > score` means equality is *retained*. The two thresholds in the
  same pipeline use opposite boundary conventions, and the only way to get both
  right is to read both.
- The label test is `"180" in label`, a **substring** test on the label string,
  not equality. With the default `label_list` it behaves identically, but a
  custom label list containing `"1800"` would also rotate.
- `cv2.rotate(img, 1)` is `ROTATE_180`. The crop is replaced in place, so
  recognition sees the rotated image and the caller's returned crop list is
  mutated.

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

## 12. Status

Contract frozen. No implementation, no artifact, no fixture. `DOCORI-001` stays
open, and its document-orientation half is now known to need a baseline reference
this checkout does not provide.
