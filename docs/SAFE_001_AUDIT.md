# `SAFE-001` — Unsafe and Native Boundary Audit

Roadmap item: `SAFE-001`
Audited: 2026-08-04
Toolchain: `rustc 1.94.0`, lockfile at `Cargo.lock`
Scope: this crate, its full dependency graph in both feature configurations, and
the one native boundary

`SAFE-001` asks for an audit of every unavoidable `unsafe` and native boundary:
its invariants, provenance, version, panic containment, thread behaviour, and
targeted tests. This is that audit. Counts below were produced by reading the
vendored sources in the local registry, not by trusting a crate's description of
itself.

## 1. This crate

`src/lib.rs` and `src/main.rs` both open with `#![forbid(unsafe_code)]`. `forbid`
rather than `deny`, so it cannot be lifted by an inner `allow` in a module — a
future contributor cannot re-enable it locally without editing the crate root,
which is visible in review.

Consequently there is **no unsafe code in this crate at all**, and no invariant
of ours to audit. Everything below concerns code we depend on.

That also bounds what the rest of this document can claim: the memory-safety
argument for this project reduces entirely to the safety of its dependencies and
of one dynamic library, so those are what get the scrutiny.

## 2. Dependency graph

The graph is deliberately small. With default features it is one direct
dependency and eight transitive; adding `onnxruntime` adds four more.

| Crate | Files containing `unsafe` | `unsafe` sites | Licence |
|---|---|---|---|
| `png` `0.18.1` | 0 | 0 | MIT OR Apache-2.0 |
| `fdeflate` `0.3.7` | 0 | 0 | MIT OR Apache-2.0 |
| `adler2` `2.0.1` | 0 | 0 | 0BSD OR MIT OR Apache-2.0 |
| `cfg-if` `1.0.4` | 0 | 0 | MIT OR Apache-2.0 |
| `miniz_oxide` `0.8.9` | 1 | 0 | MIT OR Zlib OR Apache-2.0 |
| `bitflags` `2.13.1` | 1 | 2 | MIT OR Apache-2.0 |
| `crc32fast` `1.5.0` | 2 | 6 | MIT OR Apache-2.0 |
| `flate2` `1.1.9` | 3 | 34 | MIT OR Apache-2.0 |
| `simd-adler32` `0.3.10` | 6 | 36 | MIT |
| `ort` `2.0.0-rc.13` | 64 | 375 | MIT OR Apache-2.0 |
| `ort-sys` `2.0.0-rc.13` | 4 | 34 | MIT OR Apache-2.0 |
| `libloading` `0.9.0` | 6 | 58 | ISC |
| `smallvec` `1.15.2` | 2 | 73 | MIT OR Apache-2.0 |

Dev-only: `base64` `0.22.1`, `serde_json` `1.0.151`, `sha2` `0.10.9`, all
MIT OR Apache-2.0, none reachable from a shipped binary.

Three observations:

- **The image path is essentially unsafe-free.** `png` itself contains no
  `unsafe` at all. What unsafe exists in that subtree is in SIMD checksum and
  decompression routines — `crc32fast`, `simd-adler32`, `flate2` — which are the
  most heavily exercised code in the Rust ecosystem and are not reached with
  attacker-chosen *lengths*, because this crate bounds every buffer before
  decoding.
- **Every `unsafe` site in the graph is in a dependency chosen for a stated
  reason**, recorded in `IMAGE_DECODER_DECISION.md` and
  `ADR_RT004_RUNTIME_SELECTION.md`. None arrived incidentally.
- **Every licence is permissive.** MIT, Apache-2.0, ISC, 0BSD, or Zlib. There is
  no copyleft anywhere in the graph, which is what makes the distribution
  position in `MODEL_DEC_001` tenable.

## 3. Build-time code execution

The entire graph contains **one** build script:

```
crc32fast-1.5.0/build.rs — 27 lines
```

It runs `rustc --version`, parses the minor version, and emits one `cfg` when it
is at least `1.80`, to enable stabilised ARM CRC32 intrinsics. It reads no
network, writes no files, and links nothing.

Notably `ort-sys` has **no build script**. That is a direct consequence of the
`load-dynamic` feature chosen in `RT-004`: with dynamic loading there is nothing
to find, download, or link at build time. The absence of that build script is the
most checkable evidence that the decision's central claim — nothing is downloaded
or linked at build time — actually holds.

