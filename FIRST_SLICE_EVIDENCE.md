# First OCR Slice Evidence Record

Roadmap item: SCOPE-001 (decision-support evidence)
Status: Non-binding; no model, runtime, or distribution decision has been made
Prepared: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose

This record makes the first-release choice in `SCOPE_PROPOSAL.md` evidence-led.
It does not create a Rust API, select an inference backend, obtain an artifact,
or claim compatibility. All source paths below were inspected through the
read-only `PaddleOCR/` reference. No upstream source, model, font, fixture, or
configuration is copied into this repository by this record.

Training configuration is useful architectural evidence, but it is not an
inference ABI. Before implementation, `MOD-001` must freeze the exact selected
artifact's inference configuration, tensor names, layouts, dtypes, dynamic
shape rules, dictionary, tokenizer behavior, and output semantics.

## Sources inspected

| Source path | Evidence supplied | Limitation |
|---|---|---|
| `paddleocr/_models/text_detection.py` | The modern standalone detection wrapper defaults to `PP-OCRv6_medium_det`. | Construction and execution delegate to PaddleX. |
| `paddleocr/_models/text_recognition.py` | The modern standalone recognition wrapper defaults to `PP-OCRv6_medium_rec`. | Construction and execution delegate to PaddleX. |
| `deploy/cpp_infer/src/configs/OCR.yaml` | Current native deployment configuration names the v6 medium pair and documents its pipeline-level defaults. | It also enables document orientation and unwarping, which are outside the proposed M2 slice. |
| `configs/det/PP-OCRv5/*`, `configs/det/PP-OCRv6/*` | Detector architecture and training/evaluation postprocess parameters. | Not a shipped inference artifact contract. |
| `configs/rec/PP-OCRv5/*`, `configs/rec/PP-OCRv6/*` | Recognizer architecture, dictionary family, and nominal image shape. | Not a shipped inference artifact contract. |
| `paddleocr-js/packages/core/src/resources/model-asset.ts` | The pinned browser reference names packaged ONNX asset URLs for specific v5/v6 pairs. | A missing entry is not proof that no ONNX export exists elsewhere. |
| `paddleocr-js/packages/core/src/models/det.ts` and `rec.ts` | A separate DB + CTC ONNX-oriented implementation with input/output validation and batching semantics. | It is a browser implementation, not automatically the intended native-Rust contract. |
| `tools/infer/predict_system.py`, `tools/infer/utility.py` | The legacy classic OCR orchestration, reading order, perspective crop, rotation, and score-filter behavior. | It is separate from modern PaddleX pipeline orchestration. |

## Candidate evidence matrix

| Candidate pair | Upstream role | Architecture evidence | Artifact-path evidence | Primary compatibility implication |
|---|---|---|---|---|
| `PP-OCRv6_medium_det` + `PP-OCRv6_medium_rec` | Current default standalone model pair and C++ OCR configuration pair | DB detector: PPLCNetV4 medium + RepLKPAN; recognizer: PPLCNetV4 medium + LightSVTR CTC/NRTR MultiHead | Documentation names Paddle static inference archives. The pinned browser asset map does not list a default ONNX archive for this pair. | Best alignment with current defaults, but modern wrapper parity remains affected by `BASE-002`, and format/runtime qualification is still required. |
| `PP-OCRv6_small_det` + `PP-OCRv6_small_rec` | Current-generation intentional non-default pair | DB detector: PPLCNetV4 small + RepLKFPN; recognizer: PPLCNetV4 small + LightSVTR CTC/NRTR MultiHead | Documentation names inference archives; the pinned browser asset map names ONNX archives for both models. | Current generation with a concrete browser ONNX reference path; it must still be treated as a deliberately different first-release model from the modern default. |
| `PP-OCRv6_tiny_det` + `PP-OCRv6_tiny_rec` | Current-generation smallest named pair | Dedicated v6 tiny training configurations exist. | The pinned browser asset map names ONNX archives for both models. | Lowest listed size-oriented option; accuracy, ABI, and license evidence still need qualification. |
| `PP-OCRv5_mobile_det` + `PP-OCRv5_mobile_rec` | Earlier-generation mobile pair | DB detector: PPLCNetV3 x0.75 + RSEFPN; recognizer: PPLCNetV3 x0.95 + SVTR CTC/NRTR MultiHead | Documentation names inference archives; the pinned browser asset map names ONNX archives for both models. | Most direct classic DB + CTC browser reference, but it is not parity with the current v6 medium default. |

