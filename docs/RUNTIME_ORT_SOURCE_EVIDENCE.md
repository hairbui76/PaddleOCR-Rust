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
PaddleOCR or model inference. The host spike, QEMU guest probe, separate C API
error probe, and separate C API lifecycle probe did not execute Python,
PaddleOCR, or the upstream checkout.
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

## Bounded C API lifecycle probe

A second external C harness tested a narrowly bounded sequential lifecycle
path. It dynamically loaded API version 28 only after verifying the SHA-256 of
the source-built library and both exact ONNX files. It disabled telemetry,
selected `CPUExecutionProvider`, set intra-op and inter-op threads to one,
selected sequential execution, disabled memory-pattern optimization, and used
zero-filled minimum-shape inputs. It scanned output only for expected
float32 shape/count and finite values; it did not retain, emit, or fingerprint
any output values.

| Input | SHA-256 |
|---|---|
| Temporary C harness source | `75ceb7504d51ce7bf2e875f8823093caa2f64508bb0902a5024e8a71c972d200` |
| Temporary C harness binary | `5d8cb761ef6541a49586c473cd831a9022fa01217746d85a07968534f1f27273` |
| Source-built library | `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa` |
| Detector ONNX | `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` |
| Recognizer ONNX | `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` |

The harness was compiled and run outside both repositories. Its explicit
commands were:

```sh
PATH="/usr/bin:/bin:$PATH" cc -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror \
  -I"/tmp/paddleocr-rust-ort-source.89EQ5V/onnxruntime/include/onnxruntime/core/session" \
  "/tmp/paddleocr-rust-ort-lifecycle.qHQUjg/lifecycle_probe.c" \
  -o "/tmp/paddleocr-rust-ort-lifecycle.qHQUjg/lifecycle_probe" -ldl -lcrypto -lm

ulimit -Sv 1600000
ulimit -St 600
ulimit -c 0
env -u ORT_DYLIB_PATH \
  LD_LIBRARY_PATH="/tmp/paddleocr-rust-ort-source.89EQ5V/build/Release" \
  OMP_NUM_THREADS=1 MALLOC_ARENA_MAX=1 \
  timeout --signal=TERM --kill-after=30s 600s \
  "/tmp/paddleocr-rust-ort-lifecycle.qHQUjg/lifecycle_probe" \
  "/tmp/paddleocr-rust-ort-source.89EQ5V/build/Release/libonnxruntime.so.1.28.0" \
  "/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-onnx-det-v6-medium/inference.onnx" \
  "/mnt/ssdvolumes/models/paddleocr-v6-medium/m2-onnx-rec-v6-medium/inference.onnx"
```

The process completed in 6.314 seconds with exit status zero under a 1,600,000
KiB virtual-memory limit and a 600-second CPU-time / wall-time watchdog. It
completed twelve sequential create-session, run, release-session cycles for
each model. Every cycle reported the expected output and finite values; no
`ORT_ERROR`, timeout, abort, or process panic was recorded.

| Task | Checked output | 12-cycle total session / run / release | Peak RSS | Post-release RSS first / last / maximum | Threads after all releases |
|---|---|---:|---:|---:|---:|
| Detector minimum | `[1, 1, 32, 32]`, 1,024 finite | 2078.612 / 105.102 / 98.718 ms | 194,760 KiB | 106,228 / 112,364 / 114,092 KiB | 1 |
| Recognizer minimum | `[1, 20, 18,710]`, 374,200 finite | 2332.733 / 1091.471 / 102.169 ms | 214,760 KiB | 122,308 / 119,084 / 122,308 KiB | 1 |

The compact per-cycle trace below records `session_ms/run_ms/release_ms;
RSS-after-release KiB`; every listed cycle also passed its output shape/count
and finite-value checks.

```text
detector:
01 236.011/8.554/7.118; 106228
02 169.572/8.632/8.359; 106220
03 160.858/8.996/7.531; 113304
04 156.427/8.700/7.387; 114092
05 174.028/9.335/8.645; 106220
06 164.448/9.660/9.604; 106220
07 166.088/8.797/8.450; 106220
08 171.074/8.846/9.045; 106220
09 171.906/8.495/8.359; 106220
10 164.382/8.283/8.067; 106220
11 177.689/8.664/8.434; 106220
12 166.127/8.141/7.717; 112364

recognizer:
01 198.941/87.950/7.372; 122308
02 207.773/91.672/8.648; 119660
03 207.847/91.766/8.622; 119660
04 183.088/88.409/8.344; 119084
05 184.159/88.904/11.292; 110316
06 197.181/91.744/7.786; 119084
07 185.307/100.497/8.181; 119660
08 202.075/92.149/8.599; 119660
09 195.319/91.709/9.151; 119532
10 196.486/89.702/8.185; 119084
11 187.506/90.053/8.674; 119532
12 187.052/86.916/7.314; 119084
```

