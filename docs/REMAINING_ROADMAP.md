# Remaining Port Roadmap and Next-Agent Handoff

Status date: 2026-08-05
Canonical authority: [`ROADMAP.md`](../ROADMAP.md)
Current delivery state: classic OCR, document preprocessing, layout detection,
the three-model table pipeline, and the `PP-StructureV3` orchestration all run
end to end over explicitly provisioned local ONNX artifacts, behind a typed Rust
API, three frozen JSON schemas, and a CLI with three commands. Every remaining
capability is blocked on something outside this repository, or closed by a
recorded decision.

## How to use this handoff

This file is an execution-oriented index for the next agent. It does not
replace `ROADMAP.md`: every scope, status, dependency, decision gate, and
acceptance criterion remains authoritative there. Before changing code or
architecture, read `AGENTS.md`, then `ROADMAP.md`, then the owner documents
linked below. Update `ROADMAP.md` first if new work changes scope or resolves a
decision.

This file is a **summary**, and a summary is the thing most likely to rot. An
earlier revision of it, dated 2026-08-04, still said "no functional OCR
pipeline, public OCR API, or OCR CLI exists yet" and that `src/main.rs` exits
with status `2`, long after all three were delivered. If you find this file
disagreeing with the item rows in `ROADMAP.md` section 10, the rows are right
and this file is stale — fix it in the same change.

State support from the gates that actually closed, not from this summary. No
capability here carries an **accuracy** claim: every oracle in this repository
pins preprocessing, postprocessing, and tensor construction, not detection
quality.

## Exact stop point

P0 through P6 are complete. P7 has one item in progress. P8 through P14 are
complete for every row whose dependencies exist; what is left in them is
enumerated below. As of 2026-08-05 the item rows stand at `103` `Done`, `14`
`Blocked`, `29` `Blocked by decision`, `4` `Superseded`, `1` `In progress`, and
`1` `Answered`.

