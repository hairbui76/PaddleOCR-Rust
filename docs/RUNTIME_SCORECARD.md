# RT-002 Runtime Candidate Scorecard

Roadmap item: `RT-002`
Status: In progress. This scorecard makes the remaining gap quantitative; it
selects no runtime and does not satisfy `RT-002`. Last updated 2026-08-04 after
the shape-coverage expansion raised the total from `186` to `206`
Compiled: 2026-08-04
Rubric: [`RUNTIME_RUBRIC.md`](RUNTIME_RUBRIC.md)

## What this document is, and is not

[`RUNTIME_RUBRIC.md`](RUNTIME_RUBRIC.md) states that `RT-002` scores candidates
"after passing every blocker", and that an unknown dimension scores `0` rather
than an assumed value. Not every blocker has passed, so this is deliberately a
**gap-measuring** scorecard rather than a qualifying one: it applies the `0`
rule honestly so that the distance to a defensible `RT-004` decision is a
number instead of an impression.

Filling it in early is useful because the `RT-004` decision template requires a
scorecard total and evidence links, and because the exercise exposes which
remaining gates are genuinely closable now and which are structurally blocked
until after the decision.

Nothing here selects a backend, approves an artifact, or authorizes a Cargo
dependency.

## Candidates

| Candidate | Version and configuration | Current standing |
|---|---|---|
| `ort` | `2.0.0-rc.13` with `std`, `load-dynamic`, `api-28`, against ONNX Runtime `1.28.0` CPU, both a temporary Python-wheel library and a separately source-built library | The only candidate still under evaluation |
| `tract-onnx` | `0.23.4` | **Rejected** for the exact-artifact configuration: its ordinary dynamic-shape typing fails before inference. See [`RUNTIME_TRACT_EVIDENCE.md`](RUNTIME_TRACT_EVIDENCE.md). A metadata-altered diagnostic run is explicitly non-qualifying. |

Because `tract-onnx` fails a blocker, it is not scored. The rubric's weighted
comparison exists to break ties between correct candidates, not to rescue a
rejected one.

## Blocker status

| Gate | Status | Basis |
|---|---|---|
| Exact artifact | **Pass** for the ONNX pair | Every recorded probe verifies the exact byte counts and SHA-256 values before loading. The static pair has never been loaded from Rust, and no reviewed conversion route exists. |
| Graph/operator completeness | **Pass on the host** | Updated 2026-08-04: twenty-three shapes now execute with no failure, fallback, or graph edit — the six `RT-003` controls plus ten detector and seven recognizer shapes spanning the frozen `960/max` policy and the candidate HPI range through `[8, 3, 48, 3200]`. 33,452,620 output elements, zero non-finite, identical fingerprints across two fresh processes. No-AVX coverage for the new shapes is still absent. |
| Tensor ABI | **Partial** | Names, `float32`, NCHW, rank, dynamic axes, and batch semantics are checked, but the rubric requires validation "from the pinned manifest" and `MOD-002` has not produced one. |
| Numerical equivalence | **Pass, narrowly** | `RT-003` completed on 2026-08-04: two fresh processes, byte-identical aggregates, zero `m2-tensor-v1` violations across 7,057,864 elements. It covers the Python-wheel library on an AVX host and the six declared shapes only. |
| End-to-end semantics | **Not started** | [`RUNTIME_ORT_EVIDENCE.md`](RUNTIME_ORT_EVIDENCE.md) records this gate as "Not run". Structurally it cannot run before a pipeline exists. |
| CPU support | **Partial** | Bounded QEMU TCG no-AVX execution is recorded, with guard evidence that AVX use would trap. No physical no-AVX host has been used, and no emulation-acceptance policy has been recorded. |
| Resource controls | **Partial** | Lifecycle, shared-session, reuse, and cancellation probes are recorded; [`RUNTIME_ORT_EVIDENCE.md`](RUNTIME_ORT_EVIDENCE.md) itself marks the gate "Incomplete". The gate is written "at the adapter boundary", and no adapter exists. |
| Supply chain/license | **Fail, closable** | Licenses, notices, and a clean OSV scan are recorded, but [`RUNTIME_ORT_SOURCE_EVIDENCE.md`](RUNTIME_ORT_SOURCE_EVIDENCE.md) records the source build as non-hermetic: `FETCHCONTENT_FULLY_DISCONNECTED` was `OFF` and one FetchContent dependency was not hash-enforced at fetch time. No SBOM exists. |
| Unsafe/FFI boundary | **Partial** | A pre-adoption source review of the exact wrapper closure is recorded. The gate requires the unsafe surface to sit inside a small documented adapter with tests, which is `RT-005` work. |

