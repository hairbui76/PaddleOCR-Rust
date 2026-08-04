# Table Classification Contract

Roadmap item: `TBLCLS-001`
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: contract frozen, artifact provisioned, **implemented and matched**

The second capability frozen from the PaddleX pin, and the first frozen by
**executing** the reference rather than transcribing it.

## 1. What the artifact declares

`PP-LCNet_x1_0_table_cls/inference.yml`:

```yaml
PreProcess:
  transform_ops:
  - ResizeImage: {resize_short: 256}
  - CropImage:   {size: 224}
  - NormalizeImage:
      mean: [0.485, 0.456, 0.406]
      std:  [0.229, 0.224, 0.225]
      scale: 0.00392156862745098
      order: ''
      channel_num: 3
  - ToCHWImage: null
PostProcess:
  Topk: {topk: 5, label_list: [wired_table, wireless_table]}
```

Two classes, one input at `[N, 3, 224, 224]`.

That chain is the **same shape** as the document orientation classifier's —
resize by short edge, centre crop, ImageNet normalize, top-k. It is not the same
behaviour, and the difference is the entire subject of this document.

## 2. Two operators with the same name and different results

Document orientation was frozen from `deploy/cpp_infer/`. This model is reachable
only through PaddleX, so its operators are the Python ones.

| Operator | C++ baseline | PaddleX baseline |
|---|---|---|
| `ResizeByShort` | `std::round` — half **away from zero** | `round` — half **to even** |
| `Normalize` | `(x * scale − mean) / std` | `x * (scale/std) + (−mean/std)` |

### The rounding is reachable, not theoretical

`scale = 256 / min(h, w)`, and a `512x1025` page scales by exactly `0.5`. Its
height lands on `512.5`:

| Rule | Result |
|---|---|
| `std::round` | `513` |
| Python `round` | **`512`** |

Every value in the tensor after that differs, and nothing about the output looks
wrong — a `513`-tall intermediate crops to `224x224` just as happily as a
`512`-tall one.

Both directions are captured. A `1024x1030` page gives `257.5`, whose even
neighbour is `258`, so the same rule rounds it **up**. A capture that only ever
rounded down would also have passed with plain truncation, which is a different
rule that happens to agree on one case.

### The normalization is the same arithmetic in a different order

`Normalize.__init__` folds the constants once — `alpha = scale/std`,
`beta = −mean/std` — and `norm` then multiplies and adds per channel through
`cv2.split`. `crate::tensor::classic_normalized_batch` divides last instead.

The two are algebraically identical and not identical in `f32`. Reusing the
existing helper would have been wrong in the last bit rather than visibly broken,
which is the failure mode this project has the least ability to notice by
inspection.

## 3. Channel order

`ReadImage(format="RGB")` converts **before** the resize, so this model sees
`RGB` and the ImageNet constants apply in that order. `src/table_classification.rs`
takes an already-RGB image and does not reorder, because a silent swap would be
indistinguishable from a correct result.

## 4. `topk: 5` against two classes

Upstream slices `preds.argsort(axis=1)[:, -5:][:, ::-1]`. NumPy clamps a slice
that overruns, so the effective width is `2` rather than an error. The
implementation reproduces the clamp rather than the literal, and a test asserts
that `TABLE_CLS_TOPK > TABLE_CLS_LABELS.len()` so the discrepancy stays
deliberate.

Two more details of `Topk` are captured rather than assumed:

- **Ties go to the higher class index.** The sort is ascending and then
  reversed, which is the opposite of what a descending sort gives.
- **Scores are rounded to five decimals**, half to even, in `f32`. Both captured
  boundary values sit on that tie and they round in **opposite** directions —
  `0.123455` up to `0.12346`, `0.876545` down to `0.87654` — which is what
  half-to-even does and what half-away-from-zero would not.

## 5. What the capture does differently

Every earlier capture in this project reimplemented the upstream operators inside
the capture tool and relied on that reimplementation being faithful. Three
recorded bugs came from exactly that gap.

`tools/capture_table_classification_oracle.py` **imports the pinned PaddleX
operators and calls them**. A transcription error is therefore not among the ways
this oracle can be wrong. It remains a development-time tool: nothing in the
build, test, or runtime path touches PaddleX, and the fixture is a plain JSON
file.

The model is not run. Scores are synthetic, which pins `Topk` exactly as well as
real ones would and needs no inference session.

## 6. The provisioned artifact

Apache-2.0, stored outside version control per `MODEL-DEC-001`.

| Field | Value |
|---|---|
| Model | `PP-LCNet_x1_0_table_cls`, from [`PaddlePaddle/PP-LCNet_x1_0_table_cls_onnx`](https://huggingface.co/PaddlePaddle/PP-LCNet_x1_0_table_cls_onnx) |
| Revision | `605f623b09f67a562bae77e781d5d5266f14905a` |
| `inference.onnx` | SHA-256 `f8e4cb1b58a29bebd36852edcb238b53e0acefa75116a8a4db443f13dbc72b0b`, `6,777,817` bytes |
| `inference.yml` | SHA-256 `891b1f4b0ccddaf6aca0fce8c8a38e5ab5da9f62fb9adaa7c75e161ee03bb787`, `762` bytes |

At `6.8 MB` it is the smallest artifact this project has provisioned — a
twentieth of the layout detector.

## 7. Oracle results

Six tensors, every one reproduced **bit-identically**: the sha256 over all
`150,528` `f32` values matches in all six cases, and the four `Topk` cases match
to the bit.

| Case | Source | Resized |
|---|---|---|
| `plain_portrait_297x421` | `297x421` | `256x363` |
| `half_rounds_down_512x1025` | `512x1025` | `256x512` |
| `half_rounds_up_1024x1030` | `1024x1030` | `256x258` |
| `short_side_already_256` | `256x300` | `256x300` — no resize |
| `square_256` | `256x256` | `256x256` — no resize |
| `wide_640x300` | `640x300` | `546x256` |

The two no-resize cases are there for a reason of their own: `F.resize`
short-circuits when the target size equals the source, so those inputs never
reach `cv2.resize` at all, and the implementation skips the call rather than
relying on a linear resize to identity being exactly the identity.

## 8. Status

Implemented in `src/table_classification.rs` and matched against the capture.

The module is **not wired into any pipeline**. Table classification selects
between two downstream structure models this port does not have, so exposing a
classifier whose answer nothing can act on would widen the public surface for no
capability. Composition is `P9`'s subject — the same position `src/layout.rs` and
`src/unwarp.rs` already hold.

The pin earned itself a second time here. Both divergences in §2 are in the
PaddleX source and neither is in the config, and the rounding one would have
produced a working implementation quietly fed a differently-scaled image on
exactly the inputs where the two baselines disagree.
