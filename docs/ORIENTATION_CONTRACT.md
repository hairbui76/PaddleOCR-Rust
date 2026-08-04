# Orientation Classification Contract

Roadmap item: `DOCORI-001` (contract half)
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: contract frozen from the pinned source; **no implementation**, no
artifact provisioned

This freezes the observable behaviour of orientation classification before any
Rust is written, the way `CLASSIC_OCR_CONTRACT.md` did for the classic path. Every
value below was read from the pinned checkout and is cited to the file it came
from. Nothing here is inferred from documentation or from a model card.

## 1. A scope finding: there are two different things called orientation

`DOCORI-001` names "document orientation classification". The pinned checkout
does not contain one.

What it contains is a **text-line** orientation classifier —
`tools/infer/predict_cls.py`, class `TextClassifier` — which runs on *cropped
text lines* after detection and before recognition, and decides only whether a
line is upside down.

Document-level orientation, which decides the rotation of a whole page before
detection runs, exists in the PaddleX pipeline configuration and in the
JavaScript and TypeScript SDKs in this checkout, but there is **no Python
predictor for it here**. Searching the pinned tree for `doc_orientation` finds
test files, a TypeScript model list, and a JS pipeline default — no
implementation.

**Consequence for `DOCORI-001`.** The item as written spans two capabilities with
different models, different inputs, and different geometry semantics. This
document freezes the one that exists in the pinned baseline. Document-level
orientation needs its own artifact decision and its own baseline reference before
it can be specified at all, and claiming otherwise would be specifying a model
this project has never seen.

## 2. Frozen values — text-line orientation

Read from `tools/infer/utility.py:init_args`:

| Setting | Value | Note |
|---|---|---|
| `use_angle_cls` | `False` | **Off by default upstream** |
| `cls_image_shape` | `3, 48, 192` | Width is **fixed** at `192`, unlike the recognizer's per-batch width |
| `label_list` | `["0", "180"]` | Two classes only; no `90` or `270` |
| `cls_batch_num` | `6` | Same as `rec_batch_num` |
| `cls_thresh` | `0.9` | |

## 3. Preprocessing — `predict_cls.py:resize_norm_img`

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

## 4. Batching — `predict_cls.py:__call__`

Identical in shape to recognition: sort all crops by `width / height`, process in
chunks of `cls_batch_num`, restore results to the caller's order by index.
`cls_res` is initialised to `[["", 0.0]] * img_num`, so a crop whose batch never
ran keeps an empty label rather than a fabricated one.

## 5. Postprocessing — `ppocr/postprocess/cls_postprocess.py:ClsPostProcess`

`pred_idxs = preds.argmax(axis=1)`, then `(label_list[idx], preds[i, idx])`.
NumPy's `argmax` returns the **first** maximum on a tie, so class `0` wins an
exact tie — the same lowest-index rule the CTC decoder uses.

## 6. The rotation rule, and its asymmetry

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

## 7. Geometry semantics

Rotation happens **after** cropping, on the crop, so it does not alter the
detected polygon. The returned box coordinates continue to describe the region in
the source image, and a caller who rotates a crop by 180° does not need an
inverse transform to map results back — the quadrilateral is unchanged.

That is why this capability is cheap to add to the classic path and why it is
*not* what `DOCORI-001` means by "inverse geometry semantics". Those semantics
belong to document-level orientation, which rotates the page before detection and
therefore does change every coordinate.

## 8. What an implementation would need

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

## 9. Status

Contract frozen. No implementation, no artifact, no fixture. `DOCORI-001` stays
open, and its document-orientation half is now known to need a baseline reference
this checkout does not provide.
