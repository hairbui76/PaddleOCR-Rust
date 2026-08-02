# ONNX Runtime 1.28.0 Source-Build External Evidence

Roadmap item: RT-002
Status: partial source-built CPU-route evidence; no backend or artifact accepted
Recorded: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Decision boundary

This document records a temporary build and diagnostics outside this repository.
It does not add an ort dependency, ONNX Runtime binary, model artifact, adapter,
feature flag, API, CLI behavior, or CI requirement to PaddleOCR-Rust. It does
not resolve D-006 or D-007.

The temporary Rust spike used ort 2.0.0-rc.13 with std, load-dynamic, and
api-28. It explicitly loaded the library recorded below and has no direct
libonnxruntime dynamic-link dependency. The public project workspace remains
dependency-free. Python was used only as an external build-driver environment;
under the user-authorized evidence-tooling exception it did not execute
PaddleOCR or model inference. The host spike, QEMU guest probe, and separate C
API error probe did not execute Python, PaddleOCR, or the upstream checkout.
Network isolation was not measured for the host spike or QEMU guest.

## Source, build, and artifact identity

| Field | Evidence |
|---|---|
| Source remote | https://github.com/microsoft/onnxruntime.git |
| Source ref | v1.28.0, lightweight tag at da9b5e364c465de65c49d91e696cd6485270757f |
| Source tree | 8c7c6c2207a6ec453f36386db33103bb57ed2b0b; clean after the build |
| Source acquisition caveat | The checkout is shallow. The commit carries a signature, but its public key was not present locally, so this is not a verified provenance chain. |
| Source license/notices | LICENSE SHA-256 2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c; ThirdPartyNotices.txt SHA-256 0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2. |
| Dependency declaration | cmake/deps.txt SHA-256 894bd3f342b9fa570301f1a62830751e50657ec98a22b210d176b7dbb354ccc2. |
| Build tools | GCC 13.3.0, CMake 3.31.10, Ninja 1.13.0, Linux x86_64. |
| Native library | libonnxruntime.so.1.28.0; 31,428,768 bytes; SHA-256 1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa; ELF build ID 42d00fc86ccd1d0658ab2ca5e61e14b69b7bbea6. |
| C API identity | OrtGetApiBase reports 1.28.0; exported symbol version is VERS_1.28.0. |
| Dynamic ABI | SONAME libonnxruntime.so.1; RUNPATH $ORIGIN; direct NEEDED entries are libstdc++, libm, libgcc_s, libc, and the ELF loader. |
| Host ABI caveat | The built library requires up to GLIBC_2.38 and GLIBCXX_3.4.31. It is not a portable distribution artifact. |
| Detector ONNX | 62,032,837 bytes; SHA-256 eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1. |
| Recognizer ONNX | 76,554,979 bytes; SHA-256 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba. |

The configure cache recorded a shared, non-minimal, CPU-only library:

- onnxruntime_BUILD_SHARED_LIB=ON and onnxruntime_BUILD_FOR_NATIVE_MACHINE=OFF;
- onnxruntime_USE_CUDA, onnxruntime_USE_DNNL, onnxruntime_USE_OPENVINO,
  onnxruntime_USE_TENSORRT, onnxruntime_ENABLE_PYTHON, and all training options
  were OFF;
- generic C and C++ flags were -mno-avx -mno-avx2 -mno-avx512f;
- onnxruntime_USE_AVX, onnxruntime_USE_AVX2, and onnxruntime_USE_AVX512 were
  OFF.

The build deliberately targeted only onnxruntime after configuration, not the
upstream all target or its test suite. This reduced diagnostic work; it is not
an upstream test pass. The x86 MLAS build still contains separately dispatched
SSE, AVX, AVX2, AVX-512, and AMX kernels. The compile flags therefore do not
prove no-AVX execution on their own.

## Host exact-artifact probe

The existing external release spike verified both ONNX SHA-256 values, called
init_from with this exact dynamic-library path before session construction,
selected the built-in CPU execution provider, set intra-op/inter-op thread
counts to one, and set OMP_NUM_THREADS=1. It used the observed named float32
NCHW input x and output fetch_name_0 without rewriting graph metadata, nodes,
or weights.