After both task groups, `ReleaseEnv` completed and `dlclose` returned zero:

```text
ENVIRONMENT_RELEASE_OK
DLCLOSE_RESULT status=0
LIFECYCLE_PROBE_OK detector_cycles=12 recognizer_cycles=12 clean_release=true
```

A negative harness invocation supplied the detector file in the recognizer
position. It returned exit status two at its hash check before
`DLOPEN_API_OK`, confirming this probe did not quietly substitute the model
path.

This is bounded Linux-host lifecycle evidence only. A non-monotonic,
short-window post-release RSS observation does not prove absence of leaks; no
ASan, LSan, Valgrind, long soak, concurrency, cancellation, malicious-model,
or request-level resource test ran. `dlclose` status zero does not prove that
every native global/static state was unloaded. The probe did not use `strace`
or network isolation, so it makes no network-off claim. It is not a Rust
adapter/FFI review, a numerical-equivalence result, a portability proof, a
distribution result, or a backend decision.

## No-AVX execution evidence

The first source-built artifact arrangement was copied into a temporary QEMU
system-mode initramfs together with the exact detector file. Streaming
extraction from that initramfs verified the same library SHA-256 above and the
detector SHA-256 above. Later recognizer and detector shape expansions used
separate temporary QEMU arrangements described below. No raw tensor output was
retained in this repository.

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

This first system-mode result is meaningful evidence that the current
source-built binary can load, create an exact-model session, and execute the
detector minimum shape without AVX, AVX2, AVX-512, GPU, Python, or the upstream
checkout at run time. The later expansions below broaden shape coverage, but
do not establish network isolation, a physical no-AVX machine result, broad
platform compatibility, or a distributable baseline binary.

### System-mode recognizer expansion

A separate temporary system-mode QEMU harness covered every declared
recognizer shape under both CPU models. It dynamically loaded the same native
library, verified the exact recognizer SHA-256 before loading, set CPU
intra/inter-op threads to one, used a zero-filled float32 NCHW input named x,
and fetched fetch_name_0. Each probe had a 900-second host watchdog. The
recognizer initramfs initially had SHA-256
e70a75792248eddea4e74eea99fbfb9563a1f4aaa0744b07ef5764136efa0b4c; a
harness-only clean-shutdown revision had SHA-256
ab710d72fc207804b60f5e2c93438c25a2f0a7adc97a94b4cd4282e26ed0d862. Both
archives self-verified the recorded library and recognizer model bytes. The C
harness SHA-256 was aef52a141443b018976404b1ff14e868a61b224270d0419b1246beef41ba3e4d,
and its AVX guard SHA-256 was
13097bf56a39cec755edd362a749b66ae644dc399c0fa37dac2ec5d5cdb82770.

For every recognizer probe, CPUID reported AVX=0, AVX2=0, and AVX-512F=0, and
the separate vzeroupper guard trapped with SIGILL status 132. Every output had
the expected shape/count and only finite values. The timing columns are guest
monotonic-clock milliseconds from the diagnostic C harness, not benchmarks.

