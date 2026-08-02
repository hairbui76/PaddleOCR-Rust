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

At this initial evidence stage, the exact version, resolved lockfile, MSRV
compatibility with Rust `1.94.0`, transitive dependencies, native code, unsafe
boundaries, advisories, license notices, feature graph, binary-size effect, and
maintenance posture remained unreviewed. The later committed-corpus replay
below adds a date-specific lockfile and first-pass advisory/license evidence,
but not a completed review. No claim is made that this candidate has no unsafe
code or no native dependency transitively.

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
error path, but not a complete unsafe-code or dispatch audit. At the time of
this spike, `cargo-audit`, `cargo-deny`, and `cargo-license` were unavailable;
the later full-corpus follow-up records a temporary `cargo-audit` result only.
The initial spike did not perform an advisory/license audit, native-boundary
review, fuzzing, a malicious corpus run, EXIF/BGR/alpha/oracle comparison, or
any model integration.

Accordingly, this spike does not select `image`, add a project dependency, or
change any image input behavior. At this stage `IMG-DEC-001` was still
`Planned` and `D-008` was open; later factual evidence moved only the item to
`In progress`. The temporary package must be discarded after recording this
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
| Baseline JPEG, progressive JPEG, and eight Exif JPEGs | All ten outputs had the expected dimensions; every JPEG differed in seven of eighteen BGR components with maximum absolute component delta `36`. Baseline direct BGR SHA-256 was `f060df3d12b0c4477b5ce2bfcfc64d2bdecf5aaec4a8d929f70c21a6950ab24d`. | The later full `image` replay on this same committed corpus has the same baseline BGR digest and maximum delta `36`; the earlier separate maximum-`2` probe used a non-identical corpus and cannot rank candidates. No tolerance is accepted. |
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

The new replay makes the direct pair a useful comparison/control path. The
later full `image` replay does not establish that `image` is more JPEG-faithful
on this corpus, so neither pure-Rust route is selected or preferred here. This
does not reject either candidate forever, resolve `D-008`, close
`IMG-DEC-001`, or authorize decoder implementation.

### `image` input-oracle replay and supply-chain check (2026-08-02)

A separate disposable Rust `1.94` package replayed the same committed,
self-authored [`classic-v1-image-inputs`](../tests/fixtures/classic-v1-image-inputs/)
capture against the minimal `image` candidate:

```toml
image = { version = "=0.25.10", default-features = false, features = ["jpeg", "png"] }
```

It was kept as an untracked temporary research directory and read only that
JSON fixture. It did not access `PaddleOCR/`, download a model, execute
inference, add a project dependency, or prescribe project input behavior. The
temporary harness also pinned `base64` 0.22.1, `serde` 1.0.228, `serde_json`
1.0.145, and `sha2` 0.10.9 only to read and verify the capture. Its final
manifest, lockfile, harness source, and portable release executable had
SHA-256 values
`63a3ebdec2cc491786b6d8dcb2db5e518f6f7de36e4315b81ab458fcbc86467a`,
`916c4b3cf36d66577df6ad48a00d2e92dd5e39c49da1c86d66c4148b5fa32296`,
`4a2849e009b87f82abc9627939b48f30fc61cfe411db526a7012653619ae424f`,
and `2c6e6c4ccf7f14b951debd0394a63d1744d2b14f791fc0dc8e156047a7541930`
respectively. They identify unretained research artifacts, not a project
lockfile, dependency, fixture, binary, or accepted implementation recipe.

