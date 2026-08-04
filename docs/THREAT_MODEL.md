# `THREAT-001` — Threat Model

Roadmap item: `THREAT-001`
Recorded: 2026-08-04
Scope: everything this project implements today — the classic OCR path, its CLI,
its API, and its one native boundary

A threat model for a project this small is mostly a list of surfaces that do not
exist. Saying so is more useful than inventing threats to look thorough: a model
that describes attacks against features the code does not have obscures the ones
against features it does.

## Trust boundaries

There are exactly two.

1. **The encoded image.** Hostile by assumption. A caller may pass anything.
2. **The model artifacts, dictionary, and runtime library.** Chosen by the
   caller, verified only if the caller declares a digest.

Everything else — the manifest, the options, the paths — is supplied by the same
caller who runs the process, and cannot be more privileged than they are.

## 1. Image decoders

**Surface.** `src/image.rs`, PNG only, plus `png` `0.18.1` and its decompression
chain.

**Threats and mitigations.**

| Threat | Mitigation |
|---|---|
| Decompression bomb — a small file declaring an enormous image | Dimensions are parsed from `IHDR` and checked *before* the decoder is constructed: `16,384` per side, `40,000,000` pixels, and a `256 MiB` envelope across both decode buffers |
| Allocation failure treated as abort | Both buffers use `try_reserve_exact`, so exhaustion is a typed error |
| Format confusion — a JPEG or archive named `.png` | The decoder is selected by the eight-byte content signature only. A filename or a caller hint never selects a decoder |
| Memory-unsafety in the decoder | `png` contains **zero** `unsafe`; what unsafe exists in that subtree is in SIMD checksum and decompression crates, never reached with unbounded caller-controlled lengths |
| Oversized input read into memory before the check | The bound is enforced **during** the read; a `200,000,000`-byte file is refused in `0.47 s` without being loaded |

**Residual.** A crafted PNG that is legal, within every bound, and decodes to
pixels chosen to maximise detected regions is still expensive — bounded by the
`1,000`-region cap, not by anything cheaper. A caller who accepts untrusted
images should also set `--time-budget-ms`.

## 2. Model artifacts

**Surface.** Two `.onnx` files and a dictionary, all caller-named local paths.

The important property is counter-intuitive and is stated in the user guide
because a user must know it: **the detector and recognizer are not
distinguishable by shape.** They export the same tensor names and leave the axes
this port constrains dynamic, so swapping them loads without complaint and fails
only on first use. A substituted model with the same ABI would not be caught by
any shape check either.

Identity is therefore the only mechanism, and it is optional by design —
`ADR_MODEL_DEC_001` records why. The consequence is that a caller who does not
declare digests has no protection against a substituted artifact, and this is
documented rather than mitigated.

**Not applicable:** archive extraction. Nothing here unpacks a `.tar`, `.zip`, or
any container, so path traversal during extraction has no surface.

## 3. Tensor allocation

**Surface.** `src/backend.rs`.

`RunBudget` bounds input elements, output elements, and batch **before** the
backend is called; `TensorContract` pins name, rank, and per-axis extents on both
sides; `run_validated` additionally checks the output name the backend actually
returned and rejects non-finite values. A model that declares an enormous output
cannot cause an enormous allocation, because the contract rejects the shape
before the runtime is asked to produce it.

## 4. Paths, URLs, and redirects

**Surface.** Paths only.

There is **no URL input** and **no download** anywhere in this project: not for
images, not for models, not for the runtime library. The entire SSRF class —
scheme confusion, redirects into private networks, DNS rebinding, proxy
handling, content-type spoofing — has no surface here, and `src/input.rs` records
why that is a decision rather than an omission.

Paths are used exactly as given. There is no search path, no base directory, and
no path joining from caller-supplied components, so traversal has nothing to
traverse.

## 5. Cache and temporary files

**Surface.** None in shipped code. No cache directory, no lock file, no
scratch file, no atomic-rename dance.

**One finding, in test code.** Two tests wrote to a fixed name in the shared
temporary directory. That directory is world-writable, so another user could
pre-create the path as a symlink and `fs::write` would follow it. Test-only and
low severity, but it is exactly the pattern this model exists to catch. Both now
use a process-unique path built from the process id and a counter.

## 6. Concurrency

Covered in full by [`CONC_001_EVIDENCE.md`](CONC_001_EVIDENCE.md). The relevant
security property: sharing an engine across threads is a **compile error**, not
a race, because the engine is `!Sync`. There is no lock to contend, no queue to
overflow, and no worker pool to starve, because this project spawns no threads.

## 7. Logs and diagnostics

Results go to stdout; diagnostics go to stderr. Diagnostics name the input path
and the dictionary entry count.

**The path appears in stderr but not in a typed error.** `Error::Io` carries a
`&'static str` operation name and the OS error, deliberately not the path, so a
library caller embedding this project does not leak a filesystem layout into a
structured error they might forward. The CLI prints the path because the user
typed it.

No image content, no recognized text, and no digest is logged.

## 8. Surfaces that do not exist

Listing these is the point of the model. Each is a real threat class for OCR
projects generally and has **no code here to attack**:

| Class | Why not applicable |
|---|---|
| Service and network attacks | No server, no listener, no RPC, no HTTP client |
| Archive traversal | Nothing is unpacked |
| PDF-embedded JavaScript, embedded files, remote references | No PDF support; `DOCIO-DEC-001` already forbids executing or fetching any of them if PDF is ever added |
| VLM prompt injection, and unsafe VLM output | No VLM, no prompts, no generated code paths |
| Training-data poisoning | No training |
| Deserialization gadgets | No runtime deserialization framework; the manifest parser is fifty lines of `key = value` with an unknown-key rejection |
| Environment-based configuration attacks | No artifact, path, or option is read from the environment in shipped code |

## 9. Open risks

1. **Gate `G2`.** The ONNX Runtime binary has no hermetic rebuild and no SBOM.
   This is the largest open supply-chain risk in the project, and it is not
   reduced by anything in this document.
2. **Optional identity checking.** A caller who omits digests is unprotected
   against artifact substitution. Documented, not mitigated.
3. **Shallow fuzzing.** `4,000` generation-driven cases, not coverage-guided.
   `ROB_001_EVIDENCE.md` says so plainly.
4. **Unreviewed dependency `unsafe`.** `375` sites in `ort` were counted, not
   read. `SAFE_001_AUDIT.md` says so plainly.

None of the four is closed by asserting it is small.
