# Initial Quality Profile

Roadmap item: QUAL-001
Status: Approved P0 budgets; not yet measured
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854
Applies to: the selected M2 classic `PP-OCRv6_medium` slice only

## Reference configuration

The primary reference configuration is `x86_64-unknown-linux-gnu`, Rust
`1.94.0`, and the host recorded in `P0_DECISIONS.md`. Golden semantic tests run
single-threaded. Benchmark reports must disclose actual CPU, core/thread policy,
memory, OS, toolchain, artifact hashes, model format, backend, and corpus.

The reference host is a measurement point, not an unqualified performance
claim. Other targets cannot inherit a result without their own `PLAT-001`
evidence.

## Predeclared correctness budgets

| Dimension | M2 acceptance budget | Evidence required |
|---|---|---|
| Text and output order | Exact UTF-8 text and exact result order for each approved golden fixture. | Golden/differential tests against a pinned oracle capture. |
| No-text behavior | Exact empty-result representation defined by the P2 schema; never fabricated text. | Empty-image/no-text fixture and malformed-input tests. |
| CTC scores | Absolute difference at most `0.001` from the pinned oracle for the same verified artifact/backend comparison. | Captured raw and decoded-output comparison. |
| Detector geometry | Corresponding source-image polygon vertices within `1.0` pixel absolute coordinate error after the documented inverse transform, with identical point order. | Golden polygon comparison and transformation tests. |
| Raw tensor qualification | Every matched element must satisfy `abs(candidate - reference) <= 1e-4 + 1e-4 * abs(reference)`, evaluated elementwise on `float64` promotions, for components that are expected to be numerically equivalent; an approved exception must identify the operator and tolerance. This is the `m2-tensor-v1` rule resolved on 2026-08-04 and recorded in [`FIXTURE_AND_TOLERANCE_PLAN.md`](FIXTURE_AND_TOLERANCE_PLAN.md#m2-tensor-v1-comparison-rule-resolved-2026-08-04); the earlier wording named a relative bound without a denominator, which is not computable at the near-zero outputs the candidates actually produce. | P3 backend/raw-output comparison. |
| Determinism | Repeated single-thread CPU runs on the same input/artifact produce byte-identical serialized JSON/JSONL. | Repeat-run test with recorded environment. |
| Error behavior | Invalid input, missing/corrupt/wrong model, invalid tensor metadata, and resource limits return typed errors without panic. | Unit, integration, and fuzz/property tests. |

## Predeclared resource and performance budgets

These are release gates for the selected M2 artifact. They are deliberately
stated before implementation and may change only through an evidence-backed
roadmap amendment made before accepting a failed result.

| Dimension | M2 acceptance budget | Measurement condition |
|---|---|---|
| Encoded input size | Reject input larger than `64 MiB`. | Input bytes before decoding. |
| Decoded dimensions | Reject images exceeding `40,000,000` pixels or either dimension above `16,384` pixels. | Before allocating model tensors. |
| Work units | Reject a request requiring more than `1,000` detected text regions. | Before unbounded crop/recognition work. |
| CPU execution | No GPU requirement; golden tests use one CPU thread unless a test explicitly covers concurrency. | All normal test and M2 acceptance runs. |
| Cold CLI end-to-end latency | At most `15 s` for a 1280×720 approved fixture with pre-provisioned local models on the reference host. | Process start through JSON result, one thread, no network. |
| Warm end-to-end latency | Median at most `5 s`, p95 at most `10 s` across 20 runs of the same fixture after model warmup. | In-process API, one thread, reference host. |
| Peak resident memory | At most `2 GiB` for the same end-to-end fixture, excluding OS file cache. | Native release build, reference host. |
| Stripped CLI binary | At most `100 MiB`, excluding model artifacts. | Release package inspection. |
| Model artifacts in package | `0` bytes; model artifacts are separately provisioned local files. | Package contents inspection. |

## Required reporting discipline

- A failed budget is a failed gate, not a reason to silently relax the budget.
- A benchmark without the exact model hash/format/backend and host information
  is not comparable evidence.
- Results from a different CPU, OS, runtime, or artifact must be reported as
  separate measurements.
- Performance work must never remove bounds, checksum validation, or error
  diagnostics merely to improve a benchmark.