All six fixed shapes ran once with finite output. The spike's signature is
FNV-1a 64 over returned f32 bit patterns in order; it is a repeat indicator,
not a raw-tensor equivalence oracle. These compact values are output-derived
diagnostic metadata, not raw or reconstructable model output or a reusable
fixture.

| Probe | Input shape | Output shape | Elements | Signature | Load / run (ms) |
|---|---|---|---:|---|---:|
| Detector minimum | [1, 3, 32, 32] | [1, 1, 32, 32] | 1,024 | 7ac3a00073a27b25 | 268 / 8 |
| Detector typical | [1, 3, 960, 544] | [1, 1, 960, 544] | 522,240 | 9f4dfb34d8c68085 | 153 / 1,948 |
| Detector maximum | [1, 3, 960, 960] | [1, 1, 960, 960] | 921,600 | b2a979b7477f61a5 | 156 / 3,750 |
| Recognizer minimum | [1, 3, 48, 160] | [1, 20, 18,710] | 374,200 | 33f2adb028b73e76 | 258 / 109 |
| Recognizer typical | [1, 3, 48, 320] | [1, 40, 18,710] | 748,400 | 7e55f5a0e013a6d1 | 256 / 191 |
| Recognizer maximum | [6, 3, 48, 320] | [6, 40, 18,710] | 4,490,400 | bd51d02fed358475 | 254 / 1,166 |

The first all-shape run took 8.63 seconds and had a 444,476 KiB peak resident
set. A second complete run produced the same six signatures. The host was an
AVX-capable Intel Xeon E5-2696 v3, so these figures are neither a portability
proof nor a benchmark or quality-budget pass.

An explicit nonexistent library path failed with the normal message
"failed to load from /nonexistent/libonnxruntime.so: dlopen failed" and exit
status one. No fallback or download occurred. This is only one loader-error
path, not an adapter resource/error review.

## Bounded C API error probe

A separate external C API harness dynamically loaded the same source-built
library, verified version 1.28.0, and exercised bounded intentional failures.
It used one CPU thread, a 45-second watchdog with a five-second kill grace,
and a 2 GiB process virtual-memory limit. The harness completed in 0.29
seconds with a 184,356 KiB maximum resident set and exit status zero; that
exit status means that every expected failure was observed, not that inference
or an adapter passed.

| Intentional failure | C API result | Observed behavior |
|---|---:|---|
| Missing model path | ORT_NO_SUCHFILE (3) | Structured `File doesn't exist` error. |
| Empty ONNX file | ORT_FAIL (1) | Parse/model error: `ModelProto does not have a graph`. |
| Invalid nine-byte ONNX file | ORT_INVALID_PROTOBUF (7) | `Protobuf parsing failed.` |
| Detector input name other than `x` | ORT_INVALID_ARGUMENT (2) | `Invalid input name`. |
| Detector rank-three input `[1, 3, 32]` | ORT_INVALID_ARGUMENT (2) | `Invalid rank ... Got: 3 Expected: 4`. |

The detector session used the observed `x` input and `fetch_name_0` output;
the invalid-input cases did not retain model output. Under
`strace -e trace=file,network`, the harness made no network system call. The
file trace showed the three requested failure paths and the explicit detector
path, with no observed model download or fallback-file access in this probe.
This is not a general proof for external-data ONNX graphs, other providers, or
other library configurations.

This C API exercise is not a Rust adapter review. It does not establish public
error mapping, path/message sanitization, ownership/lifetime behavior, panic
containment, cancellation, request-level memory limits, malicious or oversized
models, concurrency, or valid-inference output semantics. In particular, the
raw native messages can disclose source/build paths and must not be exposed
unchanged through a future public API or CLI.

## No-AVX execution evidence

The source-built artifact was copied into a temporary QEMU initramfs together
with the exact detector file. Streaming extraction from that initramfs verified
the same library SHA-256 above and the detector SHA-256 above. No raw tensor
output was retained in this repository.

