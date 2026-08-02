# tract-onnx 0.23.4 External Runtime Evidence

Roadmap item: RT-002
Status: candidate configuration rejected; RT-002 remains in progress
Recorded: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Decision boundary

This is evidence from a temporary spike outside this repository. It does not add
a Cargo dependency, feature flag, adapter, model resolver, model artifact,
fixture, API, CLI behavior, or CI requirement to PaddleOCR-Rust.

The user authorized local external acquisition of the two ONNX packages. That
authorization permits this bounded local diagnostic only; it does not approve
the models for project adoption, redistribution, conversion, bundling, or
retention of model-derived fixture data. The license and provenance gates in
LICENSE_REVIEW.md remain open.

tract-onnx 0.23.4 is rejected for the current exact-artifact configuration:
the ordinary loader cannot type either model's declared dynamic dimensions.
The successful metadata-specialization runs below are diagnostic data, not an
alternative accepted configuration or a graph/operator-completeness pass.

## Inputs and environment

| Field | Evidence |
|---|---|
| Candidate artifacts | The external local ONNX packages documented in LOCAL_ONNX_CANDIDATE_INSPECTION.md. |
| Detector inference.onnx | 62,032,837 bytes; SHA-256 eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1. |
| Recognizer inference.onnx | 76,554,979 bytes; SHA-256 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba. |
| Spike location | Temporary directory outside both repositories; no file from the spike is tracked here. |
| Direct Rust dependencies | tract-onnx = 0.23.4, anyhow = 1.0.104; spike Cargo.lock SHA-256 d70686aaa8dedefb9811da1d67c4e3851ab03bebcaeb48b05bdf3b6e21b35134. |
| Toolchain | rustc 1.94.0 (4a4ef493e 2026-03-02), host x86_64-unknown-linux-gnu, LLVM 21.1.8. |
| Test host | Linux 7.0.0-28-generic, x86-64, Intel Xeon E5-2696 v3. This is not the required no-AVX/AVX2 baseline host. |
| Build and thread policy | Release build; RAYON_NUM_THREADS=1 and OMP_NUM_THREADS=1 for every run. |
| Input | One zero-filled float32 NCHW tensor for each shape in RUNTIME_PROOF_PLAN.md. |

Immediately before the spike, sha256sum verified both ONNX files against the
recorded values. The spike read the external files only. It did not execute
Python/PaddleOCR, download a model, modify an ONNX package, or write an output
or fixture into this repository.

## Exact-artifact failure

The direct tract_onnx::onnx().model_for_path(...).into_optimized() path failed
before inference for the minimum shape of both exact on-disk files:

| Component | Failure point | Failure summary |
|---|---|---|
| Detector | node #224, Conv.0, ConvHir | tract could not unify 1 + DynamicDimension.1 / 2 with (DynamicDimension.1 + 1) / 2. |
| Recognizer | node #178, Conv.0, ConvHir | The corresponding width expression could not be unified. |

The detector also failed at node #491, Concat.2, after a read-only parser mode
ignored optional output-shape and value_info annotations while retaining the
declared named dynamic input dimensions. The failure was another symbolic
ceil/floor shape unification error. Therefore it is not solely due to optional
output metadata.

These are model typing/optimization failures, not unsupported-op execution
results. Under RUNTIME_PROOF_PLAN.md, this rejects this candidate configuration
unless a future roadmap change first defines and justifies a legal alternative.
No such change has been made.

## Non-qualifying metadata diagnostic

For diagnosis only, the spike loaded the exact bytes into memory, ignored the
optional output/value-info shape annotations, changed each input declaration
from a named DimParam to ONNX's anonymous "?" marker, and supplied the fixed
probe input fact. It did not alter a node, initializer, weight, tensor layout,
dtype, output order, or on-disk byte.

This is nevertheless an in-memory metadata alteration. It is explicitly not
an exact-artifact execution under the RT-002 blocker gate and cannot be used
to select tract or claim support. It merely shows that the original node and
initializer payloads can execute after that non-approved specialization.