| CPU | Input shape | Output shape / elements | Finite aggregate | Host / QEMU word-wise fingerprint | Load / run ms |
|---|---|---|---|---|---:|
| Nehalem | [1, 3, 48, 160] | [1, 20, 18,710] / 374,200 | sum 20.000002; min 4.01885833e-17; max 1 | 33f2adb028b73e76 / c7101dea8545eb1a | 7,369.681 / 48,917.411 |
| Nehalem | [1, 3, 48, 320] | [1, 40, 18,710] / 748,400 | sum 40.0000019; min 2.18770081e-17; max 1 | 7e55f5a0e013a6d1 / a7e3c55c7d17a73f | 6,821.506 / 95,462.881 |
| Nehalem | [6, 3, 48, 320] | [6, 40, 18,710] / 4,490,400 | sum 240.000011; min 2.18770081e-17; max 1 | bd51d02fed358475 / d47d392f1bf4e449 | 6,875.742 / 573,954.363 |
| qemu64 | [1, 3, 48, 160] | [1, 20, 18,710] / 374,200 | sum 20.000002; min 4.01885833e-17; max 1 | 33f2adb028b73e76 / c7101dea8545eb1a | 3,662.302 / 44,204.165 |
| qemu64 | [1, 3, 48, 320] | [1, 40, 18,710] / 748,400 | sum 40.0000019; min 2.18770081e-17; max 1 | 7e55f5a0e013a6d1 / a7e3c55c7d17a73f | 3,464.053 / 88,455.540 |
| qemu64 | [6, 3, 48, 320] | [6, 40, 18,710] / 4,490,400 | sum 240.000011; min 2.18770081e-17; max 1 | bd51d02fed358475 / d47d392f1bf4e449 | 3,637.511 / 524,624.022 |

No recognizer C probe timed out or reported a hash, CPU-feature, C API, output
shape/type, or non-finite-output error. The clean-shutdown initramfs was used
for the Nehalem minimum replay, which recorded probe exit status zero and a
clean poweroff. The initial wrapper used exec for the probe, so the other
successful runs emitted a PID 1 panic after their success records; they were
stopped only after the C probe had recorded exit status zero. That lifecycle
defect makes the other guest exits unsuitable as clean-shutdown evidence.

A controlled host calibration used the same source-built library, recognizer,
zero inputs, provider, and one-thread controls. The C harness and the Rust
spike matched the listed host signatures for minimum twice, typical once, and
maximum once. Both use the word-wise fold
h = (h XOR f32_bits) * 0x100000001b3 with seed 0xcbf29ce484222325; C copies an
IEEE-754 f32 to uint32_t and Rust uses f32::to_bits, so neither serializes bytes.
All outputs were finite, so NaN handling was not separately calibrated.

Each QEMU signature differs from its calibrated host signature. A differing
word-wise fingerprint proves at least one returned f32 bit pattern differs, but does not
identify the elements, error magnitude, cause, or tolerance impact. It is not
a static-Paddle comparison and must not be described as numerical equivalence.

### User-mode detector expansion

A separate QEMU user-mode TCG harness covered the detector typical and maximum
shapes under Nehalem and qemu64. It used qemu-user-static 9.0.2 extracted only
under /tmp; the package SHA-256 was
807f801829277a9d010a49ad84856198263dfe572eef5d57a0896edd811044be. Each
probe verified the exact detector and library hashes, dynamically loaded C API
version 28, selected the CPU provider with intra/inter-op threads set to one,
used an in-memory zero input, and had a 900-second watchdog. All four
supervisor child exit codes were zero; none timed out or received a guest
signal. QEMU user mode has no guest OS boot/shutdown result.

| CPU | Input / output shape | Finite aggregate | Session / run seconds |
|---|---|---|---:|
| Nehalem | [1, 3, 960, 544] / [1, 1, 960, 544] | 522,240 finite; min 0; max 0.0468307137; sum 2527.7130876481533 | 1.921530 / 472.603767 |
| qemu64 | [1, 3, 960, 544] / [1, 1, 960, 544] | 522,240 finite; min 0; max 0.0468307137; sum 2527.7130876481533 | 2.009043 / 508.342897 |
| Nehalem | [1, 3, 960, 960] / [1, 1, 960, 960] | 921,600 finite; min 0; max 0.0505188107; sum 9692.1489780545235 | 2.100542 / 824.577587 |
| qemu64 | [1, 3, 960, 960] / [1, 1, 960, 960] | 921,600 finite; min 0; max 0.0505188107; sum 9692.1489780545235 | 2.002515 / 795.108762 |

The user-mode detector C harness was cross-calibrated with the Rust host spike
at detector minimum: both produced 7ac3a00073a27b25 for the same source-built
library and zero input. The corresponding no-AVX QEMU minimum was
fa452214e1e92725, proving a host/QEMU bit-pattern difference at that shape.
The typical and maximum probes began before FNV instrumentation, so their raw
bit patterns were not retained; their aggregate maximum and sum nevertheless
differed from the host results (typical host maximum 0.0468301773 and sum
2527.6838610470295; maximum host maximum 0.0505194068 and sum
9692.1889959275723). Those aggregates are not an elementwise/tolerance
diagnosis.