| Field | Evidence |
|---|---|
| Emulator | QEMU 9.0.2 with TCG, not KVM |
| Guest kernel | Alpine vmlinuz-virt 6.12.94; SHA-256 12eb24189f3eb30bd0dcd919248caaa054ed4e87b799a53fdcc3999f157933e4 |
| Detector initramfs | SHA-256 ad4cddf68276a0bb278b5b9467f1004d842af00285937b644f9246b5890f9cef |
| CPU models | Nehalem and qemu64, each with one emulated CPU and 1 GiB guest memory |
| Nehalem CPUID | CPUID.1:ECX=0x80982201, AVX=0; CPUID.7.0:EBX=0x00000000, AVX2=0, AVX512F=0. |
| qemu64 CPUID | CPUID.1:ECX=0x80002001, AVX=0; CPUID.7.0:EBX=0x00000000, AVX2=0, AVX512F=0. |
| Guard against CPUID-only spoofing | A separate -mavx binary containing vzeroupper trapped with invalid opcode on both CPU models. The source-built library contains AVX instructions in dispatched paths, so successful runs exercised a real non-AVX route. |

Under both CPU models, a direct C API probe completed this sequence:

    DLOPEN_API_OK runtime_version=1.28.0 api_version=28
    SESSION_OK model=/opt/detector.onnx
    DETECTOR_RUN_OK shape=1,1,32,32 count=1024

The Nehalem replay additionally reported a finite aggregate output summary:
sum 0.00458839536, minimum 0, maximum 0.000356405973, and its harness-local
FNV-1a value 4c81e0806665413e. That value uses a different diagnostic harness
than the Rust spike and is not a cross-harness numerical comparison.

This is meaningful evidence that the current source-built binary can load,
create an exact-model session, and execute the detector minimum shape without
AVX, AVX2, AVX-512, GPU, Python, or the upstream checkout at run time. It
does not establish network isolation, all M2 shapes, recognizer execution, a
physical no-AVX machine result, broad platform compatibility, or a
distributable baseline binary.

## External commands actually run

All commands in this section ran outside the project repository. The initial
ONNX Runtime build-driver command configured the tree and began its default
full build, which was intentionally stopped after it began compiling upstream
tests. The completed diagnostic artifact was instead produced by the explicit
onnxruntime target command below.

    export PATH="/tmp/paddleocr-rust-ort-source.89EQ5V/build-venv/bin:/usr/bin:/bin:$PATH"
    export CFLAGS="-mno-avx -mno-avx2 -mno-avx512f"
    export CXXFLAGS="-mno-avx -mno-avx2 -mno-avx512f"
    cd /tmp/paddleocr-rust-ort-source.89EQ5V/onnxruntime
    python tools/ci_build/build.py --build_dir /tmp/paddleocr-rust-ort-source.89EQ5V/build --config Release --update --build --parallel 4 --build_shared_lib --skip_submodule_sync --skip_pip_install --skip_tests --skip_onnx_tests --cmake_generator Ninja --cmake_extra_defines onnxruntime_USE_AVX=OFF onnxruntime_USE_AVX2=OFF onnxruntime_USE_AVX512=OFF onnxruntime_BUILD_FOR_NATIVE_MACHINE=OFF onnxruntime_USE_CUDA=OFF onnxruntime_USE_TENSORRT=OFF onnxruntime_USE_OPENVINO=OFF onnxruntime_USE_DNNL=OFF onnxruntime_ENABLE_TRAINING=OFF
    ninja -C /tmp/paddleocr-rust-ort-source.89EQ5V/build/Release -j24 onnxruntime

The source-built all-shape probe used the following explicit dynamic route:

    env -u ORT_DYLIB_PATH ORT_SPIKE_DYLIB="/tmp/paddleocr-rust-ort-source.89EQ5V/build/Release/libonnxruntime.so.1.28.0" LD_LIBRARY_PATH="/tmp/paddleocr-rust-ort-source.89EQ5V/build/Release" OMP_NUM_THREADS=1 /tmp/paddleocr-rust-ort-spike-20260802/target/release/paddleocr-rust-ort-spike all

