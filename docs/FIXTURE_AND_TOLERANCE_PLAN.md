# M2 Fixture and Tolerance Plan

Roadmap items: `FIX-001`, `TOL-001`
Status: In progress; twelve offline fixture sets are present, including a
source-level classic score-filter boundary oracle and narrow reviewed
classic-ONNX no-text, reading-order, tall-crop, and Unicode oracles; other
end-to-end, decoder, and tensor coverage remains pending
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
| Geometry/order unit data | `classic-v1-geometry-*` | Self-authored source-level test values or later JSON data derived from the frozen classic contract; Apache-2.0 repository contribution. | Independent review of expected map rescale, sort, point order, crop size, rotation, polygon metrics, minimum-area candidate boundaries, and invalid geometry cases. | `GEO-001` is complete for its private source-level arithmetic: `src/geometry.rs` covers resize/pad, DB-style map rescale/round/inclusive-clamp, quad order/clip/filter, legacy stable reading-order swaps/boundary/stability, polygon area/perimeter, and pure crop dimensions/rotation/homography round trips. It samples inverse-map grids across rectangle, trapezoid, oblique, thin convex, and a highly skewed 16,000-by-1-pixel crop, and rejects non-finite sampler coordinates; it preserves odd and maximum-side crop dimensions and covers binary half-tie scaling. `tests/fixtures/classic-v1-geometry-min-area-candidate/` records a reviewed self-authored concave/collinear representative for the bounded private minimum-area quadrilateral candidate; rotated-order and triangular vectors remain in source tests. The candidate remains unverified against OpenCV. `src/crop.rs` adds bounded interleaved identity/border/rotation/fixed-kernel vectors plus a constant multi-channel projective case. The reviewed `classic-v1-crop-oracle` BGR fixture captures fifteen exact OpenCV 5.0.0 outputs, including non-linear interior, all-side-border, tall projective, eighth-pixel phase, one-pixel, tall-thin, high-variation cubic-rounding, cubic-weight-construction, sampling-matrix, perspective-LU, and scalar nearest-even rounding cases. Its reviewed `inverse-mappings.csv` sidecar supplies seventy-five OpenCV-captured pre-rotation warp-to-source boundary/interior checks across all fifteen cases; five selected phase/one-pixel/tall-thin source-to-warp samples supply the reverse direction. The separate `classic-v1-crop-scalar-grid` fixture adds 36 exact, broad self-authored BGR cases captured after `cv2.setUseOptimized(False)`; it varies source sides from 1 to 31 pixels, one/two-pixel source axes, far border replication, wide/tall/balanced extents, phase boundaries, and perspective perturbations. The `classic-v1-crop-channel-grid` fixture adds 21 exact scalar cases covering the private one-, two-, and four-channel paths for the first time against OpenCV, plus deliberate cubic overshoot: 17 of its 21 warps produce a pre-saturation value outside `[0, 255]`. These are not general OpenCV or decoded-image equivalence claims, and the `opaque-<n>` label deliberately withholds any colour meaning for non-BGR channel counts. Contour and offset cases remain pending as `DET-003` postprocessing evidence. |
| DB map kernel unit data | `classic-v1-db-map-*` | Self-authored row-major `f32` map vectors and zero/one bitmap vectors; Apache-2.0 repository contribution. | Exact strict-threshold boundary, map shape/value validation, row-order, mask allocation/error behavior, and bounded component-scan behavior. | In progress; `tests/fixtures/classic-v1-db-map-boundaries/` records one exact `m2-unit-v1` representative for `0.3`, adjacent `f32` values, row order, and expected bitmap bytes. `tests/fixtures/classic-v1-db-components/` records one exact 8-connected, diagonal, row-major-seed bounds/pixel-count representative. `src/db.rs` also covers wrong lengths, NaN/infinity, empty masks, independent exhaustive 3×3 and 4×4 component references, 4,096 deterministic 6×6 sparse/dense/checkerboard reference patterns, and excess isolated-component rejection. Neither fixture is a captured model tensor, OpenCV contour, score, or detector golden. |
| CTC unit data | `classic-v1-ctc-*` | Self-authored numeric score matrices and indexes; Apache-2.0 repository contribution. | Independent review of argmax, duplicate removal, blank handling, tie behavior, score mean, dictionary bounds, and filtering. | In progress; `tests/fixtures/classic-v1-ctc-greedy-path/` records one exact `m2-unit-v1` numeric representative for raw repeats, blank reset, lowest-index ties, retained indexes, and mean score. `src/ctc.rs` also covers bounded finite matrices, empty score, and malformed/boundary shapes. Dictionary/text fixtures remain blocked on the verified artifact ABI. |
| Pipeline score-filter source data | `classic-v1-ctc-score-boundary` | Self-authored labels, scores, and quadrilaterals exercised with isolated fake collaborators; Apache-2.0 repository contribution. | A clean pinned classic source checkout, exact capture of `TextSystem.__call__`, and no model/tensor retention. | In progress; `tests/fixtures/classic-v1-ctc-score-boundary/` captures the classic `score >= drop_score` loop with `nextafter(0.5, 0)`, `0.5`, and `nextafter(0.5, 1)`: the equality and above values are retained in order. It does not execute CTC decoding or a model and no Rust score-filter implementation exists. |
| Decoder/resource negatives | `classic-v1-input-*` | Self-authored PNG/JPEG streams and malformed/minimal bytes; no copied font/image/model data. | Reproducible generator, byte-level provenance, and a decoder decision before promoting any Rust decoder behavior. | In progress; `tests/fixtures/classic-v1-image-inputs/` records fifteen valid PNG/JPEG streams (including palette/tRNS, 16-bit, progressive JPEG, and Exif orientations 1–8), five bounded negatives, and an isolated OpenCV 5.0.0 `IMREAD_COLOR` BGR capture. The `m2-image-input-oracle-v1` record is decision evidence, not a selected decoder contract. |
| End-to-end image goldens | `classic-v1-e2e-*` | Original contributor material or an explicitly redistributable source only. | Fixture hash/provenance review, exact model/dictionary hashes, isolated oracle capture, and output review. | In progress; `classic-v1-e2e-no-text/` records one 3-by-2 self-authored PNG and its two-fresh-process CPU classic `TextSystem` ONNX result of `lines: []`. `classic-v1-e2e-reading-order/` records a self-authored four-word PNG and a matching four-line text/order/quad/confidence result. `classic-v1-e2e-tall-crop/` records a self-authored clockwise-rotated word and one source `get_rotate_crop_image` branch with a pre-rotation `307×145` crop, a true `>= 1.5` rotation condition, and a `145×307` result. `classic-v1-e2e-unicode/` records the self-authored CJK phrase `你好`, externally rendered with a non-bundled, pinned Noto Sans CJK font whose local package record was reviewed as OFL-1.1; it pins exact UTF-8 result bytes. None retains weights, dictionary entries, tensors, source checkout, font binary, Pillow/OpenCV source/binary, or an upstream image. None chooses a Rust decoder/runtime or proves an implemented Rust OCR path. |
| Raw tensor/backend qualifications | `classic-v1-tensor-*` | Captured only when artifact terms permit retaining the output; otherwise use an approved derived comparison method. | P3 model/runtime selection and legal review. | Blocked on P3. |