Taken together, the system-mode detector minimum, system-mode recognizer
coverage, and user-mode detector coverage show that every declared M2 detector
and recognizer shape can create an exact-model CPU session and produce a
finite, correctly shaped output on at least one no-AVX/AVX2/AVX-512 QEMU TCG
route. They do not establish bitwise determinism, numerical equivalence,
physical baseline-host support, platform support, or a distribution baseline.

### Elementwise host/QEMU diagnosis for two minimum shapes (2026-08-03)

A further disposable C harness compared each QEMU result directly with a raw
host result from the same source-built library. This is a same-runtime
host-versus-emulator diagnostic only, not a Paddle/static-reference oracle,
model-conversion comparison, or a declared tensor tolerance. The harness and
every raw output remained outside this repository; it did not execute Python,
PaddleOCR, or the linked upstream checkout.

The C source SHA-256 was
`ac7404a142c0bf03daff558e7f0991df13f76ccfc562f43f67fc4fa709ba9a14` and
the x86-64 executable SHA-256 was
`5e61523f3ab434eba30997920a242457e6a1e5bd2773b79f4be464fc62bb9f7e`.
GCC 13.3.0 built it with `-O2 -Wall -Wextra -Werror -march=x86-64
-mno-avx -mno-avx2 -mno-fma`. It dynamically loaded the library and reused
the observed `x` / `fetch_name_0` float32 contract, one intra-op thread, one
inter-op thread, and telemetry disabled. The comparison route used the
external QEMU user-mode executable SHA-256
`f50bed962ccaa52c476e14e50b02006b7dffa414566a59b13b240bdac4e4f324`
with `qemu-x86_64-static -cpu qemu64 -L /`; prior QEMU evidence records this
CPU model's AVX, AVX2, and AVX-512F flags as zero. User-mode QEMU is not a
full guest, a physical baseline host, or a network-isolation result.

For each row, the host first wrote a temporary private binary dump, a second
host process compared itself to that dump, and QEMU compared its output to the
same dump. `zero` contains only zero float32 input values. `lcg-v1` is a
self-authored bounded diagnostic input: it starts from `0x6d2b79f5`, updates
with `state = state * 1664525 + 1013904223`, and maps the high sixteen bits to
`[-1, 1)` as float32. It is neither a decoded image nor selected model
preprocessing. All host repeats were bit-exact; two independent QEMU processes
were also bit-exact for both zero-input rows.

| Input | Component / shape | Elements | Different f32 bit patterns | Largest absolute difference | Difference counts | Bounded behavioral check |
|---|---|---:|---:|---:|---|---|
| `zero` | Detector `[1, 3, 32, 32]` → `[1, 1, 32, 32]` | 1,024 | 706 | `1.78813934e-7` | 57 `> 1e-7`; 0 `> 1e-6` | 0 changes across the DB-style `> 0.3` check |
| `zero` | Recognizer `[1, 3, 48, 160]` → `[1, 20, 18,710]` | 374,200 | 366,726 | `3.57627869e-7` | 5 `> 1e-9`; 0 `> 1e-6` | 0 changes among the 20 strict-last-axis argmaxes |
| `lcg-v1` | Detector `[1, 3, 32, 32]` → `[1, 1, 32, 32]` | 1,024 | 672 | `1.78813934e-7` | 46 `> 1e-7`; 0 `> 1e-6` | 0 changes across the DB-style `> 0.3` check |
| `lcg-v1` | Recognizer `[1, 3, 48, 160]` → `[1, 20, 18,710]` | 374,200 | 374,099 | `1.85370445e-5` | 16,118 `> 1e-9`; 2,177 `> 1e-8`; 125 `> 1e-7`; 15 `> 1e-6`; 5 `> 1e-5`; 0 `> 1e-4` | 0 changes among the 20 strict-last-axis argmaxes |

The comparison reports a detector threshold crossing only for the fixed
private `> 0.3` precursor, and recognizer argmaxes only per final tensor axis;
it does not perform DB contour extraction, CTC collapse, dictionary mapping,
text decoding, score filtering, or any public postprocessing. In particular,
the lack of an argmax difference on these two synthetic minimum-shape inputs
does not establish recognition-text stability.

