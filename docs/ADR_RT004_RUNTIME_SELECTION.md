# ADR `RT-004`: initial inference backend selection

Roadmap items: `RT-004`, resolves `D-006`
Decision date: 2026-08-04
Decided by: project user, who reviewed the recorded cost and the residual gaps
and directed this decision on 2026-08-04
Status: Accepted, with three named post-decision gates and a reversal condition

## Decision

**Select ONNX Runtime 1.28.0 CPU, reached from Rust through `ort` 2.0.0-rc.13
with `std`, `load-dynamic`, and `api-28`, consuming the exact pinned ONNX
export.** The native library is loaded at runtime from an explicit caller-
supplied path; it is never bundled, downloaded, or required to build or test
this repository.

This decision is taken **before** three blocker gates are closed, because those
gates cannot be closed before it. That is stated plainly below rather than
disguised as completed evidence.

## Required decision record

```text
Candidate name/version:
  ort 2.0.0-rc.13 (std, load-dynamic, api-28) over ONNX Runtime 1.28.0 CPU.

Artifact representation and pinned revisions:
  ONNX export, not the static Paddle program.
  PaddlePaddle/PP-OCRv6_medium_det_onnx @ 61323801669c338b7891481ec7bac61ce31b576a
  PaddlePaddle/PP-OCRv6_medium_rec_onnx @ 50c7eacafc52fa7bcf4194e8cd08e46f8558504b

Local artifact/config/dictionary SHA-256 values:
  detector inference.onnx  eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1  (62,032,837 bytes)
  recognizer inference.onnx 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba  (76,554,979 bytes)
  detector inference.yml   7298d5ead546584af2504d03355f881ac7a7bc0eb1e282d3e159277c1d0af871  (886 bytes)
  recognizer inference.yml 991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129  (150,580 bytes)
  recognizer ordered dictionary stream b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d (18,708 entries)

Host/toolchain/thread policy:
  x86_64-unknown-linux-gnu, Rust 1.94.0, CPUExecutionProvider only.
  One intra-op and one inter-op thread for every recorded qualification run.
  GPU disabled. No network, Python, or upstream checkout at build, test, or run time.

Native libraries and unsafe boundary:
  libonnxruntime.so.1.28.0, dynamically loaded. The source-built control is
  1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa (31,428,768 bytes).
  This crate keeps unsafe_code = "forbid". Every unsafe and FFI call stays
  inside `ort`, behind the internal adapter defined by RT-005.

Operator/shape coverage result:
  23 of 23 declared shapes executed with no fallback and no graph edit:
  the six RT-003 qualification shapes, ten detector shapes across the frozen
  960/max policy, and seven recognizer shapes through [8, 3, 48, 3200].
  33,452,620 output elements, zero non-finite. Two fresh processes produced
  identical fingerprints (stable-core SHA-256
  1685ce10d85d4162b35ca7ec6a33d3fd065dc7011decd09c6dd9d78637d0606d).
  AVX host only.

Raw tensor comparison result:
  RT-003 passed the predeclared m2-tensor-v1 rule
  abs(onnx - static) <= 1e-4 + 1e-4 * abs(static) with ZERO violations across
  7,057,864 elements, over two fresh processes with byte-identical aggregates
  (84c4efdce28c418e0c6a216fba4a6846dc497fd48f4ed343a93a3eae80ea9ddb).
  Worst probe consumed 0.3962476 of its bound. Nearly every element still
  differs in f32 bit pattern: this is closeness, not identity.

End-to-end fixture result:
  NOT RUN. No pipeline exists yet. This is post-decision gate G1.

Latency/memory/binary result:
  NOT MEASURED against the QUALITY_PROFILE.md budgets. Isolated observations
  exist (for example a 20.51 s / 688,680 KiB twenty-three-shape run), but an
  isolated observation is not a budget pass. This is post-decision gate G3.

License/provenance/notice result:
  ort and ort-sys are MIT OR Apache-2.0. ONNX Runtime is MIT with recorded
  LICENSE and third-party notices. OSV reported no advisory for the recorded
  12-package spike lock. The source build was NOT hermetic:
  FETCHCONTENT_FULLY_DISCONNECTED was OFF and one FetchContent dependency was
  not hash-enforced at fetch time. The source tag carries a signature whose
  public key is not available on this machine, so provenance is unverified.
  No SBOM exists. This is post-decision gate G2.

Scorecard total and evidence links:
  206 / 500 for ort 2.0.0-rc.13, with every unproven dimension scored 0 per the
  rubric. See RUNTIME_SCORECARD.md, RUNTIME_PROOF_PLAN.md,
  RUNTIME_ORT_EVIDENCE.md, RUNTIME_ORT_SOURCE_EVIDENCE.md.

Rejected alternatives and reasons:
  tract-onnx 0.23.4 — rejected on a blocker: ordinary dynamic-shape typing
    fails before inference for the exact artifacts. A metadata-altered
    diagnostic run is explicitly non-qualifying. See RUNTIME_TRACT_EVIDENCE.md.
  Paddle Inference from Rust — not pursued. It served as the independent
    RT-003 oracle only, and its terms supplement permits exactly that confined
    external use. Adopting it would add a much larger native surface for no
    measured benefit.
  Static Paddle program as the shipped representation — rejected for M2. It has
    never been loaded from Rust, no reviewed conversion route exists, and its
    terms record does not cover distribution.
  Bundling the native library — rejected for M2. Dynamic loading from an
    explicit path keeps distribution and licensing decisions open.

Migration/extension strategy:
  The public API stays backend-neutral. RT-005 introduces an internal adapter
  trait plus a validation layer that owns model identity, tensor names, dtypes,
  ranks, shapes, allocation bounds, thread policy, and error mapping. The ort
  implementation lives behind an off-by-default Cargo feature, so the default
  build has no native dependency and no ort in its dependency graph. Replacing
  the backend means writing a second implementation of that trait; no public
  type, error, or schema changes.
```