No generated visualization, virtual environment, model binary, cache, upstream
checkout, or external URL is a fixture. A fixture remains unavailable until its
metadata is committed and reviewed; a catalog row is not a fixture itself.

The crop-oracle regression
`crop::tests::classic_crop_executes_every_captured_opencv_oracle_case` parses
the committed capture document and executes every recorded input,
quadrilateral, pre-rotation size, rotation decision, and output-byte array.
The companion
`crop::tests::classic_crop_executes_every_captured_opencv_scalar_grid_case`
does the same for the separately recorded scalar OpenCV grid, and
`crop::tests::classic_crop_executes_every_captured_opencv_channel_grid_case`
does the same for the interleaved-channel and saturation grid while asserting
that its one-through-four channel coverage is retained. Focused source
tests retain the individual numerical regressions. No fixture broadens
the component evidence into a general OpenCV or decoded-image oracle.

The current thirteen committed offline fixture directories are also covered by
the offline `tests/fixture_integrity.rs` integration gate. It parses each
metadata record, validates baseline/provenance/profile requirements and direct
file SHA-256 values, and verifies every crop capture/base64 payload/aggregate
digest plus the scalar-grid setting/ordered cases, the channel-grid capture
digest, ordered cases, and required per-case channel-order labels, and the
image-input capture's
valid/negative encoded bytes and BGR output aggregates. The four end-to-end
gates pin exact candidate revisions/hashes and terms-review references,
source-record digests, matching fresh-process output digests, and their
fixture-only native-result projections. The reading-order gate additionally
checks the self-authored renderer settings and fixed four-line order,
quadrilaterals, and confidence-number boundary. The tall-crop gate additionally
checks the self-authored renderer, source crop pre/post dimensions, and the
classic `>= 1.5` rotation branch. The Unicode gate additionally checks the
reviewed external font and license-record digests, renderer settings, and exact
`你好` UTF-8 bytes without retaining the font. This protects fixture-record
consistency only; it does not select a Rust backend/decoder or establish Rust
model support.
The score-filter gate separately checks its no-model source capture, fake
collaborator boundary, two fresh-process digests, and exact below/equality/above
retention record. It is not a Rust pipeline test.

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
| `classic-v1-db-components` | DB map | Private 8-connected foreground components after thresholding, including diagonal connection, row-major seed order, inclusive bounds, and pixel counts. | Exact component records for the self-authored `classic-v1-db-components` fixture, or an explicit resource-limit error for more than 1,000 components. This is not an OpenCV contour/hierarchy/simplification oracle. | `DB-002`. |
| `classic-v1-crop-pixels` | Crop | Perspective-map sampling, replicated source borders, channel preservation, cubic subpixel behavior, output allocation limits, and `np.rot90` byte order. | Exact for self-authored fixed-kernel vectors, fifteen reviewed default-OpenCV 5.0.0 BGR cases, and 36 separately reviewed `cv2.setUseOptimized(False)` BGR grid cases with source sides from 1 to 31 pixels, one/two-pixel source axes, far border replication, wide/tall/balanced extents, phase boundaries, and perspective perturbations, plus 21 reviewed `cv2.setUseOptimized(False)` interleaved cases covering one, two, three, and four channels with extreme `0`/`255` sources that force cubic overshoot in 17 of them; further approved captures are required before claiming upstream pixel equivalence. | `CROP-001`, `ORACLE-001`. |
| `classic-v1-ctc-basic` | CTC | Blank index 0, immediate duplicate removal before blank removal, selected-max score mean, and empty selection score. | Exact retained class indexes and score for the self-authored `classic-v1-ctc-greedy-path` fixture; text follows after dictionary validation. | `CTC-001`, `REC-003`. |
| `classic-v1-ctc-tie-and-dictionary` | CTC | Lowest-index argmax tie, out-of-range dictionary index, and malformed/non-finite tensor behavior. | The numeric fixture covers lowest-index tie selection; exact decoded text still follows dictionary validation, while malformed/out-of-range cases require a later dictionary binding. | `CTC-001`, `REC-003`. |
| `classic-v1-ctc-unicode` | CTC | Exact UTF-8 preservation for non-ASCII characters; no Arabic reversal in M2. | Exact UTF-8 bytes/text. | `REC-003`, verified dictionary ABI. |
| `classic-v1-ctc-score-boundary` | CTC/pipeline | Recognition score exactly equal to `0.5` is retained; a lower score is filtered. | The isolated source capture exactly retains `0.5` and the immediately-above value while dropping the immediately-below value; later Rust comparison is exact. | Source capture is recorded; `REC-003` and `OCR-001` remain required for a Rust implementation. |
| `classic-v1-input-invalid` | Input | Empty, malformed, oversized, dimension-overflow, and wrong-format input behavior. | Typed error category; no panic or allocation beyond limit. | `IMG-001`, `E2E-001`. |
| `classic-v1-e2e-no-text` | End-to-end | A legal image with no retained text has no detector/recognizer line result. | `lines: []` exactly. | Exact legal candidate and isolated classic oracle capture are recorded for fixture evidence; Rust decoder/runtime/pipeline implementation remains separately gated. |
| `classic-v1-e2e-reading-order` | End-to-end | Multiple lines/columns produce the frozen reading order and matching quadrilaterals. | Text/order exact; points within geometry tolerance. | Exact legal candidate and isolated classic oracle capture are recorded for fixture evidence; Rust decoder/runtime/pipeline implementation remains separately gated. |
| `classic-v1-e2e-tall-crop` | End-to-end | Vertically oriented text exercises the crop rotation rule. | Text/order exact; crop diagnostic and result checks. | Exact legal candidate and isolated classic oracle capture are recorded for fixture evidence; Rust decoder/runtime/pipeline implementation remains separately gated. |
| `classic-v1-e2e-unicode` | End-to-end | One reviewed CJK phrase and artifact dictionary exercise a captured non-ASCII result. | Text bytes/order exact; score tolerance. | Exact artifact and isolated oracle capture are recorded for fixture evidence; Rust decoder/runtime/pipeline implementation remains separately gated. |

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
| `m2-unit-v1` | Self-authored geometry, crop, DB, CTC, and source-level score-filter unit fixtures | Exact | Exact where the operation is integer-defined | Exact deterministic arithmetic result | Not applicable | Exact error variant/category and boundary condition. |
| `m2-image-input-oracle-v1` | The self-authored `classic-v1-image-inputs` byte corpus | Not applicable | `D-008` closed this classification on 2026-08-04. The five PNG cases are **exact**: `image::tests::classic_decode_matches_every_captured_opencv_png_case` reproduces every recorded OpenCV BGR byte. The ten JPEG cases are **intentionally unsupported in M2** and must report `Error::Unsupported`; they are retained as `IMG-003` evidence. No tolerated class exists. | Not applicable | Not applicable | The five negative records are now bound to concrete Rust outcomes by `image::tests::classic_decode_reports_every_captured_negative_outcome`: empty input is `InvalidInput{image.bytes, Empty}`, unknown bytes are `Unsupported{image format}`, a truncated PNG is `InvalidInput{.., Malformed}`, an oversized declared width is `ResourceLimit{image.width_pixels, 16384, 16385}` before any pixel allocation, and the filename-hint case decodes by content signature. |
| `m2-e2e-v1` | Captured classic end-to-end fixture with the same verified artifact | Exact UTF-8 and final order | Each corresponding quad coordinate absolute error `<= 1.0` px | Absolute error `<= 0.001` | Not applicable | Exact success/error classification; no partial result. The no-text fixture exercises only the empty `lines` case. The reading-order fixture records one four-line text/order/quad/confidence result. The tall-crop fixture additionally records one source crop-rotation branch. The Unicode fixture records one exact CJK UTF-8 result. These tolerances apply only when a later Rust path is actually compared with the fixtures. |
| `m2-tensor-v1` | P3 backend/model qualification captures | Not applicable | Not applicable | Not applicable | Every matched element must satisfy `abs(candidate - reference) <= 1e-4 + 1e-4 * abs(reference)`, evaluated elementwise on `float64` promotions of the two `float32` values. Every element of both tensors must be finite. An operator-specific exception must be documented and approved before acceptance. | Tensor shape/dtype/name mismatch is a model contract error. |
| `m2-determinism-v1` | Same-build, same-artifact single-thread repeat run | Exact | Exact serialized values | Exact serialized values | Exact serialized values where emitted to a qualification test | Byte-identical compact JSONL; no volatile output. |

