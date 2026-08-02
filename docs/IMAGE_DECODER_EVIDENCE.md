# M2 Image Decoder Evidence Packet

Decision: `D-008` (image-decoder and image-limit portion only)  
Related roadmap item: `IMG-DEC-001`  
Status: In progress; a self-authored input oracle is committed, but no decoder, dependency, or input policy is selected
Prepared: 2026-08-02  
PaddleOCR baseline: `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and boundary

This packet records facts needed to make the M2 PNG/JPEG decoder decision
without treating a library survey as an implementation or an approval. It is
not a Cargo dependency proposal, an artifact manifest, a decoder API, an
acceptance of a native dependency, or evidence that OCR works.

`IMG-DEC-001` is in progress on its evidence/fixture work but cannot close
until its declared P2 and model-contract gates are satisfied. In particular,
this record does not resolve `D-008`, add an image crate to `Cargo.toml`,
establish a BGR/RGB/alpha rule, or permit `IMG-001` implementation.

The M2 scope is intentionally narrow: one explicitly supplied local PNG or
JPEG image. PDF, animated/multipage formats, office documents, image URLs,
and model downloads are outside this packet and remain later roadmap work.

## Observed upstream input path

The pinned classic scripts call `cv2.imread(image_file)` for ordinary detector
and recognizer image inputs in:

- `PaddleOCR/tools/infer/predict_det.py`;
- `PaddleOCR/tools/infer/predict_rec.py`; and
- `PaddleOCR/tools/infer/predict_system.py`.

The classic base64 helper calls `cv2.imdecode(data, cv2.IMREAD_COLOR)` in
`PaddleOCR/tools/infer/utility.py`. OpenCV documents `IMREAD_COLOR` as a
three-channel BGR conversion and documents that ordinary image reads take EXIF
orientation into account unless an ignore-orientation or unchanged flag is
used. This is useful behavioural evidence, but it is not yet an M2
compatibility result: the pinned upstream `requirements.txt` names
`opencv-python` and `opencv-contrib-python` without pinning a version. The
committed self-authored capture below records one explicit OpenCV environment,
but it is not yet a complete upstream compatibility result.

Consequently, the following are still hypotheses to validate against an
isolated, version-recorded upstream oracle rather than implementation rules:

- which EXIF orientations are applied for the selected JPEG inputs and at what
  point in the conversion pipeline;
- how transparent PNG pixels, PNG palettes, grayscale inputs, CMYK JPEGs,
  progressive JPEGs, and non-8-bit PNGs map to the classic BGR image; and
- the exact pixel values after color conversion and any decoder-specific
  rounding.

## Committed self-authored image-input oracle (2026-08-02)

[`tests/fixtures/classic-v1-image-inputs/`](../tests/fixtures/classic-v1-image-inputs/)
now contains a reviewable, Apache-2.0 corpus produced by
[`tools/capture_image_decoder_oracle.py`](../tools/capture_image_decoder_oracle.py).
The developer-only generator uses only Python's standard library plus an
explicit external OpenCV/NumPy environment; it creates all bytes in memory and
writes one capture document to stdout. It does not import, execute, or write
to `PaddleOCR/`, download an asset, or load a model. The committed document
replays byte-for-byte with Python 3.12.3, NumPy 2.5.1, and
`opencv-python-headless` 5.0.0.93 (`cv2` 5.0.0); its SHA-256 is
`ea0541264e3789bae023fdf6bcb1f5bd7831b0f44835975e06d4db71dd24b6e6`.

It contains fifteen valid self-authored inputs:

- truecolor, RGBA, grayscale, indexed-`tRNS`, and 16-bit grayscale PNGs;
- one baseline and one progressive JPEG; and
- one baseline JPEG for each Exif orientation value 1 through 8.

It also contains five bounded negative inputs: empty, unknown bytes, a
truncated PNG, a CRC-valid header one pixel over the current width limit, and
a PNG deliberately paired with a `.jpg` filename hint. Valid byte streams and
the OpenCV `cv2.imdecode(encoded, cv2.IMREAD_COLOR)` HWC/BGR/`uint8` arrays
are separately SHA-256 pinned; the negative inputs have their own aggregate
digest. The offline Rust fixture-integrity gate verifies every base64 payload,
recorded byte length, BGR shape, per-payload digest, case ID, and aggregate.

In this exact OpenCV capture, Exif values 1–4 produce 2-by-3 output and values
5–8 produce 3-by-2 output; the recorded BGR bytes make every orientation
transform auditable. This is stronger than a format survey, but it remains
finite evidence only. `m2-image-input-oracle-v1` deliberately does **not**
make a Rust decoder's outputs exact requirements yet: `D-008` must classify
each case as exact, tolerated with a predeclared bound, or explicitly
unsupported, then record that policy at the public boundary. In particular,
the 16-bit PNG, JPEG component deltas, and malformed-input categories cannot
be silently normalized away.

## First pure-Rust candidate: `image`

The `image` crate version `0.25.10` was examined as a candidate only. Its
official documentation records native Rust codecs, separately selectable
`jpeg` and `png` feature flags, decoder dimensions/color metadata, decoder
orientation support, and `DynamicImage::apply_orientation`. Its repository is
dual licensed Apache-2.0/MIT and recommends disabling default features in a
library and enabling only the necessary formats. The default feature set also
enables a multithreading feature, so it must not be accepted implicitly.

If this candidate reaches an implementation spike, the starting feature shape
to evaluate would be equivalent to the following, but this is deliberately not
a prescribed or accepted `Cargo.toml` edit:

```toml
image = { version = "0.25.10", default-features = false, features = ["jpeg", "png"] }
```

The exact version, resolved lockfile, MSRV compatibility with Rust `1.94.0`,
transitive dependencies, native code, unsafe boundaries, advisories, license
notices, feature graph, binary-size effect, and maintenance posture remain
unreviewed. No claim is made that this candidate has no unsafe code or no
native dependency transitively.

### Resource-limit evidence and its limitation

The crate exposes `Limits` with strict maximum width and height controls. It
also exposes `max_alloc`, but its documentation calls that allocation limit
non-strict because some underlying decoders can ignore it. Its default
allocation allowance is `512 MiB`, which is not this project's accepted input
policy.

Therefore, a future `IMG-001` implementation cannot rely on decoder limits
alone. It must, at a minimum:

1. retain the already implemented `64 MiB` encoded-byte boundary;
2. install explicit strict width and height limits of `16,384` before decode;
3. check decoded width, height, checked multiplication, supported bit depth,
   channels, and final pixel count against the M2 `40,000,000`-pixel budget
   before creating project-owned image/tensor buffers;
4. choose an explicit decoder allocation ceiling only after documenting the
   accepted input representations and worst-case conversion buffers; and
5. map malformed, unsupported, dimension, allocation, orientation, and color
   conversion failures to stable project errors without panic, retry, fallback
   network access, or silent truncation.

The `image` documentation does not by itself prove that all intermediate
allocations obey the project budget. The candidate must be tested with
adversarial PNG/JPEG inputs and its exact selected codec configuration.

### Follow-up candidate-risk review

The following source-level facts were reviewed on 2026-08-02 to refine the
future spike plan. They are not a selected implementation, a resolved input
policy, or permission to add a dependency.

- The published `image` 0.25.10 manifest declares Rust 1.88 support. Its
  minimal `jpeg` feature reaches `zune-core` and `zune-jpeg`; the inspected
  manifest does not disable `zune-jpeg` default features. A real spike must
  therefore capture the resolved lockfile and `cargo tree -e features` output
  rather than infer the compiled codec/CPU surface from the top-level feature
  line.
- The inspected JPEG wrapper reads its source into an owned `Vec` and selects
  non-strict Zune decoding. This is a concrete reason to include the encoded
  input copy in a future request-memory envelope and to test malformed JPEG
  behavior. It is not proof of a flaw in a particular decode path.
- The 0.25.10 change log cautions that many decoders can panic on malicious
  input. That project-wide note is not a JPEG/PNG-specific finding, but it
  prevents a safety conclusion without the required adversarial corpus,
  no-panic checks, and fuzz/property evidence.
- `zune-jpeg` exposes direct width, height, scan, output-colorspace, strictness,
  and unsafe-use controls through its decoder options. It may provide a more
  explicit control surface, but would make the project responsible for format
  dispatch, orientation, BGR conversion, and compatibility behavior. Its
  default feature and baseline-CPU behavior require the same evidence as any
  other candidate.
- `png` exposes a limits-aware decoder but documents those limits as
  best-effort and does not bound project-owned allocations; it also exposes
  APNG frames. A single-image/APNG disposition must be fixed before adopting
  such a route. `jpeg-decoder` is a separate candidate with header and EXIF/ICC
  access, not an accepted fallback.

These observations strengthen, rather than relax, the existing decision
requirements: a selected path must have a version-locked dependency graph,
license/notice/advisory review, Rust 1.94 build evidence, CPU/unsafe/native
boundary analysis, bounded decode measurement, and oracle comparison before
`D-008` can close.

### Isolated Cargo spike (2026-08-02)

A disposable package outside this repository evaluated the exact candidate
shape below. It lived under `/tmp`, was `publish = false`, and was never added
to this workspace, its `Cargo.lock`, or its source tree. It did not read or
execute `PaddleOCR/`, download a model, or execute model inference.

```toml
image = { version = "=0.25.10", default-features = false, features = ["jpeg", "png"] }
```

The package declared Rust `1.94` and compiled with `rustc 1.94.0
(4a4ef493e 2026-03-02)` / `cargo 1.94.0 (85eff7c80 2026-01-15)`. Both
`cargo run --locked` and `cargo build --release --locked` succeeded. The
external spike's `Cargo.lock` SHA-256 was
`642d16922a4dab99b94a383a763f49e2e7b72efdeba7ab602b0f3b0eca89aa29`;
its manifest and one-file probe source SHA-256 values were
`ea1f79ef513dbe486bd4cd354567a0c161dd85d5821edb0fed12940ea918177f` and
`3bdeeac0b47e1d15978b26716782b03237d45f95e40946e890c35dba33df9c57`.
Those hashes identify a disposable research artifact, not a project input,
fixture, test, dependency lock, or supported binary.

The lock resolved these 18 registry packages:

```text
adler2 2.0.1; autocfg 1.5.1; bitflags 2.13.1; bytemuck 1.25.2;
byteorder-lite 0.1.0; cfg-if 1.0.4; crc32fast 1.5.0; fdeflate 0.3.7;
flate2 1.1.9; image 0.25.10; miniz_oxide 0.8.9; moxcms 0.8.1;
num-traits 0.2.19; png 0.18.1; pxfm 0.1.30; simd-adler32 0.3.10;
zune-core 0.5.1; zune-jpeg 0.5.15
```

`cargo metadata` reported declared Rust versions of 1.88.0 for `image`,
1.85.0 for `moxcms`, 1.73.0 for `png`, and 1.75.0 for both Zune crates;
the successful Rust 1.94 build covers this one locked graph only. Its reported
license expressions are lead data, not a completed license/notice review.

The probe generated a one-by-one RGB PNG and JPEG with the candidate's own
encoders, then explicitly selected the expected format and installed
`image::Limits` before decoding. It is therefore a narrow API/error probe, not
an interoperability, OpenCV, color, orientation, or hostile-input corpus.

| Probe | Observed PNG result | Observed JPEG result |
|---|---|---|
| Valid 1x1, width/height limit 1, `max_alloc = Some(3)` | `ok 1x1 output_bytes=3` | `ok 1x1 output_bytes=3` |
| Width/height limit 0 | `error` | `error` |
| `max_alloc = Some(0)` | `ok 1x1 output_bytes=3` | `ok 1x1 output_bytes=3` |
| Input truncated at half its generated bytes | `error` | `error` |
| PNG forced as JPEG / JPEG forced as PNG | `error` | `error` |

The error probes were wrapped with `catch_unwind`; these selected tiny cases
returned errors rather than panicking. This is not a no-panic claim for either
format. The probe allocated its final output buffer itself from
`decoder.total_bytes()` after setting limits, so the `max_alloc = Some(0)`
result only demonstrates that this limit is non-strict for these paths; it says
nothing about a total allocation envelope.

The external release spike measured 1,168,016 bytes before stripping and
946,864 bytes after `strip --strip-unneeded`. These are not PaddleOCR-Rust
binary-size measurements and do not select a release profile or dependency.

Source inspection of exactly the lock-selected crates found the following
decision-relevant facts:

- `image` 0.25.10 declares `moxcms = "0.8.0"` as an unconditional direct
  dependency. The locked minimal feature graph enabled `moxcms` default
  AVX/SSE/NEON/LUT feature paths, while the JPEG path enabled `zune-jpeg`
  default `x86`, `neon`, and `std` features.
- The `image` JPEG wrapper performs `read_to_end` into an owned `Vec` before
  decoding headers and before a future caller can reject dimensions. The
  existing project encoded-byte bound must therefore remain part of any future
  request-memory envelope.
- The `image` PNG `set_limits` implementation checks dimensions but contains a
  TODO to forward limits to `png::Reader::change_limits`, specifically noting
  unconstrained internal PNG allocations. This reinforces that decoder limits
  cannot substitute for project-owned resource checks.
- `moxcms` source conditionally dispatches AVX2/FMA and SSE4.1 paths and then
  has scalar paths. `zune-jpeg` conditionally chooses AVX2 routines through
  `DecoderOptions::use_avx2()` and makes unsafe SIMD calls on that path. The
  `moxcms` crate only enables its crate-wide `forbid(unsafe_code)` rule when no
  AVX/SSE/AVX512/NEON feature is selected. These are source-level observations,
  not a baseline-CPU execution proof or a completed unsafe-code audit.

The initial Cargo spike did not include no-AVX execution. The separately scoped
QEMU run below now covers one selected PNG/JPEG success and strict-dimension
error path, but not a complete unsafe-code or dispatch audit. `cargo-audit`,
`cargo-deny`, and `cargo-license` were unavailable. The spikes did not perform
an advisory/license audit, native-boundary review, fuzzing, a malicious corpus
run, EXIF/BGR/alpha/oracle comparison, or any model integration.

Accordingly, this spike does not select `image`, add a project dependency, or
change any image input behavior. `IMG-DEC-001` remains Planned and `D-008`
remains open. The temporary package must be discarded after recording this
evidence; a future decision needs a maintained, reviewable, and reproducible
qualification procedure.

### Isolated `image` no-AVX QEMU execution (2026-08-02)

A separate disposable Rust `1.94.0` package rebuilt the same exact minimal
`image` feature shape as a `static-pie` executable with an offline locked
graph and:

```text
-C target-cpu=x86-64 -C target-feature=+crt-static,-avx,-avx2,-fma
```

Its manifest, lockfile, probe source, initramfs init script, release binary,
QEMU log, and temporary guest-kernel package SHA-256 values were,
respectively,
`780ad94205312414c168a13c031990bf71329ac5caaf9b99b5206f4a28cbf039`,
`f53925f3b6dfb1f5578d154cf54ad0e4f03ee29505f8da834fd42b73dfbc3cc1`,
`b0b1d0bf94c68bb4c4d96d7646a86926d8ed5f67f2ae547fca54e5234a2a7851`,
`e01b2923e2eee9c12d1ee04caf5cbc2ae816788e252df6b99100b380d649e3bf`,
`3cc5fe2898de541f32cb330746ebce100bb7219a368317c23998007a2b30d68c`,
`053f691001d28f84c42d64859b009aaf8939133464d5a32d61c8c6ef5bef9914`,
and `be2d970c035b7227362faa5972a3090cabb3cf6ad5284614ce98b2bd5f828f0a`.
These identify disposable evidence only; none is a project dependency, input,
fixture, binary, or retained asset.

The probe used `PngEncoder` and `JpegEncoder` at quality 90 to make one
self-authored 2-by-1 RGB input, then decoded each byte stream with an explicitly
selected `ImageReader` format. It set strict `max_image_width = 2` and
`max_image_height = 1` plus `max_alloc = 1024`; the PNG round trip preserved
the six RGB bytes exactly, the JPEG result had the expected 2-by-1 / six-byte
RGB shape, and a separate `max_image_width = 1` decode returned an error for
both formats. This is a selected API/error probe, not proof that `max_alloc`
is strict or that other inputs cannot panic.

The static executable first ran on the host, then in a disposable QEMU `9.0.2`
TCG guest with one `qemu64` vCPU and 256 MiB memory. The guest's recorded
`/proc/cpuinfo` flags omit `avx`, `avx2`, and `fma`; it exited zero and matched
the host's output:

```text
image-noavx-ok png-bytes=75/png-fnv=2135d0191a95dd5a jpeg-bytes=642/jpeg-fnv=2ad1ed0e4ef5d182
```

This shows one `image` 0.25.10 JPEG/PNG code path can run without AVX/FMA even
though its locked graph retains optional architecture-specific code. It does
not identify the dispatched implementation, rule out other unsafe/CPU paths,
prove resource or malformed-input safety, establish OpenCV color/orientation
behaviour, or select `image` for `D-008`.

### Isolated `image` bounded mutation corpus (2026-08-02)

A fifth disposable package used the same exact `image` dependency shape to
exercise a deliberately bounded self-authored malformed-input corpus. It built
both debug and release static PIE executables with the same x86-64/no-AVX,
no-AVX2, no-FMA flags. The temporary manifest, lockfile, probe source,
initramfs init script, initramfs, debug executable, release executable, QEMU
log, and temporary guest-kernel package SHA-256 values were, respectively,
`8fee2770b880c4b8942aee388932a39ea9758eb59739fd995494681bf16aba61`,
`b1e3bbd48a95532e23ccc76921598679a13e31dfa66614004d0c4674d6e2e2f6`,
`ca8ef8e5624a1428cdf77607aab814b0cc3d9d52dc4b345857a479299c92fdb0`,
`49d71ca0eaf5aa70f603ae64addda825b16dc6d88d031f24483d39500f00a000`,
`351c18e760c422e8f67ce4574ce2f6eff21816464453835610a1bd395c5dd276`,
`acde3cc74ccb271d6ed9cac8f92e0b80d3291444ad21bd66176e48b881ef30a2`,
`93d6882f1f69e86e132681486396f1f3666f1deff5ed4db0a3276d7488c0ed2c`,
`94ae544ad39edaa5629581fcebeac875264e171274470522308a3950f765d2be`,
and `be2d970c035b7227362faa5972a3090cabb3cf6ad5284614ce98b2bd5f828f0a`.
They identify only temporary evidence artifacts and are not retained in this
repository.

The probe generated a 75-byte PNG and a 642-byte JPEG from the same
self-authored 2-by-1 RGB values, first checking valid decode behavior. For
each format it then attempted every inclusive byte-prefix, one `^ 0xff`
mutation at every byte position, and one forced-wrong format. Every decode
used explicit 64-pixel width/height limits and `max_alloc = 64 KiB`, and was
wrapped in `catch_unwind`. Thus the corpus contains 152 PNG attempts
(`76 + 75 + 1`) and 1,286 JPEG attempts (`643 + 642 + 1`).

With a 256 MiB host virtual-memory limit, both debug and release runs reported
the identical result. The release binary then produced the same result in the
one-vCPU/256 MiB QEMU `qemu64` guest whose CPU flags omit AVX/AVX2/FMA:

```text
image-mutation-ok png=attempts:152/ok:17/errors:135/panics:0 jpeg=attempts:1286/ok:507/errors:779/panics:0
```

The 507 successful JPEG cases are important: a decoder success must not be
treated as evidence that an input was an unmodified or canonical JPEG, and the
future project input policy must explicitly decide its truncation/permissive
decode behavior. The zero caught panics are useful only for this tiny generated
corpus. They do not cover aborts, hangs, out-of-memory behavior, arbitrary
malformed images, metadata/color/orientation paths, SIMD/unsafe coverage, or
the non-strict `max_alloc` limitation. This does not select `image` or permit a
decoder implementation before `D-008` closes.

### Isolated OpenCV color/orientation probe (2026-08-02)

A second disposable package outside this repository evaluated the same exact
`image = 0.25.10` JPEG/PNG feature shape against the recorded external OpenCV
environment. It used Rust `1.94.0`, Cargo `1.94.0`, `opencv-python-headless`
5.0.0.93 (`cv2.__version__ = 5.0.0`), and NumPy 2.5.1. The Rust probe decoded
each self-authored input with `ImageReader`, queried the decoder orientation,
converted the resulting `DynamicImage` with `to_rgb8`, and reversed every
component to compare BGR bytes with OpenCV. It then cloned the image and
called `apply_orientation` before a second comparison. The probe did not read
the upstream checkout, download an artifact, or execute inference.

The external capture generator created small truecolor/BGRA/grayscale PNGs, a
manually encoded indexed PNG with a `tRNS` palette, a manually encoded 16-bit
grayscale PNG, a baseline JPEG, a progressive JPEG, and all eight EXIF
orientation variants of the baseline JPEG. System Python 3.12.3 with Pillow
10.2.0 created one self-authored CMYK JPEG. OpenCV decoded each input both as
`IMREAD_COLOR | IMREAD_IGNORE_ORIENTATION` and as default `IMREAD_COLOR`.
The Rust probe and generator source remained external; the exact Cargo
manifest/lock/probe/primary-generator/CMYK-generator/capture SHA-256 values
were respectively
`3f001488a3d21d6b4d9031459beee08a5d8ce55de02643a7494885b907e0288a`,
`1641ff27fba1cbd632ee4cf78a831b8cb42c2b0925fd9d77a3650b5dc54f81fd`,
`361eb4336245467b4df9a22f4ccf78891b9aa22315708ba2d9f339f9bba030a9`,
`a9875babe8b4ff1a3650d729546182404a9013d8f0ac21370dd5edbef0b469d1`,
`b72a70764ea82d76f47780017066e5db54bdff31815cbf5aedc5b6b210e4646b`,
and `20c70ac200f8cd784daa90e204bae6b9e03c30e665b69a9d5bab219125b48699`.
The capture records every fixture's byte SHA-256 and comparison result; it is
not a repository fixture or an accepted decoder test corpus.

| Input class | Observed candidate BGR result compared with OpenCV | Decision-relevant finding |
|---|---|---|
| 8-bit truecolor, BGRA, grayscale, and indexed/tRNS PNG | Exact bytes and dimensions after explicit RGB-to-BGR reversal. | The fixed probe shows that alpha was discarded rather than composited and the selected PNG forms can be made BGR-equivalent through an explicit conversion. It does not establish all PNG semantics. |
| JPEG baseline and progressive | Same dimensions; maximum component delta `2`, not exact bytes. | The independent JPEG decoders are not a byte-exact OpenCV replacement even for this small corpus. |
| CMYK JPEG | Same dimensions; maximum component delta `1`, not exact bytes. | CMYK requires an explicit supported/unsupported policy and a larger oracle corpus; this one generated file is not enough to accept it. |
| JPEG EXIF orientations 1–8 | `image` reported `NoTransforms`, `FlipHorizontal`, `Rotate180`, `FlipVertical`, `Rotate90FlipH`, `Rotate90`, `Rotate270FlipH`, and `Rotate270` respectively. Applying the returned orientation produced the same dimensions as default OpenCV, with the same JPEG maximum delta `2`. | Orientation is not automatically applied by this candidate path. A future Rust decoder must explicitly obtain and apply the metadata transform if it elects OpenCV-style default orientation semantics. |
| 16-bit grayscale PNG | Same dimensions; one component differed by `1` for the fixed 16-bit values. | Non-8-bit conversion cannot be claimed byte-exact from this probe and needs a documented policy or an intentional rejection. |

This evidence narrows the future implementation experiment but does not close
`D-008`: it supplies neither an accepted supported-format policy nor a
project-owned resource envelope, unsafe/supply-chain audit, malformed corpus,
model-preprocessing comparison, or a tolerance decision for JPEG/16-bit
differences. In particular, a component-level image difference cannot be
silently treated as harmless for a model tensor. No Cargo dependency, decoder
code, fixture, compatibility claim, or image-input behavior changed as a
result of this external probe.

### Isolated direct-codec spike (2026-08-02)

A third disposable package outside this repository tested a lower-level
pure-Rust pairing rather than the `image` facade:

```toml
jpeg-decoder = { version = "=0.3.2", default-features = false, features = ["platform_independent"] }
png = "=0.18.1"
```

The package declared Rust `1.94`, was `publish = false`, and generated its
small 2-by-1 RGB PNG/JPEG inputs with Pillow 10.2.0 outside this repository.
It did not read `PaddleOCR/`, download an artifact, execute inference, or add
a project dependency, fixture, lockfile, or input behavior. `cargo run --locked`
passed in debug and release, and `cargo build --release --locked`
passed with `rustc 1.94.0 (4a4ef493e 2026-03-02)` / `cargo 1.94.0
(85eff7c80 2026-01-15)`. The isolated manifest, lockfile, probe, generator,
PNG, and JPEG SHA-256 values were respectively
`313e27f4ffc55e886874d61832f59e27bef53aa94536bb1f4bb6f2c811af6ce6`,
`f700fdfe8a38428b461a42acc6e5c5055adcdeee9f54920012bc5f35170a0d97`,
`63b4a878278ffe282956aa02fd77b451b2c23e21e520505c7ffd651388fc343b`,
`949d2ec237b2d2a36685b307e852774664fb81b1c39ae52a709ee6cbbe9f6442`,
`a4c468556ea0f17e53ef7c13d72e4748dce99268b794f2d9e7d6904b4160b93c`,
and `df5080e952518da1c1cbf5ba7d04b9967261a1ac8ed8d920a86828be99637b3`.
They identify the disposable experiment only. Its release executable was
941,568 bytes before stripping and 783,184 bytes after `strip --strip-unneeded`;
neither measurement is a PaddleOCR-Rust binary budget
result.

The generated lock resolved ten registry packages: `adler2` 2.0.1,
`bitflags` 2.13.1, `cfg-if` 1.0.4, `crc32fast` 1.5.0, `fdeflate` 0.3.7,
`flate2` 1.1.9, `jpeg-decoder` 0.3.2, `miniz_oxide` 0.8.9, `png` 0.18.1,
and `simd-adler32` 0.3.10. The locked `flate2` route was its Rust `miniz_oxide`
backend; no native zlib feature appeared in `cargo tree -e features`. This is
not a license, advisory, native-boundary, or unsafe-code audit.

The narrow probe checked the following without a panic:

| Probe | Observed result | Limit of the observation |
|---|---|---|
| 2-by-1 RGB PNG, `Limits.bytes = 1024` | Header, frame, and six exact RGB bytes decoded. | Only one 8-bit RGB PNG; no alpha, palette, grayscale, 16-bit, ICC, APNG, or OpenCV comparison. |
| Same PNG, `Limits.bytes = 0`; truncated PNG | Each returned an error from `read_info`. | `png::Limits` remains documented best effort and excludes caller-owned output allocations. |
| 2-by-1 RGB JPEG, `set_max_decoding_buffer_size(1024)` | Metadata reported 2-by-1 and decoding returned six bytes. | It does not establish JPEG component values, BGR behavior, or OpenCV equivalence. |
| Same JPEG, `set_max_decoding_buffer_size(0)`; truncated JPEG | Each returned an error from `decode`. | It is a tiny selected-path error probe, not a hostile corpus or no-panic proof. |
| Checksum-corrected 16,385-by-16,385 PNG header; corresponding JPEG SOF header | `read_header_info` / `read_info` exposed the oversized dimensions without requesting pixel decode. | The project must enforce its own 16,384-side and 40,000,000-pixel limits immediately after safe metadata parsing; this does not measure intermediate allocation. |

Source review found that `jpeg-decoder`'s `platform_independent` feature
excludes its `arch` module and makes the crate forbid unsafe code, while
disabling its default `rayon` feature removes the JPEG worker dependency. It
still leaves M2 work: JPEG EXIF bytes become available only after successful
decode, so this pairing offers neither a safe orientation parser nor a chosen
orientation policy. The direct `png` graph is not equally scalar: `png` 0.18.1
enables `miniz_oxide`'s `simd` feature, and its `fdeflate` dependency enables
`simd-adler32` defaults. The reviewed checksum implementation has runtime
CPU-feature dispatch with a scalar fallback. The initial tiny probe did not
execute without AVX; the separately scoped QEMU execution below covers one
successful no-AVX path, but not a complete unsafe audit. PNG also documents
its limits as best effort. Therefore this experiment makes the direct pair a
factual alternative, not an accepted decoder route or a reason to weaken the
existing resource, orientation, color, fuzzing, supply-chain, or oracle gates.

### Isolated no-AVX QEMU execution (2026-08-02)

A fourth disposable package recreated the exact direct-codec dependency shape
with Rust `1.94.0` and an offline locked build. It compiled a `static-pie`
executable using:

```text
-C target-cpu=x86-64 -C target-feature=+crt-static,-avx,-avx2,-fma
```

The temporary manifest, lockfile, probe source, initramfs init script, release
executable, QEMU log, and downloaded guest-kernel package SHA-256 values were,
respectively,
`013f95c2bde82a87c13f1f8739f018636a1f299ba898ff1a48f440fe533e43de`,
`80b3ab2be152353daa168458d59939bb3e02c23030aad67921ed614e3328b069`,
`b05fc7e64a949896e75f1978c688d38cf52cb0c6884c05b816c94e23161ee41d`,
`c3d467031cce8c571096f6581df3cd8d54bea5de5d53213c25b04b415cb48270`,
`849f10574bda52b9c81e15647e31ab208a39ae4669a763ea607b742463287c0f`,
`bafaa1d38eb80580fabca6aa64efdd22b9e29cd55cf0d82b77094bbc25686345`,
and `be2d970c035b7227362faa5972a3090cabb3cf6ad5284614ce98b2bd5f828f0a`.
They identify temporary research artifacts only; none is a project input,
fixture, dependency, binary, or retained model asset.

The probe encoded and decoded a self-authored 2-by-1 RGB PNG and decoded the
`jpeg-decoder` package's `benches/tower.jpg` only to exercise a successful JPEG
path. The latter asset was embedded in the temporary executable, never copied
into this repository, and had SHA-256
`40b5ae0df66540ba3ac60edf2840b4b8edd0500706105f3b63083e3a8993119a`
(70,657 bytes). It is not an image compatibility fixture or a redistribution.
The locked graph remained the same ten registry packages recorded above.

The executable ran first on the host, then inside a disposable initramfs under
QEMU `9.0.2` TCG with one `qemu64` vCPU and 256 MiB memory. The guest kernel
was extracted outside the repository from the configured Ubuntu archive's
`linux-image-unsigned-7.0.0-28-generic` version `7.0.0-28.28~24.04.1` package;
its package SHA-256 is recorded above. The guest's `/proc/cpuinfo` flags included
only the expected baseline SSE/SSE2-era features and contained no `avx`,
`avx2`, or `fma`. The host and guest both completed with exit status zero and
printed the same deterministic result:

```text
direct-codec-noavx-ok png=2x1/png-bytes=70/png-fnv=2135d0191a95dd5a jpeg=512x512/jpeg-bytes=786432/jpeg-fnv=c5dc0a5a82f8ed16
```

This is evidence that the selected static executable can decode these two
chosen inputs on one no-AVX/no-FMA x86-64 guest. It does not show which PNG
checksum implementation ran, rule out every CPU-dispatch or unsafe path, prove
all decoder inputs work on that CPU, establish resource safety, establish color
or EXIF behaviour, or select the direct pair for `D-008`.

### Direct-codec input-oracle replay and supply-chain check (2026-08-02)

A further disposable Rust `1.94` package tested the direct-codec pair against
the committed, self-authored
[`classic-v1-image-inputs`](../tests/fixtures/classic-v1-image-inputs/)
capture. It remained outside this repository and read only that JSON fixture;
it did not access `PaddleOCR/`, download a model, execute inference, or add a
project dependency, fixture, input policy, or supported capability. Its
manifest selected the same direct codec shape:

```toml
jpeg-decoder = { version = "=0.3.2", default-features = false, features = ["platform_independent"] }
png = "=0.18.1"
```

The temporary harness also pinned `base64` 0.22.1, `serde_json` 1.0.151, and
soft-only `sha2` 0.10.9 solely to parse the committed capture and report
digests. The final temporary manifest, lockfile, harness source, and
portable-release executable had SHA-256 values
`685f63f660bee31b2952acaba0411f6b3080a9327a10bd3f2629d60d6406c728`,
`d33092924c3dd449fc1fa696eb7c3906926c6f898091b55a3077e5fa34d42cbd`,
`1ec7ff10729c1a06a28a612d7ae0336612c2907c0328a63dddaa5444195bd32a`,
and `87d0f223b58fcaa30a9e29310959e60a91cb26da9b94ae88e07608a922efee16`
respectively. These identify unretained research artifacts only; they are not
an accepted implementation recipe or a project lockfile.

The harness selected the decoder from PNG/JPEG content signatures, not a
filename hint. It bounded encoded input at 64 MiB, checked the 16,384-side and
40,000,000-pixel limits immediately after header metadata, and set a 128 MiB
decoder/output envelope for the experiment. Those are probe controls, not an
accepted project contract. For PNG it used `EXPAND` and discarded alpha when
making comparison BGR bytes; it deliberately rejected 16-bit PNG in its normal
probe path. A separate diagnostic used `EXPAND | STRIP_16` only to measure the
committed 16-bit fixture. For JPEG it kept `jpeg-decoder`'s metadata-driven
default color transform, parsed the returned TIFF-form Exif bytes after decode,
then applied one explicit orientation transform before RGB-to-BGR conversion.
Forcing `ColorTransform::RGB` was not used for the reported result because it
would ignore the JPEG's application color metadata.

Both `cargo run --locked` and the portable release replay below completed with
zero unexpected errors or caught panics:

```text
-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma
```

The portable build must not be described as a scalar-only PNG path:
`simd-adler32` remains feature-enabled and may choose SSE/SSSE3 at runtime on
a capable host. It is useful only to show that disabling AVX, AVX2, and FMA
did not change this finite corpus result.

| Corpus group | Direct-codec result against the recorded OpenCV BGR bytes | Disposition of the observation |
|---|---|---|
| Four 8-bit PNG cases: truecolor, RGBA, grayscale, indexed+tRNS | All dimensions and all BGR bytes were exact after `EXPAND`, alpha discard, and RGB-to-BGR conversion. | Positive finite evidence only; alpha semantics are still unselected. |
| One 16-bit grayscale PNG | The normal probe rejected it explicitly. The separate diagnostic `STRIP_16` conversion happened to match this one OpenCV BGR output exactly. | Neither result selects a 16-bit policy. |
| Baseline JPEG, progressive JPEG, and eight Exif JPEGs | All ten outputs had the expected dimensions; every JPEG differed in seven of eighteen BGR components with maximum absolute component delta `36`. Baseline direct BGR SHA-256 was `f060df3d12b0c4477b5ce2bfcfc64d2bdecf5aaec4a8d929f70c21a6950ab24d`. | This is materially less faithful to this corpus than the earlier `image` candidate's maximum delta `2`; no tolerance is accepted. |
| Exif orientations 1–8 | The parsed values were 1 through 8. Applying the probe's eight explicit geometry mappings yielded byte-exact transforms of the direct baseline result, including the 2-by-3 to 3-by-2 swaps. | Geometry handling is internally consistent, but does not erase the JPEG color difference. |
| Five negatives | Empty input, unknown bytes, truncated PNG, oversized PNG header, and PNG-with-`.jpg`-hint each produced the fixture's required control outcome. The oversized header was rejected before project pixel allocation. | A finite no-panic/error-mapping observation, not a hostile-input proof. |

The exact release replay has the same five exact PNG/diagnostic outputs, ten
non-exact JPEG outputs, component counts, maximum delta, BGR hashes, Exif
mapping checks, and five passing negative controls as the debug replay. The
large JPEG difference is consistent with codec-specific chroma reconstruction,
but that is an inference rather than a proven root cause. No model tensor was
tested, so the difference cannot be declared harmless.

The codec-only transitive closure remained the ten packages named in the
previous direct-codec section. Their manifest license expressions were
reviewed as first-pass data: `jpeg-decoder`, `png`, `bitflags`, `cfg-if`,
`crc32fast`, `fdeflate`, and `flate2` declare `MIT OR Apache-2.0`; `adler2`
declares `0BSD OR MIT OR Apache-2.0`; `miniz_oxide` declares
`MIT OR Zlib OR Apache-2.0`; and `simd-adler32` declares `MIT`. The selected
`jpeg-decoder` feature compiles out its `arch` module and its crate root
forbids unsafe code; the `png` crate root also forbids unsafe code. That does
not extend to the whole closure: `simd-adler32` contains runtime-dispatched
unsafe SIMD implementations, including an SSE/SSSE3 route, even though it has
a scalar fallback. The graph still has no native zlib feature, but this is not
a complete manual unsafe or notice audit.

`cargo-audit` 0.22.2 was installed only under the temporary package and ran
`cargo-audit audit --file Cargo.lock --json` successfully. It fetched the
RustSec advisory database commit
`308808d74a1462ec8b09c1e76938471c53b55dcc`, last updated
`2026-08-02T15:17:32+02:00`, loaded 1,178 advisories, scanned the
32-package temporary lockfile, and reported zero vulnerabilities. The tool
emitted two permission warnings while probing host CA certificate paths, but
the database fetch and audit both completed with exit code zero. This is a
date- and lockfile-specific advisory result, not a permanent supply-chain
approval, a license/notice conclusion, or a waiver of future audit/fuzzing
requirements.

The new replay makes the direct pair a useful comparison/control path but a
weaker current JPEG candidate than the minimal `image` configuration for this
specific corpus. It does not select `image`, reject the direct pair forever,
resolve `D-008`, close `IMG-DEC-001`, or authorize decoder implementation.

## Decision options and current recommendation

| Option | Potential benefit | Evidence still required | Current disposition |
|---|---|---|---|
| Evaluate `image` with only `jpeg` and `png` features | Small explicit format surface, documented orientation API, and no intentional OpenCV/FFI commitment. | Exact dependency/supply-chain review; resource-limit spike; BGR/alpha/EXIF oracle comparison; malformed-input tests; MSRV and binary measurements. | First candidate in pre-gate isolated research; not selected. |
| Bind to an OpenCV-compatible native decoder | Could reduce a source-runtime difference for the classic path. | Exact library/version/distribution terms, FFI/unsafe audit, resource controls, CPU portability, and proof that it improves M2 oracle fidelity enough to justify the boundary. | Not evaluated; no dependency or implementation is authorized. |
| Evaluate another pure-Rust decoder | May offer different performance or allocation properties. | Equivalent public API, format, metadata, limits, safety, license, MSRV, and upstream-oracle evidence. | The direct `jpeg-decoder` 0.3.2 + `png` 0.18.1 replay gives exact finite PNG controls but JPEG deltas up to 36; it is a non-leading comparison route, not an accepted candidate. |

Subject to the unresolved gates, the evidence supports continuing qualification
of the minimal `image` feature configuration while retaining the direct-codec
pair as a distinct non-leading comparison route. That is not `D-008`
resolution: exact behaviour and resource safety are more important than the
library name.

## Required decision and implementation proof

Before `D-008` can close for M2, `IMG-DEC-001` must record all of the
following for the selected implementation:

1. Exact decoder and transitive dependency versions, selected Cargo features,
   lockfile, licenses/notices, advisories, MSRV result, native libraries, and
   unsafe/FFI boundary (if any).
2. A precise local-input contract: content detection versus filename hints,
   accepted JPEG/PNG variants, unsupported formats, color order, grayscale,
   alpha, bit depth, ICC/metadata disposition, EXIF orientation, and error
   mapping.
3. Enforced resource rules for encoded bytes, width, height, pixel count,
   decoded/project-owned buffer allocations, CPU work, and any decoder limit
   that cannot be guaranteed strictly.
4. Legal, small, reproducible fixtures covering JPEG EXIF orientations 1–8,
   PNG transparency/palette/grayscale variants, progressive JPEG, malformed
   headers/truncation, dimension and allocation limits, and format confusion.
5. An isolated oracle capture that records the actual OpenCV version and
   configuration, then compares dimensions, orientation, BGR bytes, and later
   preprocessing tensor values under an approved tolerance profile.
6. Negative and fuzz/property tests proving typed failure without panic or
   unbounded work. The test corpus and generators must not rely on the
   `PaddleOCR/` link.
7. A documented intentional difference for any unsupported or divergent input
   class. A difference must be visible at the public boundary; it cannot be
   hidden by silently changing pixels or geometry.

Only after these proofs, plus the roadmap dependencies, may `IMG-001` add a
decoder implementation. `IMG-002` separately freezes the model-facing BGR/RGB
and preprocessing semantics.

## Evidence sources

### Read-only upstream sources

- Pinned commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`:
  `tools/infer/predict_det.py`, `tools/infer/predict_rec.py`,
  `tools/infer/predict_system.py`, and `tools/infer/utility.py`.