The following release-mode runs completed with finite output. signature is
FNV-1a 64 over every output f32 IEEE-754 bit pattern in returned order; it is a
diagnostic repeat indicator, not a raw-oracle comparison. These compact values
are output-derived diagnostic metadata, not raw or reconstructable model output
or a reusable fixture.

| Probe | Input shape | Output shape | Elements | Signature | Load / optimize / run (ms) |
|---|---|---|---:|---|---:|
| Detector minimum | [1, 3, 32, 32] | [1, 1, 32, 32] | 1,024 | 93122e4acbbb0f25 | 127 / 504 / 16 |
| Detector typical | [1, 3, 960, 544] | [1, 1, 960, 544] | 522,240 | af41533640fb3215 | 37 / 709 / 6,012 |
| Detector maximum | [1, 3, 960, 960] | [1, 1, 960, 960] | 921,600 | 55d178364c237d4d | 47 / 853 / 11,230 |
| Recognizer minimum | [1, 3, 48, 160] | [1, 20, 18,710] | 374,200 | a3921a96c44f0175 | 45 / 304 / 207 |
| Recognizer typical | [1, 3, 48, 320] | [1, 40, 18,710] | 748,400 | 6e9dfb96e0e13785 | 45 / 306 / 403 |
| Recognizer maximum | [6, 3, 48, 320] | [6, 40, 18,710] | 4,490,400 | 120c7e96a98f3b25 | 48 / 273 / 3,751 |

The combined first run had a 507,520 KiB peak resident-set measurement. A
second identical all-shape run produced the same six signatures. Its timing
varied, so neither run is a published performance result or a M2 budget pass.
The diagnostic release executable was 32,234,032 bytes with SHA-256
d6331da1bc44a1f16671f01fba7d5555393d372b70d4c9902a7fe36a09abd8dc.

## Unfinished gates

- An isolated static-oracle raw tensor comparison under m2-tensor-v1 was not
  run. ONNX Runtime was not used by this tract spike to execute either model.
  Separate ORT evidence is recorded in RUNTIME_ORT_EVIDENCE.md and
  RUNTIME_ORT_SOURCE_EVIDENCE.md; it does not qualify the rejected tract
  configuration. ORACLE_CAPTURE.md still requires LIC-001 closure before a
  model-backed capture.
- The tract host cannot establish the required no-AVX/AVX2 CPU baseline. No
  baseline-compatible tract test host, tract emulation result, or tract
  deployment claim exists. The separate source-built ORT QEMU result does not
  qualify tract.
- No malformed-model, allocation/resource-bound, thread-control audit, supply
  chain review, end-to-end golden, or public Rust adapter proof exists.
- No comparison with the separate static Paddle representation has been made.
- RT-003, RT-004, RT-005, MODEL-DEC-001, and LIC-001 remain open.

## Reproduction outline

The temporary spike used a release build and the following modes, where
$SPIKE and $MODEL_ROOT are user-local external paths:

    cd "$SPIKE"
    sha256sum "$MODEL_ROOT/m2-onnx-det-v6-medium/inference.onnx" \
      "$MODEL_ROOT/m2-onnx-rec-v6-medium/inference.onnx"
    PATH="/usr/bin:/bin:$PATH" cargo build --release --locked
    RAYON_NUM_THREADS=1 OMP_NUM_THREADS=1 \
      target/release/paddleocr-rust-tract-spike --direct detector-minimum
    RAYON_NUM_THREADS=1 OMP_NUM_THREADS=1 \
      target/release/paddleocr-rust-tract-spike --direct recognizer-minimum
    RAYON_NUM_THREADS=1 OMP_NUM_THREADS=1 \
      target/release/paddleocr-rust-tract-spike --metadata-diagnostic all

The metadata-diagnostic command is intentionally not an acceptance test.