## The three post-decision gates

These are the blockers this decision does **not** satisfy. They are conditions
on `RT-005` and later items, not on this ADR.

| Gate | What must happen | Blocks |
|---|---|---|
| **G1 — end-to-end semantics** | The approved offline M2 goldens must pass through the frozen preprocessing, DB, crop, CTC, and result contracts. | `MODAPI-001`, `E2E-001`, and any public support claim. |
| **G2 — supply chain** | A hermetic rebuild with `FETCHCONTENT_FULLY_DISCONNECTED=ON`, hash-pinned dependencies, a generated SBOM, and a resolved provenance position for the unverifiable source-tag signature. | Any distribution, packaging, or release claim. |
| **G3 — adapter-boundary resources and budgets** | Bounded allocations, threads, batches, and inputs measured at the adapter boundary, plus the `QUALITY_PROFILE.md` latency, memory, and binary budgets. | `OCR-003`, `DOC-USER-001`, and the release gate. |

## Reversal condition

This decision is reversed, and the backend re-opened, if any of the following
is observed:

1. A G1 golden fails in a way traceable to backend numerics rather than to this
   project's own preprocessing or postprocessing.
2. G2 concludes that the native library cannot be distributed or reproduced
   under acceptable terms.
3. A physical no-AVX baseline host produces output that violates
   `m2-tensor-v1` against the static reference, or fails to execute a declared
   shape.
4. The `ort` release candidate line is abandoned upstream before a stable
   release, with no maintained successor at the pinned API level.

Because the backend sits behind the `RT-005` trait and an off-by-default
feature, reversal costs one new trait implementation and no public API change.
That cheapness is the reason this decision is acceptable this early.

## What this ADR explicitly does not do

It does not approve bundling, downloading, or redistributing the native
library or any model artifact; those remain `MODEL-DEC-001` and P13 decisions.
It does not claim PaddleOCR compatibility, detector or recognizer support, or
any measured performance. It does not close `RT-002`: that item retains its
open gates, now named G1 through G3 and owned by the items listed above.