The profiles instantiate the budgets in
[`QUALITY_PROFILE.md`](QUALITY_PROFILE.md). A future fixture may use a stricter
profile, never a looser one, unless the roadmap is amended with evidence before
the fixture is accepted.

### `m2-tensor-v1` comparison rule, resolved 2026-08-04

The original profile row required "maximum absolute and relative element error
`<= 1e-4`" without defining a denominator. That is not a computable rule: the
relative term diverges for the near-zero outputs both candidate models actually
produce, so an unqualified reading fails every capture regardless of agreement.
The first static/Paddle-versus-ONNX capture hit exactly that: all 7,057,864
absolute errors were at or below `8.6367130e-5`, while the harness's improvised
`1e-12` denominator floor reported relative failures.

The project user delegated this decision to the agent on 2026-08-04. The
selected rule is the conventional combined form used by `numpy.allclose` and
`torch.allclose`, with `atol = rtol = 1e-4`:

```text
abs(candidate - reference) <= 1e-4 + 1e-4 * abs(reference)
```

Reasons for this form over the alternatives considered:

- It is well defined at `reference == 0`, where it degrades to the absolute
  bound; a bare ratio is undefined there and a floored ratio silently invents a
  denominator that no contract states.
- It is strictly stronger than an absolute-only rule for large magnitudes, so
  it does not weaken the profile where relative error is the meaningful measure.
- It is the rule readers of numerical code already expect, which matters for a
  contract other people must audit.

Two consequences are recorded deliberately:

- The rule is **predeclared**. It is now fixed before the required two fresh
  captures are run. The earlier partial capture stays recorded as **partial**
  and is not retroactively relabelled as passing, because it had only one fresh
  process and its determinism comparison was never run.
- A read-only diagnostic over that partial capture's temporary bytes had zero
  violations under this exact inequality, with a maximum error-to-bound ratio of
  `0.5591462`. That diagnostic informed the choice of an already conventional
  rule; it is not an acceptance result, and `RT-003` still requires the two
  fresh captures.

The reference side of the inequality is the independent reference
implementation being compared against, not the candidate under qualification.
For `RT-003` that is the static/Paddle output.

## Capture and review sequence

1. Add or propose an original/reviewed input without a model or upstream
   checkout in normal test execution.
2. Add the complete metadata record and confirm its license/provenance.
3. Resolve the exact legal model/dictionary artifact and record their hashes
   and applicable terms. An isolated classic oracle may use a temporary CPU
   runtime for capture; that does not select the project's Rust runtime, which
   remains a P3 decision.
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
