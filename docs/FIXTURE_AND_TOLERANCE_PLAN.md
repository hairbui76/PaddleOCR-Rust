# M2 Fixture and Tolerance Plan

Roadmap items: `FIX-001`, `TOL-001`
Status: In progress; three component fixture sets are present, but no end-to-end model fixture or capture has been added
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Applies to: the planned M2 classic DB + CTC OCR slice only

## Purpose

This plan turns the high-level fixture policy into a reviewable M2 corpus plan
without treating a model name, a screen-visible OCR result, or an unreviewed
download as compatibility evidence. It separates deterministic unit fixtures
that can be authored in this repository from end-to-end goldens that require a
specific legal model artifact and an isolated upstream capture.

The full provenance rules remain in
[`tests/fixtures/README.md`](../tests/fixtures/README.md), and upstream capture is
governed by [`ORACLE_CAPTURE.md`](ORACLE_CAPTURE.md). This plan never permits
normal tests to read or execute the `PaddleOCR/` symlink.

## Fixture classes and readiness

| Fixture class | Planned identifier pattern | Source and license state | Required before use | Current state |
|---|---|---|---|---|
| Geometry/order unit data | `classic-v1-geometry-*` | Self-authored source-level test values or later JSON data derived from the frozen classic contract; Apache-2.0 repository contribution. | Independent review of expected map rescale, sort, point order, crop size, rotation, polygon metrics, minimum-area candidate boundaries, and invalid geometry cases. | In progress; `src/geometry.rs` covers resize/pad, DB-style map rescale/round/inclusive-clamp, quad order/clip/filter, legacy reading-order swaps/boundary/stability, polygon area/perimeter, and pure crop dimensions/rotation/homography round trips. It samples inverse-map grids across rectangle, trapezoid, oblique, thin convex, and a highly skewed 16,000-by-1-pixel crop, and rejects non-finite sampler coordinates; it preserves odd and maximum-side crop dimensions and covers binary half-tie scaling. Its bounded private minimum-area quadrilateral candidate has self-authored concave/collinear, rotated-order, and triangular vectors, but remains unverified against OpenCV. `src/crop.rs` adds bounded interleaved identity/border/rotation/fixed-kernel vectors plus a constant multi-channel projective case. The reviewed `classic-v1-crop-oracle` BGR fixture captures twelve exact OpenCV 5.0.0 outputs, including non-linear interior, all-side-border, tall projective, eighth-pixel phase, one-pixel, tall-thin, high-variation cubic-rounding, and cubic-weight-construction cases. Its reviewed `inverse-mappings.csv` sidecar supplies sixty OpenCV-captured pre-rotation warp-to-source boundary/interior checks across all twelve cases; five selected phase/one-pixel/tall-thin source-to-warp samples supply the reverse direction. This is not a general OpenCV or decoded-image equivalence claim. Contour and offset cases remain pending. |
| DB map kernel unit data | `classic-v1-db-map-*` | Self-authored row-major `f32` map vectors; Apache-2.0 repository contribution. | Exact strict-threshold boundary, map shape/value validation, row-order, and mask allocation/error behavior. | In progress; `tests/fixtures/classic-v1-db-map-boundaries/` records one exact `m2-unit-v1` representative for `0.3`, adjacent `f32` values, row order, and expected bitmap bytes. `src/db.rs` also covers wrong lengths and NaN/infinity. It is not a captured model tensor, contour, score, or detector golden. |
| CTC unit data | `classic-v1-ctc-*` | Self-authored numeric score matrices and indexes; Apache-2.0 repository contribution. | Independent review of argmax, duplicate removal, blank handling, tie behavior, score mean, dictionary bounds, and filtering. | In progress; `tests/fixtures/classic-v1-ctc-greedy-path/` records one exact `m2-unit-v1` numeric representative for raw repeats, blank reset, lowest-index ties, retained indexes, and mean score. `src/ctc.rs` also covers bounded finite matrices, empty score, and malformed/boundary shapes. Dictionary/text fixtures remain blocked on the verified artifact ABI. |
| Decoder/resource negatives | `classic-v1-input-*` | Self-authored malformed/minimal inputs or generated test bytes; no copied font/image/model data. | P4 decoder decision and a reproducible generator or byte-level provenance record. | Planned; no file added yet. |
| End-to-end image goldens | `classic-v1-e2e-*` | Original contributor material or an explicitly redistributable source only. | Fixture hash/provenance review, exact model/dictionary hashes, isolated oracle capture, and output review. | Blocked on P3 artifacts and approval. |
| Raw tensor/backend qualifications | `classic-v1-tensor-*` | Captured only when artifact terms permit retaining the output; otherwise use an approved derived comparison method. | P3 model/runtime selection and legal review. | Blocked on P3. |

