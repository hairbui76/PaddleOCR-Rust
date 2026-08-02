# Initial Scope Proposal

Roadmap items: SCOPE-001 and SCOPE-002
Status: Approved bootstrap scope; see P0_DECISIONS.md
Prepared: 2026-08-02
Reference inventory: INVENTORY.md
Full-goal baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854
Decision-support evidence: FIRST_SLICE_EVIDENCE.md

## Purpose

The user objective is to port PaddleOCR to Rust. This proposal preserves that
full objective. It does not reduce the eventual pinned-baseline inventory to a
single OCR model or a small API subset.

It separates two decisions that should not be conflated:

1. Full Port Target: the complete pinned-baseline inventory that M7 must
   reconcile before the project may be described as a completed PaddleOCR port.
2. First Stable Release: the earliest useful, evidence-backed Rust vertical slice
   used to establish the native architecture and delivery path.

Every item not in the first release remains on the roadmap as Later, not Out of
scope, unless the user explicitly changes the objective.

## Proposed full-port target

The proposed SCOPE-002 target is every inventory row in INVENTORY.md at the
pinned PaddleOCR commit, represented by an idiomatic Rust equivalent where a
literal language-specific port is inappropriate.

| Inventory area | Proposed full-port treatment |
|---|---|
| Modern standalone models | Native Rust module equivalents with explicit model/artifact contracts |
| Modern pipelines | Native Rust orchestration equivalents; PaddleX behavior is referenced only after BASE-002 is resolved |
| Classic inference and postprocessing | Native Rust implementation, prioritizing exact observable semantics |
| Classical training/evaluation/export | Native Rust track under P12; each config is individually reconciled |
| Document structure/recovery | Native Rust modules and structured JSON/Markdown/export contracts |
| VLM, ChatOCR, and translation | Explicit local and/or remote capabilities after model/provider decisions |
| Cloud client, service, deployment | Rust equivalents only where the selected support matrix requires them |
| C++, JavaScript, Go, TypeScript, Android, iOS source | Reference behavior or Rust interoperability/binding surfaces; no automatic requirement to reproduce a foreign-language UI |
| Tests, fixtures, dictionaries, fonts, weights | Reimplemented or imported only after legal/provenance review |
| Historical upstream releases and translations | Not part of this pinned-baseline target unless explicitly added later |

This definition keeps the full target measurable without claiming that a Rust
project must reproduce every source language, demo UI, or historical revision
verbatim.

## Proposed first stable release

The first stable release should be M2: a useful classic OCR path with:

- a native Rust library and CLI;
- one explicitly named detector plus recognizer artifact pair;
- bounded local PNG/JPEG image input;
- deterministic detector to crop/sort to recognizer orchestration;
- typed results and versioned JSON/JSONL output with text, confidence, and
  source-image quadrilaterals or polygons;
- no Python, PaddleX, upstream checkout, network, or GPU requirement after
  explicit local model provisioning;
- explicit model checksum/provenance and bounded malformed-input behavior;
- legal offline fixtures and compatibility tests;
- all remaining full-port rows retained as Later milestones.

This is a sequencing milestone, not a claim that only image OCR matters to the
project.

## Candidate first model slices

| Candidate | Alignment | Evidence already available | Main risk | Suitable when |
|---|---|---|---|---|
| PP-OCRv6_medium detector and recognizer | Matches current modern Python default wrappers and C++ default config | Current wrapper defaults, module docs, C++ configuration | Larger models and potentially more difficult runtime/operator qualification | Fidelity to current default behavior matters most |
| PP-OCRv6_small or PP-OCRv6_tiny detector and recognizer | Current generation but intentionally different from default | Current configs and module docs | Exact converted/runtime artifact and numerical behavior still need qualification | A compact current-generation foundation is preferred |
| PP-OCRv5_mobile detector and recognizer | Earlier-generation intentional difference | Upstream browser core provides a DB plus CTC ONNX-oriented orchestration reference | Cannot be described as parity with the current v6 medium default | Fastest low-risk proof of native architecture is preferred |

