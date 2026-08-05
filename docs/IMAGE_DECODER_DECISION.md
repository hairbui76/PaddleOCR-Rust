# Decision D-008 (M2 image portion): PNG-only bounded decoding

Roadmap items: `IMG-DEC-001`, `IMG-001`
Decision date: 2026-08-04
Decided by: project user, who explicitly delegated this choice to the agent on
2026-08-04 after being shown the recorded candidate evidence
Status: Resolved for the M2 image slice only; the PDF/office portion of `D-008`
and the JPEG portion remain open

## Context

`D-008` blocked every remaining image, tensor, and pipeline item. The recorded
evidence in [`IMAGE_DECODER_EVIDENCE.md`](IMAGE_DECODER_EVIDENCE.md) and
[`IMAGE_DECODER_SOURCE_REVIEW.md`](IMAGE_DECODER_SOURCE_REVIEW.md) compared four
routes against the committed `classic-v1-image-inputs` OpenCV 5.0.0
`IMREAD_COLOR` oracle. Its decisive split is:

- **Every evaluated route reproduces the committed 8-bit PNG cases exactly.**
- **No pure-Rust JPEG route reproduces the committed JPEG cases**; the replay on
  the committed corpus recorded a maximum component delta of `36` for both the
  `image` 0.25.10 facade and the direct `jpeg-decoder` 0.3.2 route.
- The only byte-exact JPEG route, `libjpeg-turbo-rs` 0.8.0, carries
  `get_unchecked` and raw-pointer code on the hostile-input path with a
  self-declared incomplete "mechanical safety sweep".

## Decision

**M2 supports PNG input only, decoded with `png` 0.18.1 and default features.**

JPEG is not supported in M2. It is deferred to the new roadmap item `IMG-003`,
whose entry gate is a measurement of what the recorded delta `36` actually does
to a model input tensor. That measurement is cheap once a pipeline exists and
impossible to do honestly before one does, so building the PNG slice first makes
the JPEG choice evidence-driven instead of speculative.

### Why PNG-only rather than accepting a JPEG tolerance

1. Every committed end-to-end fixture input is already a PNG
   (`classic-v1-e2e-no-text`, `-reading-order`, `-tall-crop`, `-unicode`), so
   the M2 golden corpus is fully reachable without JPEG.
2. Accepting a maximum component delta of `36` would mean publishing a decoder
   tolerance with no evidence about its downstream effect. `TOL-001` explicitly
   deferred that classification to this decision, and the honest answer is that
   the evidence to set it does not exist yet.
3. Adopting the byte-exact JPEG route would put unreviewed `get_unchecked` and
   raw-pointer code on the untrusted-input path of a crate that declares
   `unsafe_code = "forbid"` and whose policy is to treat image data as hostile.
4. Narrow, tested slices are this project's stated working method. PNG-only is
   the narrowest slice that unblocks `IMG-001`, `IMG-002`, `TEN-001`, `PRE-001`,
   and the detector/recognizer work behind them.

### Rejected alternatives

| Alternative | Why not now |
|---|---|
| `image` 0.25.10 facade | JPEG delta `36`; pulls 18 crates including SIMD colour-management code that this slice does not need. |
| `jpeg-decoder` 0.3.2 + `png` 0.18.1 | Same JPEG delta `36`. Its JPEG feature does forbid unsafe, so it stays the leading `IMG-003` candidate, but it buys nothing for a PNG-only slice. |
| `libjpeg-turbo-rs` 0.8.0 + `png` 0.18.1 | Byte-exact for JPEG, but unreviewed unchecked indexing on the hostile-input path; not acceptable without the targeted safety review that has not happened. |
| Native libjpeg-turbo through FFI | No runnable Rust binding in this environment (`turbojpeg` 1.5.1 needs `cmake`; the system `libturbojpeg.so.0` is ABI v2 and lacks `tj3Init`), and it would add a C dependency and an `unsafe` boundary. |

## Frozen M2 input semantics

The output contract mirrors OpenCV `cv2.imdecode(..., IMREAD_COLOR)`, because
that is the pinned upstream decode path.

| Aspect | M2 behaviour |
|---|---|
| Accepted format | PNG only, detected by the 8-byte content signature. A filename, extension, or caller-supplied hint never selects a decoder. |
| Colour output | Interleaved `uint8` BGR, three channels, row-major, top-left origin. |
| Grayscale | Replicated into all three channels, matching `IMREAD_COLOR`. |
| Palette | Applied; the resulting colour is emitted as BGR. |
| Alpha | Discarded, never composited, matching `IMREAD_COLOR`. This applies to truecolor+alpha, grayscale+alpha, and `tRNS`. |
| 16-bit samples | Truncated to the high byte (`sample >> 8`). This exactly reproduces the committed OpenCV oracle for `classic-v1-image-input-png-grayscale16-3x2`; a rounding conversion such as `round(sample / 257)` does not, and is therefore rejected. |
| Interlace | Adam7 is handled by the decoder; no separate policy. |
| Metadata orientation | No PNG metadata rotates or flips the image. An `eXIf` chunk is ignored. EXIF orientation belongs to the deferred JPEG work. |
| Bit depths 1/2/4 | Expanded by the decoder to 8-bit samples before conversion. |
| Animated PNG | Only the first frame is defined; `acTL` does not make a file multi-page in M2. |

## Frozen resource behaviour