There is no active critical path that this repository can execute alone. The
next real move is a **user decision or an external artifact**, not more code —
see [Where the next move has to come from](#where-the-next-move-has-to-come-from).

### What is delivered

| Capability | Modules | Frozen contract |
|---|---|---|
| Classic OCR: decode → detect → order → crop → recognize → filter | `src/detector.rs`, `src/db.rs`, `src/recognizer.rs`, `src/ctc.rs`, `src/pipeline.rs`, `src/api.rs` | [`CLASSIC_OCR_CONTRACT.md`](CLASSIC_OCR_CONTRACT.md), [`DB_POSTPROCESS_SPEC.md`](DB_POSTPROCESS_SPEC.md) |
| Text-line and page orientation, unwarping, composed preprocessing | `src/orientation.rs`, `src/document_orientation.rs`, `src/unwarp.rs`, `src/document_pipeline.rs` | [`ORIENTATION_CONTRACT.md`](ORIENTATION_CONTRACT.md), [`UNWARPING_CONTRACT.md`](UNWARPING_CONTRACT.md) |
| Layout detection | `src/layout.rs`, `src/paddlex_detection.rs`, `src/resize_cubic.rs` | [`LAYOUT_CONTRACT.md`](LAYOUT_CONTRACT.md) |
| Table recognition: classify → cells → structure → HTML | `src/table_classification.rs`, `src/table_cells.rs`, `src/table_structure.rs`, `src/table_pipeline.rs`, `src/table_engine.rs` | [`TABLE_PIPELINE_CONTRACT.md`](TABLE_PIPELINE_CONTRACT.md) and the three module contracts it names |
| `PP-StructureV3`: ordering, block assembly, Markdown | `src/reading_order.rs`, `src/layout_order.rs`, `src/structure_assembly.rs`, `src/structure_engine.rs`, `src/markdown.rs`, `src/markdown_v2.rs` | [`READING_ORDER_CONTRACT.md`](READING_ORDER_CONTRACT.md), [`RECONSTRUCTION_CONTRACT.md`](RECONSTRUCTION_CONTRACT.md) |
| Public surface: typed API, JSON schemas, CLI | `src/api.rs`, `src/result_json.rs`, `src/structure_json.rs`, `src/main.rs` | [`API_CONTRACT.md`](API_CONTRACT.md), [`SPECIALIZED_API.md`](SPECIALIZED_API.md) |

The runtime is ONNX Runtime `1.24.0` reached through `ort` `load-dynamic`,
selected in [`ADR_RT004_RUNTIME_SELECTION.md`](ADR_RT004_RUNTIME_SELECTION.md).
Model artifacts are resolved from explicit local paths with declared digests;
there is no downloader and no cache, because
[`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md)
makes offline structural rather than a mode.

The model/runtime evidence that used to occupy this file is closed: `RT-003`
completed with two fresh static/Paddle-versus-ONNX captures producing
byte-identical aggregates and zero `m2-tensor-v1` violations across `7,057,864`
elements. The history, including the earlier partial capture that is **not**
retroactively relabelled as passing, stays in
[`RUNTIME_PROOF_PLAN.md`](RUNTIME_PROOF_PLAN.md).

## Where the next move has to come from

Four blocker classes, and none of them is effort inside this repository.

### 1. Upstream publishes no ONNX export (`5` rows, plus `4` that depend on them)

`FORM-001`, `SEAL-001`, `CHART-001`, `KIE-001`, and `CHART-002` need artifacts
that upstream ships only as Paddle `inference.json` + `inference.pdiparams` or
`model_state.pdparams`. `MODEL-DEC-001` does not permit converting them locally,
so these are artifact-blocked, not unstarted. `FORMPIPE-001`, `SEALPIPE-001`,
and the remaining slices of `STRUCT-001` and `RECON-001` are blocked only by
this — the mode the provisioned artifacts do cover is done and matched. The
per-artifact evidence is in
[`P8_ARTIFACT_AVAILABILITY.md`](P8_ARTIFACT_AVAILABILITY.md).

`SR-001` is blocked more deeply: super-resolution has **no inference path in
either pinned baseline**, so there is nothing to port even if an artifact
appeared.

To unblock any of these, an ONNX export must become available from upstream, or
the user must change `MODEL-DEC-001`. Do not convert a model locally to make a
row move.

### 2. The PDF renderer has no approved entry gate (`2` rows, plus `1` in progress)

`PDF-001` and `MPAGE-001` wait on the five-part entry gate in
[`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md).
Only the piece needing no renderer exists: the render-scale planner in
`src/pdf_render_plan.rs`, with the renderer contract measured in
[`PDF_RENDER_CONTRACT.md`](PDF_RENDER_CONTRACT.md).

This is the **only** `In progress` row in the roadmap: `DOC-E2E-001`'s blank,
corrupt, oversized, rotated, and unwarp-refusal cases pass in
`tests/end_to_end.rs`; its multipage and password cases need that renderer.
Office formats are decided against, not pending.

What the renderer does **not** block: the document-assembly functions
themselves. `src/multipage.rs` ports `concatenate_markdown_pages` and
`merge_text_across_page` and matches twelve captured cases, because both are
pure over per-page Markdown, continuation flags, and blocks. They are verified
and not reachable — a distinction worth keeping, since the row would otherwise
read as though the renderer were the only thing standing between this port and
multipage documents.

### 3. Hardware this project does not have (`2` rows)

`ACCEL-001` needs a GPU or accelerator to qualify a backend through the raw
tensor, component, E2E, determinism, security, license, and platform gates.
`PLAT-001` needs the platforms themselves. Neither may be claimed from a
type-check — but `tools/gate.sh` does cross-type-check
`x86_64-pc-windows-msvc` and `wasm32-unknown-unknown` on every run, so
portability is a standing property rather than a measurement that decays.

### 4. Closed by a recorded decision (`29` rows)

These are **decided against**, not waiting. Do not reopen one without a recorded
user decision in `ROADMAP.md`.

| Decision | Outcome | Evidence |
|---|---|---|
| `D-007` / `MODEL-DEC-001` | No downloads, no local conversion; explicit local paths with digests | [`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md) |
| `D-008` / `DOCIO-DEC-001` | DOCX/XLSX/PPTX input rejected; structured export follows it | [`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md) |
| `D-010` / `VLM-DEC-001` | Remote VLM adapters rejected permanently; local VLM in scope but artifact-blocked | [`VLM_DEC_001_EVIDENCE.md`](VLM_DEC_001_EVIDENCE.md) |
| `D-011` / `TRAIN-DEC-001` | Training permanently out of scope — no bit-level oracle exists for a stochastic surface | [`TRAIN_DEC_001_EVIDENCE.md`](TRAIN_DEC_001_EVIDENCE.md) |
| `D-012` / `DEPLOY-DEC-001` | Release targets are the Rust library and CLI on desktop; service, C ABI, mobile, accelerator, ecosystem out of scope | [`DEPLOY_DEC_001_EVIDENCE.md`](DEPLOY_DEC_001_EVIDENCE.md) |
| `REL-001` | No external effects: nothing tagged, packaged, or published; `publish = false` stays set | [`RC_001_RELEASE_EVIDENCE.md`](RC_001_RELEASE_EVIDENCE.md) |

`USER-GATE-001` was answered *continue* on 2026-08-04, which is why work went
past `M2`. Gate P14 (`M7`) remains the only gate that can formally complete the
roadmap, and it needs the blocked rows above resolved or explicitly excluded by
the user.

## Non-negotiable operating constraints

- `PaddleOCR/` is a read-only symlink to the upstream Python checkout, and
  `PaddleX 3.7.2` is the second read-only baseline. Never modify, stage, format,
  test, install in, or otherwise write through either.
- Keep all model weights, caches, raw outputs, temporary runtime builds, and
  external harnesses outside the repository. Do not commit them.
- Preserve the user-owned untracked file `0.3`; do not stage, edit, delete, or
  reset it.
- Normal Rust builds/tests must remain offline and independent of Python,
  PaddleOCR, PaddleX, GPU, the model directory, and the upstream symlink.
- Public conversational replies are Vietnamese by default. Source, docs,
  comments, commands, commit messages, errors, and all repository artifacts
  remain English.
- Run `tools/gate.sh` as a **separate command** before committing, and quote
  only figures it printed. Writing the gate and the commit in one shell
  invocation has already produced a pushed commit whose message claimed a green
  gate over a red one.

## Handoff verification checklist

Before resuming implementation, the next agent should run read-only checks:

```sh
git status --short
git -C PaddleOCR status --short
git log --oneline -5
tools/gate.sh
```

The gate is offline and locked, needs no model, network, GPU, or upstream
checkout, and sets `/usr/bin/gcc` as the Cargo linker — this environment has
another program named `cc` earlier in `PATH`, so a bare `cargo run` fails at
link time without
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc`. On 2026-08-05 it
printed `fmt` and `clippy` clean and `477`/`495`/`483` tests passed across the
default, `--all-features`, and `--features fuzzing` configurations, plus both
cross-target type-checks.

Then read the exact owner documents for the next roadmap item. A clean Rust
test suite does not prove any model/runtime, OCR, or release gate that it does
not cover.