No generated visualization, virtual environment, model binary, cache, upstream
checkout, or external URL is a fixture. A fixture remains unavailable until its
metadata is committed and reviewed; a catalog row is not a fixture itself.

The source-level geometry tests are intentionally not presented as end-to-end
goldens. They validate deterministic contract arithmetic only and do not
replace a provenance-approved image/model/oracle fixture.

## Required M2 coverage

The following cases are M2 acceptance requirements. The named IDs are stable
intentions, not evidence that a file or behavior exists today.

| ID | Layer | Required observable check | Expected comparison | Blocking prerequisite |
|---|---|---|---|---|
| `classic-v1-geometry-reading-order` | Geometry/order | Top-to-bottom plus same-row left-to-right swaps, equal-coordinate stability, and original-order restoration after aspect batching. | Exact order of line IDs. | `GEO-001`, `OCR-002`. |
| `classic-v1-geometry-quad` | Geometry | Clockwise point order, clipping, small/degenerate quadrilateral rejection, and crop dimensions. | Exact integer points/error category; crop dimensions exact. | `GEO-001`, `CROP-001`. |
| `classic-v1-geometry-tall-crop` | Crop | The `height / width >= 1.5` counter-clockwise rotation boundary, including equality. | Exact rotation decision and diagnostic dimensions. | `CROP-001`. |
| `classic-v1-db-segmentation` | DB map | Strict `map_value > 0.3` binary output, exact equality exclusion, row-major order, and malformed/non-finite map rejection. | Exact zero/one bytes for the self-authored `classic-v1-db-map-boundaries` fixture, or typed model tensor-contract error. | `DB-001`. |
| `classic-v1-crop-pixels` | Crop | Perspective-map sampling, replicated source borders, channel preservation, cubic subpixel behavior, output allocation limits, and `np.rot90` byte order. | Exact for self-authored fixed-kernel vectors and twelve reviewed OpenCV 5.0.0 BGR cases, including non-linear fractional interior, all-side-border, tall-projective, eighth-pixel phase, one-pixel, tall-thin, high-variation cubic-rounding, and cubic-weight-construction cases; further approved captures are required before claiming upstream pixel equivalence. | `CROP-001`, `ORACLE-001`. |
| `classic-v1-ctc-basic` | CTC | Blank index 0, immediate duplicate removal before blank removal, selected-max score mean, and empty selection score. | Exact retained class indexes and score for the self-authored `classic-v1-ctc-greedy-path` fixture; text follows after dictionary validation. | `CTC-001`, `REC-003`. |
| `classic-v1-ctc-tie-and-dictionary` | CTC | Lowest-index argmax tie, out-of-range dictionary index, and malformed/non-finite tensor behavior. | The numeric fixture covers lowest-index tie selection; exact decoded text still follows dictionary validation, while malformed/out-of-range cases require a later dictionary binding. | `CTC-001`, `REC-003`. |
| `classic-v1-ctc-unicode` | CTC | Exact UTF-8 preservation for non-ASCII characters; no Arabic reversal in M2. | Exact UTF-8 bytes/text. | `REC-003`, verified dictionary ABI. |
| `classic-v1-ctc-score-boundary` | CTC/pipeline | Recognition score exactly equal to `0.5` is retained; a lower score is filtered. | Exact retained line set. | `REC-003`, `OCR-001`. |
| `classic-v1-input-invalid` | Input | Empty, malformed, oversized, dimension-overflow, and wrong-format input behavior. | Typed error category; no panic or allocation beyond limit. | `IMG-001`, `E2E-001`. |
| `classic-v1-e2e-no-text` | End-to-end | A legal image with no retained text has no detector/recognizer line result. | `lines: []` exactly. | P3 artifact + oracle capture. |
| `classic-v1-e2e-reading-order` | End-to-end | Multiple lines/columns produce the frozen reading order and matching quadrilaterals. | Text/order exact; points within geometry tolerance. | P3 artifact + oracle capture. |
| `classic-v1-e2e-tall-crop` | End-to-end | Vertically oriented text exercises the crop rotation rule. | Text/order exact; crop diagnostic and result checks. | P3 artifact + oracle capture. |
| `classic-v1-e2e-unicode` | End-to-end | A reviewed image and artifact dictionary exercise verified non-ASCII text. | Text bytes/order exact; score tolerance. | P3 artifact + oracle capture. |