The independently replayed Nehalem detector probe used:

    timeout 240 qemu-system-x86_64 -accel tcg -cpu Nehalem -m 1024M -kernel /tmp/paddleocr-rust-noavx-qemu.61P7lF/vmlinuz-virt -initrd /tmp/paddleocr-rust-noavx-source.kxNLHl/source-initramfs.cpio.gz -append "console=ttyS0 rdinit=/init-detector" -nographic -no-reboot

The recorded QEMU command did not include `-nic none`, so it is not evidence
of network isolation. A future replay that needs such a claim must disable the
guest NIC explicitly and record the host/guest isolation conditions.

These paths are temporary developer evidence, not a supported installation or
distribution recipe.

## Supply-chain and license limits

This source build is not hermetic or approved for distribution:

- CMake FetchContent downloaded 13 CPU dependencies. Twelve archives were
  checked with declared SHA-1 values, but the boostorg/mp11 FetchContent
  declaration did not enforce its declared hash. The downloaded mp11 archive's
  post-hoc SHA-256 was
  81431bdc44c439a324e02c07ed067f8f556419fd86f2d8b486ff568df6aac899.
- FETCHCONTENT_FULLY_DISCONNECTED was OFF. This build did not use a trusted
  mirror, SHA-256 pins for every input, a build-specific SBOM, or an
  attestation.
- The source license is MIT and ThirdPartyNotices.txt exists, but neither file
  proves the exact set of components linked into this shared library. Nested
  terms, notices, patches, vulnerability posture, Rust wrapper, toolchain, and
  dynamic-loader distribution terms remain unreviewed.
- The model and dictionary licensing gates remain open. This experiment does
  not approve retention, conversion, redistribution, bundling, or
  model-derived fixture capture.

## RT-002 gate status

| Gate | Current source-build result |
|---|---|
| Exact local artifact | Partial pass: the verified detector and recognizer ONNX bytes ran through the host spike; the QEMU probe verified the exact detector byte. |
| Graph/operator/shape | Partial pass: all six declared shapes ran on the host source-built library. The no-AVX probe covered detector minimum only. |
| Tensor ABI | Partial pass: the external Rust spike checked the observed named float32 NCHW input/output and batch-six recognizer shape. No project adapter exists. |
| Numerical equivalence | Not run: no approved static Paddle oracle or m2-tensor-v1 element comparison exists. |
| End-to-end semantics | Not run. |
| CPU support | Partial pass: QEMU TCG successfully ran the exact detector minimum without AVX/AVX2/AVX-512. All shapes/recognizer and physical baseline-host coverage remain absent. |
| Resources and errors | Partial pass: one-thread settings, two host all-shape runs, first-run RSS, one missing-library error, and five bounded C API failures were observed under a process watchdog and address-space limit. No Rust adapter, request-level bound, cancellation, lifecycle, malicious-input, concurrency, or public-error review exists. |
| Supply chain/license | Incomplete and blocked for acceptance; see the preceding limits. |
| Unsafe/FFI boundary | Incomplete: the external ort/ort-sys native boundary has no project adapter review. |

## Required next evidence

1. Close LIC-001 before retaining model-derived oracle outputs or using an
   isolated model-backed oracle capture.
2. Compare raw detector/recognizer outputs against the approved static reference
   under m2-tensor-v1 and diagnose every element error above 1e-4.
3. Run the no-AVX route across every required detector and recognizer shape,
   then confirm it on an approved physical baseline host or formally define an
   acceptable emulation policy.
4. Produce a reproducible, verified source/binary provenance route with a
   build-specific SBOM, complete notices, enforced strong hashes, and a
   reviewed distribution policy.
5. Expand the bounded C API evidence to malicious/external-data and oversized
   inputs, lifecycle, cancellation, concurrency, and request-level limits;
   then review the Rust adapter's ownership and sanitized public errors.
6. Run end-to-end offline golden and RT-003 scorecard experiments before
   RT-004 considers a backend decision.
