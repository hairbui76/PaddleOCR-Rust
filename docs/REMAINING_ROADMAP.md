# Remaining Port Roadmap and Next-Agent Handoff

Status date: 2026-08-04
Canonical authority: [`ROADMAP.md`](../ROADMAP.md)
Current delivery state: bounded PNG input decoding exists in Rust; no functional OCR pipeline, public OCR API, or OCR
CLI exists yet

## How to use this handoff

This file is an execution-oriented index for the next agent. It does not
replace `ROADMAP.md`: every scope, status, dependency, decision gate, and
acceptance criterion remains authoritative there. Before changing code or
architecture, read `AGENTS.md`, then `ROADMAP.md`, then the owner documents
linked below. Update `ROADMAP.md` first if new work changes scope or resolves a
decision.

Do not state that PaddleOCR, PP-OCRv6, a detector, a recognizer, or an OCR CLI
is supported until the corresponding roadmap gate is actually complete.

## Exact stop point

P0 and P1 are done. P2, P3, P4, and P13 are in progress. P5 through P12 and
P14 remain planned. The implemented Rust foundations are private, bounded
geometry/crop, DB thresholding/components, CTC greedy-index work, and bounded
PNG decoding into the classic BGR convention; they are not a detector,
recognizer, or end-to-end OCR implementation, and `src/main.rs` still exits
with status `2`.

The active critical path is P3 `RT-003`: establish a defensible independent
static/Paddle raw-tensor reference for the selected PP-OCRv6 medium ONNX pair,
then use that evidence to choose a runtime. The selected exact external model
packages are:

| Candidate key | External directory | Pinned revision |
|---|---|---|
| `m2-static-det-v6-medium` | `/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-static-det-v6-medium` | `8e0f56fb2ef86b461d99cfc7ac5c137738985f61` |
| `m2-static-rec-v6-medium` | `/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-static-rec-v6-medium` | `e5a92bcbc5cc1b494628e458d267778f0704fd7c` |
| `m2-onnx-det-v6-medium` | `/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-onnx-det-v6-medium` | `61323801669c338b7891481ec7bac61ce31b576a` |
| `m2-onnx-rec-v6-medium` | `/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-onnx-rec-v6-medium` | `50c7eacafc52fa7bcf4194e8cd08e46f8558504b` |

Their external machine record is
`/mnt/ssdvolumes/models/paddleocr-v6-medium/provisioning-evidence.json`.
It is not a final manifest or an approval to bundle/distribute a model.

## Static-versus-ONNX work left unfinished

The static pair has a confined terms review for this external oracle only;
see [`LICENSE_REVIEW.md`](LICENSE_REVIEW.md). It is not selected as the Rust
backend or release artifact.

