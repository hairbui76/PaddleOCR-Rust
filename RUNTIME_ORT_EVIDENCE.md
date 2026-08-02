# ort 2.0.0-rc.13 External Runtime Evidence

Roadmap item: RT-002
Status: partial exact-artifact graph/shape pass; no backend or artifact accepted
Recorded: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Decision boundary

This evidence comes from a temporary Rust spike outside this repository. It
does not add an ort dependency, native library, model artifact, adapter,
feature flag, API, CLI behavior, or CI requirement to PaddleOCR-Rust.

The historical all-shape invocation resolved a fixed temporary ONNX Runtime
dynamic-library path compiled into the throwaway spike. The ORT_SPIKE_DYLIB
environment variable overrode that path for the missing-library diagnostic. It
did not use ort default binary downloading or copy-dylibs features, and its
executable has no direct libonnxruntime dynamic-link dependency. The hard-coded
path is a reproducibility defect in this temporary diagnostic, not an approved
runtime-distribution route or backend selection.

## Artifact and native-library identity

| Field | Evidence |
|---|---|
| Detector ONNX | 62,032,837 bytes; SHA-256 eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1. |
| Recognizer ONNX | 76,554,979 bytes; SHA-256 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba. |
| Rust wrapper | ort 2.0.0-rc.13, Rust MSRV 1.88, package license MIT OR Apache-2.0. |
| Enabled ort features | std, load-dynamic, api-28. The spike lockfile has no download-binaries or copy-dylibs feature. |
| FFI crate | ort-sys 2.0.0-rc.13 with disable-linking through load-dynamic. |
| Native runtime source | onnxruntime 1.28.0 Python wheel installed only in the temporary Python environment; pip metadata reports MIT License. |
| Native library | libonnxruntime.so.1.28.0, 24,272,944 bytes, SHA-256 aa4079d18f4ea7a5f3a94d80cd4bbe0f2740436626622d64d793803a20381083. |
| Adjacent provider library | libonnxruntime_providers_shared.so, 14,632 bytes, SHA-256 086ec1d5388f64153d9c63470d126693db9a182c8ce236d3a1119068471b8a0d. |
| Wheel license file | SHA-256 2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c. |
| C API compatibility | A no-model ctypes probe reported native version 1.28.0 and support for C API versions 1 through 28. |
| Spike build | rustc 1.94.0, release binary 751,256 bytes, SHA-256 e407aad0fcd9adaa7ef526001947c26fdd41d1a02e157fca5ad36307ff3243d5; lockfile SHA-256 8910d47728d1b4b465e7de9fafe126f981bff7eeff4401336249fa34a9427582. |

The wrapper called init_from with the exact dynamic-library path before any
session API. Its release binary linked only the normal system C runtime
libraries; ldd did not list libonnxruntime. The native library itself has
SONAME libonnxruntime.so.1 and an ORIGIN runpath, with standard glibc,
libstdc++, libgcc, libm, libpthread, librt, and libdl dependencies. This is
dynamic-loader evidence only, not a portable deployment approval.

## Exact-artifact probe

Before the spike, sha256sum verified both external ONNX files. The spike then
used SessionBuilder.commit_from_file on the exact paths, selected only the
built-in CPU execution provider, set intra- and inter-op thread counts to one,
and set OMP_NUM_THREADS=1. It supplied the named float32 NCHW input x and
retrieved the named float32 output fetch_name_0. It did not rewrite input
metadata, change nodes or weights, convert the model, or fetch a model.

All six fixed M2 shape probes loaded, ran, had one finite output, and matched
the observed rank/shape/class dimensions. signature is FNV-1a 64 over the
returned output f32 bit patterns in order. It is a repeat indicator, not a
numerical-equivalence oracle. These compact values are output-derived
diagnostic metadata, not raw or reconstructable model output or a reusable
fixture.

| Probe | Input shape | Output shape | Elements | Signature | Load / run (ms) |
|---|---|---|---:|---|---:|
| Detector minimum | [1, 3, 32, 32] | [1, 1, 32, 32] | 1,024 | 7ac3a00073a27b25 | 276 / 8 |
| Detector typical | [1, 3, 960, 544] | [1, 1, 960, 544] | 522,240 | 9f4dfb34d8c68085 | 159 / 1,871 |
| Detector maximum | [1, 3, 960, 960] | [1, 1, 960, 960] | 921,600 | b2a979b7477f61a5 | 151 / 3,728 |
| Recognizer minimum | [1, 3, 48, 160] | [1, 20, 18,710] | 374,200 | 33f2adb028b73e76 | 263 / 101 |
| Recognizer typical | [1, 3, 48, 320] | [1, 40, 18,710] | 748,400 | 7e55f5a0e013a6d1 | 236 / 192 |
| Recognizer maximum | [6, 3, 48, 320] | [6, 40, 18,710] | 4,490,400 | bd51d02fed358475 | 241 / 1,093 |