The harness chose PNG/JPEG solely from their content signatures, rejecting an
empty byte slice before selection and ignoring the fixture's deliberately
misleading filename hint. It applied experimental controls of 64 MiB encoded
input, a 16,384-pixel maximum side, 40,000,000 maximum pixels, and a 128 MiB
decoder/output envelope. It constructed `ImageReader` with an explicit format
and `Limits`, queried the decoder orientation, checked dimensions and
`total_bytes` before creating a project-style image buffer, decoded to a
`DynamicImage`, explicitly applied the reported orientation, converted to
RGB8, and reversed components to BGR. These are research controls only:
they do not make `image::Limits::max_alloc` strict, select an alpha/16-bit/ICC
policy, or define the future public contract. As a path check, the baseline
JPEG's manual decoder sequence produced the same BGR bytes as
`ImageReader::decode` with the same limits.

Both debug and release runs with the following compiler profile completed with
no unexpected error or caught panic:

```text
-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma
```

This portable compilation is not a scalar-only proof: the selected feature
graph retains runtime-dispatched architecture-specific code, and the replay
ran on the host rather than a new QEMU guest. Earlier limited `image` QEMU
evidence remains limited to its stated probe/corpus.

| Corpus group | `image` result against the recorded OpenCV BGR bytes | Disposition of the observation |
|---|---|---|
| Four 8-bit PNG cases: truecolor, RGBA, grayscale, indexed+tRNS | All dimensions and BGR bytes were exact after RGB8 conversion and RGB-to-BGR reversal. | Positive finite evidence only; transparent-pixel and alpha policy remain unselected. |
| One 16-bit grayscale PNG | Dimensions matched, but three BGR components differed with maximum absolute delta `1`. | No 16-bit conversion tolerance or support/rejection policy is accepted. |
| Baseline JPEG, progressive JPEG, and eight Exif JPEGs | All ten outputs had expected dimensions, but each differed in seven of eighteen BGR components with maximum absolute delta `36`. The baseline BGR SHA-256 was `f060df3d12b0c4477b5ce2bfcfc64d2bdecf5aaec4a8d929f70c21a6950ab24d`. | The previous separate `image` probe's maximum delta `2` was not reproduced on this committed corpus. No JPEG tolerance is accepted. |
| Exif orientations 1–8 | The decoder reported the expected `image::metadata::Orientation` values; explicit application produced the recorded 3-by-2 or 2-by-3 geometry, while retaining the JPEG component differences above. | Explicit orientation application is required by this investigated path; its final policy remains unselected. |
| Five negatives | Empty, unknown, truncated PNG, oversized PNG header, and content/name-confusion inputs produced each fixture's required control outcome. The oversized header was rejected before a project-style pixel buffer was created. | Finite no-panic/error-mapping evidence only; it does not prove library-internal allocation, hostile-input, or CPU-work bounds. |

The candidate's resolved `cargo tree` closure had eighteen registry packages
when its `autocfg` build dependency was included: `adler2`, `autocfg`,
`bitflags`, `bytemuck`, `byteorder-lite`, `cfg-if`, `crc32fast`, `fdeflate`,
`flate2`, `image`, `miniz_oxide`, `moxcms`, `num-traits`, `png`, `pxfm`,
`simd-adler32`, `zune-core`, and `zune-jpeg`. The resolved graph did not name
`cc`, `bindgen`, or an FFI binding crate, but that is a packaging
observation rather than a completed native-boundary audit. Manifest license
expressions are compatible first-pass data: the graph offers an Apache-2.0,
MIT, BSD-3-Clause, Zlib, 0BSD, or Unlicense alternative as applicable, with
`moxcms` and `pxfm` specifically declaring `BSD-3-Clause OR Apache-2.0` and
the Zune crates declaring `MIT OR Apache-2.0 OR Zlib`. This is not a notice,
source-license, or distribution conclusion.

