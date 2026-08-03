# M2 Runtime Qualification Rubric

Roadmap item: `RT-001`
Status: Done as a qualification rubric; no runtime, format, or backend is selected
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Applies to: the planned M2 v6-medium detector and recognizer candidates only

## Purpose and decision boundary

This rubric governs the evidence required to resolve `D-006`. It prevents a
runtime from becoming architecture merely because it can load one convenient
file or because its benchmark appears attractive. `RT-002` must apply every
gate below to the exact local artifacts recorded in
[`MODEL_CANDIDATES.md`](MODEL_CANDIDATES.md).

No score, package name, or experiment in this document selects a runtime. A
runtime/format decision requires `RT-004`, including measured evidence,
artifact identity, platform, conversion status, and a documented migration
strategy. The public Rust API remains backend-neutral regardless of the choice.

## Non-negotiable blockers

A candidate is rejected for M2 if any condition in this table is not met. A
high total score cannot override a blocker.

| Gate | Required evidence | Blocker condition |
|---|---|---|
| Exact artifact | Load the exact local static or ONNX files by verified manifest/hash; never fetch implicitly. | Model name alone, moving URL, unverified hash, bundled cache, or runtime download. |
| Graph/operator completeness | Inspect the actual graph and execute detector and recognizer with every required operator/shape. | Unsupported operator, hidden fallback, opaque partial graph, or model-specific source patch. |
| Tensor ABI | Validate names, `float32`, NCHW layout, rank, dynamic dimensions, batch semantics, and output order from the pinned manifest. | Silent reshape/layout/dtype coercion or output-position guessing. |
| Numerical equivalence | Compare raw detector/recognizer outputs with the isolated static oracle under `m2-tensor-v1`. | Element error above `1e-4` without a reviewed, predeclared operator-specific exception. |
| End-to-end semantics | Pass approved offline M2 goldens using the frozen preprocessing, DB, crop, CTC, and result contracts. | Text/order mismatch, unaccounted geometry drift, threshold drift, fabricated partial result, or unknown dictionary mapping. |
| CPU support | Run on the stated `x86_64-unknown-linux-gnu` baseline without an AVX/AVX2 requirement or GPU. | Required unsupported instruction set, GPU, Python, upstream checkout, or network. |
| Resource controls | Bound allocations, threads, batches, inputs, model loading, and errors at the adapter boundary. | Unbounded user-controlled allocation/work, panic/abort on malformed model/input, or ignored cancellation/resource policy. |
| Supply chain/license | Record Rust/native dependencies, artifact/conversion licenses, notices, CVEs/maintenance posture, and deployment terms. | Unreviewed proprietary/unclear terms, unacceptable native binary, or missing notice/provenance. |
| Unsafe/FFI boundary | Keep all necessary unsafe/native calls inside a small documented adapter with tests and error mapping. | Unsafe or C/C++ implementation details leak through the public API, or boundary cannot be audited. |

## Candidate representations to qualify

`MODEL_CANDIDATES.md` intentionally records two representations, not selected
backends:

1. The revision-pinned Paddle static package (`inference.json` plus
   `inference.pdiparams`). A candidate runtime must either consume that exact
   format or demonstrate an explicitly reviewed conversion route.
2. The separately revision-pinned official ONNX export. It is a distinct
   candidate artifact, not proof that it is numerically identical to the
   static package.

Any future representation (for example a safe tensor export) enters this
rubric only after an exact source, revision, hashes, conversion tool/version,
license, and static-oracle differential plan are recorded. Custom
transliteration of a Paddle graph is not an M2 shortcut unless it passes the
same qualification and security requirements.

## Weighted comparison scorecard

After passing every blocker, `RT-002` scores each serious runtime candidate
from 0 through 5 in each dimension. Evidence must be linked beside each score;
unknown is `0`, not an assumption. The weights total 100 and guide tradeoffs
only after correctness is proven.