The temporary zero-output dumps had SHA-256 values
`0b26ddaa8f30fcbb89cf12f399858da50f75502adc771f0af63cc89bf4399d81`
(detector host) and
`afe2d545b34f2f82f9bf32d80b2b4cfdf799edde1b942a82fab90f0819b96194`
(recognizer host); the corresponding LCG dumps had SHA-256 values
`95a56911a685f5760dc0656dd2e449f586e6f3a773911350e36d270bb4fc16e7`
and `c31c3c365663dbe70c29f07571d2c6519ac19b733dec3f53fd71942b0fa96df0`.
These are external diagnostic artifacts, not committed model-derived fixtures.

This narrows the earlier compact-fingerprint observation only for two minimum
shapes and two self-authored inputs. It does not cover typical/maximum shapes,
actual preprocessing, images, postprocessing, thread counts, physical CPUs,
other QEMU modes, a static Paddle reference, another backend, model conversion,
or a user-facing tolerance. It therefore neither completes RT-002 nor relaxes
any P3/P4/P5 gate.

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
| Exact local artifact | Partial pass: the verified detector and recognizer ONNX bytes ran through the host spike. The QEMU arrangements independently verified their respective exact model/library hashes before session creation. |
| Graph/operator/shape | Partial pass: all six declared shapes ran on the host source-built library and on at least one no-AVX QEMU TCG route. This is not arbitrary-shape or physical-host coverage. |
| Tensor ABI | Partial pass: the external Rust spike and QEMU C probes checked observed x/fetch_name_0 float32 NCHW shapes, including batch-six recognition. No project adapter exists. |
| Numerical equivalence | Not passed: no approved static Paddle oracle or m2-tensor-v1 element comparison exists. Calibrated C/Rust probes observed different host versus no-AVX QEMU bit-pattern signatures for detector minimum and every recognizer shape; detector typical/maximum aggregates also differ. A later same-runtime elementwise diagnosis at only two minimum shapes found no difference above `1e-4`, no fixed detector `> 0.3` crossing, and no final-axis recognizer argmax change for zero and one synthetic input, but it establishes no general tolerance, preprocessing, postprocessing, or static-reference equivalence. |
| End-to-end semantics | Not run. |
| CPU support | Partial pass: every declared detector/recognizer shape produced a finite, correctly shaped output under no-AVX QEMU TCG routes with one-thread controls. System- and user-mode emulation, numerical mismatches, no physical baseline host, no platform matrix, and no distribution binary remain material limits. |
| Resources and errors | Partial pass: one-thread settings, two host all-shape runs, first-run RSS, QEMU watchdog observations, one missing-library error, and five bounded C API failures were observed. A separate 1,600,000 KiB / 600-second lifecycle probe completed twelve sequential create/run/release cycles for each minimum-shape exact model, with finite outputs, one observed post-release thread, short-window bounded RSS, `ReleaseEnv`, and `dlclose` status zero. No Rust adapter, request-level bound, cancellation, long-soak/leak analysis, malicious-input, concurrency, or public-error review exists. |
| Supply chain/license | Incomplete and blocked for acceptance; see the preceding limits. |
| Unsafe/FFI boundary | Incomplete: the external ort/ort-sys native boundary has no project adapter review. |

## Required next evidence

1. Close LIC-001 before retaining model-derived oracle outputs or using an
   isolated model-backed oracle capture.
2. After LIC-001 permits an isolated model-backed capture, compare raw
   detector/recognizer outputs against the approved static reference under
   m2-tensor-v1, characterize host/QEMU differences across the declared
   representative tensors, and diagnose every element error above `1e-4`.
   The two-minimum-shape zero/LCG diagnostic above does not satisfy this
   static-reference or representative-input requirement.
3. Confirm the now-complete emulated no-AVX shape coverage on an approved
   physical baseline host or formally define an acceptable emulation policy.
4. Produce a reproducible, verified source/binary provenance route with a
   build-specific SBOM, complete notices, enforced strong hashes, and a
   reviewed distribution policy.
5. Expand the bounded C API evidence to malicious/external-data and oversized
   inputs, long-soak/leak analysis, cancellation, concurrency, and request-level
   limits; then review the Rust adapter's ownership and sanitized public errors.
6. Run end-to-end offline golden and RT-003 scorecard experiments before
   RT-004 considers a backend decision.