| Bound | Value and enforcement |
|---|---|
| Encoded input | The existing `MAX_ENCODED_IMAGE_BYTES` (64 MiB), enforced by `EncodedImage` before any decoder work. |
| Declared side length | The existing `MAX_IMAGE_SIDE_LENGTH` (16,384), checked against the PNG header **before** any pixel buffer is allocated. |
| Declared pixel count | The existing `MAX_IMAGE_PIXELS` (40,000,000), checked against the header before allocation. |
| Output allocation | One BGR buffer, reserved with `try_reserve_exact`, so an allocation failure becomes a typed error instead of an abort. The decoder's own row buffer is bounded by the already-checked header. |
| Errors | Typed `Error` values only. No panic, no `unwrap`, no silent partial image. |

The single-buffer rule is deliberate: the recorded 180,000 KiB probe aborted
with SIGABRT precisely because separate output and BGR buffers were alive at the
same time.

## Consequences and non-claims

- `classic-v1-image-inputs` case classification under `m2-image-input-oracle-v1`
  is now: the five PNG cases are **exact**; the ten JPEG cases are
  **intentionally unsupported in M2** and are retained as `IMG-003` evidence.
- This decision selects no PDF, office, TIFF, WebP, BMP, or animated-image
  behaviour, and makes no OCR, detector, recognizer, or compatibility claim.
- `png` 0.18.1 is `MIT OR Apache-2.0`. Its locked transitive closure adds
  `adler2`, `bitflags`, `crc32fast`, `fdeflate`, `flate2`, `miniz_oxide`, and
  `simd-adler32`, taking the workspace lock from 24 to 31 packages. No C
  toolchain, `cc`, `cmake`, or `bindgen` build dependency is introduced.
- `simd-adler32` contains `unsafe`; the project crate keeps
  `unsafe_code = "forbid"` and adds no `unsafe` of its own. A dependency-level
  unsafe/supply-chain audit remains `P13` work.
- Reversing this decision means changing one module boundary: the decoder is
  private and no public API exposes it.

---

# Amendment (2026-08-05): JPEG adopted under the `IMG-003` decision

Decision date: 2026-08-05
Decided by: project user, choosing outcome 1 of the three `IMG-003` outcomes
after being shown the corrected measurements
Status: JPEG is supported with a measured tolerance; CMYK and 12-bit JPEG
remain typed `Unsupported`

## What changed since the original decision

The original decision deferred JPEG because the recorded maximum component
delta of `36` was **unmeasured in consequence**. `IMG-003` made measuring the
consequence the precondition, and the two-stage measurement in
[`IMG_003_DELTA_MEASUREMENT.md`](IMG_003_DELTA_MEASUREMENT.md) resolved it in
both directions at once:

- the `36` is an artifact of the `3x2`-pixel probe corpus — a single partial
  MCU. On page-shaped JPEG content `jpeg-decoder 0.3.2` measures at most
  **`1`** from the OpenCV oracle (~1% of pixels), and **`3`** on saturated
  colour;
- through the whole pipeline, perturbations at `+/-1` change **zero
  characters** on the dense three-regime corpus, and `+/-3` changes one; the
  pathological `36` applied with a decoder-realistic block-correlated shape
  **does** destroy low-contrast detection, which is exactly why it matters
  that it does not occur on page-shaped inputs.

## Decision

**JPEG input is decoded with `jpeg-decoder 0.3.2`, `default-features = false`
(no `rayon` thread pool), under the tolerance profile the measurement
established.** Concretely:

- baseline and progressive scans decode; the committed ten-probe corpus is
  compared against the recorded OpenCV bytes with a per-component bound of
  `36` — the measured worst case for these few-pixel probes — and exact
  shape;
- **EXIF orientation is applied**, because the captured oracle proves modern
  OpenCV applies it at decode: the eight committed orientation probes record
  flipped bytes and transposed shapes, and this port reproduces all eight
  within the tolerance. The orientation tag is parsed from the `APP1` segment
  directly; a malformed or absent tag means no transform, as in OpenCV;
- **CMYK and 12-bit (`L16`) JPEG are refused** as typed `Unsupported`: no
  captured oracle pins a conversion, and a conversion whose constants nothing
  pins would be a plausible wrong image;
- grayscale JPEG replicates into BGR, mirroring the PNG convention;
- the same declared-header resource checks run before pixel decode: project
  side/pixel limits from the parsed SOF dimensions, and the shared `256 MiB`
  allocation envelope covering samples plus output.

The `_png`-named API functions are deprecated aliases of new `_image` names
(`recognize_image`, `detect_image`, `decode_image`), exactly as
[`STABLE_001_API_REVIEW.md`](STABLE_001_API_REVIEW.md) planned for this event:
widening a function named `_png` silently would make its name a lie.

## Consequences and non-claims

- The compatibility vocabulary for JPEG is **tolerance, not exactness**, and
  the tolerance is measured, not assumed: `36` on the committed probes, `1`-`3`
  on page-shaped content, zero recognized-character changes at `+/-1` on the
  committed dense corpus. No claim is made about real scans with a compression
  history, which no synthetic corpus can close.
- `jpeg-decoder 0.3.2` is `MIT OR Apache-2.0`, adds **no** transitive
  dependencies with default features off, and contains no build script. The
  crate keeps `unsafe_code = "forbid"`.
- Reversing this amendment means removing one dispatch branch and one
  dependency; the PNG path is untouched.
