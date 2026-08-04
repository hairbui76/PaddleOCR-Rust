# Table Structure Recognition Contract

Roadmap item: `TBLSTRUCT-001`
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: contract frozen, artifact provisioned, **implemented and matched**

The first `P8` module whose contract is not a variation on one already ported.
Its preprocessing is new in three ways and its postprocess emits **HTML
structure tokens** rather than boxes.

## 1. What the artifact declares

`SLANeXt_wired/inference.yml`:

```yaml
PreProcess:
  transform_ops:
  - DecodeImage:        {channel_first: false, img_mode: BGR}
  - TableLabelEncode:   {merge_no_span_structure: true, ...}
  - TableBoxEncode:     {in_box_format: xyxyxyxy, out_box_format: xyxyxyxy}
  - ResizeTableImage:   {max_len: 512, resize_bboxes: true}
  - NormalizeImage:     {mean: [...], std: [...], scale: '1./255.', order: hwc}
  - PaddingTableImage:  {size: [512, 512]}
  - ToCHWImage:
PostProcess:
  name: TableLabelDecode
  merge_no_span_structure: true
  character_dict: [<thead>, </thead>, ..., ' rowspan="20"']   # 47 entries
```

`TableLabelEncode` and `TableBoxEncode` are registered to functions returning
`None` — they are training-time operators and do not run at inference. Only
`merge_no_span_structure`, read out of `TableLabelEncode`'s arguments, survives
into the decoder.

## 2. Three facts that live in the registration functions

Reading the operators alone gives the wrong answer for all three.

| Fact | What the operator says | What actually runs |
|---|---|---|
| The pad value | `Pad.__init__` defaults to `127.5` | `build_padding` passes `pad_value=0` |
| The scale | config says `scale: '1./255.'` | `build_normalize` **never forwards it** |
| The order | — | pad runs **after** normalize |

### The pad runs after the normalize

This is the one that changes the picture most. The border is a **normalized
zero**, not a black pixel — a black pixel would be
`(0/255 − 0.485)/0.229 = −2.117` in the first channel. Padding before
normalizing produces a plausible border and the wrong one, and nothing
downstream can tell the two apart.

The test asserts both values, so the distinction is checked rather than
described.

### `build_normalize` discards the config's scale

```python
def build_normalize(self, mean=..., std=..., scale=1 / 255, order="hwc"):
    return Normalize(mean=mean, std=std)      # `scale` is accepted and dropped
```

The config's value is the *string* `'1./255.'`, which never has to be parsed
because it is never read. Nothing differs today — `Normalize`'s own default is
the same number — but a different scale in a future config would be silently
ignored, so it is recorded and asserted.

## 3. A fourth channel order

`DecodeImage` declares `img_mode: BGR`, and `build_readimg` asserts it. Every
other PaddleX model this port has touched reads `RGB`.

| Model | Channel order |
|---|---|
| Layout, table cells, table classification | `RGB` |
| **Table structure** | **`BGR`** |

Getting this backwards swaps two of the three planes. That is not hypothetical:
`docs/TABLE_CELLS_CONTRACT.md` records finding exactly that bug in `LAY-001`.

## 4. The vocabulary is built, not read

`TableLabelDecode.__init__` **mutates** the character list it is given: with
`merge_no_span_structure` it appends `<td></td>` and removes `<td>`, then
`add_special_char` wraps the result in `sos` and `eos`.

So the config's 47-entry `character_dict` is not the vocabulary — it is the
input to a transformation, and the indices the model emits point into the
**50-entry result**. A port that indexed the config list directly would be off
by one from `<td` onwards and would silently emit the wrong token for every cell.

The mutation is in place, which the capture tool has to work around: it copies
the list per call, because reusing the config's own list gives a different
vocabulary the second time.

## 5. Two decode rules captured rather than assumed

- **`argmax` ties go to the lower class index.** NumPy returns the first
  maximum. That is the **opposite** of `Topk`'s rule in
  `docs/TABLE_CLASSIFICATION_CONTRACT.md`, where the sort-then-reverse puts the
  higher index first. Two decoders in the same repository, two opposite
  tie-breaks, both captured.
- **`sos` and `eos` are ignored wherever they appear**, but `eos` past position
  zero also **breaks** the loop. A sequence beginning with `sos` therefore
  decodes normally, and one with no `eos` runs to the end. Both are captured.

`_get_bbox_scales` has two branches — `SLANet` uses the original size directly,
`SLANeXt` uses the padded size over the resize ratio — and the `SLANeXt` pair is
**named backwards** in the source: it returns `w/ratio, h/ratio` into variables
called `h_scale, w_scale`. With a square pad both values collapse to the long
side, so the naming costs nothing today. It is reproduced rather than tidied,
because a future non-square pad would make it matter.

## 6. The provisioned artifact

Apache-2.0, stored outside version control per `MODEL-DEC-001`.

| Field | Value |
|---|---|
| Model | `SLANeXt_wired`, from [`PaddlePaddle/SLANeXt_wired_onnx`](https://huggingface.co/PaddlePaddle/SLANeXt_wired_onnx) |
| Revision | `04356de883011f433f83e5098793f3a501a9af6e` |
| `inference.onnx` | SHA-256 `0a6e063b56e35a434eb6669eb2342113c6bd76a6ce5acaa0331f370c9e00732f`, `367,743,373` bytes |
| `inference.yml` | SHA-256 `abbbd1b4dc6b1a2e9cd34c035514da53a1a6b1ec267292b0b8802025650a33bf`, `2,066` bytes |

At `368 MB` it is by a wide margin the largest artifact this project has
provisioned — nearly three times the layout detector, and more than the classic
detector and recognizer combined. That is a resource fact a caller loading a full
table pipeline has to plan for.

## 7. Oracle results

Five tensors, every one reproduced **bit-identically**, and five decode cases
matched exactly on tokens, score, and cell boxes.

| Case | Source | Resized |
|---|---|---|
| `landscape_800x300` | `800x300` | `512x192` |
| `portrait_300x800` | `300x800` | `192x512` |
| `square_512` | `512x512` | unchanged, no pad |
| `small_256x171` | `256x171` | `512x342`, upscaled |
| `half_boundary_513x1025` | `513x1025` | `256x512` |

The decode cases cover a simple table, a spanning cell, a leading `sos`, an
unterminated sequence, and the `SLANet` box-scaling branch.

The model is never run. Structure probabilities are synthetic, which pins the
decode exactly as well as real ones and needs no inference session for a
`368 MB` artifact.

## 8. Status

Implemented in `src/table_structure.rs` and matched. **Not wired into any
pipeline**: structure tokens compose with cell boxes and recognized text to make
a table, and that composition is `P9`'s subject.

With this row done, three of the nine `P8` modules are complete. Four of the
remaining six have **no published ONNX export** and are blocked under
`MODEL-DEC-001` — see `docs/P8_ARTIFACT_AVAILABILITY.md`, which is the ceiling
on P8 and is not about effort.