The browser asset map currently lists these model archive identifiers:

```text
PP-OCRv5_mobile_det_onnx_infer.tar
PP-OCRv5_mobile_rec_onnx_infer.tar
PP-OCRv6_small_det_onnx_infer.tar
PP-OCRv6_small_rec_onnx_infer.tar
PP-OCRv6_tiny_det_onnx_infer.tar
PP-OCRv6_tiny_rec_onnx_infer.tar
```

This is distribution-reference evidence only. It is not approval to download,
redistribute, rely on, or bundle any of those files.

## Parameters that must not be silently conflated

The checked sources expose multiple values for what appear to be the same
detector knobs. They must be modeled as provenance-tagged configuration, not
merged by convenience.

| Parameter | v6 medium training config | v6 small training config | C++ default OCR configuration | Consequence |
|---|---:|---:|---:|---|
| DB `thresh` | `0.2` | `0.2` | `0.3` | The selected artifact/configuration must decide the inference value. |
| DB `box_thresh` | `0.45` | `0.45` | `0.6` | A Rust port must not inherit a training value while claiming C++ pipeline behavior. |
| DB `unclip_ratio` | `1.4` | `1.4` | `1.5` | Geometry and box outputs can materially change. |
| DB `max_candidates` | `3000` | `3000` | not declared in this pipeline file | The resource/ordering policy must be explicit. |
| `limit_side_len` / type | not the same training-level field | not the same training-level field | `64` / `min` | Input resizing must be captured from the selected inference artifact or oracle. |
| `max_side_limit` | not the same training-level field | not the same training-level field | `4000` | This is also an input-resource bound, not just an accuracy option. |

The browser model parser has independent fallback defaults, including DB
`thresh=0.3`, `boxThresh=0.6`, and `unclipRatio=2.0`. It parses the archive's
`inference.yml` when present. Therefore neither browser fallbacks nor training
YAML should be used as an implicit native-Rust manifest.

## Shared classic-OCR behavior to preserve or explicitly differ from

For a selected DB + CTC slice, the legacy `TextSystem` path provides a useful
observable sequence to freeze later in `CTR-001`:

1. Run detection and return no OCR result if detection returns no boxes.
2. Sort quadrilateral boxes top-to-bottom and left-to-right; adjacent rows use
   a vertical tolerance of less than 10 pixels before x-order adjustment.
3. Perspective-crop each quadrilateral using edge lengths as output dimensions
   and replicate borders with cubic interpolation.
4. Rotate a crop 90 degrees when `height / width >= 1.5`.
5. Optionally classify orientation; the proposed M2 boundary intentionally has
   no orientation classifier unless the approved artifact requires one.
6. Recognize crops in aspect-ratio batches, restore original crop order, and
   filter recognized results using the explicit score threshold.

These steps are not yet a Rust contract. In particular, coordinate rounding,
degenerate-crop errors, no-text results, score calculation, dictionary
handling, and JSON representation remain P2 decisions with fixtures and
tolerances.

## Decision consequences

| If the priority is | Evidence-led candidate direction | Required wording in compatibility documentation |
|---|---|---|
| Current default-model fidelity | v6 medium pair | Initial release has a selected component pair matching current standalone defaults; it does not yet claim full PaddleX pipeline parity. |
| Current-generation ONNX-oriented proof path | v6 small or v6 tiny pair | The model selection intentionally differs from current default standalone wrappers. |
| Fastest legacy/browser DB + CTC reference | v5 mobile pair | The model selection intentionally differs from the current v6 default and is not a claim of default-model parity. |

Selecting any row still requires all of the following before P3 can start:

- a user-approved pair and initial language/script;
- a legally reviewed source artifact and distribution policy;
- SHA-256 and bounded acquisition/provisioning rules;
- exact exported format and runtime candidate qualification;
- inference configuration and tensor ABI capture;
- dictionary/license/provenance validation;
- predeclared output, geometry, numerical, and resource tolerances.

## Remaining unknowns

This research deliberately does not resolve the following:

- model artifact license terms and redistributability;
- exact archive contents, hashes, inference configuration, and tensor ABI;
- whether a given format can run through a safe maintained Rust backend on the
  chosen baseline platform;
- accuracy, latency, memory, binary-size, and startup budgets;
- exact language coverage and dictionary selection for the first public API;
- PaddleX-dependent modern wrapper/pipeline semantics, pending `BASE-002`.

Those unknowns are decision gates, not gaps that may be filled with guesses.