The selected M2 pair is `PP-OCRv6_medium_det` plus
`PP-OCRv6_medium_rec`. It is selected for default-model alignment, not because
an exact executable artifact or runtime has already been qualified. It must
become an exact P3 manifest with model source, hash, format, dictionary,
license, conversion provenance, backend evidence, and tolerances.

`FIRST_SLICE_EVIDENCE.md` records the source paths, model-family differences,
known archive-format evidence, and configuration divergences behind this table.
It is intentionally non-binding and does not select a backend or artifact.

## Proposed first-release compatibility boundary

| Surface | Proposed first-release treatment | Later roadmap treatment |
|---|---|---|
| Image input | Local PNG/JPEG bytes/path under explicit limits | PDF, multipage, URL, directories, office documents in P7/P9 |
| Text detection | One selected DB-compatible detector | Other detector families and text spotting in P12 |
| Text recognition | One selected CTC-compatible recognizer and dictionary | Multilingual/model matrix and non-CTC decoders in P5/P12 |
| Orientation/unwarping | No requirement unless the selected model contract needs it | P5/P7 |
| CLI | One native OCR command with explicit model path and JSON/JSONL | Full module, pipeline, API-client, serving commands in P8-P11 |
| API | Typed Rust library plus stable output types | Python-style facade differences, web client/server, C ABI, WASM, mobile in later phases |
| Model acquisition | User-supplied local files first; download only after P3 policy approval | Approved signed/checksummed registry/cache support |
| Hardware | One approved baseline CPU target | Other CPU architectures, GPU/accelerators, mobile, browser in P11/P13 |
| Training | Not part of M2 | P12, without removing it from the Full Port Target |
| Structure/VLM | Not part of M2 | P8-P10, without removing them from the Full Port Target |

## Decisions required before Cargo initialization

The following choices are material and cannot be inferred safely:

| Decision | What must be confirmed | Why it matters |
|---|---|---|
| Product identity and license | crate/package name, project license, attribution/notice and branding policy | DEC-001 controls package metadata and copied/adapted material; see `IDENTITY_LICENSE_EVIDENCE.md` |
| First model slice | one candidate above or another exact pair; target language/script | DEC-002/D-005 controls model contract, tests, and runtime qualification |
| Baseline CPU/platform policy | initial OS and architecture targets; Rust MSRV/stable policy | DEC-002/D-003 controls CI and native dependency evaluation |
| Model distribution | user-provided files only, opt-in download, or permitted bundled artifacts | DEC-003/D-007 controls security, cache, license, and release contents |
| Modern PaddleX oracle | original resolved artifact, an approved v3.7.x tag, or temporary deferral | BASE-002 controls modern-facade compatibility claims |
| First-release API compatibility | idiomatic Rust-only API/CLI or a defined subset of Python CLI/JSON behavior | API-DEC-001 controls public types and serialized schema |

## Recommended decision sequence

1. Confirm that the Full Port Target above matches the intended meaning of the
   user objective.
2. Select one exact first detector/recognizer pair and the initial CPU target.
3. Select the package identity/license and model-distribution policy.
4. Select or defer the PaddleX oracle according to PADDLEX_BASELINE.md.
5. Create COMPATIBILITY.md with Must rows for the approved M2 scope.
6. Proceed through P1, P2, and P3 without changing the selected scope silently.

## Approval record

The user directed the project to continue after reviewing the proposal and its
evidence. The conservative bootstrap decisions are recorded in
`P0_DECISIONS.md` and ROADMAP.md. They may be changed only through an explicit
roadmap amendment.

| Field | Current value |
|---|---|
| Full Port Target approved | Yes: all inventory rows remain M7 work |
| First model slice approved | `PP-OCRv6_medium_det` + `PP-OCRv6_medium_rec` |
| Initial platform/MSRV approved | `x86_64-unknown-linux-gnu`, Rust `1.94.0` |
| Project identity/license approved | `paddleocr-rust`, Apache-2.0, independent/non-affiliated |
| Model-distribution policy approved | Explicit local provisioning only for M2 |
| PaddleX oracle approved or deferred | Deferred from M2; still required before modern-parity claims |
| SCOPE-001 status | Done |
