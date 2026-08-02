# PaddleOCR-Rust Port Roadmap

Roadmap version: `0.1.0`  
Bootstrap date: `2026-08-02`  
Current phase: `P2–P4 — Contracts, model/runtime qualification, and geometry foundations`  
Current upstream reference: `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## 1. Purpose and authority

This file is the canonical execution plan for PaddleOCR-Rust until the project
Definition of Finished is satisfied. `AGENTS.md` remains authoritative for
safety, repository boundaries, language policy, and engineering conduct. This
roadmap controls project scope, dependencies, decision gates, implementation
order, compatibility claims, milestones, and completion status.

Every repository change must map to a work-item ID in this roadmap. Before
starting work:

1. Read the relevant phase, dependencies, decisions, and acceptance criteria.
2. Set the item to `In progress` only when work actually begins.
3. Do not cross an unresolved decision gate or an incomplete dependency.
4. Implement the item together with its required tests, documentation,
   compatibility evidence, and security/licensing work.
5. Record validation evidence and mark it `Done` only after every acceptance
   criterion is satisfied.

If requested work is absent, conflicts with this plan, or changes its order,
scope, baseline, or Definition of Finished, amend this file before implementing
the request. Record the reason in the change log. A direct user instruction can
change the roadmap, but it does not silently bypass it.

Independent items may run in parallel only when their declared dependencies and
decision gates permit it. Later-phase work may start early under the same rule,
but no phase exit gate may be skipped. Nothing in this roadmap authorizes a
write through `./PaddleOCR`; that symlink and its target are always read-only.

## 2. Status and evidence rules

Work-item statuses are:

- `Planned`: scoped, but work has not begun.
- `In progress`: implementation, investigation needed for the deliverable, or
  validation has begun.
- `Blocked`: unfinished; the item must record the blocker, impact, and exact
  condition needed to resume.
- `Done`: all acceptance criteria are satisfied and validation evidence is
  recorded.
- `Deferred`: removed from the current milestone by a recorded user-approved
  scope decision; it is not complete.
- `Out of scope`: deliberately unsupported by a recorded user-approved scope
  decision; it is not complete and must not be advertised.

Compatibility rows use `Unassessed`, `Planned`, `In progress`, `Verified`,
`Intentional difference`, or `Out of scope`. Only a `Verified` capability may
be described as supported. A work item can be `Done` while recording an
`Intentional difference`, provided the difference is deliberate, documented,
tested, and approved by the relevant scope/API decision.

Evidence for every completed implementation item must identify:

- implementation paths;
- test and legally usable fixture paths;
- exact upstream commit/release and inspected source paths;
- relevant decision records;
- validation commands actually run and their results;
- numerical, geometry, text, ordering, and performance tolerances;
- known limitations and intentional differences.

Scaffolding, unvalidated code, an unrun required check, or a broad API with
placeholder behavior is never `Done`.

## 3. Fixed constraints

The finished implementation must remain:

- an independent native-Rust library and CLI, with no Python dependency at
  build time or runtime;
- independent of `./PaddleOCR` for builds, tests, packaging, installation, and
  runtime;
- backend-neutral at its public API boundary;
- explicit about exact model, language, platform, and schema compatibility;
- safe for untrusted images and documents within documented resource limits;
- reproducible, with model provenance and integrity verification;
- conservative about `unsafe`, bundled assets, native dependencies, and
  compatibility or benchmark claims;
- testable offline without a GPU or large model download for normal unit tests.

Python may be used only by separately documented maintainer-side oracle, model
conversion, or user-authorized external native-runtime build/ABI evidence
tooling in an isolated environment. The last category requires direct user
authorization and may use Python only as a native-runtime build or inspection
driver; it must not execute PaddleOCR or model inference. Such tooling must
never execute inside the linked upstream checkout, write through the symlink,
or become a consumer/build/runtime requirement.

## 4. Pinned upstream baseline

The initial behavioral inventory was inspected on 2026-08-02 from the local
read-only checkout:

| Field | Value |
|---|---|
| Repository link | `./PaddleOCR -> ../PaddleOCR` |
| Commit | `2661c7c0ef5c613e8f93c6e93b2e052399f0f854` |
| Commit date/context | 2026-07-22 checkout state |
| Upstream license | Apache License 2.0 for the repository source |
| Modern facade | `PaddleOCR/paddleocr/` |
| Classic implementation | `PaddleOCR/ppocr/`, `PaddleOCR/tools/` |
| Legacy structure implementation | `PaddleOCR/ppstructure/` |
| Native inference reference | `PaddleOCR/deploy/cpp_infer/` |
| Browser/ONNX reference | `PaddleOCR/paddleocr-js/packages/core/` |
| Tests and behavioral fixtures | `PaddleOCR/tests/` |

The modern `paddleocr/` package delegates substantial behavior to PaddleX
`>=3.7.0,<3.8.0`. `BASE-002` must pin the exact transitive PaddleX reference
needed to explain behavior before modern-pipeline parity is claimed. The
classic `ppocr/`, C++, and browser sources are useful independent references,
but none alone defines all modern behavior.

See [docs/PADDLEX_BASELINE.md](docs/PADDLEX_BASELINE.md) for the observed dependency range,
official candidate tags, delegated interfaces, and the explicit condition needed
to resolve the missing exact reference.

An upstream version change never silently changes this baseline. Updating it
requires an inventory diff, compatibility-ledger update, fixture/tolerance
review, and a roadmap amendment approved as a scope change.

## 5. Initial capability inventory

This inventory defines what must be classified during `INV-001` and `SCOPE-001`.
It does not claim Rust support.

The row-level baseline inventory is maintained in [docs/INVENTORY.md](docs/INVENTORY.md).
This section is the roadmap summary; `docs/INVENTORY.md` records the public facade,
classic algorithms/configurations, tests, assets, deployment, and ecosystem
surfaces that `SCOPE-001` must classify.

### 5.1 Public single-model modules

| Capability | Upstream reference | Default at baseline | Planned track |
|---|---|---|---|
| Text detection | `paddleocr/_models/text_detection.py` | `PP-OCRv6_medium_det` | P5 |
| Text recognition | `paddleocr/_models/text_recognition.py` | `PP-OCRv6_medium_rec` | P5 |
| Text-line orientation | `paddleocr/_models/textline_orientation_classification.py` | `PP-LCNet_x0_25_textline_ori` | P5 |
| Document orientation | `paddleocr/_models/doc_img_orientation_classification.py` | `PP-LCNet_x1_0_doc_ori` | P7 |
| Text image unwarping | `paddleocr/_models/text_image_unwarping.py` | `UVDoc` | P7 |
| Layout detection | `paddleocr/_models/layout_detection.py` | `PP-DocLayout_plus-L` | P8 |
| Table classification | `paddleocr/_models/table_classification.py` | `PP-LCNet_x1_0_table_cls` | P8 |
| Table-cell detection | `paddleocr/_models/table_cells_detection.py` | `RT-DETR-L_wired_table_cell_det` | P8 |
| Table structure recognition | `paddleocr/_models/table_structure_recognition.py` | `SLANet` | P8 |
| Formula recognition | `paddleocr/_models/formula_recognition.py` | `PP-FormulaNet_plus-M` | P8 |
| Seal text detection | `paddleocr/_models/seal_text_detection.py` | `PP-OCRv4_mobile_seal_det` | P8 |
| Chart parsing | `paddleocr/_models/chart_parsing.py` | `PP-Chart2Table` | P8/P10 |
| Document VLM | `paddleocr/_models/doc_vlm.py` | `PP-DocBee2-3B` | P10 |

Expected result contracts include detection polygons/scores, recognized
text/scores, classification IDs/labels/scores, object boxes, table structure and
scores, unwarped images, formulas, and VLM structured results. Exact schemas are
frozen in P2 before implementation.

### 5.2 Public pipelines and utilities

| Capability | Upstream reference | Principal dependencies | Planned track |
|---|---|---|---|
| General OCR | `paddleocr/_pipelines/ocr.py` | doc preprocess, det, crop/sort, line orientation, rec | P6 |
| Document preprocessing | `paddleocr/_pipelines/doc_preprocessor.py` | doc orientation, unwarping | P7 |
| Formula recognition pipeline | `paddleocr/_pipelines/formula_recognition.py` | doc preprocess, layout, formula | P9 |
| Seal recognition | `paddleocr/_pipelines/seal_recognition.py` | doc preprocess, layout, seal det/rec | P9 |
| Table recognition v2 | `paddleocr/_pipelines/table_recognition_v2.py` | OCR, layout, table cls/structure/cells, matching | P9 |
| PP-StructureV3 | `paddleocr/_pipelines/pp_structurev3.py` | OCR, layout, table, seal, formula, chart, regions | P9 |
| PaddleOCR-VL parser | `paddleocr/_pipelines/paddleocr_vl.py` | doc preprocess, layout, VLM, optional OCR modules | P10 |
| Document understanding | `paddleocr/_pipelines/doc_understanding.py` | image/query VLM | P10 |
| PP-ChatOCRv4Doc | `paddleocr/_pipelines/pp_chatocrv4_doc.py` | layout parsing, retrieval, MLLM/LLM | P10 |
| Document translation | `paddleocr/_pipelines/pp_doctranslation.py` | layout/Markdown, translation model/service | P10 |
| Office document to Markdown | `paddleocr/_doc2md/` | DOCX/XLSX/PPTX readers and conversion | P9 |
| Sync/async cloud API client | `paddleocr/_api_client/` | HTTP jobs, polling, result/resource handling | P11 |
| CLI and service commands | `paddleocr/_cli.py` | models, pipelines, API, doc2md, GenAI serving | P6/P11 |

### 5.3 Classic algorithm, training, and export surface

| Family | Baseline source | Included algorithms/capabilities | Planned track |
|---|---|---|---|
| Detection | `ppocr/modeling/heads/det_*.py`, `ppocr/postprocess/` | DB/DB++, EAST, SAST, PSE, FCE, CT, DRRG | P12 |
| End-to-end text spotting | `configs/e2e/`, PG heads/losses/postprocess | PGNet | P12 |
| Recognition | `ppocr/modeling/backbones/rec_*.py`, `heads/rec_*.py` | CTC/CRNN/PP-OCR/SVTR, SAR, SRN, NRTR, RARE, ABINet, VisionLAN, RobustScanner, SPIN, RFL, SATRN, ParseQ, CPPD | P12 |
| Formula recognition | recognition backbones/heads/losses | CAN, LaTeXOCR, UniMERNet, PP-FormulaNet | P8/P12 |
| Classification | `cls_head.py`, `cls_postprocess.py` | text/document orientation classifiers | P5/P7/P12 |
| Tables | `configs/table/`, table heads/postprocess | SLA/SLANet/TableMaster and HTML matching | P8/P9/P12 |
| KIE | `configs/kie/`, `ppstructure/kie/` | SDMGR, LayoutLM/LayoutLMv2/LayoutXLM SER/RE | P8/P12 |
| Super-resolution | `configs/sr/`, SR transforms/heads/losses | TSRN, TBSRN, Telescope | P8/P12 |
| Data and augmentation | `ppocr/data/` | simple, multiscale, LMDB, PGNet, PubTab, LaTeX datasets; label and augmentation registry | P12 |
| Training and evaluation | `tools/train.py`, `tools/eval.py`, `tools/program.py` | losses, metrics, optimizers, schedulers, AMP, distributed, EMA, checkpoints | P12 |
| Export and optimization | `tools/export_model.py`, `deploy/slim/` | export, distillation, pruning, quantization, compression | P12 |

The baseline contains roughly 155 training configurations. Algorithm-family
parity is therefore a separate late track, not part of the first OCR vertical
slice and not something a single inference milestone may claim.

### 5.4 Deployment and ecosystem surfaces

The inventory must classify the current C++ inference path, ONNX conversion,
Paddle Lite, Android/iOS demos, browser SDK, Docker/serving, cloud API SDKs,
high-performance/VLM serving, MCP server, and LangChain integration. A Rust port
may provide an equivalent interoperability surface instead of copying a
language-specific demo, but every omission or replacement needs an explicit
`SCOPE-001` classification. Translated upstream documentation and historical
release branches are provisional non-goals pending that decision.

## 6. Compatibility ledger requirements

`docs/COMPATIBILITY.md` will be created in P0 and maintained for the life of this
roadmap. Each row must contain:

| Field | Required meaning |
|---|---|
| Capability ID | Stable feature/work-item identifier |
| Priority | Must, Should, Later, or Out of scope |
| Upstream baseline | Exact commit/release |
| Upstream references | Public docs and source paths inspected |
| Observable contract | Inputs, defaults, outputs, ordering, and errors |
| Rust surface | Rust API, CLI, schema, or deployment surface |
| Model/artifact | Exact family, format, manifest identity, and checksum |
| Fixtures | Provenance-approved input and expected-result paths |
| Tolerances | Exact text, numeric, geometry, score, or performance rule |
| Status | Unassessed, Planned, In progress, Verified, Intentional difference, or Out of scope |
| Difference record | Decision record for each deliberate semantic difference |
| Evidence | Tests and validation runs supporting the row |

Compatibility is claimed per exact model artifact, backend, platform, and
output contract—not with an unqualified “PaddleOCR compatible” label.

## 7. Open decision register

No dependent implementation may assume an answer while a decision is open.

| ID | Decision | Required by | Resolution item | Status |
|---|---|---|---|---|
| `D-001` | Project/crate/package identity, license, notices, and trademark wording | Gate P0 | `DEC-001` | Resolved for bootstrap in `docs/P0_DECISIONS.md` |
| `D-002` | Full pinned-baseline completion scope and first-release subset | Gate P0 | `SCOPE-001`, `SCOPE-002` | Resolved for bootstrap in `docs/P0_DECISIONS.md` |
| `D-003` | Stable Rust policy/MSRV, OS/architecture matrix, CPU baseline | Gate P0 | `DEC-002` | Resolved for bootstrap in `docs/P0_DECISIONS.md` |
| `D-004` | Minimal workspace/crate boundaries and public API compatibility level | Gate P0/P1 | `DEC-002`, `FND-001` | Resolved for bootstrap in `docs/P0_DECISIONS.md` |
| `D-005` | First detector/recognizer artifacts and language/script scope | Gate P0 | `DEC-002` | Resolved for the M2 family; exact artifact ABI remains P3 |
| `D-006` | Inference/model format and backend, selected from measured proofs | Gate P3 | `RT-004` | Open |
| `D-007` | Model distribution, conversion, cache, offline, checksum, and licensing policy | Gate P3 | `DEC-003`, `MODEL-DEC-001` | M2 local-only policy resolved; exact artifact policy remains P3 |
| `D-008` | Image/PDF/office decoder choices and input resource limits | Gate P4/P7 | `IMG-DEC-001`, `DOCIO-DEC-001` | Open |
| `D-009` | JSON schema/API parity level and intentional Python differences | Gate P2/P6 | `API-DEC-001` | Resolved for M2 in `docs/API_CONTRACT.md`; later pipeline schemas require their own contracts |
| `D-010` | Local VLM runtime versus server adapters and supported VLM families | Gate P10 | `VLM-DEC-001` | Open |
| `D-011` | Native Rust training framework/backend and Paddle checkpoint interoperability | Gate P12 | `TRAIN-DEC-001` | Open |
| `D-012` | Service, C ABI, WASM/browser, mobile, and accelerator release targets | Gate P11/P13 | `DEPLOY-DEC-001` | Open |

For the first OCR slice, `D-005` must compare at least:

- `PP-OCRv5_mobile`, the lowest-risk path because the upstream browser ONNX
  implementation already demonstrates a complete detector/recognizer slice;
- `PP-OCRv6_tiny` or `PP-OCRv6_small`, for a newer compact target; and
- `PP-OCRv6_medium`, the current Python and C++ default whose parity cost may be
  higher.

A compact v5/v6 model must not be described as parity with the current medium
default unless that exact claim is validated.

## 8. Dependency and milestone overview

```text
P0 governance, baseline, decisions
 |
