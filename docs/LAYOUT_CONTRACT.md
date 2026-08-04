# Layout Detection Contract

Roadmap item: `LAY-001` (contract half)
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: contract frozen and artifact provisioned; **no implementation**

The first capability frozen from the second baseline `D-013` pinned. It
immediately justified the pin: three of the four values below are **invisible in
the artifact's config** and two of them mean the opposite of what the config
looks like it says.

## 1. What the artifact declares

`PP-DocLayout_plus-L/inference.yml`:

```yaml
Preprocess:
- type: Resize
  target_size: [800, 800]
  keep_ratio: false
  interp: 2
- type: NormalizeImage
  norm_type: none
  mean: [0.0, 0.0, 0.0]
  std:  [1.0, 1.0, 1.0]
- type: Permute
label_list: [paragraph_title, image, text, number, abstract, content,
             figure_title, formula, table, reference, doc_title, footnote,
             header, algorithm, footer, seal, chart, formula_number,
             aside_text, reference_content]
```

Twenty classes. Two input tensors, `image` at `[N, 3, 800, 800]` and `im_shape`
at `[N, 2]`.

## 2. What the config does not say

### `interp: 2` is **bicubic**, not linear

`paddlex/inference/models/object_detection/predictor.py:build_resize`:

```python
interp = {0: "NEAREST", 1: "LINEAR", 2: "BICUBIC", 3: "AREA", 4: "LANCZOS4"}[interp]
```

Every resize in this project so far has been `INTER_LINEAR` — the detector's, the
recognizer's, both orientation classifiers'. This one is **cubic**, and reusing
the linear path would be a plausible wrong image rather than an error.

### `norm_type: none` does **not** mean no normalization

`build_normalize`, read in full:

```python
if is_scale:                       # is_scale defaults to True; the config omits it
    scale = 1.0 / 255.0
if not norm_type or norm_type == "none":
    norm_type = "mean_std"         # "none" is rewritten to "mean_std"
if norm_type != "mean_std":        # ...so this branch never runs
    mean = 0
    std = 1
return Normalize(scale=scale, mean=mean, std=std)
```

`"none"` is rewritten to `"mean_std"`, which makes the branch that would zero the
mean and standard deviation unreachable. The config's own `mean: [0,0,0]` and
`std: [1,1,1]` survive, and `is_scale` defaults to true.

So the effective transform is **`x / 255`** — the same as unwarping, and the
fifth normalization convention in this project. A reader who took `norm_type:
none` at face value would pass raw `0..255` values to the model.

### `target_size` is **reversed** before use

```python
op = Resize(target_size=target_size[::-1], ...)
```

`[800, 800]` is symmetric so nothing is observable here, and that is exactly why
it is worth recording: the first non-square layout model would silently transpose
its target.

### The resize does not preserve aspect ratio

`keep_ratio: false` means the `800x800` target is used directly, so a page is
distorted rather than letterboxed. `scale_factors` — `[width_out/width_in,
height_out/height_in]` — is what maps detections back, and it differs per axis.

## 3. Normalization conventions so far

| Path | Transform |
|---|---|
| Detector, both orientation classifiers | `(x/255 − ImageNet mean) / ImageNet std` |
| Recognizer, legacy text-line classifier | `(x/255 − 0.5) / 0.5` |
| Unwarping | `x/255` |
| **Layout** | **`x/255`**, reached through a config that says `none` |

`Normalize` applies them as `alpha = scale/std` and `beta = −mean/std`, per
channel through `cv2.split`, which is an evaluation order worth preserving for
the same reason `crop.rs` preserves OpenCV's cubic weight order.

## 4. What an implementation must produce

1. **A cubic resize to a fixed non-aspect-preserving target**, which this
   project does not have: `src/resize.rs` is linear, and `src/crop.rs`'s cubic
   path is projective with replicated borders.
2. **`scale_factors` carried through**, since detections come back in `800x800`
   space and must be divided per axis to reach the page.
3. **A captured oracle** in the shape `PRE-001` established, elementwise on the
   input tensor and on the boxes.
4. **A gate against the real artifact**, the `LANG-001` bar.
5. **A twenty-class map** recorded from the artifact rather than transcribed by
   hand.
6. **A resource position**: at `129,736,329` bytes the artifact is larger than
   the detector and recognizer combined, which changes the memory budget a
   caller loading everything must plan for.

## 5. Status

Contract frozen from the pinned PaddleX baseline, artifact provisioned and
hashed. No implementation, no fixture, no capture.

The pin earned itself here. `interp: 2` meaning bicubic, `norm_type: none`
meaning `x/255`, and the reversed `target_size` are all in the source and none is
in the config — and the first two would each have produced a working
implementation quietly fed the wrong pixels.
