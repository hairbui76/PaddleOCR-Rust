# Foundation Architecture

Roadmap item: FND-004
Status: Implemented foundation boundaries only

## Principle

Public Rust types must not expose an inference-runtime implementation. The
repository deliberately creates a module boundary only when that responsibility
has code and tests; empty module trees and speculative traits are avoided.

## Implemented boundaries

| Module | Responsibility | Public status |
|---|---|---|
| `error` | Stable structured error categories and `Result` alias. | Public foundation API. |
| `types` | Checked encoded-byte boundary, dimensions, geometry values, transforms, scores, model identity, page index, and recognized text. | Public foundation API; it does not decode images. |
| `geometry` | Private classic detector resize/pad planning, detector-map-to-source rescale/round/clip, quadrilateral order/clip/filter, stable reading-order sort, polygon area/perimeter metrics, a bounded minimum-area quadrilateral candidate, and no-allocation perspective crop plan with forward/inverse homographies. | Completed `GEO-001` private foundation; no image decode, contour extraction, polygon offset, OpenCV rectangle-equivalence claim, or public OCR API. |
| `db` | Private checked borrowed one-map DB segmentation with fixed strict M2 threshold and bounded byte-mask output. | `DB-001` only; no runtime tensor ABI, contours, scoring, polygon expansion, boxes, or detector support. |
| `ctc` | Private checked borrowed one-matrix greedy CTC index decoding with classic tie/repeat/blank/mean rules. | `CTC-001` only; no dictionary, text, batch/runtime ABI, or recognizer support. |
| `crop` | Private checked interleaved byte buffer plus scalar perspective sampling, replicated borders, and discrete counter-clockwise post-warp rotation. CI compiles the current path against the `D-003` x86-64 baseline with AVX, AVX2, and FMA disabled. | Early `CROP-001` slice; no decoder, color-space/alpha policy, public image type, portable binary proof, or OpenCV bit-equivalence claim. |
| binary entrypoint | Clearly rejects OCR use until a verified implementation exists. | Not a functional OCR CLI. |

## Later private boundaries

The following boundaries are planned but intentionally do not exist as empty
source modules. They become private implementation details when their owning
roadmap items begin:

| Boundary | Owning roadmap work | Required rule |
|---|---|---|
| Image decoding and remaining input limits | `IMG-*` | Decode checked untrusted bytes with explicit format, dimension, metadata, pixel, and allocation bounds. |
| Detector contour/offset postprocessing | `DET-003` | Preserve selected OpenCV-equivalent contour, rectangle, offset, score, scale, and output ordering semantics only after its runtime and fixture gates. |
| Tensor preprocessing | `TEN-*`, `PRE-001` | Validate dtype, layout, shape, stride, normalization, and allocations. |
| Artifact manifest/resolution | `MOD-*` | Require local identity, hash, format, tensor ABI, and provenance. |
| Inference backend adapter | `RT-*` | Keep backend types private and validate every allocation/tensor boundary. |
| Detection and recognition postprocessing | `DET-*`, `REC-*` | Preserve selected observable semantics and tolerance evidence. |
| OCR orchestration and presentation | `OCR-*`, `API-*`, `CLI-*` | Keep result schema/API/CLI versioned and independent of backend types. |

No later module may make `PaddleOCR/`, Python, PaddleX, or a live upstream
checkout a build, test, or runtime dependency.