The feature tree confirms that `moxcms` default AVX/SSE/NEON paths and
`zune-jpeg` default `x86`/`neon` paths remain enabled, while the PNG path pulls
`simd-adler32` and `miniz_oxide` SIMD support. Source inspection therefore
does not establish an unsafe-free closure; the existing runtime-dispatch and
unsafe review requirements remain open. A temporary `cargo-audit` 0.22.2 scan
of the 40-package harness lockfile used RustSec advisory database commit
`308808d74a1462ec8b09c1e76938471c53b55dcc`, last updated
`2026-08-02T15:17:32+02:00`, loaded 1,178 advisories, and reported zero
vulnerabilities. It emitted two host CA-certificate permission warnings but
completed successfully. The result is date- and lockfile-specific; it is not
a permanent security approval or a replacement for fuzzing, ongoing audits,
or a license/notice review.

This replay corrects the earlier non-comparable ranking: on the committed
corpus, neither the minimal `image` route nor the direct codec pair can claim
better JPEG BGR fidelity. No decoder is selected, no input policy is resolved,
and `D-008` and `IMG-DEC-001` remain open.

### Native libjpeg-turbo fidelity and Rust-binding feasibility (2026-08-02)

The recorded OpenCV oracle environment reports `build-libjpeg-turbo (ver
3.1.2-70)` for JPEG. As a command-level numerical control, the host's
`/usr/bin/djpeg` reports `libjpeg-turbo version 2.1.5 (build 20240408)` and
links to `/usr/lib/x86_64-linux-gnu/libjpeg.so.8.2.2`, owned by the
`libjpeg-turbo8` 2.1.5-2ubuntu2 package. A disposable Python verifier fed each
of the ten committed JPEG streams to `djpeg -rgb` through standard input,
reversed RGB to BGR, then applied the fixture's known orientation mapping.
All ten BGR outputs and oriented dimensions were exact against the recorded
OpenCV bytes. The temporary verifier source SHA-256 was
`164c1ee813e859e2733342b63d44b3e82d89a094b42ac69dfd44b891345f87c1`.

This is useful finite fidelity evidence across two libjpeg-turbo releases, but
it is deliberately narrower than a decoder qualification. `djpeg` is a
process-level tool, not a Rust API; the verifier used the fixture's known
orientation value rather than parsing Exif; it did not exercise PNG, Rust FFI,
allocation/work bounds, malformed JPEG behavior, color management, parallel
use, or a distributable binary. It does not show that an arbitrary
libjpeg-turbo configuration will match OpenCV.

An additional untracked Rust `1.94` wrapper feasibility spike declared:

```toml
turbojpeg = { version = "=1.5.1", default-features = false, features = ["cmake"] }
```

Its manifest, lockfile, and intended JPEG-only harness source had SHA-256
values `c2de8719b63d680593de4e1c65df332eabe7b8f4f9cd8f82df649afc859a935a`,
`be0a23550d38043f8dde94ae632fe1c84ee113e4684009ce83b7e4cbef9a8ef6`,
and `23a1f587078697bfc8bc1db9b4dc41a19359f4795f2f86eaa8ebecc9577f454b`
respectively. It attempted a header-before-allocation BGR decode and a
fixture-specific orientation control, but did not compile: the selected
`turbojpeg-sys` 1.2.0 CMake build could not find a `cmake` executable. The
crate's bundled libjpeg-turbo source identifies itself as 3.1.0. Its normal
default features also enable `cmake`, `pkg-config`, and `require-simd`, so the
spike's no-default-features shape was only a configuration experiment, not a
recommended build policy.

The system `libturbojpeg.so.0` is not a substitute for that wrapper: it exports
the v2 API symbols such as `tjInitDecompress` and `tjDecompress2`, but not the
v3 `tj3Init` symbol used by `turbojpeg` 1.5.1. The wrapper source also contains
an explicit `unsafe` FFI boundary and `unsafe impl Send` for its decompressor.
Its Rust crates declare `Unlicense OR MIT`, while the bundled native library's
Debian copyright record includes IJG attribution/notice conditions and several
component licenses. None of this is a complete source, security, distribution,
or notice review.

