# Fixture Policy

Roadmap item: TEST-001

This directory intentionally contains no upstream asset at bootstrap time.
Every future fixture must be small, legal to redistribute, and offline.
The required M2 fixture classes, metadata shape, and tolerance profiles are
defined in [`FIXTURE_AND_TOLERANCE_PLAN.md`](../../docs/FIXTURE_AND_TOLERANCE_PLAN.md).

## Required metadata

Each fixture set must add a reviewable metadata record containing at least:

- a stable fixture identifier;
- input file path and SHA-256;
- provenance/source URL or original-author statement;
- applicable license/terms and review date;
- exact PaddleOCR baseline/source paths used as behavioral reference;
- selected model/artifact identity and hash when applicable;
- expected-result/golden file path and schema version;
- tolerance rule and the test(s) that enforce it;
- whether the fixture contains personal data or restricted content.

## Offline integrity gate

`tests/fixture_integrity.rs` parses every committed `metadata.json` record and
checks the current fixture corpus without network access, Python, a GPU,
or the upstream checkout. It requires the pinned baseline, a non-empty source
reference/test/limitation record, explicit Apache-2.0 input provenance, normal
non-symlink paths, and a valid `m2-unit-v1`, `m2-image-input-oracle-v1`, or
`m2-e2e-v1` expected comparison profile. All fixtures except the reviewed
`classic-v1-e2e-no-text`, `classic-v1-e2e-reading-order`, and
`classic-v1-e2e-tall-crop` candidates must have null model artifacts. It
verifies SHA-256 values for direct fixture files.

For `classic-v1-crop-oracle`, the gate also verifies the full capture document,
its inverse-mapping CSV, every base64 `uint8` BGR payload and shape, and the
metadata's concatenated input/output digests across all fifteen reviewed cases.
For `classic-v1-crop-scalar-grid`, it verifies the separate 36-case capture,
its explicit disabled-OpenCV-optimization setting, ordered case IDs, every BGR
payload, and aggregate digests. The gate proves repository fixture integrity
and metadata consistency only; it does not replace source review, artifact-term
review, or oracle/model equivalence. Its dev-only SHA-256 implementation uses
the `sha2` `force-soft` feature, so the hash check itself does not select a
CPU-dispatched test path.

`classic-v1-db-components` is a self-authored component-unit fixture. It
records only private 8-connected bitmap components; it is not a model tensor,
OpenCV contour, or detector result fixture.

`classic-v1-image-inputs` records self-authored encoded PNG/JPEG byte streams,
five bounded negative inputs, and a version-recorded OpenCV `IMREAD_COLOR` BGR
capture. Its generator writes only stdout and runs outside normal Rust tests.
The integrity gate validates every base64 payload, HWC BGR output shape, case
identifier, and aggregate digest. This establishes fixture consistency and
decision evidence only; it does not select a Rust decoder or claim image/OCR
compatibility.

`classic-v1-e2e-no-text`, `classic-v1-e2e-reading-order`, and
`classic-v1-e2e-tall-crop` are the three model-backed fixture records accepted
by the gate. The former contains an 81-byte self-authored PNG and a `lines: []`
result. The reading-order fixture contains an 8,988-byte self-authored
four-word PNG and an expected four-line native-result projection. The tall-crop
fixture contains a 6,913-byte self-authored clockwise-rotated word, a one-line
projection, and the observed source crop pre/post dimensions. Their generators
use external `cv2.putText` and, where applicable, `cv2.rotate`; no font binary,
OpenCV code/binary, upstream image, model byte, dictionary entry, crop pixels,
or raw tensor is committed.

The gate checks all three records' candidate revisions/hashes, terms-review
references, source-result digests, matching two-fresh-process output digests,
and projected results. For the reading-order record it additionally checks the
renderer settings and fixed text/order/quadrilateral values. For the tall-crop
record it additionally checks the renderer, pre/post crop dimensions, and the
source `>= 1.5` rotation branch. It never loads a model or runs OCR: this is an
offline corpus-consistency test, not a Rust OCR differential test or a
supported-model claim.
`tools/generate_e2e_no_text_fixture_input.py`,
`tools/generate_e2e_reading_order_fixture_input.py`, and
`tools/generate_e2e_tall_crop_fixture_input.py` regenerate their inputs only
from reviewed self-authored data, validate exact digests, and use exclusive
output creation; none contacts a model host or upstream checkout.

## Golden format

Use UTF-8 JSON for structured end-to-end expected results. A component-unit
fixture may instead use a small UTF-8 CSV or text grammar when its metadata
declares a schema version and comparison profile. A golden must record its
schema version, fixture identifier, upstream baseline, model/artifact identity,
and comparison tolerance in addition to its observable output. Golden review
is a source-code review step; overwriting a snapshot is never evidence of
parity.

## Safety rule

Fixtures may not require network access, a GPU, Python, PaddleOCR/PaddleX, or
the `PaddleOCR/` symlink during normal tests.
