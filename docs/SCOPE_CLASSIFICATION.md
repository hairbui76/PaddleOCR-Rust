# Scope Classification

Roadmap items: SCOPE-001 and SCOPE-002
Status: Approved bootstrap classification
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Classification rule

Every row in `INVENTORY.md` remains part of the M7 Full Port Target. A `Later`
classification schedules a row after M2; it does not exclude it. There are no
`Out of scope` rows in this bootstrap classification.

`Must` means required to reach M2 after all subsequent technical gates pass.
`Later` means required for M7 but not an M2 implementation promise. `Should`
is intentionally unused: this prevents a vague middle category from masking
whether a capability is required for the next release.

## Classification by inventory area

| Inventory area / rows | Priority | Milestone | Reason |
|---|---|---|---|
| Classic detection → sort/crop → recognition orchestration in `tools/infer/predict_system.py` and helpers | Must | M2 | Defines the selected native OCR vertical slice. |
| Selected DB detection behavior for `PP-OCRv6_medium_det` | Must | M2 | The explicitly selected detector family. |
| Selected CTC recognition behavior for `PP-OCRv6_medium_rec` and its verified dictionary | Must | M2 | The explicitly selected recognizer family. |
| Bounded local PNG/JPEG input, typed Rust API, one CLI command, and versioned JSON/JSONL result output | Must | M2 | Minimum usable native delivery path. |
| M2 fixtures, oracle evidence, provenance, model manifest, local provisioning, safety tests, and user documentation | Must | M2 | Required for an evidence-backed rather than aspirational implementation. |
| Other classic detector, recognizer, classifier, end-to-end, KIE, super-resolution, table, formula, and structure algorithms | Later | M3–M7 | Retained full-port work, sequenced after the first verified slice. |
| All 155 exact training/evaluation/export configuration paths listed in `INVENTORY.md` | Later | M6 | Each configuration needs its own native training/evaluation/export reconciliation. This includes the selected v6 training YAML because its training implementation is not part of M2. |
| Modern standalone model-wrapper rows, including `TextDetection` and `TextRecognition` | Later | M3–M7 | Modern facade compatibility is deferred until the PaddleX oracle is resolved. The M2 classic path must not be marketed as modern wrapper parity. |
| All 10 modern pipeline rows | Later | M3–M7 | Pipelines depend on document, structure, VLM, and/or deferred PaddleX behavior. |
| Python utilities, configuration behavior, API client, serving, and CLI rows not explicitly selected above | Later | M3–M7 | Native equivalents need separate public contracts and security review. |
| C++, JavaScript, Go, TypeScript, Android, iOS, Docker, and ecosystem deployment rows | Later | M5–M7 | They are behavior/deployment references until an approved Rust interoperability or delivery target exists. |
| PDF/office/document IO, orientation, unwarping, multi-page order, and recovery | Later | M3–M4 | Require image/document dependency and geometry decisions. |
| Layout, tables, formula, seal, chart, KIE, and structured reconstruction | Later | M4 | Each has its own model and output-schema contract. |
| DocVLM, ChatOCR, translation, remote services, cloud API clients, and benchmark helpers | Later | M5 | Require provider/model/security/deployment decisions. |
| Existing upstream tests, fixtures, dictionaries, fonts, weights, and datasets | Later as reusable assets; Must only when a newly approved M2 fixture is added | M2–M7 | Assets are not copied or used until provenance and terms are verified. |
| Historical releases and translated upstream documentation | Later | M7 or later amendment | The pinned baseline excludes historical revision parity unless explicitly added. |

## M2 exclusion wording

For M2 public documentation, use only the following meaning: "PaddleOCR-Rust
implements one native classic OCR slice for explicitly provisioned
`PP-OCRv6_medium` artifacts on Linux x86-64." Do not claim full PaddleOCR,
PaddleX, model, language, format, or pipeline compatibility until each
corresponding ledger row is verified.

## Full-target preservation check

| Requirement | Result |
|---|---|
| Every inventory area has a classification | Yes; the table above covers public wrappers/pipelines, classic algorithms/configs, assets/tests, documents, deployment, and historical material. |
| Every exact config path is classified | Yes; all 155 paths are retained as `Later` M6 work. |
| Any inventory row excluded from M7 | No. |
| Any M2 claim broadened to a modern wrapper or full pipeline | No. |
