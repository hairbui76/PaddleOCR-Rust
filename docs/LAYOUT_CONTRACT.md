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

## 5. The cubic resize, implemented and matched

`src/resize_cubic.rs` implements `cv2.resize` with `INTER_CUBIC`, reproducing all
five captured cases **byte for byte** — pure upscale, pure downscale, mixed axes,
a `4x` upscale, and a heavy downscale.

Two details carried it:

- **The mapping is centre aligned**: `src = (dst + 0.5) * scale - 0.5`. Dropping
  the half pixel makes a `2x` upscale sample at integers and reproduce the source
  exactly in every other column, which looks plausible and matches nothing.
- **The border replicates**, and the cubic weights are the same construction
  `src/crop.rs` already pins against `72` captured OpenCV cases, shared rather
  than rewritten.

Neither existing path was reusable: `src/resize.rs` is `INTER_LINEAR` with
fixed-point weights, and `src/crop.rs`'s cubic sampler is a projective warp for
an arbitrary quadrilateral rather than the separable axis-aligned scale
`cv2.resize` performs.

## 6. A fifth reversal, and a correction to §5

`ToBatch` reverses **again** what `Resize` computed:

```python
if key == "img_size":       img_sizes = [data[key][::-1] for data in datas]
elif key == "scale_factors": scale_factors = [data.get(key, [1.0, 1.0])[::-1] ...]
```

So the model receives `im_shape` as `[h, w]` and `scale_factor` as
`[h_scale, w_scale]`, where `Resize` produced `[w, h]` and `[w_scale, h_scale]`.

This one a reading did not catch — a capture did. Passing the unreversed factor
produced boxes reaching `y = 1021` on a `720`-tall page. **Coordinates outside
the source image are the cheapest signal that a transform is wrong**, and the
oracle test now asserts containment for exactly that reason.

### The cubic resize does not match at page scale

§5 said the cubic resize reproduces its captured cases byte for byte. That is
true of the five committed cases and **false at page scale**: a `297x421` to
`800x800` resize differs from OpenCV in `24` bytes out of `1,920,000`, each off
by one.

Seven variants have been tried and measured against the `297x421` to `800x800`
case, which is the only size that exposes the defect at all:

| Attempt | Differing bytes of `1,920,000` |
|---|---|
| **Two passes, `crop.rs` weight form** — current | **23** |
| Two passes, `interpolateCubic`'s own weight form | 31 |
| Fused `4x4` accumulation, `crop.rs` weight form | 24 |
| Fused `4x4` accumulation, `interpolateCubic`'s form | 33 |
| Fixed point, descaling between passes | 82,990 |
| Fixed point, single descale by `22`, read from `resize.cpp` | 74,043 |
| Fixed point horizontal, float vertical, per `VResizeCubicVec_32s8u` | 73,914 |

Two things are settled. **Fixed point is not the path this dispatch takes**:
three variants, all read from `resize.cpp` rather than guessed, all landing
around `74,000`, which is not a near miss but a different algorithm. And **the
two-pass structure is right** — `HResizeCubic` and `VResizeCubic` are separate
structs with a buffer between them, float addition is not associative, and
separating the passes improved every coefficient form it was paired with.

One thing is not settled, and is recorded as an oddity rather than explained
away: the coefficient form that measures better is `crop.rs`'s **warp** table,
not `resize`'s own `interpolateCubic` — by `8` bytes, consistently, in both
structures. That is the opposite of what reading the source predicts, which
means the remaining cause is somewhere none of these seven attempts reached.

The best measured implementation stands, the defect is open, and the test bounds
it at `23` so it cannot grow unnoticed.

## 7. Status

Contract frozen from the pinned PaddleX baseline, artifact provisioned and
hashed, and the first operator it needs — the cubic non-aspect-preserving resize
— implemented and matched byte for byte against a capture.

`src/layout.rs` now builds the input tensor, computes the reversed scale factor,
and decodes detections into source-page regions with the twenty-class map. Two of
three captured tensors reproduce bit-identically; the third is recorded as
`reproduced_exactly: false` in the fixture, and the test **bounds** its
divergence — no sampled value off by more than one 8-bit step, no more than two
differing samples — rather than hiding it behind a tolerance.

The module is not wired into any pipeline. Composing layout with the classic path
is `P9`'s subject, and exposing an API built on an operator that is knowingly one
step off at page scale would sell a precision this port does not have.

The pin earned itself here. `interp: 2` meaning bicubic, `norm_type: none`
meaning `x/255`, and the reversed `target_size` are all in the source and none is
in the config — and the first two would each have produced a working
implementation quietly fed the wrong pixels.