The combined first run measured a 446,572 KiB peak resident set. A second
identical all-shape run produced the same six signatures. Timings varied and
the host is not the reference baseline, so these figures are neither a
benchmark nor a quality-budget pass.

The signatures differ from the separate non-qualifying tract metadata
diagnostic. A content-hash difference alone does not identify a numerical
mismatch and cannot replace the required static-oracle element comparison.

## Dynamic-loader behavior

The spike also tested an explicitly nonexistent library path through its
ORT_SPIKE_DYLIB diagnostic override. init_from returned a normal failure
message, failed to load from the requested path, and the process exited one;
there was no native-library fallback or automatic download. This does not test
malformed models, missing model files, allocation limits, cancellation, or the
future adapter error contract.

## Gate status

| RT-002 gate | Current result |
|---|---|
| Exact local artifact | Partial pass: both verified ONNX files were loaded directly by the external spike. |
| Graph/operator/shape | Partial pass: every declared M2 zero-input shape executed through the exact ONNX files with the CPU provider. |
| Tensor ABI | Partial pass: the observed x and fetch_name_0 names, float32, NCHW shapes, and batch-six recognizer output were checked. No public adapter exists. |
| Numerical equivalence | Not run: no isolated static Paddle oracle or m2-tensor-v1 element comparison exists. |
| End-to-end semantics | Not run. |
| CPU portability | Not established. The host is an AVX-capable Xeon v3; no no-AVX/AVX2 baseline test or binary assurance exists. |
| Resources and errors | Incomplete. One-thread controls, two runs, peak RSS, and a missing-library error were observed only. |
| Supply chain/license | Incomplete. The dynamic library is a temporary Python-wheel artifact and its native/transitive notices, CVEs, distribution terms, and deployment provenance remain unreviewed. |
| Unsafe/FFI boundary | Incomplete. ort and ort-sys contain the candidate native boundary; no project adapter review exists. |

This table describes the temporary Python-wheel route only. A later source-built
route added partial no-AVX detector-minimum evidence, documented separately in
[RUNTIME_ORT_SOURCE_EVIDENCE.md](RUNTIME_ORT_SOURCE_EVIDENCE.md); it does not
retroactively qualify this wheel or clear the remaining RT-002 blockers.

## Required next evidence

- Close LIC-001 before retaining model-derived oracle outputs or using an
  isolated model-backed oracle capture.
- Compare raw outputs against the approved static reference under
  m2-tensor-v1, then diagnose any element error above 1e-4.
- Extend the partial source-built no-AVX/AVX2 evidence in
  [RUNTIME_ORT_SOURCE_EVIDENCE.md](RUNTIME_ORT_SOURCE_EVIDENCE.md) from the
  detector minimum shape to every required detector/recognizer shape, then
  confirm it on an approved physical baseline host or under a formal emulation
  policy. The temporary Python wheel cannot establish that claim.
- Review the complete native library, wrapper, transitive dependency,
  vulnerability, notice, dynamic-loader, resource-limit, and error surface.
- Run malformed-model and bounded-resource experiments, end-to-end offline
  goldens, and the RT-003 scorecard before RT-004 considers a backend decision.

## Reproduction outline

The historical normal invocation below used the spike's hard-coded temporary
fallback path, so it is retained as an execution record rather than a clean
reproduction recipe:

    cd "$SPIKE"
    sha256sum "$MODEL_ROOT/m2-onnx-det-v6-medium/inference.onnx" \
      "$MODEL_ROOT/m2-onnx-rec-v6-medium/inference.onnx"
    PATH="/usr/bin:/bin:$PATH" cargo build --release --locked
    OMP_NUM_THREADS=1 target/release/paddleocr-rust-ort-spike all
    ORT_SPIKE_DYLIB="/nonexistent/libonnxruntime.so" \
      target/release/paddleocr-rust-ort-spike detector-minimum

Any future wheel-route rerun must provide the normal library path explicitly
and record a fresh result, for example:

    ORT_SPIKE_DYLIB="$ORT_LIBRARY" LD_LIBRARY_PATH="$ORT_LIBRARY_DIR" OMP_NUM_THREADS=1 target/release/paddleocr-rust-ort-spike all

That command was not the source of the historical table above. The native
library is intentionally supplied out of band. No project code may use this
temporary path or Python environment.
