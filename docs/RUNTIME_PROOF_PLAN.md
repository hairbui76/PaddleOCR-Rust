# Runtime Candidate Proof Plan

Roadmap item: RT-002  
Status: In progress — `tract-onnx` 0.23.4 was rejected for the exact-artifact
configuration; external `ort` 2.0.0-rc.13 dynamic-load proofs passed the six
fixed exact-artifact shape probes through both a temporary wheel and a
source-built ONNX Runtime route. A later temporary Python-to-Rust same-runtime
raw relay is bit-identical for all six LCG shapes, but remains unqualified; no
runtime, format, or public implementation is selected
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854  
Applies to: the user-provisioned, hash-verified v6-medium ONNX detector and
recognizer candidates only

## Purpose and boundary

This plan starts the bounded candidate proof required by
RUNTIME_RUBRIC.md. It does not add a Cargo dependency, feature flag, runtime
adapter, model path, cache, downloader, public API, or CI requirement to
PaddleOCR-Rust.

Each spike must live outside this repository and the read-only PaddleOCR
checkout. It may read only the externally provisioned regular files recorded
in LOCAL_ONNX_CANDIDATE_INSPECTION.md after independently verifying their
expected byte counts and SHA-256 values. It must never fetch a model, modify
the candidate package, execute PaddleOCR, or write a model-derived fixture into
this repository. Under the narrow user-authorized exception in the root `ROADMAP.md`, an
external native-runtime build or ABI check may use Python as a build/inspection
driver only; candidate model loading and inference in the runtime spike must
not execute Python.

The only outcome of a proof is evidence: pass, rejection, or an explicit
unresolved result. A passing smoke test does not select a backend.

## First proof: tract-onnx 0.23.4

The first external spike uses tract-onnx 0.23.4 because it is a current
pure-Rust ONNX candidate with Rust 1.91 compatibility, within the project
Rust 1.94 policy. The proof will record:

1. crate version, lockfile, host, compiler, and exact model hashes;
2. model parse/load result for both ONNX graphs;
3. model typing/optimization result for each fixed test shape;
4. one deterministic zero-filled FLOAT NCHW run for each shape that optimizes;
5. output rank, shape, element type, finite-value check, and a content hash;
6. elapsed load/optimize/run time and any error without altering graph,
   operator, layout, dtype, or tensor order.

The M2 contract, rather than candidate HPI metadata, defines the first shape
set:

| Component | Minimum | Typical | Maximum for this proof |
|---|---|---|---|
| Detector | [1, 3, 32, 32] | [1, 3, 960, 544] | [1, 3, 960, 960] |
| Recognizer | [1, 3, 48, 160] | [1, 3, 48, 320] | [6, 3, 48, 320] |

The detector maximum follows the frozen M2 legacy resize limit of 960 rather
than the candidate HPI TensorRT profile's unrelated 4000-pixel maximum. The
recognizer maximum follows the frozen M2 batch size of 6. These shapes are
qualification probes only; they do not implement preprocessing or establish
the final runtime tensor contract.

A tract failure due to an operator, dynamic shape, model typing, optimization,
execution, resource bound, or non-finite output is rejection evidence for
this candidate configuration unless a future roadmap amendment explicitly
defines a legal alternative. The spike must not patch or simplify the graph.

### Observed result

The exact-artifact `tract-onnx` 0.23.4 path failed during symbolic shape
typing for both minimum probes before inference. An in-memory
metadata-specialization diagnostic could run all six zero-filled shape probes,
but it is not an exact-artifact execution and does not satisfy the blocker
gate. The complete commands, hashes, output signatures, resource observations,
and unfinished gates are recorded in
[`RUNTIME_TRACT_EVIDENCE.md`](RUNTIME_TRACT_EVIDENCE.md). This is rejection
evidence, not a runtime decision or a permitted graph-normalization path.

## Second candidate: ort

The current ort 2.0.0-rc.13 crate advertises Rust 1.88 compatibility and
default ONNX Runtime binary download features. Its official release notes
state that current x86-64 binaries target x86-64-v3. That conflicts with the
M2 rubric's baseline prohibition on an AVX/AVX2 requirement, so a prebuilt
binary route is not an acceptable baseline path without separate contrary
platform evidence.