| Dimension | Weight | A score of 5 requires |
|---|---:|---|
| Exact graph/operator/shape coverage | 20 | Both exact artifacts execute every required dynamic shape with no custom graph edits. |
| Raw numerical and end-to-end fidelity | 20 | Tensor, geometry, text, score, and deterministic-output gates all pass. |
| CPU portability and determinism | 12 | Baseline x86-64 execution, explicit thread controls, repeatable output, and no hidden ISA/GPU dependency. |
| Memory/latency/binary budget | 12 | Meets the M2 budgets in `QUALITY_PROFILE.md` with recorded host/artifacts. |
| Safety and native boundary | 10 | No unsafe code in this crate, or an exceptionally small audited adapter with bounded errors/ownership. |
| Licensing, maintenance, and supply chain | 10 | Clear compatible licenses, active maintenance/security posture, locked/reproducible dependencies, and notices. |
| Deployment path | 6 | A feasible later path to supported targets without committing M2 to unsupported platforms. |
| Diagnostics and operability | 5 | Safe actionable errors, model/tensor diagnostics, explicit version reporting, and no sensitive-data leakage. |
| Rust integration cost | 5 | Minimal stable dependencies and clean ownership/concurrency behavior without public backend leakage. |

The scorecard must list the candidate's version, features, native libraries,
platform/CPU, compiler, model hashes, conversion status, and command lines.
No benchmark number may be compared across different artifacts or thread policy.

## Required proof sequence

1. Obtain user-provisioned local artifacts that match the selected candidate
   revision and SHA-256. Do not place them in this repository.
2. Use an isolated environment outside both repositories to inspect the
   artifact and capture static-oracle raw tensors following
   [`ORACLE_CAPTURE.md`](ORACLE_CAPTURE.md).
3. Build a bounded runtime spike outside the public API. It must load only
   local files and expose enough diagnostics to validate the manifest.
4. Exercise detector shapes at the planned minimum/typical/maximum sizes and
   recognizer batches/widths documented by the candidate metadata. Record
   operator failures instead of adapting the graph silently.
5. Compare raw outputs with `m2-tensor-v1`, then run approved end-to-end
   fixtures with `m2-e2e-v1` and `m2-determinism-v1`.
6. Measure cold/warm latency, resident memory, binary size, thread behavior,
   and error/resource behavior on the reference host.
7. Record the scorecard and rejection evidence for every serious candidate.
   Only then may `RT-004` choose an initial backend/format.

Spikes are developer-only evidence. They must not become a normal Cargo
feature, CI dependency, model download, or user-facing API until the decision
and manifest gates close.

## Decision record template for `RT-004`

The later ADR must answer every field below:

```text
Candidate name/version:
Artifact representation and pinned revisions:
Local artifact/config/dictionary SHA-256 values:
Host/toolchain/thread policy:
Native libraries and unsafe boundary:
Operator/shape coverage result:
Raw tensor comparison result:
End-to-end fixture result:
Latency/memory/binary result:
License/provenance/notice result:
Scorecard total and evidence links:
Rejected alternatives and reasons:
Migration/extension strategy:
```

## Completion boundary

`RT-001` is complete because the qualification criteria and scoring method are
now frozen. `RT-002` is in progress: the exact-artifact `tract-onnx` 0.23.4
route was rejected, while an external `ort` 2.0.0-rc.13 dynamic-load spike
partially passed exact graph/shape and named-tensor ABI probes. A separate
source-built ONNX Runtime 1.28.0 route repeated all six host probes and ran
every declared detector/recognizer shape through no-AVX QEMU TCG routes.
Calibrated compact fingerprints show host/QEMU bit-pattern differences for
detector minimum and all recognizer profiles. A later temporary direct Python
ONNX Runtime-to-Rust `ort` relay is bit-identical across all six LCG shapes,
but both sides use one common temporary native library and it is neither an
independent raw elementwise comparison nor a static Paddle comparison. See [`RUNTIME_TRACT_EVIDENCE.md`](RUNTIME_TRACT_EVIDENCE.md),
[`RUNTIME_ORT_EVIDENCE.md`](RUNTIME_ORT_EVIDENCE.md), and
[`RUNTIME_ORT_SOURCE_EVIDENCE.md`](RUNTIME_ORT_SOURCE_EVIDENCE.md). A separate
source-built C API lifecycle probe also completed twelve sequential
create/run/release cycles for each exact minimum-shape model under one-thread
controls and bounded Linux-host resources; its short RSS observation is not a
leak, cancellation, concurrency, or Rust-adapter proof. No result accepts a
local artifact or clears every blocker gate: independent raw-tensor
equivalence, end-to-end semantics, physical CPU/platform coverage,
resource/error behavior, supply chain, and the native boundary remain
incomplete. `RT-003` is in progress, `RT-004` remains planned, and `D-006`
remains open.
