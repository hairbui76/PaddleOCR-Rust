# Fixture Policy

Roadmap item: TEST-001

This directory intentionally contains no upstream asset at bootstrap time.
Every future fixture must be small, legal to redistribute, and offline.
The required M2 fixture classes, metadata shape, and tolerance profiles are
defined in [`FIXTURE_AND_TOLERANCE_PLAN.md`](../../FIXTURE_AND_TOLERANCE_PLAN.md).

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

## Golden format

Use UTF-8 JSON for structured expected results. A golden must record its schema
version, fixture identifier, upstream baseline, model/artifact identity, and
comparison tolerance in addition to its observable output. Golden review is a
source-code review step; overwriting a snapshot is never evidence of parity.

## Safety rule

Fixtures may not require network access, a GPU, Python, PaddleOCR/PaddleX, or
the `PaddleOCR/` symlink during normal tests.