P1 workspace and engineering foundation
 |
P2 compatibility contracts and offline oracle fixtures
 |                    \
P3 runtime/models      P4 image/geometry/tensor foundations
 |                    /
P5 detector + recognizer + optional orientation modules
 |
P6 classic OCR pipeline, Rust API, result schema, and CLI
 |
P7 document input, orientation, unwarping, and multipage processing
 |
P8 layout/table/formula/seal/chart/KIE/SR modules
 |
P9 structured document pipelines and reconstruction
 |                    \
P10 VLM/GenAI           P11 service/deployment/ecosystem
 |                    /
P12 training/evaluation/export/compression (may begin after P3 decisions)
 |
P13 security/performance/platform hardening (continuous, final gate here)
 |
P14 compatibility closeout and releases
```

Delivery milestones are cumulative:

| Milestone | Outcome | Required phases/gates |
|---|---|---|
| `M0` Foundation | Standalone Cargo project, contracts, fixtures, CI, model/runtime decision | P0–P3 |
| `M1` First model | One chosen recognizer or detector runs safely on baseline CPU | P4–P5 component gate |
| `M2` Useful classic OCR | Detector → crop/sort → recognizer through Rust API and CLI | P6 |
| `M3` Robust documents | Orientation, unwarping, PDFs/multipage within defined scope | P7 |
| `M4` Structured documents | Layout/table/formula/seal and PP-Structure scope | P8–P9 |
| `M5` VLM and integration | Approved VLM, API/service, and deployment targets | P10–P11 |
| `M6` Training and algorithm breadth | Approved classic training/export/config families | P12 |
| `M7` Port completion | Pinned-baseline inventory reconciled and final release approved | P13–P14 |

Reaching `M2` is a useful Rust OCR release; it is not completion of the full
roadmap or proof of broad PaddleOCR parity.

Current phase status:

| Phase | Status | Exit condition |
|---|---|---|
| P0 | Done | Approved bootstrap baseline, scopes, decisions, ledger, budgets, and risks; exact model/runtime implementation remains later-gated. |
| P1 | Done | Standalone workspace and offline engineering gate passed in an isolated no-network validation. |
| P2 | In progress | Frozen classic/API contracts and oracle procedure are complete; legal fixtures, exact artifacts, and tolerances remain pending. |
| P3 | In progress | Artifact/ABI discovery and the backend qualification rubric are complete; user-authorized local ONNX candidates were hash- and graph-inspected. The external `tract-onnx` proof rejected the exact-artifact configuration on symbolic shape typing. A separately documented external `ort` dynamic-load proof ran the exact ONNX files at all six qualification shapes, but CPU-baseline portability, raw-tensor comparison, license/supply-chain review, lifecycle/error gates, and a backend decision remain pending. |
| P4 | In progress | Early checked geometry and private crop-pixel work are underway after `FND-002`; a narrow OpenCV crop component oracle exists, while bounded decoding, color semantics, tensors, broader OpenCV evidence, and the phase exit gate remain pending. |
| P5 | Planned | Verified detector, recognizer, and scoped orientation modules |
| P6 | Planned | End-to-end classic OCR API/CLI (`M2`) |
| P7 | Planned | Verified document preprocessing and multipage scope (`M3`) |
| P8 | Planned | Verified scoped structure/specialized modules |
| P9 | Planned | Verified structured document pipelines (`M4`) |
| P10 | Planned | Verified scoped VLM/GenAI capabilities |
| P11 | Planned | Verified service/deployment/ecosystem targets (`M5`) |
| P12 | Planned | Verified scoped training/config families (`M6`) |
| P13 | Planned | Final security, licensing, performance, and platform gate |
| P14 | Planned | Reconciled baseline and user-approved release (`M7`) |

## 9. Global capability Definition of Done

A supported capability is complete only when:

1. Its Rust contract documents inputs, outputs, defaults, units, coordinate
   convention, ordering, errors, concurrency, and resource limits.
2. Its exact upstream baseline and inspected behavior paths are recorded.
3. Preprocessing, model contract, inference, postprocessing, inverse geometry,
   and orchestration are implemented as applicable.
4. Legally usable fixtures cover success, malformed input, boundary conditions,
   empty output, representative non-ASCII input, and deterministic ordering.
5. Golden/differential tolerances are chosen before final comparison, justified,
   and pass.
6. API, CLI, schema, model, language, platform, and known differences are
   documented without broader claims.
7. Normal tests require no network, Python, GPU, large model download, or
   upstream checkout.
8. Security, licensing, memory, and performance consequences are reviewed.
9. The compatibility-ledger row is `Verified` or records an approved, tested
   `Intentional difference`.
10. All required validation commands were actually run and recorded.

## 10. Phase plan

### P0 — Governance, baseline, and product scope

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `GOV-001` | Done | None | `AGENTS.md`, `CLAUDE.md`, and this roadmap define language, read-only upstream, roadmap authority, statuses, gates, and completion rules consistently. |
| `BASE-001` | Done | None | Record upstream commit, inspection date, license, and primary source surfaces without modifying upstream. Evidence is in Sections 4–5. |
| `BASE-002` | Done | `BASE-001` | M2 explicitly defers modern PaddleX-wrapper/pipeline parity; see `docs/PADDLEX_BASELINE.md` and `docs/P0_DECISIONS.md`. An exact PaddleX oracle remains required before modern-parity work or claims. Do not add PaddleX as a Rust runtime dependency. |
| `INV-001` | Done | `BASE-001` | `docs/INVENTORY.md` is the row-complete inventory of public modules, pipelines, classic algorithms/configs, outputs, CLI/API, deployment, assets, and tests. Inventory never implies support. |
| `SCOPE-001` | Done | `INV-001` | `docs/SCOPE_PROPOSAL.md`, `docs/SCOPE_CLASSIFICATION.md`, and `docs/P0_DECISIONS.md` preserve the Full Port Target and classify all inventory work without an M7 exclusion. |
| `SCOPE-002` | Done | `SCOPE-001` | The complete pinned-baseline target is frozen for M7. Every current non-M2 row is `Later`, not excluded; any future exclusion or Rust-equivalent replacement needs explicit user approval. |
| `DEC-001` | Done | `BASE-001` | `docs/P0_DECISIONS.md`, `LICENSE`, and `NOTICE` resolve identity, Apache-2.0 license, attribution, and independent-branding direction. Asset-specific reviews remain later work. |
| `DEC-002` | Done | `SCOPE-001` | `docs/P0_DECISIONS.md` resolves M2 scope, Linux x86-64/Rust 1.94 policy, one-package API direction, and the v6 medium model-family target. |
| `DEC-003` | Done | `DEC-001`, `DEC-002` | M2 accepts only explicitly provisioned local models; no automatic acquisition, cache, conversion, or bundled weights. P3 still resolves artifact-specific `D-007`. |
| `QUAL-001` | Done | `DEC-002` | `docs/QUALITY_PROFILE.md` records reference hardware plus predeclared correctness, determinism, latency, memory, binary-size, and resource budgets. |
| `COMP-001` | Done | `SCOPE-001` | `docs/COMPATIBILITY.md` has the complete M2 Must ledger with upstream paths, intended surfaces, fixture plans, and comparison measures. |
| `RISK-001` | Done | All P0 decisions | `docs/RISK_REGISTER.md` covers licenses, operator gaps, conversion drift, Unicode, geometry, hostile inputs, downloads/assets, native code, platforms, and performance. |

Gate P0 requires an approved scope, baseline, identity/license direction,
platform/toolchain targets, first model slice, initial API/package direction,
quality budgets, compatibility ledger, and risk register. No runtime or crate
architecture may be selected by preference alone.

Gate P0 passed for the approved M2 bootstrap on 2026-08-02. This does not
select an inference backend or exact model artifact; those remain P3 gates.

### P1 — Rust workspace and engineering foundation

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `FND-001` | Done | Gate P0 | Created the single-package Rust 2024 workspace, pinned Rust 1.94 policy, Apache-2.0 metadata, versioned `Cargo.lock`, minimal features, and an explicitly non-functional CLI. |
| `FND-002` | Done | `FND-001` | Implemented checked encoded-byte input, dimensions, finite points, quadrilaterals/polygons, scores, affine transforms, recognized text, page indices, and model identity in `src/types.rs`. Image decoding remains P4 work. |
| `FND-003` | Done | `FND-001` | Implemented structured input, resource, model, backend, I/O, unsupported, and cancellation errors in `src/error.rs`; foundation constructors validate user input without panicking. |
| `FND-004` | Done | `FND-002`, `FND-003` | `docs/ARCHITECTURE.md` records only the implemented public `error`/`types` boundaries and defers private backend/image/pipeline boundaries until their responsibilities have code. |
| `FND-005` | Done | `FND-001` | Configured formatting, manifest lints, missing-docs/no-unsafe policy, release stripping, locked dependency policy, pinned toolchain, and local no-warning validation. |
| `DOCS-001` | Done | `GOV-001`, direct user request 2026-08-02 | Moved supporting project Markdown into `docs/`, retained only standard root entrypoints (`README.md`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, and this roadmap), added `docs/README.md`, updated repository links/references and Rust `include_str!` paths, and validated all local Markdown targets plus the normal workspace gate. This is an organization change only; it does not alter capability scope, artifact policy, or compatibility status. |
| `TEST-001` | Done | `FND-001`, `COMP-001` | Added fixture provenance/golden policy, integration-test metadata/tolerance helper, and offline foundation tests. No upstream asset was copied. |
| `TEST-002` | In progress | `FND-002`, `FND-003` | Foundation regressions cover empty/oversized encoded input, invalid dimensions, NaN/infinity, degenerate quadrilaterals, Unicode, resource bounds, a checked borrowed DB map with strict threshold equality and malformed-value rejection, a bounded CTC matrix with raw-repeat/blank/tie/empty/malformed paths, affine/projective transform round trips across diverse convex grids plus a thin skewed crop and non-finite sampler errors, binary half-tie detector scaling, bounded minimum-area candidate cases for concave/collinear/rotated/triangular polygons, private interleaved crop bytes/borders/fixed-kernel sampling/rotation/allocation limits, seven reviewed BGR OpenCV crop-oracle cases including non-linear fractional interior, all-side-border, and tall-projective paths, classic stable-order edge cases, and M2 Must contract-coverage drift. Tensor properties and broader OpenCV crop-oracle coverage await their owning implementations. |
| `CI-001` | In progress | `FND-005`, `TEST-001` | Added an offline GitHub Actions gate that verifies the upstream target is absent and runs format, Clippy, tests, and docs. It has not yet produced remote-run evidence. |
| `DOC-001` | In progress | `CI-001` | Added README, architecture, contribution, fixture, decision, compatibility, and risk documents. Oracle-refresh/model-integration documentation awaits P2/P3. |

Gate P1 requires a clean standalone clone to pass the normal quality gate and
exercise core types/errors/fixture infrastructure without models, network,
Python, GPU, or the upstream checkout. Once Cargo exists, the baseline is:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Gate P1 local acceptance passed on 2026-08-02 in an ephemeral `bwrap` sandbox:
only the Rust workspace files were copied into an isolated writable `/tmp`,
the root source was read-only, network was unshared, `PaddleOCR` was absent,
and formatting, Clippy, tests, documentation, and the explicitly unsupported
CLI behavior all passed. `CI-001` remains in progress until remote workflow
evidence exists.

### P2 — Compatibility contracts and oracle fixtures

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `CTR-001` | Done | Gate P1 | `docs/CLASSIC_OCR_CONTRACT.md` freezes the selected M2 legacy DB + CTC semantics, source references, defaults, ordering, crop behavior, resource/error rules, and explicit contrasts with current C++ v6 configuration. |
| `API-DEC-001` | Done | `CTR-001`, `COMP-001` | `docs/API_CONTRACT.md` resolves `D-009` for M2: typed native Rust API, a versioned native JSON schema, defaults, path/privacy policy, and explicit Python/PaddleX differences. |
| `CTR-002` | Done | `API-DEC-001` | `docs/API_CONTRACT.md` freezes the planned `paddleocr-rust/ocr-result/v1` result/API/JSON contract, including text, CTC scores, quadrilateral order, nullable page indices, required model/dictionary metadata, optional identifiers, errors, and deterministic JSONL rules. |
| `CTR-003` | Done | `INV-001` | `docs/M2_CONTRACT_COVERAGE.md` maps every M2 Must row to its authoritative contract, existing evidence, executable start gate, and current non-claim boundary. The coverage matrix is complete for the present M2 scope and must be updated before each corresponding implementation starts. |
| `ORACLE-001` | Done | `BASE-001`, `BASE-002`, `TEST-001` | `docs/ORACLE_CAPTURE.md` defines an isolated classic-M2 oracle-capture procedure that never executes or writes inside `./PaddleOCR`; modern wrapper/pipeline capture remains deferred until its separate PaddleX oracle is resolved. |
| `FIX-001` | In progress | `ORACLE-001` | `docs/FIXTURE_AND_TOLERANCE_PLAN.md` defines the legal M2 corpus, exact required coverage, metadata, and blocked model-backed classes. Build the approved offline representatives; do not copy unclear upstream fonts/assets. |
| `TOL-001` | In progress | `CTR-001`, `FIX-001`, `QUAL-001` | `docs/FIXTURE_AND_TOLERANCE_PLAN.md` binds unit, end-to-end, tensor, and determinism profiles to the predeclared budgets. Bind each committed fixture to its actual profile and artifact evidence before completion. |
| `COMP-002` | Planned | All above | Populate compatibility rows and link each Must row to its contract, fixture, tolerance, decision, and planned tests. |

Gate P2 requires evidence-ready contracts and legal offline fixtures before
behavioral implementation claims begin.

### P3 — Model contracts, runtime qualification, and model lifecycle

Artifact provenance/ABI discovery is intentionally allowed to overlap the
remaining fixture work once `CTR-001` and `CTR-002` are frozen. `FIX-001`
cannot produce a model-backed offline golden without an exact legal artifact,
so making `MOD-001` wait for the entire P2 gate would create a dependency
cycle. This early `MOD-001` work may identify and inspect local artifacts only;
it does not select a runtime, permit behavioral implementation, or justify a
compatibility claim. Runtime qualification and model-backed implementation
remain gated by the completed P2 evidence requirements.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `MOD-001` | In progress | Gate P1, `CTR-001`, `CTR-002`, `D-005` | `docs/MODEL_CANDIDATES.md` records revision-pinned official static/ONNX candidates, hashes, observed I/O ABI, and manifest conflicts; `docs/CANDIDATE_PROVISIONING_LEDGER.md` maps that evidence to a candidate-only local verification workflow. The user-authorized external ONNX pair has passed exact file-hash and parse-only graph inspection in `docs/LOCAL_ONNX_CANDIDATE_INSPECTION.md`; complete local provenance/license/dictionary/operator/runtime validation before acceptance. It may begin as provenance/ABI discovery before Gate P2, but runtime selection and behavioral claims remain blocked until that gate passes. |
| `RT-001` | Done | `MOD-001`, `D-003` | `docs/RUNTIME_RUBRIC.md` freezes the blocker gates, weighted scorecard, candidate-representation boundary, proof sequence, and `RT-004` decision template. It selects no runtime or format. |
| `RT-002` | In progress | `RT-001` | `docs/RUNTIME_PROOF_PLAN.md`, `docs/RUNTIME_TRACT_EVIDENCE.md`, `docs/RUNTIME_ORT_EVIDENCE.md`, and `docs/RUNTIME_ORT_SOURCE_EVIDENCE.md` record bounded external proofs against the exact local ONNX pair. `tract-onnx` 0.23.4 is rejected for the exact-artifact configuration because its ordinary dynamic-shape typing fails before inference; a metadata-altered diagnostic run is explicitly non-qualifying. External `ort` 2.0.0-rc.13 spikes dynamically loaded a temporary Python-wheel library and a separately source-built ONNX Runtime 1.28.0 library. The source-built route ran all six exact-artifact shapes on the host and every declared shape through no-AVX QEMU TCG routes, five bounded C API failure cases, and twelve sequential C API create/run/release cycles for each minimum-shape model under one-thread and bounded-resource controls. Calibrated compact fingerprints reveal host/QEMU bit-pattern differences for detector minimum and all recognizer profiles; detector typical/maximum aggregates differ, with no elementwise diagnosis. The route remains non-hermetic and has not established raw-output equivalence, physical CPU/platform coverage, Rust adapter/long-soak/concurrency/resource behavior, supply-chain approval, or distribution approval. Candidate spikes do not become public architecture. |
| `RT-003` | Planned | `RT-002`, `TOL-001` | Compare raw outputs for representative tensors and measure model conversion/backend drift using predeclared tolerances. |
| `RT-004` | Planned | `RT-003` | Resolve `D-006` in an ADR; select the initial backend/format with evidence and a migration strategy while keeping the public API neutral. |
| `RT-005` | Planned | `RT-004`, `FND-004` | Implement a narrow internal adapter that validates model identity, tensor names, shapes, dtypes, allocation bounds, threads, and errors. |
| `MODEL-DEC-001` | Planned | `RT-003`, `DEC-003`, `LIC-001` | Resolve `D-007` with the exact artifact/conversion, distribution, cache/offline, integrity, provenance, and licensing policy. |
| `MOD-002` | Planned | `RT-005`, `MODEL-DEC-001` | Define a versioned manifest with task/family/version, format/backend, URLs, byte limits, SHA-256, tensor contract, config/dictionary fingerprints, license/provenance, and compatibility. |
| `MOD-003` | Planned | `MOD-002` | Implement explicit local path resolution, offline mode, cache rules, artifact/config/dictionary validation, and actionable mismatch errors. |
| `MOD-004` | Planned | `MOD-003` | If approved, implement opt-in downloads with trusted schemes/hosts, redirects policy, time/size bounds, checksums, atomic writes, cache locking, and corrupt/partial cleanup. |
| `LIC-001` | In progress | `MOD-001`, `DEC-003` | `docs/LICENSE_REVIEW.md` records preliminary revision-specific model-card/listing evidence, local package observations, and explicit gaps for the M2 candidates. Verify candidate model, dictionary, tokenizer, fixture, conversion-tool, and retained-notice terms before the final distribution decision; unclear assets are never distributed. |

Gate P3 requires every first-slice model component to run on the baseline CPU
from Rust with validated raw outputs and no Python/upstream runtime dependency.

### P4 — Bounded image, geometry, crop, and tensor foundations

`GEO-001` may begin before the complete P2/P3 gates because its declared
dependency is `FND-002` and it operates only on checked dimensions. This early
pure-geometry work cannot select a decoder/backend or make a behavioral OCR
claim; all decode, tensor, crop, and model-dependent work retains its listed
dependencies.

`CROP-001` and `GEO-002` may begin once the specific `GEO-001` crop-plan or
geometry sub-deliverable they consume has code and tests, even while unrelated
contour/offset work keeps the parent item in progress. Such parallel P4 work is
limited to private checked buffers and geometry validation; it does not select
an image decoder, establish color semantics, or permit an OCR compatibility
claim before the remaining gates.

`GEO-001` may also provide a bounded private minimum-area quadrilateral
candidate over already checked polygon vertices. That narrow mathematical slice
does not establish OpenCV `minAreaRect` equivalence, contour behavior, DB
postprocessing, or detector support; those remain gated `DET-003` work.

`DB-001` may begin as a private, bounded raw-map kernel after `CTR-001` because
strict DB segmentation does not select a runtime or model artifact. It may
validate one borrowed finite map and produce the fixed M2 `value > 0.3` binary
mask only. Tensor rank/batch ABI, contour extraction, scoring, polygon offset,
inverse scaling, inference integration, and any detector claim remain later
P3–P5 work.

`CTC-001` may begin as a private bounded greedy-index kernel after `CTR-001`.
It may validate one borrowed finite `[time, classes]` score matrix, preserve
lowest-index ties, collapse adjacent raw class indexes before removing blank
`0`, and compute the selected-max mean or empty `0.0` score. It must not choose
a dictionary, assume logits/probabilities or softmax, validate a batched runtime
ABI, emit text, reverse Arabic, or claim recognizer support; those remain P3–P5
work.

Pre-gate research may record factual `D-008` evidence in
[`docs/IMAGE_DECODER_EVIDENCE.md`](docs/IMAGE_DECODER_EVIDENCE.md) only when it adds no
Cargo dependency, decoder code, input behaviour, or decision. Such research
does not change `IMG-DEC-001` from `Planned`, satisfy Gate P2, or authorize an
implementation before its declared dependencies.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `IMG-DEC-001` | Planned | Gate P2, `MOD-001` | Resolve the image-decoder and image-limit portion of `D-008`, including formats, metadata/orientation, color/alpha, native dependencies, licenses, and hard limits. |
| `IMG-001` | Planned | Gate P1, `IMG-DEC-001` | Implement bounded decoding for first-slice formats with explicit dimensions/pixels/bytes, alpha, metadata/orientation, allocation, and malformed-input behavior. |
| `IMG-002` | Planned | `IMG-001`, `MOD-001` | Freeze RGB/BGR, grayscale, alpha, EXIF orientation, scale, mean/std, channel layout, and conversion semantics per model. |
| `GEO-001` | In progress | `FND-002` | `src/geometry.rs` implements checked classic detector resize/pad planning; DB-style detector-map-to-source scale, ties-to-even rounding, and inclusive clamp; sum/difference quad order, final exclusive clipping, truncating side filter, and public-geometry rejection after clipping; legacy stable reading-order sorting with exact same-row boundary behavior; signed/absolute polygon area plus closed perimeter; a bounded convex-hull/per-edge minimum-area quadrilateral candidate over checked `Polygon` vertices; and a no-allocation classic crop plan with truncating edge dimensions, tall-crop decision, and forward/inverse homographies. The candidate has only self-authored evidence; OpenCV equivalence is still unverified. Contour/offset geometry and remaining inverse-mapping validation are still required before completion. |
| `GEO-002` | In progress | `GEO-001`, `TEST-002` | `src/geometry.rs` samples projective forward/inverse grid round trips across rectangle, trapezoid, oblique, thin convex, and highly skewed 16,000-by-1-pixel quadrilaterals; preserves odd and maximum-side crop dimensions; rejects non-finite sampler coordinates; and covers binary-representable half ties in DB scaling. Extend this with boundary golden data and approved inverse-mapping evidence before completion. |
| `CROP-001` | In progress | `GEO-001`, `CTR-001` | `src/crop.rs` applies a checked plan to bounded private interleaved pixels with replicated borders, a fixed cubic candidate, and `np.rot90`-equivalent counter-clockwise byte rotation; it rejects output dimensions before allocation. The reviewed self-authored BGR fixture `classic-v1-crop-oracle` records exact OpenCV 5.0.0 outputs for identity, border, fractional-projective, tall-rotation, non-linear interior, all-side-border, and tall-projective cases and is checked offline by Rust. Pixel-level OpenCV interpolation/rounding equivalence beyond those cases, decoded-image integration, color/alpha behavior, and approved model-backed oracle fixtures remain required before completion. |
| `DB-001` | Done | `CTR-001`, `FND-002`, `FND-003` | `src/db.rs` implements a private checked one-map DB segmentation kernel: exact borrowed row-major length, finite values, bounded mask allocation, and fixed strict `value > 0.3` output. Self-authored tests cover equality, row order, length mismatch, and NaN/infinity. It does not validate runtime tensor ABI, extract contours, score regions, offset polygons, emit boxes, or claim detector support. |
| `CTC-001` | Done | `CTR-001`, `FND-003` | `src/ctc.rs` implements a private checked one-matrix CTC greedy-index kernel: at most `16,384` time steps, `65,536` classes, and `40,000,000` borrowed values; finite values; lowest-index argmax ties; duplicate-before-blank collapse; selected-max `f32` mean/empty `0.0`; and bounded output allocation. Self-authored tests cover repeats, blanks, ties, empty time, wrong shape, non-finite values, and all bounds. It does not bind a dictionary, batch/runtime ABI, logits/probability semantics, text normalization, or recognizer support. |
| `TEN-001` | Planned | `IMG-002`, `GEO-001`, `MOD-001` | Implement checked normalization, layout/dtype conversion, padding, batching, strides/contiguity, and allocation arithmetic. |
| `PRE-001` | Planned | `TEN-001`, `TOL-001` | Match captured detector/recognizer/classifier input tensors within declared tolerances. |
| `SEC-IMG-001` | Planned | All above | Add malformed corpus, property tests, and fuzz targets for decoder configuration, geometry, shape arithmetic, and tensor metadata without panic or unbounded work. |

Gate P4 requires reproducible input tensors and correct output-to-source inverse
geometry under documented limits.

### P5 — Classic OCR single-model modules

Detection and recognition may proceed in parallel after Gates P3–P4.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `REC-001` | Planned | Gates P3–P4 | Freeze recognition resize/pad, aspect-ratio batching, dictionary ABI, blank/special tokens, text normalization/script rules, score calculation, and maximum lengths. |
| `REC-002` | Planned | `REC-001`, `RT-005` | Integrate checked single/batched recognition inference. |
| `REC-003` | Planned | `REC-002`, `CTC-001` | Bind the selected CTC greedy-index kernel to the verified dictionary; preserve dictionary order, map index errors safely, produce text, and validate compatible confidence. |
| `REC-004` | Planned | `REC-003` | Verify empty/repeated/invalid output, wide/narrow crops, mixed and in-scope non-ASCII scripts, exact text, scores, order restoration, and resource bounds. |
| `DET-001` | Planned | Gates P3–P4 | Freeze resize-limit strategy, normalization, thresholds, score mode, unclip, max candidates, box/polygon format, clipping, and ordering. |
| `DET-002` | Planned | `DET-001`, `RT-005` | Integrate checked detector inference and output tensor validation. |
| `DET-003` | Planned | `DET-002` | Implement selected postprocessing; for DB, cover bitmap threshold, contours, min-area geometry, fast/slow scoring, area/perimeter expansion, filtering, rescale/round/clip. |
| `DET-004` | Planned | `DET-003` | Verify no-text, rotated/small/edge text, overlaps, threshold boundaries, extreme aspect, malformed tensors, degenerate regions, geometry, scores, and ordering. |
| `ORI-001` | Planned | Gates P3–P4, `SCOPE-001` | If Must, freeze labels/angles/thresholds and its position in the pipeline. |
| `ORI-002` | Planned | `ORI-001`, `RT-005` | Implement preprocessing, inference, conditional rotation, and score handling. |
| `ORI-003` | Planned | `ORI-002` | Verify every angle/label, ambiguous and empty crops, thresholds, and geometry preservation. |
| `MODAPI-001` | Planned | `REC-004`, `DET-004`, optional `ORI-003` | Expose typed standalone module APIs and schemas with exact model compatibility and unsupported behavior documented. |

Gate P5 requires component-level golden/differential parity for the selected
detector/recognizer and any first-release orientation classifier.

### P6 — End-to-end classic OCR, Rust API, schema, and CLI

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `OCR-001` | Planned | Gate P5, `CROP-001` | Implement detect → stable reading-order sort → perspective crop → tall-crop rotation → optional orientation → aspect-sorted recognition batch → original-order restore → score filter. |
| `OCR-002` | Planned | `OCR-001`, `CTR-001` | Match deterministic reading order, including same-row/tie behavior, coordinate preservation, duplicate/overlap behavior, and batch order restoration. |
| `OCR-003` | Planned | `OCR-001` | Define bounded batch sizes, session reuse, threads, cancellation/time policy where supported, memory limits, and whole-input versus per-item failure semantics. |
| `LANG-001` | Planned | `REC-004`, `SCOPE-001` | Add only verified model/language mappings and dictionary selection; never infer generic multilingual support from one artifact. |
| `API-001` | Planned | `OCR-001`–`OCR-003`, `D-009` | Stabilize idiomatic typed constructors/builders, inputs, options, outputs, errors, ownership, concurrency, and model identity. |
| `SCHEMA-001` | Planned | `API-001`, `CTR-002` | Implement versioned JSON/JSONL with text, confidence, polygons/quads, page/input order, model metadata, optional fields, and deterministic serialization. |
| `CLI-001` | Planned | `API-001`, `SCHEMA-001` | Implement scriptable image/model/options/output handling, stdout/stderr separation, exit codes, offline behavior, and explicit acquisition. |
| `E2E-001` | Planned | All above | Test no text, multiple lines, rotations, mixed scripts, thresholds, corrupted/oversized input, missing/corrupt/wrong model, stable order, concurrency, and resource-limit errors. |
| `DOC-USER-001` | Planned | `E2E-001` | Document installation, exact models/languages/formats/platforms, offline provisioning, limits, API/CLI examples, output schema, and known differences. |

Gate P6 (`M2`) requires the approved detector/recognizer path to run end to end
through both Rust API and CLI on the baseline CPU, offline after explicit model
provisioning, without Python or the upstream checkout.

### P7 — Document input and preprocessing

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `DOCORI-001` | Planned | Gate P6, P3/P4 capability packet | Implement and verify document orientation classification with angle, score, image, and inverse geometry semantics. |
| `UNWARP-001` | Planned | `DOCORI-001`, P3/P4 capability packet | Implement and verify UVDoc-compatible unwarping, result image/geometry, invalid meshes, and resource bounds. |
| `DOCPIPE-001` | Planned | `DOCORI-001`, `UNWARP-001` | Compose configurable document preprocessing and preserve every transform needed by downstream coordinates. |
| `DOCIO-DEC-001` | Planned | Gate P4, `SCOPE-001` | Resolve the PDF/office portion of `D-008`: formats, renderer/parser libraries, native dependencies, licenses, password/metadata behavior, and page/work/resource limits. |
| `PDF-001` | Planned | `DOCIO-DEC-001` | Implement the selected bounded PDF renderer; define password, page range/order, DPI, pixel/page/time/memory limits, and unsupported feature errors. |
| `MPAGE-001` | Planned | `PDF-001`, `OCR-001` | Add deterministic multipage execution, page metadata/order, bounded parallelism, cancellation, and partial-failure semantics. |
| `INPUT-001` | Planned | `IMG-001`, optional `PDF-001` | Define bytes/path/stream and any approved URL inputs; URL support requires SSRF, redirects, scheme/host, size/time, and content-validation policy. |
| `DOC-E2E-001` | Planned | All above | Verify rotated/warped/multipage/empty/corrupt/password/oversized cases and mapped downstream coordinates. |

Gate P7 (`M3`) requires every promised document input/preprocessing mode to be
bounded, deterministic, and independently documented.

### P8 — Structure and specialized single-model modules

Each module follows the global capability packet: contract, artifact/operator
qualification, implementation, legal fixtures, malformed/boundary tests,
security/license review, docs, and compatibility evidence.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `LAY-001` | Planned | Gates P3–P7 | Layout detection with class mapping, boxes/polygons, score/NMS, page coordinates, order, and model variants. |
| `TBLCLS-001` | Planned | Gates P3–P4 | Wired/wireless/table classification labels, scores, and thresholds. |
| `TBLCELL-001` | Planned | `LAY-001` | Wired/wireless table cell/object detection with clipped page/table coordinates. |
| `TBLSTRUCT-001` | Planned | Gates P3–P4 | SLANet/approved table structure tokens, cell boxes, HTML structure, scores, and invalid sequence handling. |
| `FORM-001` | Planned | Gates P3–P7 | Formula region preprocessing, tokenization/decoding, LaTeX normalization policy, scores, length/resource bounds, and safe serialization. |
| `SEAL-001` | Planned | `DET-004`, `REC-004`, `LAY-001` | Seal-specific detection, crop/order, recognition, geometry, and script behavior. |
| `CHART-001` | Planned | `LAY-001`, `D-010` | Chart-to-table contract and model/runtime path; defer to P10 if it requires a VLM. |
| `KIE-001` | Planned | `OCR-001`, `LAY-001` | Approved SER/RE scope with token/box alignment, entity/relation schema, tokenizer ABI, and long-document bounds. |
| `SR-001` | Planned | Gates P3–P4 | Approved text super-resolution model(s), image result contract, quality metric, size/memory bounds, and downstream integration policy. |
| `SPECAPI-001` | Planned | Completed Must modules above | Typed standalone APIs/CLI/schema/docs and accurate compatibility rows for each verified specialized module. |

Gate P8 requires every module classified Must for `M4` to be independently
verified; unimplemented rows remain explicit.

### P9 — Structured document pipelines and reconstruction

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `FORMPIPE-001` | Planned | `DOCPIPE-001`, `LAY-001`, `FORM-001` | Formula pipeline with optional layout selection, page/order/geometry, results, and failures. |
| `SEALPIPE-001` | Planned | `DOCPIPE-001`, `LAY-001`, `SEAL-001` | Seal pipeline with region filtering, OCR results, order, and page geometry. |
| `TABLEPIPE-001` | Planned | OCR, layout, all Must table modules | TableRecognitionV2-equivalent orchestration, wired/wireless route, OCR-to-cell matching, spans, HTML, scores, and malformed structures. |
| `STRUCT-001` | Planned | All Must P8 modules/pipelines | PP-StructureV3-equivalent orchestration with layout blocks, OCR, table, formula, seal, chart/region scope, reading order, coordinates, and partial failures. |
| `RECON-001` | Planned | `STRUCT-001` | Deterministic JSON/Markdown reconstruction, page separation/merging, headings, tables, formulas, images/resources, escaping, and path safety. |
| `DOC2MD-001` | Planned | `D-008`, `RECON-001` | If in scope, bounded DOCX/XLSX/PPTX conversion with format-specific fixtures, formulas/resources, unsupported constructs, and no hidden Python tools. |
| `EXPORT-001` | Planned | `RECON-001` | If in scope, DOCX or other structured exports with safe resources and round-trip/visual acceptance criteria. |
| `STRUCT-E2E-001` | Planned | All above | Verify complex pages, empty pages, cross-page tables, overlapping regions, reading order, corrupt/hostile files, deterministic outputs, and resource limits. |

Gate P9 (`M4`) requires evidence-backed output schemas and reconstruction for
every structured pipeline in the approved milestone scope.

### P10 — VLM and GenAI capabilities

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `VLM-DEC-001` | Planned | Gates P3/P7, `D-010` | Decide supported local Rust VLM runtime(s), remote server adapters, tokenizer/model formats, precision, hardware, context/output limits, and license/distribution. |
| `DOCVLM-001` | Planned | `VLM-DEC-001` | Implement approved DocVLM image/query generation with deterministic controls where possible, bounded inputs/context/output, cancellation, and structured errors. |
| `CHART-002` | Planned | `DOCVLM-001`, `CHART-001` | Complete VLM-backed chart parsing and table/JSON contract if selected. |
| `PPOCRVL-001` | Planned | `DOCPIPE-001`, `LAY-001`, `DOCVLM-001` | Implement explicitly selected PaddleOCR-VL v1/v1.5/v1.6 or HPD-Parsing scope, dynamic-resolution processing, layout/region recognition, page restructuring, Markdown/JSON, and model-specific claims. |
| `DOCUND-001` | Planned | `DOCVLM-001` | Document-understanding image/query API, prompt/input policy, result schema, limits, and injection/untrusted-output documentation. |
| `CHAT-001` | Planned | `STRUCT-001`, `DOCVLM-001` | If in scope, visual extraction, vector build/retrieval, MLLM/LLM query stages, typed provider boundary, persistence/privacy policy, and reproducible non-network unit tests. |
| `TRANS-001` | Planned | `RECON-001`, approved language model/provider | If in scope, document translation preserving structure/resources, language controls, privacy, retries, limits, and partial failures. |
| `GENAI-001` | Planned | VLM/provider items | Implement approved vLLM/SGLang/FastDeploy/MLX/llama.cpp/OpenAI-compatible adapters without leaking provider types into public document results. |
| `VLM-E2E-001` | Planned | All Must items above | Verify structure, language, long/complex pages, deterministic settings, malformed output, timeout/cancel, resource exhaustion, provider failure, privacy, and model/hardware matrix. |

Gate P10 requires exact model/provider/hardware claims. A remote adapter does
not count as a local native VLM implementation, and neither may masquerade as
the other.

### P11 — API client, serving, deployment, and ecosystem

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `DEPLOY-DEC-001` | Planned | `SCOPE-001`, Gates P6/P10 as applicable | Resolve `D-012`: classify cloud client, local service, C ABI, WASM, mobile, containers, accelerators, and ecosystem targets with platform/release criteria. |
| `CLIENT-001` | Planned | `DEPLOY-DEC-001`, P2 contracts | If in scope, typed sync/async Rust cloud client for submit/poll/batch/results/resources with timeouts, cancellation, auth redaction, URL/path safety, atomic saves, and mocked offline tests. |
| `SERVER-001` | Planned | Gate P6, `DEPLOY-DEC-001` | If in scope, versioned local HTTP service/OpenAPI with bounded uploads/jobs, cancellation, backpressure, auth/network policy, health, errors, and schema parity. |
| `OBS-001` | Planned | `SERVER-001` or `OCR-003` | Structured logs/metrics/traces without leaking document text, credentials, URLs, or paths; benchmark overhead and cardinality. |
| `ACCEL-001` | Planned | `RT-004`, `D-012` | Qualify each optional GPU/accelerator/backend independently through raw tensor, component, E2E, determinism, security, license, and platform gates. |
| `CABI-001` | Planned | `API-001`, `D-012` | If in scope, stable C ABI with ownership, buffers, errors, versioning, panic containment, examples, and ABI tests. |
| `WASM-001` | Planned | `API-001`, `D-012` | If in scope, browser/WASM worker and model-loading path with web resource limits, CSP/origin/cache policy, and verified compatible models. |
| `MOBILE-001` | Planned | `CABI-001` or target-native decision | If in scope, Android/iOS packaging/bindings and lifecycle/thread/memory tests; duplicating upstream demo UI is not automatic scope. |
| `DEPLOY-001` | Planned | Approved server/CLI targets | Reproducible release/container packages, configuration, offline model mounts, least privilege, health/shutdown, notices, SBOM, and checksums. |
| `ECO-001` | Planned | `SERVER-001`/`CLIENT-001` | Classify and, if approved, provide MCP/LangChain or equivalent Rust/interoperability integrations with security and contract tests. |

Gate P11 (`M5`) requires every promised integration/deployment target to have an
explicit support matrix and release-quality evidence.

### P12 — Native training, evaluation, export, and optimization

This track is separate from inference delivery. It may begin after P3 only when
`D-011` is resolved and cannot destabilize the verified inference API.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `TRAIN-DEC-001` | Planned | `D-011`, `SCOPE-002` | Select native Rust tensor/autograd/distributed/checkpoint strategy by operator coverage, numerical evidence, platform/license, and model interoperability. |
| `DATA-001` | Planned | `TRAIN-DEC-001` | Implement approved simple/multiscale/LMDB/PGNet/PubTab/LaTeX dataset contracts, deterministic shuffling, safe parsing, bounded records, and worker behavior. |
| `AUG-001` | Planned | `DATA-001`, P4 geometry | Implement scoped augmentations and label encoders with seeded determinism and golden geometry/label tests. |
| `ARCH-DET-001` | Planned | `TRAIN-DEC-001` | Implement and validate in-scope detection/e2e Transform→Backbone→Neck→Head configurations against raw activations/parameters. |
| `ARCH-REC-001` | Planned | `TRAIN-DEC-001` | Implement and validate in-scope recognition/formula configurations. |
| `ARCH-STRUCT-001` | Planned | `TRAIN-DEC-001` | Implement and validate in-scope classification/table/KIE/SR configurations. |
| `LOSS-001` | Planned | Architecture scope | Implement scoped losses with forward and gradient comparisons, invalid-shape handling, and numerical stability tests. |
| `METRIC-001` | Planned | Architecture/data scope | Implement detection/recognition/table/KIE/SR metrics with exact normalization, ignore rules, distributed reduction, and fixture comparisons. |
| `OPT-001` | Planned | `LOSS-001` | Implement approved optimizers, regularizers, schedulers, gradient clipping, accumulation, EMA, AMP, and deterministic seed policy. |
| `LOOP-001` | Planned | Data/architecture/loss/metric/optimizer items | Implement bounded train/eval loops, checkpoint/resume, logging, validation cadence, cancellation, distributed/mixed-precision scope, and failure recovery. |
| `CKPT-001` | Planned | `LOOP-001` | Define safe versioned native checkpoints and approved Paddle import/conversion with shape/name/provenance validation; never deserialize unsafe arbitrary objects. |
| `EXPORTMODEL-001` | Planned | `CKPT-001`, `MOD-002` | Export verified inference artifacts/manifests with reproducible conversion and training-to-inference parity tests. |
| `SLIM-001` | Planned | `EXPORTMODEL-001` | If in scope, distillation, pruning, quantization, and compression with measured accuracy/size/performance budgets. |
| `CONFIG-001` | Planned | All training components | Reconcile each of the baseline ~155 configs as Verified, Intentional difference, Deferred, or user-approved Out of scope; no generic family claim from one config. |

Gate P12 (`M6`) is reached only for the algorithm/config rows individually
verified in `docs/COMPATIBILITY.md`. Partial training coverage must be named precisely.

### P13 — Security, reliability, licensing, performance, and platforms

These concerns are implemented in every prior phase; P13 is the final audit and
hardening gate, not their first appearance.

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `THREAT-001` | Planned | Gates P6–P12 as applicable | Complete threat model for decoders, archives/models, tensor allocation, paths/URLs/redirects, cache/temp files, services, concurrency, VLM prompts/output, logs, and training data. |
| `FUZZ-001` | Planned | Pure processing surfaces | Maintain fuzz targets for manifests/configs, geometry, postprocessors, schema parsers, document formats, and retained regressions; native decoder boundaries receive malformed corpora. |
| `ROB-001` | Planned | All runtime/model modules | Verify corrupt models/configs, bad tensors, NaN/infinity, invalid token indices, backend/provider failures, cancellation, cleanup, and no fabricated partial success. |
| `SAFE-001` | Planned | All native/unsafe dependencies | Audit every unavoidable `unsafe`/native boundary, invariants, provenance, version, panic/exception containment, thread behavior, and targeted tests. |
| `LIC-002` | Planned | All dependencies/assets | Audit source, dependencies, native libraries, weights, dictionaries/tokenizers, fixtures, fonts, conversion tools, notices, and distribution; unresolved assets are excluded. |
| `PERF-001` | Planned | `QUAL-001`, each milestone | Reproducible benchmarks record hardware, OS/toolchain, artifact hashes, corpus, warmup, threads, latency/throughput/startup, peak memory, binary/model size, and comparison limits. |
| `PERF-002` | Planned | `PERF-001` | Meet predeclared budgets or amend them transparently with evidence and user approval; never relax them after seeing a failure without recording why. |
| `CONC-001` | Planned | Pipeline/service/training scope | Prove documented thread safety, bounded queues/workers, session reuse, deterministic order, allowed numerical variation, cancellation, and clean shutdown. |
| `PLAT-001` | Planned | `D-003`, `D-012` | Run applicable unit/integration/model smoke/package tests on every promised OS/architecture/backend; Rust portability alone is not platform evidence. |
| `SUPPLY-001` | Planned | Release dependencies | Add approved advisories/dependency/license checks, lock/reproducibility policy, artifact provenance, checksums/signing policy, and SBOM. |

Gate P13 requires no unresolved critical security, licensing, correctness, or
platform finding; no known panic/hang/unbounded path for covered untrusted input;
and met or explicitly re-approved quality/performance budgets.

### P14 — Compatibility closeout and release qualification

| ID | Status | Dependencies | Deliverable and acceptance criteria |
|---|---|---|---|
| `CLOSE-001` | Planned | Gates P6–P13 | Audit every pinned-baseline inventory row; each is Verified, an approved Intentional difference, Deferred to a named future milestone, or user-approved Out of scope. |
| `STABLE-001` | Planned | `CLOSE-001` | Review public API/schema/CLI names, defaults, ownership, errors, feature flags, backend neutrality, semver policy, deprecations, and experimental surfaces. |
| `DOC-FINAL-001` | Planned | `STABLE-001` | Complete installation, offline models, examples, formats/languages/models/platforms, compatibility, limits, security, troubleshooting, architecture, contributor, and release docs. |
| `CLEAN-001` | Planned | Gates P13, `DOC-FINAL-001` | From a clean checkout without `PaddleOCR`, Python, network, or GPU, normal build/test/docs/package passes and explicitly provisioned CPU E2E OCR runs. |
| `PKG-001` | Planned | `CLEAN-001`, `SUPPLY-001` | Release packages contain only intended files, no caches/weights/secrets/build output, and include licenses/notices/SBOM/checksums/provenance. |
| `RC-001` | Planned | All above | Produce release evidence: toolchain/targets, manifests/hashes, commands/results, compatibility summary, benchmarks, security/license review, and known limitations. |
| `USER-GATE-001` | Planned | `RC-001` | Obtain explicit user confirmation of the final targeted release scope and whether the roadmap meets the requested meaning of finished. |
| `REL-001` | Planned | `USER-GATE-001` | Publish/tag/package only with explicit authorization for external effects; record exact artifacts and evidence. |

Gate P14 (`M7`) is the only gate that can formally complete this roadmap.

## 11. Project Definition of Finished

This roadmap is finished only when all of the following are true:

1. Gates P0 through P14 are complete for the user-approved pinned-baseline
   target in `SCOPE-002`.
2. Every required inventory row is `Verified`; no row is silently deleted,
   weakened, or relabeled during final review. Any `Out of scope` row has an
   explicit user-approved rationale and is not implied by public claims.
3. At least one exact, useful detector-plus-recognizer family runs end to end on
   every promised baseline CPU target through both native Rust API and CLI.
4. Build, normal tests, docs, packages, installation, and runtime require no
   Python, PaddleOCR/PaddleX FFI, upstream checkout, network, or GPU.
5. Model provisioning is explicit, checksummed, reproducible, license-reviewed,
   and offline-capable after provisioning.
6. Geometry, ordering, text, confidence, defaults, errors, JSON schema, models,
   languages, and intentional differences are documented and verified against
   the exact upstream baseline.
7. All in-scope modules, composite pipelines, VLM/service/deployment targets,
   and training/config families satisfy the global capability Definition of
   Done individually.
8. Malformed input and resource-limit behavior are tested, and no unresolved
   critical security, licensing, correctness, reliability, or platform issue
   remains.
9. Accuracy/parity and performance budgets pass on named reference systems or
   were changed before release through an evidence-backed, user-approved
   roadmap amendment.
10. A clean release candidate passes every supported-platform gate and carries
    complete compatibility, provenance, notices, checksums, and known limits.
11. User documentation makes no unsupported full-parity, model, language,
    platform, deployment, training, or benchmark claim.
12. The user explicitly confirms the frozen completion scope and final evidence.

“Finished” applies to the exact pinned and approved baseline, not every future
PaddleOCR release. Full PaddleOCR parity may be stated only if every capability
in that baseline inventory is individually verified. A future upstream release,
model, platform, or accelerator starts a new versioned milestone; it does not
silently reopen or redefine a completed one.

## 12. Evidence and change log

| Date | Version/item | Change or evidence |
|---|---|---|
| 2026-08-02 | `0.1.0`, `GOV-001` | Bootstrapped language policy, roadmap authority, statuses, decision/dependency gates, 15-phase plan, milestones, and Definition of Finished at direct user request. Structural validation confirmed consistent Markdown table widths, balanced code fences, defined work-item references, and resolving local guidance links. |
| 2026-08-02 | `BASE-001` | Read-only inspection recorded upstream commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, repository Apache-2.0 source license, and primary modern/classic/native/browser reference paths. `readlink`, `git rev-parse`, and an empty `git status --short` verified the symlink target/revision and no upstream changes. |
| 2026-08-02 | `INV-001` | Completed `docs/INVENTORY.md`: 13 public model wrappers, 10 pipeline wrappers, utility/API/CLI surfaces, classic source families, 155 exact configuration paths, document/recovery, deployment/ecosystem, tests, fixtures, dictionaries, fonts, and model-format boundaries. Direct source comparison verified every configuration listing against the pinned checkout. `docs/COMPATIBILITY.md` remains the later scope-approved support ledger. |
| 2026-08-02 | `BASE-002` | Read-only discovery found the declared range `>=3.7.0,<3.8.0`, no resolver lock/local source, and official candidates v3.7.0, v3.7.1, and v3.7.2. Documented exact commits and delegated facade interfaces in `docs/PADDLEX_BASELINE.md`; blocked only on selecting the reproducible oracle or deferring wrapper parity. No Rust dependency or behavior claim was added. |
| 2026-08-02 | `SCOPE-001` | Began an approval-ready scope proposal in `docs/SCOPE_PROPOSAL.md`. It retains the complete pinned-baseline port as the M7 target and separates that goal from the first stable native OCR slice; no package, model, backend, platform, or licensing choice was assumed. |
| 2026-08-02 | `SCOPE-001` | Added `docs/FIRST_SLICE_EVIDENCE.md` from read-only pinned-upstream inspection. It distinguishes v6 medium default alignment from v5/v6 browser ONNX asset-map coverage and records DB configuration divergences that cannot be silently merged. No artifact, runtime, model, or distribution decision was made. |
| 2026-08-02 | `DEC-001` | Began identity/license decision research in `docs/IDENTITY_LICENSE_EVIDENCE.md`. It records Apache-2.0 source evidence, its trademark non-grant, and the separate status of weights, datasets, fonts, dictionaries, converted artifacts, and third-party runtimes. No project name, license, or asset-distribution policy was assumed. |
| 2026-08-02 | Gate P0 | User direction to continue was recorded as the conservative bootstrap baseline in `docs/P0_DECISIONS.md`: full M7 inventory retained, M2 v6-medium classic OCR target, Linux x86-64/Rust 1.94, independent Apache-2.0 package identity, local-only model provisioning, and deferred modern PaddleX parity. Added `docs/SCOPE_CLASSIFICATION.md`, `docs/QUALITY_PROFILE.md`, `docs/COMPATIBILITY.md`, `docs/RISK_REGISTER.md`, `LICENSE`, and `NOTICE`. No runtime/backend, model artifact, weight, conversion, download, or modern-wrapper compatibility claim was selected. |
| 2026-08-02 | `FND-001`–`FND-005`, `TEST-001` | Created the standalone Rust workspace and tested foundation types/errors with no dependencies or model artifacts. Added locked toolchain/quality policy, architecture and contributor documentation, fixture provenance/tolerance infrastructure, and a CI workflow that requires the upstream target to be absent. Local `fmt`, `clippy -D warnings`, `test`, and `doc` passed with the system compiler placed before an unrelated shell `cc` wrapper. |
| 2026-08-02 | Gate P1, `CTR-001`, `ORACLE-001` | Verified the normal Rust gate in an ephemeral no-network sandbox without the upstream checkout. Added `docs/CLASSIC_OCR_CONTRACT.md` for selected legacy DB/CTC orchestration and `docs/ORACLE_CAPTURE.md` for a read-only upstream-safe fixture procedure. No upstream code executed, model artifact selected, fixture copied, backend chosen, or compatibility claim made. |
| 2026-08-02 | `API-DEC-001`, `CTR-002` | Added `docs/API_CONTRACT.md`, resolving the M2 native typed API and `paddleocr-rust/ocr-result/v1` contract. It deliberately differs from both classic-script and modern PaddleX result schemas, omits unverified fields, requires model/dictionary provenance, and defers serializer implementation and all behavioral verification to P3–P6. |
| 2026-08-02 | `FIX-001`, `TOL-001` | Added `docs/FIXTURE_AND_TOLERANCE_PLAN.md`, separating self-authored unit data from model-backed end-to-end goldens, enumerating M2 coverage, defining metadata/provenance requirements, and binding planned fixture classes to strict comparison profiles. No image, model, dictionary, or oracle result was added. |
| 2026-08-02 | `FND-002`, `TEST-002` | Added the checked borrowed `EncodedImage` foundation and `64 MiB` encoded-byte enforcement with empty-input and over-limit regressions. This is a pre-decode input boundary only; no image format, decoding, or OCR capability was added. |
| 2026-08-02 | P2/P3 dependency clarification | Allowed `MOD-001` provenance/ABI discovery after the frozen classic and API contracts, because model-backed P2 fixtures require an exact legal artifact. Runtime selection, behavioral implementation, and compatibility claims still require the completed P2 gate. |
| 2026-08-02 | `MOD-001` discovery | Added `docs/MODEL_CANDIDATES.md` with pinned official static/ONNX v6-medium candidate revisions, hashes, byte sizes, input/output shapes, metadata hashes, and unresolved license/dictionary/runtime checks. Corrected the classic contract so generic `SVTR_LCNet` CLI defaults are not mistaken for a verified v6-medium ABI. No model binary, runtime, converter, or fixture was added. |
| 2026-08-02 | `GEO-001` early slice | Added private classic detector resize/pad planning plus classic sum/difference quadrilateral ordering, inclusive clipping, and minimum-side filtering over checked dimensions. Tests cover small-image padding, max-side rules, ties-to-even stride rounding, point ordering/clipping, and degenerate rejection. No decoder, pixel transform, perspective crop, tensor, model, or OCR result was added. |
| 2026-08-02 | `RT-001` | Added `docs/RUNTIME_RUBRIC.md`, freezing the evidence-first runtime qualification gates and scorecard. It distinguishes static/ONNX representations from runtimes and leaves `D-006` fully open pending local artifact proofs. |
| 2026-08-02 | `LIC-001` | Added `docs/LICENSE_REVIEW.md` with revision-pinned model-card/license-field and package-listing evidence for the M2 static/ONNX candidates. The record treats the observed Apache-2.0 card metadata as preliminary, records the missing standalone top-level license-file evidence, and keeps all models, dictionaries, fixtures, conversions, and distributions unapproved. |
| 2026-08-02 | `GEO-001`, `TEST-002` | Added private legacy `sorted_boxes`-equivalent stable ordering for checked quadrilaterals. Unit tests cover the backwards same-row swaps, strict `< 10`-pixel row boundary, and equal-top-left stability. Local `fmt`, Clippy, 19 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace containing no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. No crop, decoder, model, or public OCR behavior was added. |
| 2026-08-02 | `GEO-001`, `TEST-002` | Added a private no-allocation classic perspective-crop plan using the Python helper's truncating max-edge dimensions and `>= 1.5` tall-crop decision. It calculates checked forward/inverse homographies and rejects zero-sized crop extents; tests cover rectangle corner maps, oblique round trip, rotation equality, and invalid subpixel width. Local `fmt`, Clippy, 23 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace containing no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. Pixel warp/interpolation/border behavior remains `CROP-001`. |
| 2026-08-02 | `GEO-001`, `TEST-002` | Added private DB-style detector-map-to-source scaling with ties-to-even rounding, inclusive source bounds, and the separately verified final exclusive clip/filter pass. Added signed/absolute polygon area and closed perimeter arithmetic for later unclip work. Local `fmt`, Clippy, 26 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace containing no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. No contour extraction, polygon offset, detector inference, pixel crop, model, or public OCR behavior was added. |
| 2026-08-02 | `MOD-001` provisioning evidence | Added `docs/CANDIDATE_PROVISIONING_LEDGER.md`, which maps the four pinned static/ONNX candidates to explicit external local-only inventory and hash-verification requirements without adding a resolver, cache, download path, model artifact, runtime, or acceptance decision. Captured the pinned ONNX `inference.yml` byte sizes and SHA-256 values by streaming only those text configurations under a 1 MiB limit; matching static/ONNX config fingerprints are explicitly not treated as graph, numerical, legal, or dictionary equivalence. Local `fmt`, Clippy, 26 library plus 3 integration tests, and docs passed. |
| 2026-08-02 | `CROP-001`, `TEST-002` | Added private `src/crop.rs`: bounded 1–4-channel interleaved pixels, inverse-homography perspective sampling, replicated borders, a fixed `a = -0.75` cubic candidate, and exact discrete counter-clockwise rotation at the `height / width == 1.5` boundary. Self-authored tests cover malformed bytes/channels, identity/channel preservation, border replication, subpixel fixed-kernel behavior, rotation bytes, and output dimension rejection before allocation. Local `fmt`, Clippy, 32 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace where the `PaddleOCR` symlink target was absent and the intentionally unsupported CLI exited `2`. No decoder, color-space/alpha decision, model, OCR API, or OpenCV pixel-equivalence claim was added. |
| 2026-08-02 | `GEO-002`, `TEST-002` | Added deterministic grid round-trip coverage for the perspective plan across rectangle, trapezoid, oblique, and thin convex quadrilaterals, including the retained `f64` sampler-coordinate path. Added DB scaling checks for every binary-representable `.5` tie across the even/odd boundary. Local `fmt`, Clippy, 34 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace where the `PaddleOCR` symlink target was absent and the intentionally unsupported CLI exited `2`. This is source-level geometry evidence only; it does not validate decoder, contour, pixel-oracle, model, or OCR behavior. |
| 2026-08-02 | P4 dependency clarification | Clarified that `CROP-001` and `GEO-002` may consume an implemented, tested `GEO-001` sub-deliverable while unrelated parent geometry work remains in progress. This records the private crop/grid sequencing already used and explicitly preserves the decoder, color, artifact, and compatibility gates. |
| 2026-08-02 | `CROP-001`, `GEO-002`, `TEST-002` | Added odd-size and maximum-side crop-plan corner/round-trip coverage plus a constant multi-channel projective crop invariant. Local `fmt`, Clippy, 36 library plus 3 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace where the `PaddleOCR` symlink target was absent and the intentionally unsupported CLI exited `2`. OpenCV pixel equivalence, decoder/color semantics, contours/offsets, and all model behavior remain unverified. |
| 2026-08-02 | `CTR-003`, `TEST-002` | Added `docs/M2_CONTRACT_COVERAGE.md`, mapping every M2 Must compatibility row to its authoritative contract, present evidence, implementation start gate, and explicit non-claim boundary. Added two integration checks that require all seven M2 Must rows and the open `D-006`/`D-007`/`D-008` boundaries to remain represented. Local `fmt`, Clippy, 36 library plus 5 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace with no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. No model artifact, runtime, decoder, detector, recognizer, public OCR API, or CLI behavior was added. |
| 2026-08-02 | `GEO-001`, `TEST-002`, P4 boundary clarification | Clarified that `GEO-001` may hold a bounded private candidate over checked polygon vertices without claiming OpenCV `minAreaRect`, contour, DB postprocessing, or detector support. Added a convex-hull/per-edge minimum-area quadrilateral candidate with explicit allocation failure, canonical classic corner ordering, and short-side output. Self-authored tests cover concave/collinear input, rotated input-order stability, and a triangle. Local `fmt`, Clippy, 39 library plus 5 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace with no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. OpenCV rectangle parity, contour extraction, offsetting, decoder/tensor/model behavior, and public OCR support remain unverified. |
| 2026-08-02 | `CTR-003` completion | Marked the M2 Must contract-coverage matrix complete after every current row had an authoritative contract, present-evidence statement, executable start gate, and non-claim boundary, including the new private geometry candidate. The two integration checks guard the Must-row and open-decision coverage. This closes only the matrix work item; P2 fixture/tolerance work, P3 model/runtime decisions, P4 decode/tensor evidence, and all OCR implementation remain open. |
| 2026-08-02 | `DB-001`, `TEST-002`, P4 boundary clarification | Clarified the early private DB raw-map kernel boundary, then added `src/db.rs`: one borrowed finite row-major map of exact checked length produces a bounded zero/one mask using the frozen strict `value > 0.3` rule. Self-authored tests cover equality exclusion, row order, length mismatch, and NaN/infinity. Local `fmt`, Clippy, 42 library plus 5 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace with no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. There is still no runtime tensor ABI, contour extraction, score/offset/box postprocessing, model, detector, or OCR support. |
| 2026-08-02 | `CTC-001`, `TEST-002`, P4 boundary clarification | Clarified the early private numeric CTC boundary, then added `src/ctc.rs`: a borrowed bounded finite `[time, classes]` score matrix produces greedy class indexes with first-index argmax ties, raw-repeat-before-blank collapse, and selected-max `f32` mean/empty `0.0`. Self-authored tests cover repeats, blank reset, ties, zero time, wrong length, non-finite values, zero/excessive classes, excessive time, and matrix-value bounds. Local `fmt`, Clippy, 47 library plus 5 integration tests, and docs passed; the same gate passed in a no-network `bwrap` workspace with no `PaddleOCR` link, where the intentionally unsupported CLI exited `2`. There is still no artifact dictionary, text decoding, runtime tensor ABI, recognizer, or OCR support. |
| 2026-08-02 | `D-008` pre-gate research | Added `docs/IMAGE_DECODER_EVIDENCE.md` to record a non-binding PNG/JPEG decoder evaluation. It captures the unpinned upstream OpenCV input-path evidence, the candidate `image` 0.25.10 API/limit evidence, and the exact oracle, color/alpha/orientation, resource, licensing, and dependency proofs still required. No Cargo dependency, decoder, input policy, model artifact, runtime, or compatibility claim was added; `IMG-DEC-001` remains Planned and `D-008` remains open. |
| 2026-08-02 | `CROP-001`, `FIX-001`, `TOL-001` evidence tooling | Added `tools/capture_crop_oracle.py` and `docs/CROP_ORACLE_CAPTURE.md` for a developer-only, self-authored BGR OpenCV crop corpus. The tool mirrors the pinned classic crop configuration, emits JSON only to stdout with environment/version, an OpenCV-build fingerprint, perspective matrices, and byte hashes, and neither imports/executes the upstream checkout nor loads/downloads models. Local OpenCV is not installed, so no oracle JSON or compatibility claim was produced; the tool's syntax and dependency-free listing mode were checked separately. Rust build/test/runtime remain independent of Python and this tool. |
| 2026-08-02 | `CROP-001`, `FIX-001`, `TOL-001`, `TEST-002` crop component evidence | Created an isolated temporary Python environment outside both repositories and captured the self-authored BGR OpenCV corpus with Python 3.12.3, NumPy 2.5.1, OpenCV 5.0.0, and opencv-python-headless 5.0.0.93. Committed the reviewed JSON and provenance metadata under `tests/fixtures/classic-v1-crop-oracle/`; its JSON SHA-256 is `772da4733c0950760ffa4ddc8ef6d7f89ca3b895566f50b5cfc60e7a18f8f5a0`. Added an offline Rust regression for identity, border replication, fractional projective interpolation, and the tall-rotation boundary. Local format, Clippy, 48 library plus 5 integration tests, and docs passed; the same gate passed in a no-network sandbox without the upstream symlink, where the intentional bootstrap CLI exited `2`. This is exact evidence only for the four recorded BGR cases and capture environment; no decoder, model, OCR, universal OpenCV, or upstream-environment parity claim is made. |
| 2026-08-02 | `MOD-001`, `LIC-001` local ONNX inventory | At direct user authorization, downloaded the full revision-pinned ONNX detector and recognizer packages into a user-owned external directory using `hf 1.26.0`. Every expected package-root file was a regular non-symlink, local Hugging Face metadata recorded the requested commits, and the runtime-relevant file hashes matched `docs/MODEL_CANDIDATES.md`. An isolated `onnx 1.22.0` parse-only `check_model` inspection recorded actual opsets, tensor signatures, operator sets, node counts, and no external tensor data in `docs/LOCAL_ONNX_CANDIDATE_INSPECTION.md`; it did not execute inference or select a runtime. README license fields remain preliminary and each package lacks a top-level `LICENSE` file, so the candidates remain unapproved and unsupported. |
| 2026-08-02 | `RT-002`, `MOD-001`, `LIC-001` tract diagnostic | Ran an external, release-mode `tract-onnx` 0.23.4 spike against verified external ONNX bytes. The ordinary exact-artifact path failed symbolic shape typing for both detector and recognizer before inference, rejecting that candidate configuration. A separately labeled in-memory metadata-specialization diagnostic ran all six fixed M2 zero-input shapes with finite output and repeatable content signatures, but is non-qualifying and does not select a runtime. `docs/RUNTIME_TRACT_EVIDENCE.md` records hashes, commands, resource observations, and pending gates. Corrected the artifact records to distinguish this user-local diagnostic from project adoption; no raw or reconstructable model output, reusable model-derived fixture, runtime dependency, source change, or compatibility claim was added. |
| 2026-08-02 | `RT-002` ort diagnostic | Ran an external release-mode `ort` 2.0.0-rc.13 spike against the same verified ONNX bytes, using a temporary ONNX Runtime 1.28.0 native-library file, CPU execution, one-thread controls, and named tensor I/O. Both exact models loaded and executed all six fixed qualification shapes with finite, repeatable output signatures; an explicit nonexistent-library probe failed normally without fallback or download. `docs/RUNTIME_ORT_EVIDENCE.md` records the historical hard-coded temporary-path limitation, hashes, feature/dependency observations, shapes, resource observations, and the incomplete portability, raw-tensor, native supply-chain, error, lifecycle, and backend-decision gates. No project dependency, native binary, raw or reconstructable model output, reusable model-derived fixture, implementation, or compatibility claim was added. |
| 2026-08-02 | `LIC-001` immutable revision probe | Queried the public Hugging Face revision API for both exact ONNX commits. It confirmed each model-card `license: apache-2.0` declaration and a sibling list without `LICENSE`; each immutable `resolve/<revision>/LICENSE` URL returned `404`. `docs/LICENSE_REVIEW.md` records the direct API URLs and retains the status as unapproved: model-card metadata and absent-file evidence do not establish complete terms or redistribution rights for every package file. |
| 2026-08-02 | `CROP-001`, `FIX-001`, `TOL-001`, `TEST-002` expanded crop component evidence | Expanded the reviewed self-authored BGR OpenCV crop corpus from four to seven cases in the existing isolated Python 3.12.3 / NumPy 2.5.1 / OpenCV 5.0.0 environment. The new non-linear cases cover fractional interior sampling, replicated borders across every image side, and fractional tall-projective sampling before the exact counter-clockwise rotation. The generator output diffed byte-for-byte against the committed fixture; `metadata.json` records the seven-case JSON SHA-256 `ae225eeb7d05169fdea080fdc2b47a9d05bce1b33f0bb72a087327aa63ebe01a` and aggregate input/output hashes. Added an offline Rust regression that matches all new expected bytes. Local format, Clippy, 49 library plus 5 integration tests, and docs passed. This remains exact evidence only for seven self-authored BGR cases in the recorded environment; no decoder, model, OCR, universal OpenCV, or upstream-environment parity claim was added. |
| 2026-08-02 | `GEO-002`, `TEST-002` adversarial crop-map regression | Added an offline regression for a legal but highly skewed 16,000-by-1-pixel quadrilateral. It round-trips three fractional pre-rotation sampler coordinates through the checked projective plan and verifies typed rejection of NaN and infinite sampler coordinates. Local format, Clippy, 50 library plus 5 integration tests, and docs passed. This strengthens the private crop-map safety boundary only; it does not establish decoder, OpenCV matrix, contour, model, or public OCR equivalence. |
| 2026-08-02 | `RT-002` source-built CPU route | Built ONNX Runtime v1.28.0 from the clean external source commit da9b5e364c465de65c49d91e696cd6485270757f with CPU-only settings and global `-mno-avx`/`-mno-avx2`/`-mno-avx512f` compile settings. The 31,428,768-byte shared library SHA-256 is `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa`. An external dynamic-load Rust spike executed all six exact M2 ONNX shapes with finite repeatable signatures, and a QEMU TCG no-AVX proof verified exact detector minimum session creation and inference under both Nehalem and qemu64 CPU models. A separate bounded C API probe returned structured errors for a missing model, empty/invalid ONNX, wrong input name, and wrong input rank; its scoped `strace` observed no network system call. The build is non-hermetic and source/library/license/raw-oracle/resource/adapter gates remain incomplete; `docs/RUNTIME_ORT_SOURCE_EVIDENCE.md` records the full limits. No project dependency, native binary, raw or reconstructable model output, reusable model-derived fixture, implementation, or backend decision was added. |
| 2026-08-02 | `RT-002` evidence-tooling clarification | Recorded the user's direct authorization for an isolated external Python environment to drive native-runtime build/ABI evidence only. The exception permits no Python model inference or PaddleOCR execution and adds no project build/runtime dependency. Corrected historical wheel-path, QEMU-network, and compact-output-signature wording so no temporary proof is presented as deployment or reproducibility evidence. |
| 2026-08-02 | `LIC-001` immutable terms audit | Rechecked the exact public/ungated ONNX revisions through their immutable Hugging Face APIs and recursive trees. The APIs display `author: PaddlePaddle` and `cardData.license: apache-2.0`, but have no `license_link`, dataset, or base-model evidence; the trees contain only the five recorded files and no `LICENSE`/`NOTICE`, and immutable `LICENSE` URLs return `404`. Local canonical text assets match the remote bytes; the README `./LICENSE` badge is dangling, and the recognizer's 18,708-entry embedded dictionary has no terms/provenance text. `docs/LICENSE_REVIEW.md` records the exact URLs and retains all artifacts, conversion, redistribution, and model-derived fixtures as unapproved. |
| 2026-08-02 | `RT-002` no-AVX shape expansion | External source-built ONNX Runtime diagnostics ran every declared detector/recognizer shape under no-AVX QEMU TCG routes: system-mode recognizer probes and user-mode detector typical/maximum probes, alongside the previous system-mode detector minimum proof. All reported the expected shape/count and finite output under one-thread controls, but they are emulator-only results. Calibrated C/Rust word-wise fingerprints prove host/QEMU bit-pattern differences for detector minimum and every recognizer profile; detector typical/maximum aggregate sums/maxima also differ. No raw tensor, elementwise tolerance, static-Paddle oracle, physical baseline-host, runtime dependency, native binary, model-derived fixture, implementation, or backend decision was added. |
| 2026-08-02 | `RT-002` bounded lifecycle evidence | An external C API harness verified exact library/model hashes, then completed twelve sequential create/run/release cycles for each minimum-shape ONNX model with telemetry disabled, one CPU thread, sequential execution, memory-pattern off, a 1,600,000 KiB virtual-memory limit, and a 600-second watchdog. Every output had expected shape/count and finite values; the short post-release RSS window was bounded, `ReleaseEnv` completed, and `dlclose` returned zero. This is Linux-host-only short lifecycle evidence, not a leak-free soak, network-off, cancellation, concurrency, malicious-input, Rust-adapter, numerical-equivalence, portability, distribution, or backend-decision result. |
| 2026-08-02 | `LIC-001`, `MOD-001` publisher/dictionary trace | Added immutable PaddleX official-model routing and Hugging Face LFS object records as stronger publisher/byte chain-of-custody evidence for the exact ONNX candidates. Read-only comparison established that the recognizer's canonicalized `character_dict` entries match the pinned PaddleOCR `ppocrv6_dict.txt` with SHA-256 `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d`. The source-tree Apache-2.0 license is a dictionary-terms lead, but the dictionary README and exact ONNX revisions still lack package-specific terms/notices; all weights, exports, configurations, dictionary distribution, conversion, fixtures, and runtime selection remain unapproved. |
| 2026-08-02 | `DOCS-001` | At direct user request, moved 27 supporting Markdown records to `docs/`, added `docs/README.md` as their topical index, and retained only `README.md`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, and `ROADMAP.md` at the root. Updated local Markdown links/references, root contributor/discovery links, and contract-test `include_str!` paths. Verified every local Markdown target, `git diff --check`, and the full locked Cargo format/Clippy/test/doc gate (50 library tests and 5 integration tests). |
| 2026-08-02 | `D-008` decoder pre-gate risk review | Extended `docs/IMAGE_DECODER_EVIDENCE.md` with source-level candidate facts for `image` 0.25.10, Zune, PNG, and `jpeg-decoder`: resolved feature/CPU surface, full-input buffering, best-effort decoder limits, APNG, and malicious-input evidence remain decision risks. The review adds no Cargo dependency, decoder, format/input policy, compatibility claim, or decision; `IMG-DEC-001` remains Planned and `D-008` remains open. |
