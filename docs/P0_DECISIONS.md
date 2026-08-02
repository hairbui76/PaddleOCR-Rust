# P0 Bootstrap Decisions

Roadmap items: SCOPE-001, SCOPE-002, DEC-001, DEC-002, DEC-003, QUAL-001,
COMP-001, RISK-001, and BASE-002
Status: Approved bootstrap baseline
Decision date: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Decision authority and amendment rule

After reviewing the scope, model, identity, and PaddleX evidence records, the
user directed the project to continue. This record turns that direction into
explicit, conservative bootstrap choices rather than silently inventing
architecture or asset behavior.

These choices govern the repository until an explicit user-approved roadmap
amendment changes them. The full M7 target is not narrowed by the M2 sequence.
No decision here selects an inference backend, downloads an artifact, or grants
rights to a model asset.

## Resolved P0 decisions

| ID | Resolution | Rationale and boundary |
|---|---|---|
| `D-001` | Project display name: `PaddleOCR-Rust`; Cargo package: `paddleocr-rust`; Rust library crate: `paddleocr_rust`; Apache-2.0 for project-authored source, documentation, and self-authored fixtures; no dual license; initial package publication disabled. | On 2026-08-02, the project user confirmed that project-authored repository content will be open source and publicly accessible. Apache-2.0 is compatible with the upstream source license and preserves a clear notice path for any later adapted material. `NOTICE` states independent, non-affiliated status. This decision grants no rights to upstream or third-party assets. |
| `D-002` | The full M7 target is every row in `INVENTORY.md` for the pinned PaddleOCR commit. The first stable release is M2 only. | M2 is a sequencing milestone, not a redefinition of the requested full Rust port. |
| `D-003` | Initial supported target: `x86_64-unknown-linux-gnu`; baseline toolchain/MSRV: Rust `1.94.0`; portable x86-64 code generation only, with no AVX/AVX2 requirement. | This is the verified development/qualification host. Windows and other targets remain Later and require independent evidence. |
| `D-004` | Start with one workspace package containing a library and a native CLI. The first public surface is idiomatic Rust plus versioned JSON/JSONL, not Python API emulation. Backend details remain private. | A single package is the minimum justified P1 boundary. Exact API/schema design remains P2 work. |
| `D-005` | M2 targets the named pair `PP-OCRv6_medium_det` + `PP-OCRv6_medium_rec`. Language target is the selected artifact's documented unified 50-language dictionary; the Rust project makes no language-support claim until its exact dictionary/artifact is verified. | This pair aligns with current standalone wrapper and C++ OCR defaults. Exact artifact format, hash, ABI, and license remain P3 gates. |
| Preliminary `D-007` policy | M2 accepts only explicitly provisioned local model files. No model download, cache population, conversion, or bundled weight is implemented or assumed. | It makes normal tests offline and avoids unreviewed asset distribution. `MODEL-DEC-001` must later resolve artifact-specific terms and integrity. |
| `BASE-002` scoped resolution | Modern PaddleX-wrapper and pipeline parity is deferred from M2. It remains a required full-port workstream before any modern-wrapper/pipeline parity claim. | The pinned PaddleOCR checkout has no exact PaddleX resolver record. Classic/C++/browser sources remain reference-only evidence for M2. |

## Scope consequences

M2 implements a native classic OCR vertical slice only after its later model,
runtime, image, geometry, detector, recognizer, API, and CLI gates pass. It
does not initially include document orientation/unwarping, PDFs, pages,
automatic model downloads, modern PaddleX wrappers, training, structure,
VLMs, services, mobile, browser, or foreign-language UI demos.

Those capabilities remain included in the M7 target and are classified as
Later in `SCOPE_CLASSIFICATION.md`; none is marked Out of scope.

## Initial platform and quality reference

The current qualification machine is recorded only as a benchmark reference:

| Property | Recorded value |
|---|---|
| Operating system | Ubuntu 24.04.1-derived Linux kernel `7.0.0-28-generic` |
| Rust host | `x86_64-unknown-linux-gnu` |
| Rust compiler | `rustc 1.94.0 (4a4ef493e 2026-03-02)` |
| CPU | Intel Xeon E5-2696 v3; 72 logical CPUs; no ISA extension is required by the support policy |
| Available memory at inspection | approximately 92 GiB |

`QUALITY_PROFILE.md` defines acceptance budgets. This host description is not a
promise that every configuration of the same processor family has identical
performance.

## Decisions intentionally left open

The following must not be inferred from the bootstrap baseline:

- `D-006`: model format and inference runtime/backend, which requires P3
  measured qualification;
- `D-008`: decoder and document-input library choices, which require P4/P7
  evaluation;
- `D-009`: exact JSON schema and API behavior, which requires P2 contracts;
- all later VLM, service, deployment, training, and accelerator decisions;
- exact model archives, checksums, license terms, tokenizer/dictionary ABI, and
  conversion path.

## Evidence links

- `SCOPE_PROPOSAL.md` and `SCOPE_CLASSIFICATION.md` define the approved scope.
- `FIRST_SLICE_EVIDENCE.md` records model-family and classic OCR evidence.
- `IDENTITY_LICENSE_EVIDENCE.md` records licensing and branding boundaries.
- `PADDLEX_BASELINE.md` records the deferred modern-wrapper oracle issue.
- `QUALITY_PROFILE.md`, `COMPATIBILITY.md`, and `RISK_REGISTER.md` satisfy the
  remaining P0 planning deliverables.
