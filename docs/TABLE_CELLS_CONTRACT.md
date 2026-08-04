# Table Cell Detection Contract

Roadmap item: `TBLCELL-001`
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: contract frozen, **implemented and matched** — and it corrected `LAY-001`

`RT-DETR-L_wired_table_cell_det` and `RT-DETR-L_wireless_table_cell_det` declare
the **same operator chain as the layout detector**, differing only in the target
side and the class list. That made this the cheapest module in `P8` to port and
the most valuable one to capture, because it put a second, independent oracle on
a code path that already had one.

The second oracle disagreed with the first. It was right.

## 1. What the artifacts declare

Both `inference.yml` files, on every field this module reads:

```yaml
arch: DETR
draw_threshold: 0.5
Preprocess:
- {type: Resize, target_size: [640, 640], keep_ratio: false, interp: 2}
- {type: NormalizeImage, norm_type: none, mean: [0,0,0], std: [1,1,1]}
- {type: Permute}
label_list: [cell]
```

`interp: 2` is bicubic and `norm_type: none` means `x/255`, both for the reasons
`docs/LAYOUT_CONTRACT.md` records. `DETR` appears in `model_name`, which puts
both models in `models_required_imgsize`, so `ToBatch` hands them the same three
inputs in the same order the layout detector gets.

The two artifacts differ only in weights. There is one code path and a
`TableCellModel` tag recording which artifact produced a result.

## 2. Two bugs in `LAY-001`, found by capturing this one

`LAY-001`'s oracle was captured by an ad-hoc script that **transcribed** the
upstream operators. This one **executes** them. Where the two disagreed, the
executing capture was right, and the transcription was wrong twice.

### The normalization is a multiply, not a divide

Upstream folds `alpha = scale/std` once in `Normalize.__init__` and then
multiplies. The transcription divided by `255`.

```
x * f32(1/255)   !=   x / 255.0f
```

`1/255` is not representable in binary, so the two disagree on **`126` of the
`256` possible byte values** — roughly half of every input, each by one `f32`
step. The layout tensor had been wrong in about half its elements since
`LAY-001` was marked done, and its own oracle agreed with it because both sides
made the same mistake.

### The source was `BGR` where upstream reads `RGB`

`ObjectDetection` builds `ReadImage(format="RGB")`, which converts before the
resize. The transcription fed OpenCV's native `BGR` straight through, which
**swaps two of the three planes** of the tensor.

Neither bug produces an error, an exception, or an implausible box. Both produce
a working detector fed the wrong pixels.

### Why the transcription is the root cause

This project has now recorded four bugs whose cause was reading an operator and
reimplementing it, rather than running it. The fix is not more care; it is the
capture method. `tools/capture_layout_oracle.py` is committed as part of this
change and imports the pinned operators, so `LAY-001` can no longer drift from
its reference in a way its own oracle cannot see.

## 3. What the cubic divergence actually measures

`LAYOUT_CONTRACT.md` recorded `23` differing bytes of `1,920,000` at
`297x421 → 800x800`. The `640` target gives a second data point, measured the
same way through `resize_cubic`'s probe:

| Case | Differing bytes |
|---|---|
| `297x421 → 800` | `23` of `1,920,000` |
| `297x421 → 640` | **`10`** of `1,228,800` |
| `960x240 → 640` | `0` |
| `120x90 → 640` | `0` |
| `640x640 → 640` | `0` |

So the defect is not a property of "page scale" as such — three of the five
cases are exact, including one that upscales both axes by more than five times.
It survives, bounded, and the probe is now parameterized by its reference file so
a new target can be measured without copying the diagnostic.

## 4. What an implementation must produce, and what this one does

`src/paddlex_detection.rs` holds the shared path — input tensor, both reversed
batch inputs, and the `[N, 6]` decode — and `src/layout.rs` and
`src/table_cells.rs` are thin over it. Sharing rather than copying is deliberate:
the cubic resize has an open defect, and a copy would mean fixing it twice or,
more likely, fixing it once.

The decoder carries a `DetectionFields` value so that sharing does not blur which
model refused. A generic `detection.class_index` would be a correct error and a
worse one; the field is the only part of a typed error that says where to look.

## 5. Oracle results

Four tensors at `640`. Three reproduce **bit-identically**; `table_crop_297x421`
inherits the cubic divergence and is **bounded** — no sampled value off by more
than one 8-bit step, no more than two differing samples — rather than accepted.

Both reversed batch inputs are checked against the capture on a deliberately
non-square page, where a transposed pair is visible rather than plausible.

No model is run. The two artifacts' configs agree on every field this module
reads, so the preprocessing is the whole compatibility surface a capture can pin
without making an accuracy claim this fixture is not entitled to make.

## 6. Status

Implemented and matched, and **not wired into any pipeline**: cell boxes are an
input to table structure recognition, which this port does not have. Composition
is `P9`'s subject.

The correction to `LAY-001` is the part of this change that matters most. A
module that was marked done was quietly feeding its model roughly half-wrong
values in swapped planes, and the only reason it is not still doing so is that a
second model shared its code path and was captured by a better method.