Consequently, the native route has a promising command-level JPEG fidelity
signal but no runnable Rust binding proof on this machine, no approved native
dependency boundary, and no selected deployment policy. It does not select
libjpeg-turbo, authorize `unsafe`, add a Cargo dependency, or close `D-008`.

### Direct `zune-jpeg` configuration control (2026-08-02)

Because `image` 0.25.10 uses `zune-jpeg` on its JPEG path, a further
disposable Rust `1.94` control called `zune-jpeg` 0.5.15 directly rather than
going through `DynamicImage`. Its candidate shape enabled only `std` and `x86`
features (not the crate's `neon` default feature) and used `zune-core` 0.5.1.
For each of the ten committed JPEG fixtures it requested direct BGR output,
strict mode, 16,384-side limits, a 100-scan limit, the same experimental
64 MiB/40,000,000-pixel/128 MiB envelopes, and the fixture's known orientation
mapping. The direct call therefore still does not test Exif parsing, project
input dispatch, PNG, library-internal allocation, or public error mapping.

The temporary manifest, lockfile, harness source, and portable release binary
had SHA-256 values
`bf779e6695d267def8557e3cdf068ac3b2fe54bef5b2fdb8be7c1763328892d7`,
`802c52b07f2716b55a4e3607d76777adca2c9f89fb41fb24e46fc5e6f2149776`,
`06a61b8325aa221515d6dc0390c90a2d936d242d250958763efe5af88163d65a`,
and `8e94084db06187f5af8857d4f53e541fba2f90e5107fc0769273fb5dc96fef19`
respectively. They identify unretained research artifacts only. Debug runs
with `set_use_unsafe(false)` and `set_use_unsafe(true)` both produced the
same results: all ten JPEGs had the expected oriented dimensions but differed
in seven of eighteen BGR components with maximum delta `36`, and had the same
per-case BGR digests as the prior `image` replay and the same baseline BGR
digest as the direct-codec replay. A release
`x86-64` build with AVX, AVX2, and FMA disabled and
`set_use_unsafe(false)` had the same finite result.

Thus neither direct BGR selection, strict-mode selection, nor this runtime
unsafe toggle remedies the recorded JPEG difference. This is finite evidence
only: it does not diagnose the algorithmic cause, prove every feature/CPU path
is identical, establish a full unsafe audit, or reject `zune-jpeg` for every
future role. It confirms that the direct configuration is not a currently
better JPEG-fidelity route for the committed oracle and does not select any
decoder.

### Pure-Rust `libjpeg-turbo-rs` JPEG control (2026-08-02)

A further disposable Rust `1.94` package evaluated the published
`libjpeg-turbo-rs` `0.8.0` package as a JPEG-only comparison control. The
candidate itself declares Rust `1.87`, `MIT OR Apache-2.0`, `build = false`,
and a default feature set of `std` plus `simd`. The scalar experiment instead
used the following non-default shape; no manifest from this experiment was
added to this repository:

```toml
libjpeg-turbo-rs = { version = "=0.8.0", default-features = false, features = ["std"] }
```

The registry lock checksum for that package was
`a0d8a1c652b51dbb85c3c3164b1da63b88dafcc3fc12ecceb52f7577738c21f1`.
The external harness lockfile and source had SHA-256 values
`64caebc53dc266dd6b9a68b8bd6a898691e0f450abc691be6114c32b8b394883` and
`db0ad127f8b1cd828640677c5c9e9fd237fef93e187f2aa8eccd680d0ddd89f9`.
Those hashes identify disposable research inputs only; the harness, its build
outputs, and no model or upstream asset are repository content. The candidate
normal dependency edge was only `libjpeg-turbo-rs -> thiserror`; neither the
26-package temporary lockfile nor its resolved dependency tree named `cc`,
`cmake`, `bindgen`, or `pkg-config`. This is a packaging observation for the
recorded version, not a perpetual native-boundary guarantee.

The harness read only the committed `classic-v1-image-inputs` capture. It set
`PixelFormat::Bgr`, enabled `set_stop_on_warning(true)`, called
`Image::apply_orientation()` on the decoder-returned Exif data rather than
using the fixture's known orientation, and checked the resulting dimensions
and BGR bytes against the frozen OpenCV oracle. The baseline JPEG, progressive
JPEG, and all eight Exif-orientation JPEG streams were byte-exact: all ten
matched their recorded BGR payloads and shapes, including the 3-by-2 to 2-by-3
orientation changes. The empty and unknown-byte negative controls were
rejected. This contrasts with the previously recorded finite delta of up to
36 for the other pure-Rust JPEG paths; it is not evidence for PNG, color
management, arbitrary JPEG conformance, or a project compatibility claim.

The same harness used `DecodeLimits` of a 16,384-pixel maximum side,
40,000,000 maximum pixels, 100 scans, and 128 MiB estimated decode memory.
It rewrote only the baseline control's SOF dimensions, without creating a
large image buffer. A 1-by-16,385 header returned the typed `image width`
limit error with actual `16,385` and limit `16,384`. A 7,000-by-5,000 header
is below both declared side and pixel limits but returned the typed `estimated
decode memory` error with actual `210,000,000` and limit `134,217,728`. Source
inspection locates these checks before the decoder's output-scale allocation.
The controls establish the selected API's observed header-error behavior, not
a proof of total allocation, CPU-work, or parser-resource safety.

For malformed-input signal, the ten JPEG streams each underwent 144 bounded
deterministic mutations (truncation, bit flip, overwrite, insertion,
duplication, and appended bytes), for 1,440 decode attempts. The scalar and
SIMD host runs both reported 733 successful decodes, 707 typed errors, and
zero caught Rust panics. The successful mutations are intentionally not
treated as accepted project inputs. Debug scalar, debug SIMD, and a release
scalar host build with `-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma`
all produced the exact control outputs and the same mutation totals. No QEMU
guest or broader hostile corpus was run for this candidate.

An external `cargo-audit` scan of the exact 26-package harness lockfile loaded
1,186 advisories from RustSec revision
`d91a8fc9492378f23cba86b81770c6d16de6ebba` and reported no vulnerability
records. It completed with two host CA-certificate permission warnings. This
is date-, lockfile-, and advisory-database-specific screening only; it is not
a security approval, a substitute for continued monitoring, or a complete
license/notice review.

Despite the absence of a C build dependency in this snapshot, this is not an
unsafe-free candidate. Source review found raw-pointer/`unsafe impl`
initialization in `common/huffman_table.rs`, `get_unchecked` paths in baseline
and progressive decode, and scaled-IDCT unsafe operations even when the
optional `simd` feature is disabled. The default `simd` feature adds
architecture-intrinsic and runtime CPU-dispatch paths. The candidate's
published manifest license is compatible first-pass data, but its source
provenance, notices, update process, full `unsafe` boundary, concurrency,
resource limits, and supported JPEG subset require a separate review.

Accordingly, `libjpeg-turbo-rs` is an exact finite JPEG comparison control,
not a selected decoder. It supplies no PNG route, no project dependency, no
public input policy, no decoder implementation, and no closure of `D-008` or
`IMG-DEC-001`.

### Hybrid `libjpeg-turbo-rs` + `png` control (2026-08-02)

To test the promising JPEG control alongside the already examined native-Rust
PNG path, a second disposable Rust `1.94` package combined the exact scalar
JPEG candidate with `png` `0.18.1`:

```toml
libjpeg-turbo-rs = { version = "=0.8.0", default-features = false, features = ["std"] }
png = "=0.18.1"
```

Its manifest, lockfile, and harness source had SHA-256 values
`07fd6abd0ec59f12000277db1b5f7af6607f0228778d718154b541484b8105ea`,
`92fe78da653962bf2b9477f563ac3bd1e6857b0d154187f5504c2140c0603ea9`,
and `81c1b0a380c5e152c10b74c6890fcfab6dd20093e6c67e43265b3fae9491a594`.
The 34-package lock pinned the `libjpeg-turbo-rs` checksum recorded above and
the `png` `0.18.1` checksum
`60769b8b31b2a9f263dae2776c37b1b28ae246943cf719eb6946a1db05128a61`.
It contained no `cc`, `cmake`, `bindgen`, or `pkg-config` package. As with the
other spikes, these are unretained external research artifacts, not repository
dependencies or a deployment guarantee.

The control used content signatures rather than a filename hint and applied
the following experimental bounds: 64 MiB encoded input, 16,384-pixel sides,
40,000,000 pixels, 128 MiB decoder/project BGR buffers, and 100 JPEG scans.
All fourteen normal 8-bit controls were byte-exact to the frozen OpenCV BGR
capture: four PNGs (RGB, RGBA with discarded alpha, grayscale, and indexed
`tRNS`) plus the baseline, progressive, and eight Exif-oriented JPEGs. JPEG
used strict warnings and the candidate's own Exif orientation application.
The five negative controls all had their required result: empty input,
unknown bytes, malformed PNG, the oversized PNG header before a project pixel
buffer, and content-name confusion. This is exact only for the listed fixture
streams and control implementation; it does not define project support for
the relevant PNG/JPEG classes.

The normal control deliberately rejected 16-bit PNG rather than silently
choosing a conversion policy. A separately labelled diagnostic used the
`png` crate's `normalize_to_color8()` transform. It had the expected 3-by-2
geometry and happened to be BGR-byte-exact for the one committed 16-bit PNG.
One small conversion is not a supported 16-bit policy, color-management
result, or guarantee for other depths/transfer functions; the input class
remains unresolved at the public boundary.

The hybrid control repeated the JPEG-only header checks: a modified 1-by-16,385
SOF failed the typed side limit, and a 7,000-by-5,000 SOF failed the typed
128 MiB estimated-memory limit before output-scale allocation. Its 1,440
deterministic JPEG mutations again reported 733 successful decodes, 707
errors, and zero caught panics.

### Hybrid PNG mutation follow-up (2026-08-02)

A follow-up copied the prior disposable hybrid harness, changed only its
source to add a PNG mutation loop, and retained the same manifest and lockfile.
The resulting source SHA-256 was
`5eae3f200c2411087fa6fc7c32200b020f1200258aa8a00e3aa06ec74d4392b8`;
the manifest and lockfile hashes remain those recorded above. No dependency,
fixture, model, or project source changed.

For each of the five committed PNG streams—including the normal-route
16-bit-policy rejection—the control applied 144 deterministic mutations:
truncation, a one-bit flip, a byte overwrite, a one-byte insertion, byte
duplication, or up to 32 appended pseudo-random bytes. Each derived input was
fed through the same content dispatcher and normal (non-`normalize_to_color8`)
PNG route inside `catch_unwind`. Across 720 attempts, the scalar debug,
`libjpeg-turbo-rs` SIMD debug, and scalar release builds with
`-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma` all reported 164
successful decodes, 556 controlled errors, and zero caught Rust panics. The
unchanged fourteen exact 8-bit controls, one labelled 16-bit diagnostic, five
negative controls, JPEG mutation totals, and JPEG header checks also passed in
each run.

This is a bounded deterministic mutation signal for this exact combined
control, not a general PNG fuzz campaign, parser safety proof, allocation or
CPU-work proof, supported-format policy, or decoder selection. No QEMU run of
this exact combined package, general hostile corpus, allocation
instrumentation, concurrency proof, or full work-bound proof has been
performed.

`cargo-audit` scanned the exact 34-package lockfile against RustSec revision
`d91a8fc9492378f23cba86b81770c6d16de6ebba`, loaded 1,186 advisories, and
reported no vulnerability records. It emitted the same two host
CA-certificate permission warnings. Metadata listed compatible license
expressions as first-pass data: `libjpeg-turbo-rs` and `png` are `MIT OR
Apache-2.0`; the known `png` closure includes `simd-adler32` (MIT),
`miniz_oxide` (`MIT OR Zlib OR Apache-2.0`), and the other previously
reviewed compatible expressions. This is not a complete source/provenance,
notice, vulnerability, or distribution review.

The combined path inherits both unresolved unsafe surfaces: the JPEG crate
uses unchecked/raw-pointer operations even without its optional SIMD feature,
and the PNG closure includes `simd-adler32` runtime-SIMD unsafe code even
though `png` itself forbids unsafe code. Consequently, this is a particularly
useful finite comparison route, not a decision. It adds no dependency, image
implementation, supported-format policy, or OCR claim, and `D-008` and
`IMG-DEC-001` remain open.

## Decision options and current recommendation

| Option | Potential benefit | Evidence still required | Current disposition |
|---|---|---|---|
| Evaluate `image` with only `jpeg` and `png` features | Small explicit format surface, documented orientation API, and no intentional OpenCV/FFI commitment. | Complete unsafe/native/notice review; strict allocation/CPU-resource proof; broader malicious/fuzz corpus; model-preprocessing comparison; and a supported/unsupported policy for divergent JPEG/16-bit/alpha/ICC cases. | Full committed input-oracle replay and first-pass advisory/license graph review are recorded, but no candidate is selected. |
| Bind to an OpenCV-compatible native decoder | Could reduce a source-runtime difference for the classic path. | Exact library/version/distribution terms, FFI/unsafe audit, resource controls, CPU portability, a runnable Rust binding, and proof that the selected build improves M2 oracle fidelity enough to justify the boundary. | A command-level libjpeg-turbo control is byte-exact for the ten committed JPEGs, but no Rust binding or deployment configuration is qualified or authorized. |
| Evaluate `libjpeg-turbo-rs` 0.8.0 as a JPEG-only route | Its recorded scalar and SIMD controls are byte-exact for the ten committed JPEGs without a C build dependency. | Full source/provenance/notice/unsafe review; broader bounded hostile corpus; independent allocation/work measurements; concurrency and CPU-portability qualification; and a complete PNG/input-policy route. | Exact finite JPEG and header-limit controls are recorded, but its unchecked/raw-pointer/SIMD surface and all non-JPEG work remain open; it is not selected. |
| Evaluate `libjpeg-turbo-rs` 0.8.0 plus `png` 0.18.1 | The combined scalar/SIMD control is byte-exact for all fourteen committed 8-bit PNG/JPEG inputs and handles the five negative controls without a C build dependency. | The separate JPEG and PNG unsafe closures; full hostile corpus; allocation/work/concurrency/physical-baseline CPU proof; 16-bit/color/ICC/APNG policy; and complete notices/provenance review. | Useful exact finite combined control only. It deliberately rejects 16-bit PNG in normal mode and has no selected input contract or decoder implementation. |
| Evaluate another pure-Rust decoder | May offer different performance or allocation properties. | Equivalent public API, format, metadata, limits, safety, license, MSRV, and upstream-oracle evidence. | The direct `jpeg-decoder` 0.3.2 + `png` 0.18.1 replay gives exact finite PNG controls but JPEG deltas up to 36; it remains a comparison route, not an accepted candidate. |

Subject to the unresolved gates, the evidence supports retaining the direct,
hybrid, facade, and native numerical paths as distinct comparison routes while
investigating whether one decoder can satisfy the actual image and
model-preprocessing contract. That is not `D-008` resolution: exact behaviour
and resource safety are more important than the library name.

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
- [`libjpeg-turbo-rs` 0.8.0](https://docs.rs/libjpeg-turbo-rs/0.8.0/libjpeg_turbo_rs/)
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
