# P8 — Four modules have no published ONNX export

Roadmap items: `SEAL-001`, `FORM-001`, `CHART-001`, `KIE-001`
Recorded: 2026-08-05
Status: a blocking finding under an existing decision, not a new decision

`docs/P8_BASELINE_FINDING.md` established that P8 could not be frozen from the
pinned PaddleOCR checkout, and `D-013` resolved that by pinning PaddleX `3.7.2`.
That unblocked the **contracts**. It says nothing about the **artifacts**, and
four of the six remaining modules fail there instead.

## 1. What `MODEL-DEC-001` already decided

> This project performs **no model conversion** — no Paddle-to-ONNX step, no
> quantization, no graph rewriting, no operator fusion of its own.
>
> **Consequence.** If a needed model has no published ONNX export, that model is
> out of scope until either an export exists or a conversion policy is written
> with its own verification. "Convert it locally and hope" is not available.

That decision was made for M2 and it applies here unchanged. This document is
the consequence being collected, not a new position.

## 2. What is actually published

Checked against the Hugging Face API on 2026-08-05, by listing each repository's
files rather than by trying a URL and reading the error:

| Module | Default model | Repository | ONNX |
|---|---|---|---|
| `TBLCLS-001` | `PP-LCNet_x1_0_table_cls` | `..._table_cls_onnx` | **yes** |
| `TBLCELL-001` | `RT-DETR-L_*_table_cell_det` | `..._onnx` | **yes** |
| `TBLSTRUCT-001` | `SLANeXt_wired` | `SLANeXt_wired_onnx` | **yes** |
| `SEAL-001` | `PP-OCRv4_mobile_seal_det` | `PP-OCRv4_mobile_seal_det` | **no** |
| `FORM-001` | `PP-FormulaNet_plus-M` | `PP-FormulaNet_plus-M` | **no** |
| `CHART-001` | `PP-Chart2Table` | `PP-Chart2Table` | **no** |
| `KIE-001` | `PP-DocBee2-3B` | `PP-DocBee2-3B` | **no** |

The four without an export publish `inference.json` plus `inference.pdiparams`,
or `model_state.pdparams` — Paddle's own formats. There is no `*_onnx`
repository for any of them, and the base repositories contain no `.onnx` file.

`PP-OCRv4_server_seal_det` was checked separately and is in the same position, so
the seal blocker is not specific to the mobile variant.

## 3. What is still worth doing for the blocked four

The contract half is not blocked. `SEAL-001` in particular is the cheapest of
them: `SealTextDetection` derives from PaddleX's `TextDetection`, whose operator
chain — `DetResizeForTest`, `NormalizeImage`, `ToCHWImage`, `DBPostProcess` — is
the one this project already implements for the classic detector. Its `inference.yml`
is a plain text file in a public repository and can be read without provisioning
weights.

What cannot be done is the part that makes a claim: **there is no artifact to
capture an oracle against**, so a contract frozen for those four would be a
reading with nothing to check it. This project has recorded five bugs whose only
detector was a capture — four from transcription, one from a shared code path —
and the pattern is consistent enough that "frozen from a careful reading" is not
a status this repository should award.

So the four rows stay `Blocked` with this document as the reason, rather than
`Planned`. `Planned` would imply the work is merely unstarted.

## 4. What would unblock them

Any one of:

1. **An upstream ONNX export appears.** Nothing here needs to change; the row
   becomes ordinary work.
2. **A conversion policy is written with its own verification** — the escape
   hatch `MODEL-DEC-001` names. That is a material decision and needs a recorded
   user answer, because it makes a conversion step part of this project's trust
   base: a divergence would then have two possible causes, which is precisely
   what `MODEL-DEC-001` was written to avoid.
3. **The rows are closed as out of scope**, the way office formats were closed
   in `DOCIO-DEC-001`.

No recommendation is offered between them, because the choice is about how much
of this port's verification story the user is willing to trade for coverage, and
that is not a technical question.

`KIE-001` has a second blocker regardless: `PP-DocBee2-3B` is a vision-language
model, which is `D-010`'s subject and unresolved.

## 5. `SR-001` is blocked differently, and more deeply

An earlier revision of this section said `SR-001` was "blocked on scope rather
than on artifacts". That was right about artifacts and wrong about the reason,
and the difference matters because it decides which decision could unblock it.

Super-resolution has **no inference path in either pinned baseline**:

| Checked | Result |
|---|---|
| `paddlex/inference/models/*sr*` | none |
| `paddlex/inference/pipelines/*sr*` | none |
| `PaddleOCR/paddleocr/_models/*sr*` | none |
| `PaddleOCR/configs/sr/` | **two files** |
| `PaddleOCR/ppocr/metrics/sr_metric.py` | present |

It exists only as **training configuration** — two configs, a loss, and a
metric — with no predictor, no pipeline, and no artifact published under any
plausible name.

So `SR-001` does not belong to P8's inference scope at all. It sits inside
`D-011`'s training scope, and **no artifact appearing would unblock it**, because
nothing in either baseline would consume one. See
`docs/TRAIN_DEC_001_EVIDENCE.md`.

That is a stronger block than the four rows above, and "blocked on scope"
understated it.

## 6. What this does not affect

`TBLSTRUCT-001` is done, and `TBLCLS-001` and `TBLCELL-001` with it.