Tables, formulas, seals, document pages, orientation classification, and
unwarping deliberately do not appear in the M2 corpus. They are separate later
capabilities and must add their own rows before implementation begins.

## Metadata record

Every real fixture directory must contain `metadata.json` or an equivalent
reviewed UTF-8 JSON record with the following fields in addition to the global
fixture policy:

```json
{
  "fixture_id": "classic-v1-e2e-reading-order",
  "kind": "end_to_end",
  "input": {
    "path": "input.png",
    "sha256": "<64 lowercase hexadecimal characters>",
    "provenance": "original contributor work",
    "license": "Apache-2.0"
  },
  "upstream": {
    "commit": "2661c7c0ef5c613e8f93c6e93b2e052399f0f854",
    "reference_paths": ["tools/infer/predict_system.py"]
  },
  "artifacts": {
    "detector_sha256": "<required for model-backed cases>",
    "recognizer_sha256": "<required for model-backed cases>",
    "dictionary_sha256": "<required for model-backed cases>"
  },
  "expected": {
    "path": "expected.json",
    "schema_version": "paddleocr-rust/ocr-result/v1",
    "comparison_profile": "m2-e2e-v1"
  }
}
```

Placeholder strings are not valid metadata values. They show the required shape
only. Unit fixtures that do not load a model must state `artifacts: null` and
identify their source contract and hand-derived expectation. Each record must
also identify the test name that consumes it and the review date.

## Tolerance profiles

Tolerances are set before comparing a Rust implementation with an oracle. They
do not authorize changing a golden after a mismatch. All source-image geometry
uses pixels with origin at top-left and the point order in
[`CLASSIC_OCR_CONTRACT.md`](CLASSIC_OCR_CONTRACT.md).

| Profile | Applies to | Text/order | Geometry | Scores | Raw tensors | Error/resource behavior |
|---|---|---|---|---|---|---|
| `m2-unit-v1` | Self-authored geometry, DB, and CTC unit fixtures | Exact | Exact where the operation is integer-defined | Exact deterministic arithmetic result | Not applicable | Exact error variant/category and boundary condition. |
| `m2-e2e-v1` | Captured classic end-to-end fixture with the same verified artifact | Exact UTF-8 and final order | Each corresponding quad coordinate absolute error `<= 1.0` px | Absolute error `<= 0.001` | Not applicable | Exact success/error classification; no partial result. |
| `m2-tensor-v1` | P3 backend/model qualification captures | Not applicable | Not applicable | Not applicable | Maximum absolute and relative element error `<= 1e-4`, unless a documented operator-specific exception is approved before acceptance. | Tensor shape/dtype/name mismatch is a model contract error. |
| `m2-determinism-v1` | Same-build, same-artifact single-thread repeat run | Exact | Exact serialized values | Exact serialized values | Exact serialized values where emitted to a qualification test | Byte-identical compact JSONL; no volatile output. |

The profiles instantiate the budgets in
[`QUALITY_PROFILE.md`](QUALITY_PROFILE.md). A future fixture may use a stricter
profile, never a looser one, unless the roadmap is amended with evidence before
the fixture is accepted.

## Capture and review sequence

1. Add or propose an original/reviewed input without a model or upstream
   checkout in normal test execution.
2. Add the complete metadata record and confirm its license/provenance.
3. Resolve the exact model/dictionary artifact and runtime in P3; record their
   hashes and applicable terms.
4. Follow [`ORACLE_CAPTURE.md`](ORACLE_CAPTURE.md) in an isolated checkout to
   obtain candidate expected output.
5. Review the raw diff, expected JSON, model metadata, and selected tolerance.
   Do not normalize text, reorder lines, or round values just to pass.
6. Commit the approved small fixture/golden and an offline Rust test that reads
   only repository data.

## Completion condition

`FIX-001` is complete only after every required M2 fixture class has at least
one approved, offline, reviewable representative where applicable. `TOL-001`
is complete only after every committed fixture points to one of the profiles
above (or a reviewed stricter profile), and the model-backed cases record their
exact artifact hashes. Neither item is completed by this planning document.