- Pinned commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`:
  `requirements.txt` (unversioned OpenCV package entries).

### External primary documentation consulted on 2026-08-02

- [`image` repository README and licenses](https://github.com/image-rs/image)
- [`image` 0.25.10 feature listing](https://docs.rs/crate/image/0.25.10/features)
- [`image::ImageDecoder` 0.25.10](https://docs.rs/image/0.25.10/image/trait.ImageDecoder.html)
- [`image::Orientation` 0.25.10](https://docs.rs/image/0.25.10/image/metadata/enum.Orientation.html)
- [`jpeg-decoder` 0.3.2 source package](https://docs.rs/crate/jpeg-decoder/0.3.2/source/)
- [`png` 0.18.1 source package](https://docs.rs/crate/png/0.18.1/source/)
- [`image::Limits` 0.25.10](https://docs.rs/image/0.25.10/image/struct.Limits.html)
- [`image` 0.25.10 manifest](https://docs.rs/crate/image/0.25.10/source/Cargo.toml)
- [`image` JPEG decoder source, 0.25.10](https://docs.rs/image/0.25.10/src/image/codecs/jpeg/decoder.rs.html)
- [`image` 0.25.10 change log](https://docs.rs/crate/image/0.25.10/source/CHANGES.md)
- [`zune-jpeg` 0.5.15](https://docs.rs/zune-jpeg/0.5.15/zune_jpeg/)
- [`zune-core::DecoderOptions` 0.5.1](https://docs.rs/zune-core/0.5.1/zune_core/options/struct.DecoderOptions.html)
- [`moxcms` 0.8.1](https://docs.rs/moxcms/0.8.1/moxcms/)
- [`png` 0.18.1](https://docs.rs/png/0.18.1/png/)
- [`jpeg-decoder` 0.3.2](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/)
- [`cargo-audit` 0.22.2](https://crates.io/crates/cargo-audit/0.22.2)
- [RustSec advisory database](https://github.com/RustSec/advisory-db)
- [OpenCV image-codec documentation, version 4.5.5](https://docs.opencv.org/4.5.5/d4/da8/group__imgcodecs.html)

The external documentation informs a candidate evaluation only. It does not
replace version-recorded upstream oracle evidence or a reproducible
dependency/supply-chain review.