An external ort proof ran with `ort` 2.0.0-rc.13 and an explicitly supplied,
temporary ONNX Runtime 1.28.0 library from a Python wheel. It loaded each exact
ONNX file, selected CPU execution with one-thread controls, and ran every
fixed shape in this plan through the observed named `x` input and
`fetch_name_0` output. The dynamic-loader missing-path probe failed normally
without fallback or download. The full evidence, including identities, output
shapes, repeat signatures, and incomplete gates, is in
[`RUNTIME_ORT_EVIDENCE.md`](RUNTIME_ORT_EVIDENCE.md).

This is a partial exact-artifact graph/shape and tensor-ABI pass only. The
temporary Python-wheel native library is not an approved baseline-compatible
system or source-built route. A follow-on source build from ONNX Runtime
v1.28.0 ran all six host probes and every declared detector/recognizer shape
through no-AVX QEMU TCG routes; see
[`RUNTIME_ORT_SOURCE_EVIDENCE.md`](RUNTIME_ORT_SOURCE_EVIDENCE.md). Its
calibrated compact fingerprints show host/QEMU bit-pattern differences for
detector minimum and every recognizer profile, while detector typical/maximum
aggregates also differ. A later two-minimum-shape, same-runtime elementwise
diagnostic characterized zero and synthetic-input deviations without finding a
value above `1e-4`, a fixed detector `> 0.3` crossing, or a recognizer
last-axis argmax change; it remains neither a static-reference comparison nor
a selected tolerance. The source-build evidence also includes five bounded C
API failure cases under a process watchdog and address-space limit. It is
still partial: it does not establish raw-output equivalence, a physical
baseline host, deployment, native supply-chain review, a Rust adapter/resource
policy, or a backend decision. No spike may implicitly download, copy, or
commit a native binary.

A separate external C API lifecycle probe then verified the same library and
both ONNX hashes before dynamically loading API version 28. With telemetry
disabled, sequential CPU execution, one intra/inter-op thread, memory-pattern
off, a 1,600,000 KiB virtual-memory limit, and a 600-second watchdog, it
completed twelve create/run/release cycles for each minimum-shape model. Every
cycle had the expected finite output; the short observed post-release RSS
window remained bounded, `ReleaseEnv` completed, and `dlclose` returned zero.
This is only short sequential Linux-host evidence: it does not show a
leak-free long soak, network isolation, Rust ownership, cancellation,
concurrency, malicious-input behavior, portability, or numerical equivalence.
The full commands, hashes, and cycle trace are in
[`RUNTIME_ORT_SOURCE_EVIDENCE.md`](RUNTIME_ORT_SOURCE_EVIDENCE.md).

A further temporary C API shared-session probe created one session per model,
then used four POSIX workers to make eight simultaneous zero-input minimum-shape
`Run` calls each. Both detector and recognizer probes completed 32 calls twice,
checking only type/shape/count/finite output and releasing every output; a
swapped-recognizer hash check failed before dynamic loading. This is a narrow
Linux-host concurrency signal only, not a long-soak, Rust synchronization,
network, resource, numerical-equivalence, portability, distribution, or
backend result. The source/binary hashes, commands, measurements, and limits
are recorded in `RUNTIME_ORT_SOURCE_EVIDENCE.md`.

A separate C API extension then held one session at a time and made 256
sequential zero-input minimum-shape calls per model, twice independently. It
verified the same pinned hashes before `dlopen`, checked and released each
output, and reported only timing, finite ABI checks, process RSS snapshots,
and thread counts. The swapped-model negative again failed before the loader.
This adds a bounded 512-call-per-model reuse observation, not a long-soak or
leak-freedom result: it has no sanitizer, network, malformed-model,
request-limit, cancellation, concurrency, physical-no-AVX, raw-tensor, Rust
adapter, distribution, or backend claim. The exact source/binary/log hashes
and limitations are in `RUNTIME_ORT_SOURCE_EVIDENCE.md`.

A separate temporary Rust wrapper probe used the same source-built library and
exact hashes. It verifies all three regular files before calling
`ort::init_from`, then uses one CPU `Session` per minimum-shape model for 24
sequential calls; two positive invocations checked only output name/type/shape/
count/finiteness and retained no output values. A swapped detector and a
recognizer symlink each rejected before the harness loader call. The lock,
source, binary, build checks, and limits are recorded in
`RUNTIME_ORT_SOURCE_EVIDENCE.md`. This is narrow external Rust-wrapper reuse
evidence, not a repository adapter, bounded public resource policy,
concurrency/cancellation proof, numerical-equivalence result, or backend
selection.