## Weighted scorecard: `ort` 2.0.0-rc.13

Scores are `0` where the evidence does not yet exist, per the rubric.

| Dimension | Weight | Score | Weighted | Basis for not scoring higher |
|---|---:|---:|---:|---|
| Exact graph/operator/shape coverage | 20 | 4 | 80 | Raised from `3` on 2026-08-04: twenty-three shapes across the frozen policy range now execute cleanly and repeatably. Held below `5` because the expansion ran only on the AVX host and the static representation has still never been loaded from Rust. |
| Raw numerical and end-to-end fidelity | 20 | 2 | 40 | The tensor gate passes narrowly; geometry, text, score, and deterministic-output gates have not run. |
| CPU portability and determinism | 12 | 2 | 24 | Explicit thread controls and repeatable host output, but only emulated no-AVX coverage, no accepted emulation policy, and recorded host/QEMU bit-pattern differences. |
| Memory/latency/binary budget | 12 | 0 | 0 | No measurement against the `QUALITY_PROFILE.md` budgets exists. Peak-RSS observations from unrelated probes are not a budget pass. |
| Safety and native boundary | 10 | 1 | 10 | A source review exists; the required small audited adapter does not. |
| Licensing, maintenance, and supply chain | 10 | 2 | 20 | Licenses and notices recorded and OSV clean, but the build is not hermetic and there is no SBOM or reproducible lock. |
| Deployment path | 6 | 2 | 12 | Dynamic loading is demonstrated and glibc requirements are recorded; no packaging or distribution path is decided. |
| Diagnostics and operability | 5 | 2 | 10 | Version reporting and model diagnostics exist; the wrapper surfaces raw native error strings, which is recorded as a gap. |
| Rust integration cost | 5 | 2 | 10 | Process-global loader and environment ownership, manual native-pointer `Send`/`Sync` assertions, and an explicit non-concurrent same-session contract all raise integration cost. |
| **Total** | **100** | | **206 / 500** | |

A total is only comparable against another candidate scored the same way. There
is currently no second candidate to compare with, which is itself a finding:
the rubric's weighted comparison cannot do its job with one survivor.

## What the number means

`206 / 500` is not a verdict on `ort`'s quality. Most of the deficit sits in
dimensions that **cannot** be scored before a decision is made:

- End-to-end fidelity, adapter-boundary resource controls, and the audited
  unsafe boundary all depend on `RT-005`, which depends on `RT-004`, which
  depends on this item. That is a dependency cycle in the roadmap, not a defect
  in the candidate.
- Memory, latency, and binary budgets cannot be measured without the pipeline
  those budgets describe.

Two deficits are genuinely closable now and are not part of that cycle:

1. **Supply chain (weight 10).** A hermetic rebuild with
   `FETCHCONTENT_FULLY_DISCONNECTED=ON`, hash-pinned dependencies including the
   currently unpinned one, a verified source tag, and a generated SBOM.
   Estimated cost: a multi-hour native build plus review.
2. **Shape coverage (weight 20).** ~~Probing additional valid `960/max` shapes
   with the already-built library.~~ **Closed on 2026-08-04**: twenty-three
   shapes executed cleanly and repeatably in 20.51 s using the existing
   library. The dimension is still not a `5` because the expansion covers the
   AVX host only.

One deficit needs a recorded decision rather than an experiment:

3. **CPU support.** Either a physical no-AVX host is obtained, or an emulation
   acceptance policy is recorded that states exactly what QEMU TCG coverage is
   accepted to stand for and what it is not. No physical pre-AVX host is
   available in the current environment.

## Recommendation for `RT-004`

Deciding `RT-004` today would require the ADR to state plainly that three
blockers are being converted into **post-decision gates** with a recorded
reversal condition. That is a legitimate way to break the dependency cycle, but
it is a scope decision, not an evidence outcome, and it must be recorded as
such before `RT-005` begins.

The honest sequence is: close (1) and (2), record (3), then decide `RT-004`
with the cycle-bound gates named explicitly as conditions on `RT-005`. Nothing
in this document authorizes skipping that.
