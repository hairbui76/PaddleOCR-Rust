# Compatibility Ledger

Roadmap item: COMP-001
Status: every M2 Must row is Verified as of 2026-08-04; see `CLOSE-001`
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Claim boundary

This ledger records M2 behaviour. Every `Must` row below is now `Verified`
against the transition rule at the end of this document: each names a frozen
contract, a committed fixture, a recorded tolerance, and a test that reproduces
it. Gate `P6` evidence is in [`GATE_P6_EVIDENCE.md`](GATE_P6_EVIDENCE.md).

What `Verified` does **not** mean: that a capability outside this table works,
that a second artifact pair is supported, or that any input format other than
PNG is accepted. Verification is per row and per artifact, never by extension.

[M2_CONTRACT_COVERAGE.md](M2_CONTRACT_COVERAGE.md) maps each Must row to its
current contract authority, implementation evidence, and start gate. It records
the absence of unsupported layers as deliberately as implemented ones.

All inventory rows outside this table remain M7 work under
`SCOPE_CLASSIFICATION.md`.

| ID | Priority | Upstream references | Intended Rust surface | Artifact / fixture / tolerance plan | Status |
|---|---|---|---|---|---|
| `M2-DET-001` | Must | `tools/infer/predict_det.py`; `ppocr/postprocess/db_postprocess.py`; `configs/det/PP-OCRv6/PP-OCRv6_medium_det.yml`; `deploy/cpp_infer/src/configs/OCR.yaml`; `MODEL_CANDIDATES.md` | Private detector component behind a backend-neutral internal boundary. | Exact local artifact and tensor contract are P3; candidate manifest thresholds conflict with M2 and require validation. New legal fixtures; source polygons within `1.0` px. | **Verified** — `src/detector.rs` and `src/detector_boxes.rs` implement the frozen DB path: `0.3` segmentation threshold, `box_score_fast`, `unclip_ratio` `1.5`, `max_candidates` `1,000`, short-side checks, half-to-even rescale, inclusive clipping. Oracle matches: contours `18/18`, `minAreaRect` `16/16`, `box_score_fast` `8/8`, `unclip` `16/16`. Detector input tensors are **bit-identical** to a captured upstream capture across `4,048,896` elements (`PRE-001`). Boundary coverage in `DET-004`. |
| `M2-REC-001` | Must | `tools/infer/predict_rec.py`; `ppocr/postprocess/rec_postprocess.py`; `configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml`; `MODEL_CANDIDATES.md` | Private recognizer component with typed text/score outputs. | Exact local artifact/dictionary ABI are P3; candidate exposes CTC-shaped output from a multi-head architecture. New legal fixtures; text/order exact, score abs. error `<= 0.001`. | **Verified** — `src/recognizer.rs`, `src/recognizer_batch.rs`, `src/ctc.rs`, `src/dictionary.rs`. Batching is `rec_batch_num` `6` with per-batch `max_wh_ratio`, the batch width truncating and the per-crop width ceiling as upstream does — both corrected against a captured oracle. Greedy CTC collapses raw repeats before removing blanks, lowest index on ties. Dictionary preserves exact scalars with no normalization. Recognizer input tensors match a captured upstream capture. Boundary coverage in `REC-004`. |
| `M2-GEO-001` | Must | `tools/infer/predict_system.py`; `tools/infer/utility.py:get_rotate_crop_image`; `sorted_boxes`; `ppocr/postprocess/db_postprocess.py:DBPostProcess.get_mini_boxes` | Checked geometry, sort, perspective-crop, and inverse-transform internals. | Synthetic legal geometry fixtures; documented point order; `1.0` px geometry tolerance. | **Verified** — `src/geometry.rs`, `src/crop.rs`, `src/min_area.rs`. Perspective crop with cubic sampling matches the captured OpenCV oracle across `72` cases; the tall-crop counter-clockwise rotation is exercised by the `classic-v1-e2e-tall-crop` fixture through real artifacts. Reading order is the upstream two-stage sort with a ten-pixel row tolerance, pinned by tests for the backwards swap, the exact boundary, and equal top-left corners. |
| `M2-OCR-001` | Must | `tools/infer/predict_system.py:TextSystem.__call__`; `CLASSIC_OCR_CONTRACT.md`; `API_CONTRACT.md` | Native `Ocr` orchestration API with one complete single-image result or structured error. | Text/no-text/multi-line/rotation fixtures; exact output order; classifier excluded as an intentional M2 difference. | **Verified** — `src/pipeline.rs` runs detect, sort, crop, rotate, batch, restore, filter. Gate `G1` reproduces all four committed end-to-end fixtures exactly, text and confidence within `1e-5`, through real `PP-OCRv6_medium` artifacts. Failure is whole-input with the reason recorded. A real twelve-line book page reads correctly. |
| `M2-API-001` | Must | Classic Python result behavior plus modern module docs for observable fields; `API_CONTRACT.md` | Idiomatic Rust library API with structured errors and `paddleocr-rust/ocr-result/v1`. | P2 fixes API/JSON details. Negative tests cover malformed input and missing/corrupt models. | **Verified** — `src/api.rs` exposes `Artifacts`, `Dictionary`, `OcrOptions`, `TextLine`, `OcrEngine`, `recognize_png`, `recognize_path`, `recognize_reader`, leaking no backend or private type. `paddleocr-rust/ocr-result/v1` is byte-deterministic. `OcrEngine` is `!Sync` and the compiler enforces one engine per thread. `tests/end_to_end.rs` exercises the public surface only. |
| `M2-CLI-001` | Must | `tools/infer/predict_system.py` and module CLI documentation | One native OCR CLI command with explicit local model paths and JSON/JSONL output. | CLI integration fixtures; no network/Python/upstream dependency. | **Verified** — `src/main.rs`. Explicit paths for every artifact, optional digest verification, `--manifest`, `--time-budget-ms`, `--json`, several images with one reused engine and JSONL output. stdout carries results and stderr diagnostics. Demonstrated outside the repository under `env -i` in `GATE_P6_EVIDENCE.md`. |
| `M2-MODEL-001` | Must | v6 model docs, C++ OCR config, and selected artifact provenance | Local manifest/resolution policy without automatic download. | SHA-256, byte limits, format/tensor/dictionary metadata, license/provenance review in P3. | **Verified** — `src/manifest.rs` defines `paddleocr-rust/model-manifest/v1` with provenance, digests, byte counts, and dictionary fingerprint; `src/backend.rs` verifies identity by streaming SHA-256 before a session is created. No download, cache, search path, or environment lookup. Policy in `ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`. |

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