A later disposable Rust wrapper probe exercised `RunOptions` cancellation and
post-cancellation reuse against the detector only. One pre-terminated
minimum-shape run failed as expected and recovered after `unterminate`; three
maximum-shape runs were each terminated after 50 ms from a helper thread, then
the same session and options successfully completed a minimum-shape recovery
run. Four bounded positive invocations returned zero under the same 1,600,000
KiB address-space limit and a 90-second watchdog; output checks were limited to
the pinned ABI and finiteness, with no values retained. The session itself was
called only from the main thread. This is a finite external wrapper signal, not
a project cancellation API, long-soak, leak/race proof, request-level resource
policy, recognizer test, numerical-equivalence result, or backend selection.
The exact sources, hashes, replay command, measurements, negative controls,
and non-claims are in `RUNTIME_ORT_SOURCE_EVIDENCE.md`.

A later read-only pre-adoption review of that exact `ort`/`ort-sys` source and
feature closure found process-global dynamic-loader/environment state, native
pointer ownership behind manual `Send`/`Sync` assertions, and a wrapper
documented requirement to avoid simultaneous runs on one session. It also
confirmed that the tested manifest excludes download/copy/model-fetch features
and recorded a point-in-time empty OSV result for its 12-package normal
closure. These are constraints for a future adapter, not an approval of its
unsafe boundary, the native library, or a backend. The review and its limits
are recorded in `RUNTIME_ORT_SOURCE_EVIDENCE.md`.

### Direct Python-to-Rust raw relay

On 2026-08-04, a separately disposable direct-ONNX relay used the exact local
ONNX detector/recognizer and one temporary ONNX Runtime 1.28.0 CPU wheel
library. A Python 3.12.3 / NumPy 1.26.4 producer created deterministic
self-authored LCG `f32` inputs for all six declared M2 shapes, retained raw
input/output bytes only in a temporary directory, and ran twice with identical
capture metadata. A Rust 1.94.0 wrapper with `ort` 2.0.0-rc.13
`std`/`load-dynamic`/`api-28` verified every supplied model/input/output hash
before loading, then compared all 7,057,864 output elements. Two fresh Rust
processes were bit-identical to the Python output on every element with zero
absolute/relative error and identical aggregate result JSON.

This validates one direct wrapper's model/path/name/shape/dtype/byte-order
handling against the same native library, not an independent backend or static
Paddle oracle. It does not exercise image preprocessing, no-AVX execution,
resource policy, real adapter ownership, dictionary/CTC behavior, or
end-to-end semantics. Exact temporary hashes, commands, and non-claims are in
`RUNTIME_ORT_EVIDENCE.md`; no raw tensors, model bytes, library, harness, or
Cargo dependency entered this repository.

## Required evidence after any candidate smoke result

Smoke results are insufficient for RT-002 completion. Every viable candidate
still needs:

1. all M2 shape/operator probes plus explicit physical-baseline CPU/thread
   behavior; the QEMU shape probes are only partial portability evidence;
2. a legal static-oracle capture and raw tensor comparison under m2-tensor-v1;
3. deterministic repeat, latency, memory, binary, malformed-model,
   resource-limit, long-soak, cancellation-policy, and adapter-level
   concurrency evidence;
4. dependency/native-library license, maintenance, vulnerability, and notice
   review, including a locked adoption closure rather than the external spike;
5. scorecard and rejected-alternative evidence in RUNTIME_RUBRIC.md;
6. a project-specific unsafe/FFI design review covering explicit loader and
   process-init ownership, telemetry, error sanitization, and same-session
   concurrency before any adapter is introduced;
7. a separate RT-004 decision record before any repository runtime dependency
   or adapter is introduced.

## Sources

- https://crates.io/crates/tract-onnx/0.23.4
- https://crates.io/crates/ort/2.0.0-rc.13
- https://github.com/pykeio/ort/releases
- https://github.com/pykeio/ort
- https://api.osv.dev/v1/querybatch