## 4. The native boundary

There is exactly one: ONNX Runtime, opened at runtime by `libloading` from a path
the caller supplies.

| Property | Value |
|---|---|
| Library | `libonnxruntime.so.1.28.0` |
| Provenance | caller-supplied local path; never bundled, downloaded, or searched for |
| Verified build | SHA-256 `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa` |
| Linkage | dynamic, at runtime, via `libloading` `0.9.0` |
| API version | pinned by the `api-28` feature |
| Supply chain | **open** — gate `G2`, no hermetic rebuild and no SBOM |

### What crosses the boundary

Only what this crate's adapter permits. `run_validated` in `src/backend.rs`
checks, on the way in, the input tensor's name, rank, and every axis extent
against a `TensorContract`, plus element counts against a `RunBudget`; and on the
way out, the returned tensor's *actual* name, its rank and extents, and that
every value is finite. A backend implementation cannot surface a backend type
through the trait, so a malformed result cannot leak past the adapter as
something typed.

That is the containment argument: the crate's own code never dereferences a raw
pointer, and the data it accepts from the boundary is validated before it reaches
any indexing arithmetic.

### Panic and exception containment

`ort` contains exactly one `catch_unwind`, in `environment.rs`, guarding a
user-supplied thread-manager runner. This crate does not use custom thread
managers, so that path is not reached.

The relevant direction is the other one: a C++ exception inside ONNX Runtime does
not unwind into Rust; ONNX Runtime's C API converts failures into `OrtStatus`
values, which `ort` turns into `Result`, which `src/backend_ort.rs` maps to typed
`Error::Backend`. Verified behaviourally rather than by reading: a swapped
detector/recognizer pair — a model that loads and then cannot run — returns

```
paddleocr-rust: backend error: the ONNX Runtime session failed to run
```

and the process exits `2`. No abort, no panic, no partial result.

### Thread behaviour

Both sessions are created with intra-op and inter-op thread counts of `1`, and
the measured cold run used `99%` of one CPU, which confirms the setting took
effect rather than merely being requested.

`ort` does spawn threads in its own mutex and environment utilities; this crate
starts none. The session is held in a `RefCell`, so `OcrEngine` is `!Sync` and
the compiler refuses to share one across threads. `api::concurrency_position`
asserts that property so a future change cannot quietly relax it, and the
provisioned end-to-end suite runs four threads with one engine each and requires
them to agree.

## 5. Targeted tests

- `src/backend.rs` — eleven tests over a fake backend: conforming results, wrong
  output name, wrong output shape, non-finite output, inputs rejected before
  reaching the backend, budget rejection, mapped backend failure, digest-format
  validation, and artifact verification that reads nothing when the path is
  missing.
- `src/detector.rs`, `src/recognizer.rs` — contract violations at the adapter
  boundary: wrong rank, wrong channel count, wrong extent, wrong output name,
  wrong row count, class-count mismatch.
- `src/input.rs` — bounded reads including a stream that never ends.
- `src/fuzz.rs` and the `fuzz-primitives` target — bounded decoder, DB, CTC,
  geometry, and crop kernels driven from at most `16 KiB` of arbitrary bytes.
- `tests/end_to_end.rs` — corrupt, truncated, and oversized input, a missing
  artifact, a digest mismatch, a text file loaded as a model, and an engine that
  survives all of them.

## 6. Findings

No unsafe code in this crate; no invariant of ours to violate. One build script
in the whole graph, doing something trivial and inspectable. One native boundary,
fully mediated by a validating adapter, with failures observed to arrive as typed
errors.

**Two things this audit does not close:**

1. **Gate `G2` remains open.** The ONNX Runtime binary has no hermetic rebuild
   and no SBOM, and cannot fully close on this host because the source-tag
   signature's public key is not available locally. Everything above concerns
   how this project *uses* that library; it says nothing about how the library
   was produced.
2. **`unsafe` in dependencies was counted, not reviewed line by line.** 375
   sites in `ort` alone is not an amount this audit read individually, and
   claiming otherwise would be the kind of overstatement this repository
   consistently refuses. What is claimed is narrower and checkable: the counts
   are accurate, the crates are permissively licensed, each was chosen for a
   recorded reason, and none is reached with unbounded caller-controlled sizes.

A line-by-line review of `ort`'s unsafe surface would be a separate exercise, and
is the honest prerequisite for any claim stronger than the one above.
