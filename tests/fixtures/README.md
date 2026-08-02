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
checks the current self-authored corpus without network access, Python, a GPU,
or the upstream checkout. It requires the pinned baseline, a non-empty source
reference/test/limitation record, explicit Apache-2.0 input provenance, null
model artifacts, normal non-symlink paths, and a valid `m2-unit-v1` or
`m2-image-input-oracle-v1` expected comparison profile. It verifies SHA-256
values for direct fixture files.

For `classic-v1-crop-oracle`, the gate also verifies the full capture document,
its inverse-mapping CSV, every base64 `uint8` BGR payload and shape, and the
metadata's concatenated input/output digests across all fifteen reviewed cases. The
gate proves repository fixture integrity and metadata consistency only; it does
not replace source review, artifact-term review, or oracle/model equivalence.
Its dev-only SHA-256 implementation uses the `sha2` `force-soft` feature, so
the hash check itself does not select a CPU-dispatched test path.

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
