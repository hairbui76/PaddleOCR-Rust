# Compatibility Ledger

Roadmap item: COMP-001
Status: M2 planning ledger; no row is implemented or verified
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Claim boundary

This ledger records intended M2 behavior only. It is not evidence that
PaddleOCR-Rust currently performs OCR or is compatible with PaddleOCR. An entry
can become `Verified` only after the associated contract, legal fixture,
artifact manifest, tolerance, tests, and validation evidence exist.

[M2_CONTRACT_COVERAGE.md](M2_CONTRACT_COVERAGE.md) maps each Must row to its
current contract authority, implementation evidence, and start gate. It records
the absence of unsupported layers as deliberately as implemented ones.

All inventory rows outside this table remain M7 work under
`SCOPE_CLASSIFICATION.md`.

| ID | Priority | Upstream references | Intended Rust surface | Artifact / fixture / tolerance plan | Status |
|---|---|---|---|---|---|
| `M2-DET-001` | Must | `tools/infer/predict_det.py`; `ppocr/postprocess/db_postprocess.py`; `configs/det/PP-OCRv6/PP-OCRv6_medium_det.yml`; `deploy/cpp_infer/src/configs/OCR.yaml`; `MODEL_CANDIDATES.md` | Private detector component behind a backend-neutral internal boundary. | Exact local artifact and tensor contract are P3; candidate manifest thresholds conflict with M2 and require validation. New legal fixtures; source polygons within `1.0` px. | In progress — `DB-001` implements only a private checked strict-threshold map kernel; no runtime tensor ABI, contour, scoring, box, or detector support exists. |
| `M2-REC-001` | Must | `tools/infer/predict_rec.py`; `ppocr/postprocess/rec_postprocess.py`; `configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml`; `MODEL_CANDIDATES.md` | Private recognizer component with typed text/score outputs. | Exact local artifact/dictionary ABI are P3; candidate exposes CTC-shaped output from a multi-head architecture. New legal fixtures; text/order exact, score abs. error `<= 0.001`. | In progress — `CTC-001` implements only a private numeric greedy-index kernel; no runtime tensor ABI, dictionary/text decoding, language, or recognizer support exists. |
| `M2-GEO-001` | Must | `tools/infer/predict_system.py`; `tools/infer/utility.py:get_rotate_crop_image`; `sorted_boxes`; `ppocr/postprocess/db_postprocess.py:DBPostProcess.get_mini_boxes` | Checked geometry, sort, perspective-crop, and inverse-transform internals. | Synthetic legal geometry fixtures; documented point order; `1.0` px geometry tolerance. | In progress — private crop bytes cover a fixed cubic candidate, replicated borders, and rotation; a bounded convex-hull minimum-area candidate has self-authored vectors only. No decoded-image or OpenCV pixel/rectangle oracle has verified upstream equivalence. |
| `M2-OCR-001` | Must | `tools/infer/predict_system.py:TextSystem.__call__`; `CLASSIC_OCR_CONTRACT.md`; `API_CONTRACT.md` | Native `Ocr` orchestration API with one complete single-image result or structured error. | Text/no-text/multi-line/rotation fixtures; exact output order; classifier excluded as an intentional M2 difference. | Planned |
| `M2-API-001` | Must | Classic Python result behavior plus modern module docs for observable fields; `API_CONTRACT.md` | Idiomatic Rust library API with structured errors and `paddleocr-rust/ocr-result/v1`. | P2 fixes API/JSON details. Negative tests cover malformed input and missing/corrupt models. | Planned |
| `M2-CLI-001` | Must | `tools/infer/predict_system.py` and module CLI documentation | One native OCR CLI command with explicit local model paths and JSON/JSONL output. | CLI integration fixtures; no network/Python/upstream dependency. | Planned |
| `M2-MODEL-001` | Must | v6 model docs, C++ OCR config, and selected artifact provenance | Local manifest/resolution policy without automatic download. | SHA-256, byte limits, format/tensor/dictionary metadata, license/provenance review in P3. | Planned |

## Intentional M2 differences

| Difference | Reason | Public wording required |
|---|---|---|
| No modern PaddleX wrapper/pipeline API | The exact PaddleX resolver is not pinned for the inspected upstream checkout. | "Classic native OCR slice"; never "PaddleX wrapper compatible." |
| No orientation classifier or document unwarping | They are later capabilities and the M2 contract must be explicit. | "Orientation/unwarping are not supported in M2." |
| No automatic model acquisition or bundled weights | Asset provenance and terms are not yet approved. | "Models must be explicitly provisioned locally." |
| Idiomatic Rust API rather than Python API reproduction | The roadmap permits typed Rust design with explicit schema differences. | Describe the exact Rust API/schema rather than claiming Python API parity. |

## Verification transition rule

Before any row changes from `Planned`, link it to a concrete contract, model
manifest, fixture directory, tolerance record, test command/result, and any
approved intentional difference. A model name alone is never compatibility
evidence.
