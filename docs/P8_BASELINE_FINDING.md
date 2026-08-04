# P8 — The specialized modules are not in the pinned baseline

Roadmap items: `LAY-001`, `TBLCLS-001`, `TBLCELL-001`, `TBLSTRUCT-001`,
`FORM-001`, `SEAL-001`, `CHART-001`, `KIE-001`, `SR-001`
Recorded: 2026-08-04
Status: a blocking finding, not a decision — the decision it forces is `D-013`,
proposed below

Every capability this project has ported so far was frozen by reading the pinned
upstream checkout. P8 cannot be, and the reason is structural rather than a
matter of effort.

## 1. What is actually in the checkout

`PaddleOCR/paddleocr/_models/` contains a file per specialized module —
`layout_detection.py`, `table_structure_recognition.py`,
`formula_recognition.py`, `seal_text_detection.py`, and the rest. They look like
implementations. They are not.

`layout_detection.py` in full, minus imports, is:

```python
class LayoutDetection(ObjectDetection):
    @property
    def default_model_name(self):
        return "PP-DocLayout_plus-L"
```

`ObjectDetection` derives from `PaddleXPredictorWrapper`, and `base.py` opens
with:

```python
from paddlex import create_predictor
```

**PaddleX is not vendored in this checkout.** `pyproject.toml` declares it as an
external dependency, `paddlex[ocr-core]>=3.7.0,<3.8.0`.

So for every P8 module the pinned baseline contains a name, a default model, and
a CLI subcommand. The preprocessing, the postprocessing, the class maps, the
thresholds, the NMS, the token grammars — everything a contract would freeze —
lives in a package this project has never pinned and cannot read.

## 2. Why this is unlike anything so far

| Capability | Where its behaviour was frozen from |
|---|---|
| Detector, recognizer, crop, CTC | `tools/infer/*.py` and `ppocr/**` — in the checkout |
| Text-line orientation | `tools/infer/predict_cls.py` **and** `deploy/cpp_infer/**` |
| Document orientation, unwarping | `deploy/cpp_infer/**`, plus operator arguments from the artifact's `inference.yml` |
| **Every P8 module** | **nowhere in the checkout** |

The document-orientation case is the near miss worth naming. Its Python
predictor was absent too, and an earlier revision of
`ORIENTATION_CONTRACT.md` wrongly concluded the capability was absent — until the
C++ deployment tree turned out to contain a full implementation. That correction
is why this document checked more than one language before concluding anything.

Here the check comes back empty in all of them:
`deploy/cpp_infer/src/modules/` contains exactly four modules —
`image_classification`, `image_unwarping`, `text_detection`,
`text_recognition`. There is no layout, table, formula, seal, chart, KIE, or
super-resolution module in the C++ tree, and `OCR.yaml` never mentions one.

## 3. Why artifact configs are not enough either

The orientation models were frozen partly from their own `inference.yml`, which
named `ResizeImage`, `CropImage`, `NormalizeImage`, and `Topk`. That worked
because the **C++ source defined what those operators do** — the rounding in
`ResizeByShort`, the integer division in `Crop`, the `cvRound` in the `uint8`
conversion. Three of those were load-bearing and none was in the config.

A P8 artifact's config would name operators with no definition anywhere in the
pinned tree. Implementing from it would mean guessing each operator's rounding,
border, and ordering — precisely the guesses that this project has caught being
wrong four times: the batch split, the batch-width truncation, the rotation's
one-pixel offset, and the `convertTo` rounding. Every one of those was found by
comparing against a capture, and every one would have produced working code that
was quietly wrong.

## 4. The decision this forces — proposed `D-013`

P8 cannot proceed under the current baseline. Three options, with their costs:

1. **Pin PaddleX as a second baseline.** A second read-only reference at a
   pinned version, alongside `PaddleOCR`. Restores the method exactly: read the
   source, freeze the contract, capture the oracle, compare. Cost: a second
   upstream to track, and `pyproject.toml` allows a range (`>=3.7.0,<3.8.0`), so
   the exact version must be chosen and recorded rather than inherited.
2. **Freeze from captures alone.** Treat each model as a black box: capture
   input tensors and outputs from the reference implementation and match them,
   without reading its source. Weaker — a capture shows *what* happened on the
   cases captured, not *why*, so edge cases outside the corpus stay unknown. It
   is how this project would have missed the `ceil`-versus-truncate boundary,
   which only one crop shape in ten exposes.
3. **Declare P8 out of scope** and close its rows as user-approved exclusions,
   the way office formats were closed in `DOCIO-DEC-001`.

### Option 1 was checked, not assumed

The recommendation below would be worth little if PaddleX turned out to be
another layer of wrappers. It is not. `paddlex` `3.7.2` — the version the
declared range `>=3.7.0,<3.8.0` currently resolves to — contains
`paddlex/inference/models/object_detection/processors.py`, whose classes are
exactly what a contract needs to freeze:

```
ReadImage   Resize   Normalize   ToCHWImage   ToBatch
DetPad      PadStride           WarpAffine   DetPostProcess
```

That is the same shape of source the classic path was frozen from: operators with
their rounding, padding, and ordering written out. Pinning it restores the method
in full rather than approximating it.

The version must be **chosen and recorded**, not inherited: upstream declares a
range, and a range is not a baseline. `3.7.2` is what it resolves to today.

**Recommendation: option 1**, because it preserves the only method that has
actually caught this project's mistakes. Option 2 is available as a fallback for
any single module whose PaddleX source turns out to be unreadable or absent.

This is a material scope decision and needs a recorded user answer before P8
work begins. Until then every P8 row stays `Blocked` with this document as the
reason, rather than `Planned` — because `Planned` implies the work is merely
unstarted, and it is not.

## 5. What is not affected

P9 (structured pipelines) depends on P8 and is blocked with it. P10–P12 are
independent of this finding and blocked on their own decisions — `D-010` for VLM,
and a serving and training scope that no artifact in this repository touches.

The classic OCR path, document preprocessing, and everything in P13 and P14 are
unaffected: they were frozen from sources that are in the checkout, and they stay
frozen.