One isolated static/Paddle versus ONNX capture did complete all six declared
LCG shapes. Its full aggregate evidence and limitations are in
[`RUNTIME_PROOF_PLAN.md`](RUNTIME_PROOF_PLAN.md#first-staticpaddle-capture-partial-2026-08-04).
The result must remain **partial**, for two independent reasons:

1. Only one fresh process completed. The required second fresh process and
   determinism comparison were deliberately not run.
2. All absolute errors were below `1e-4`, but the harness used an unapproved
   relative denominator floor (`1e-12`), so its near-zero relative values failed
   the then-undefined relative term.

`m2-tensor-v1` was amended on 2026-08-04 under an explicit user delegation. The
predeclared rule is now
`abs(candidate - reference) <= 1e-4 + 1e-4 * abs(reference)`, evaluated
elementwise on `float64` promotions with the independent reference on the
reference side; see
[`FIXTURE_AND_TOLERANCE_PLAN.md`](FIXTURE_AND_TOLERANCE_PLAN.md#m2-tensor-v1-comparison-rule-resolved-2026-08-04).
The earlier capture is **not** retroactively relabelled as passing: it still has
only one fresh process and no determinism comparison.

To resume this experiment, rebuild a disposable
external harness, verify every model package file before loading it, run the
six exact shapes twice in fresh processes, retain no raw tensors, and record
only compact hashes and aggregate comparison data. Never run PaddleOCR or
PaddleX during this direct static/Paddle reference experiment.

## Ordered remaining execution plan

### 1. Close P3 model/runtime evidence before adding inference code

1. `RT-003` is **complete**: the predeclared `m2-tensor-v1` rule was fixed
   first, then two fresh static/Paddle versus ONNX captures produced
   byte-identical aggregates with zero violations across 7,057,864 elements.
2. Complete `MOD-001` evidence: static graph/ABI inspection, actual runtime
   dictionary behavior, and a written static-versus-ONNX disposition. Do not
   treat a matching YAML or model name as equivalence. The static graph/ABI
   half is now recorded in [`STATIC_ABI_INSPECTION.md`](STATIC_ABI_INSPECTION.md)
   with a standard-library parse-only tool, together with the structural
   static-versus-ONNX comparison; the runtime dictionary behaviour and the
   numerical half of the disposition still depend on `RT-003`.
3. Complete the unresolved `RT-002` evidence: baseline-CPU/no-AVX coverage,
   resource/error behavior, longer reuse/soak, cancellation/concurrency policy,
   native dependency and unsafe-boundary review, and supply-chain evidence.
4. Resolve `RT-004` / `D-006` in an ADR. The choice must name the exact
   artifact, backend version/features, CPU baseline, numerical results,
   limitations, rejected alternatives, and migration path.
5. Resolve `MODEL-DEC-001` / `D-007`, then implement `RT-005` and `MOD-002`
   through `MOD-003`: a small internal backend-neutral adapter, explicit
   hash-checked local model resolution, typed tensor/shape errors, resource
   bounds, and offline behavior. `MOD-004` download support remains later and
   opt-in only.

Do not add `ort`, Paddle, Python, a model path, a downloader, or a runtime
feature to the project merely to make a smoke test pass. The backend decision
must precede that integration.

### 2. Finish P2/P4 input and tensor foundations

1. Complete remaining `FIX-001`, `TOL-001`, and `COMP-002` fixture/ledger
   work: detector thresholds and tensor representatives. The `m2-tensor-v1`
   comparison rule and the `classic-v1-image-inputs` exact/unsupported
   classification are now resolved.
2. `D-008` (M2 image portion) and `IMG-DEC-001` are resolved: PNG-only via
   `png` 0.18.1, recorded in [`IMAGE_DECODER_DECISION.md`](IMAGE_DECODER_DECISION.md).
   JPEG is deferred to the new `IMG-003` item, whose entry gate is a
   tensor-level measurement of the recorded component delta `36`.
3. `IMG-001` is implemented in `src/image.rs`. `IMG-002`, `TEN-001`, and
   `PRE-001` remain: exact per-model scale/mean/std policy,
   resize/pad/normalization/layout, and reproducible model input tensors.
4. Complete `CROP-001` pixel evidence and `SEC-IMG-001` malformed/fuzzing
   coverage. Existing crop/geometry work is only a bounded precursor, not a
   universal OpenCV or decoded-image parity claim.

### 3. Build verified OCR components (P5)

1. `REC-001` through `REC-004`: freeze resize/pad/batching and dictionary
   semantics, integrate the approved runtime, safely bind CTC indexes to the
   verified dictionary, then test text, scores, Unicode, invalid outputs, and
   resource limits.
2. `DET-001` through `DET-004`: freeze preprocessing and DB policy, integrate
   detector inference, implement contour/geometry/unclip/scoring/postprocess,
   then test no-text, rotations, edges, thresholds, degenerate regions, and
   ordering.
3. Implement orientation only if the approved scope requires it; do not make a
   generic orientation or multilingual claim from the chosen pair.
4. Expose standalone module APIs only after their component-level golden and
   differential requirements pass.

### 4. Deliver the first usable M2 slice (P6)

Implement and verify the complete classic sequence:

```text
decode -> detector -> stable reading order -> perspective crop -> tall rotation
-> optional orientation -> aspect-sorted recognition batch -> restore order
-> inclusive score filter -> typed Rust API -> versioned JSON/JSONL -> CLI
```

`OCR-001` through `E2E-001` require offline, explicitly provisioned models;
deterministic results; typed errors; bounded batches/resources; missing/corrupt
model checks; and no Python or upstream checkout at build, test, or runtime.
Only after that may `DOC-USER-001` document a supported first release.

### 5. Continue only through the pinned remaining scope

After a verified M2 release, follow the already-declared dependency order:

| Phases | Remaining target |
|---|---|
| P7 | Document input/preprocessing and bounded multipage handling (`M3`). |
| P8–P9 | Scoped structure/specialized modules and structured document pipelines (`M4`). |
| P10 | Scoped VLM/GenAI capabilities. |
| P11 | Service, deployment, and ecosystem targets (`M5`). |
| P12 | Native training, evaluation, export, and optimization (`M6`). |
| P13 | Security, licensing, performance, reliability, and platform final gates. |
| P14 | Compatibility closeout and user-approved release (`M7`). |

Do not collapse or silently mark any of these areas out of scope. A direct user
decision may change scope, but it must be recorded in `ROADMAP.md` first.

## Non-negotiable operating constraints

- `PaddleOCR/` is a read-only symlink to the upstream Python checkout. Never
  modify, stage, format, test, install in, or otherwise write through it.
- Keep all model weights, caches, raw outputs, temporary runtime builds, and
  external harnesses outside the repository. Do not commit them.
- Preserve the user-owned untracked file `0.3`; do not stage, edit, delete, or
  reset it.
- Normal Rust builds/tests must remain offline and independent of Python,
  PaddleOCR, PaddleX, GPU, the model directory, and the upstream symlink.
- Public conversational replies are Vietnamese by default. Source, docs,
  comments, commands, commit messages, errors, and all repository artifacts
  remain English.
- Use the normal locked gates after a code change and report only checks that
  actually ran. Use `/usr/bin/gcc` as the Cargo linker in this environment.

## Handoff verification checklist

Before resuming implementation, the next agent should run read-only checks:

```sh
git status --short
git -C PaddleOCR status --short
git log --oneline -5
```

Then read the exact owner documents for the next roadmap item. A clean Rust
test suite does not prove any model/runtime, OCR, or release gate that it does
not cover.
