# M2 Must Contract Coverage

Roadmap item: `CTR-003`  
Status: Done — current M2 Must coverage is mapped; each future owning
implementation must update its row before code starts  
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and non-claim

This record maps every M2 Must row in [COMPATIBILITY.md](COMPATIBILITY.md) to
its current behavioral contract, implementation evidence, and explicit start
gate. It prevents a partially implemented private layer from being mistaken for
a detector, recognizer, OCR API, CLI, or supported model path.

It is a coverage index, not a second source of algorithm defaults, a public API
specification, an artifact manifest, or a compatibility claim. The linked
contract documents remain authoritative. An `In progress` row below does not
mean that M2 OCR is usable.

## M2 Must matrix

| Compatibility row | Current contract authority | Implemented evidence | Required before the owning implementation may advance | Current boundary |
|---|---|---|---|---|
| `M2-GEO-001` | [CLASSIC_OCR_CONTRACT.md](CLASSIC_OCR_CONTRACT.md) defines resize, DB-coordinate, reading-order, crop, border, rotation, and source-coordinate rules; [FIXTURE_AND_TOLERANCE_PLAN.md](FIXTURE_AND_TOLERANCE_PLAN.md) defines unit/oracle evidence. | Private `src/geometry.rs` and `src/crop.rs` implement checked geometry, a bounded convex-hull minimum-area candidate over checked polygon vertices, and a bounded crop candidate with self-authored unit vectors. Crop geometry includes a skewed 16,000-by-1-pixel homography round trip, non-finite sampler-coordinate rejection, five selected source-to-warp checks, and fifty OpenCV-captured pre-rotation warp-to-source boundary/interior checks across the ten reviewed crop cases. `tests/fixtures/classic-v1-crop-oracle/` supplies ten exact BGR OpenCV 5.0.0 crop regressions, including non-linear fractional interior, all-side-border, tall-projective, eighth-pixel phase, one-pixel, and tall-thin cases. Neither candidate is a general OpenCV-equivalence claim. | Broader approved OpenCV pixel coverage; decoded-image/color semantics; contour/offset implementation; verified OpenCV minimum-area semantics. | Private only. No decoded-image or upstream pixel/rectangle-equivalence claim. |
| `M2-DET-001` | The detector preprocessing and DB map/postprocessing sections of [CLASSIC_OCR_CONTRACT.md](CLASSIC_OCR_CONTRACT.md); candidate tensor observations in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md). | `src/db.rs` implements `DB-001`: private checked strict segmentation over one borrowed finite row-major map. No detector tensor ABI, inference adapter, contour, scoring, offset, or DB box postprocessor exists. | `IMG-DEC-001`, `IMG-002`, `TEN-001`, accepted local artifact/ABI, runtime qualification, and the `DET-001`/`DET-002` gates. The candidate manifest thresholds must not replace the frozen M2 profile. | Absent; no detector support claim. |
| `M2-REC-001` | The recognition/CTC section of [CLASSIC_OCR_CONTRACT.md](CLASSIC_OCR_CONTRACT.md); candidate recognizer/dictionary evidence in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md). | `src/ctc.rs` implements `CTC-001`: private checked numeric greedy indexes with classic tie/repeat/blank/mean behavior. No recognizer tensor batching/inference, dictionary, text decoder, or language behavior exists. | `IMG-002`, `TEN-001`, verified artifact dictionary ABI, runtime qualification, and `REC-001`/`REC-002`. | Absent; no language or CTC-output compatibility claim. |
| `M2-OCR-001` | The classic system sequence in [CLASSIC_OCR_CONTRACT.md](CLASSIC_OCR_CONTRACT.md) and result invariants in [API_CONTRACT.md](API_CONTRACT.md). | No public `Ocr` engine or pipeline orchestration exists. | Gate P5 components, `CROP-001` oracle/decoder evidence, `OCR-001` through `OCR-003`, and offline end-to-end fixtures. | Absent; binary intentionally rejects OCR use. |
| `M2-API-001` | [API_CONTRACT.md](API_CONTRACT.md) freezes the native typed surface, JSON schema, privacy, errors, and intentional differences. | Public foundation types/errors exist, but no `Ocr`, request/options/result/line types, or serializer exists. | Approved detector/recognizer/model provenance and P6 `API-001`/`SCHEMA-001` implementation with negative and deterministic serialization tests. | Contract-only; no OCR API exists. |
| `M2-CLI-001` | [API_CONTRACT.md](API_CONTRACT.md) fixes stdout/privacy/schema principles; detailed command syntax, exit codes, multi-input behavior, and model-path handling are deliberately owned by `CLI-001`. | `src/main.rs` reports that OCR is unsupported and exits with the documented bootstrap failure behavior. | A functioning P6 API/schema plus a `CLI-001` contract that fixes arguments, exit codes, stderr, local model provisioning, and JSONL behavior. | Bootstrap-only; not an OCR CLI. |
| `M2-MODEL-001` | [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md), [CANDIDATE_PROVISIONING_LEDGER.md](CANDIDATE_PROVISIONING_LEDGER.md), [LICENSE_REVIEW.md](LICENSE_REVIEW.md), and [RUNTIME_RUBRIC.md](RUNTIME_RUBRIC.md). | Candidate revisions, known hashes, and a local-only verification procedure are recorded; no artifact resolver or runtime exists. | User-provisioned candidate files outside the repository, revision-specific provenance/terms, dictionary/operator validation, `RT-002`/`RT-003` evidence, `RT-004`, and `MODEL-DEC-001`. | No supported artifact, download, cache, conversion, or backend. |

## Cross-cutting contract gaps

The following missing evidence blocks multiple rows and must not be papered over
with broad API scaffolding:

1. `D-006` remains open: no inference format or backend has been chosen.
2. `D-007` remains only local-only at bootstrap level: no exact artifact,
   conversion, distribution, cache, or download policy is accepted.
3. `D-008` remains open: decoder format support, orientation, color/alpha
   handling, and native dependencies have not been selected.
4. The recognizer's 18,710-class output and inline dictionary do not yet prove
   CTC index, blank, space, duplicate, or language semantics.
5. No legally reviewed image/model fixture or model-backed end-to-end oracle
   output exists for upstream differential validation. The narrow BGR crop
   component oracle does not close this gap.

These are evidence and decision gaps, not invitations to create fallback
behavior. Unsupported surfaces remain absent or return a structured unsupported
error once a public surface exists.

## Transition discipline

Before an owning implementation begins, update this matrix and the matching
compatibility row with the exact contract revision, fixture/tolerance profile,
and dependency evidence it consumes. Before a row becomes `Verified`, link the
implementation, tests, artifact identity, platform/runtime evidence, and any
approved intentional difference. A green geometry unit test or a model family
name alone is insufficient.

## Completion boundary

`CTR-003` is complete because every current M2 Must row has a stable contract
for its implemented responsibility, including all public API/CLI details that
code is about to expose, and every unresolved semantic boundary is assigned to
its owning later gate. The matrix must be revised before a later implementation
adds a responsibility. This does not close P2 fixture/tolerance work, P3–P6
implementation, or compatibility verification.
