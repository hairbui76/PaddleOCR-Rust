# M2 Image Decoder Evidence Packet

Decision: `D-008` (image-decoder and image-limit portion only)  
Related roadmap item: `IMG-DEC-001`  
Status: Pre-gate research only; no decoder, dependency, or input policy is selected  
Prepared: 2026-08-02  
PaddleOCR baseline: `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and boundary

This packet records facts needed to make the M2 PNG/JPEG decoder decision
without treating a library survey as an implementation or an approval. It is
not a Cargo dependency proposal, an artifact manifest, a decoder API, an
acceptance of a native dependency, or evidence that OCR works.

`IMG-DEC-001` remains planned until its declared P2 and model-contract gates
are satisfied. In particular, this record does not resolve `D-008`, add an
image crate to `Cargo.toml`, establish a BGR/RGB/alpha rule, or permit
`IMG-001` implementation.

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
`opencv-python` and `opencv-contrib-python` without pinning a version, and no
approved image fixture/oracle capture exists.

Consequently, the following are still hypotheses to validate against an
isolated, version-recorded upstream oracle rather than implementation rules:

- which EXIF orientations are applied for the selected JPEG inputs and at what
  point in the conversion pipeline;
- how transparent PNG pixels, PNG palettes, grayscale inputs, CMYK JPEGs,
  progressive JPEGs, and non-8-bit PNGs map to the classic BGR image; and
- the exact pixel values after color conversion and any decoder-specific
  rounding.

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

## Decision options and current recommendation

| Option | Potential benefit | Evidence still required | Current disposition |
|---|---|---|---|
| Evaluate `image` with only `jpeg` and `png` features | Small explicit format surface, documented orientation API, and no intentional OpenCV/FFI commitment. | Exact dependency/supply-chain review; resource-limit spike; BGR/alpha/EXIF oracle comparison; malformed-input tests; MSRV and binary measurements. | First candidate in pre-gate isolated research; not selected. |
| Bind to an OpenCV-compatible native decoder | Could reduce a source-runtime difference for the classic path. | Exact library/version/distribution terms, FFI/unsafe audit, resource controls, CPU portability, and proof that it improves M2 oracle fidelity enough to justify the boundary. | Not evaluated; no dependency or implementation is authorized. |
| Evaluate another pure-Rust decoder | May offer different performance or allocation properties. | Equivalent public API, format, metadata, limits, safety, license, MSRV, and upstream-oracle evidence. | The direct `jpeg-decoder` 0.3.2 + `png` 0.18.1 spike above is pre-gate evidence only; no candidate is accepted. |

Subject to the unresolved gates, the evidence supports continuing qualification
of the minimal `image` feature configuration while retaining the direct-codec
pair as a distinct non-selected comparison route. That is not `D-008`
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
- [OpenCV image-codec documentation, version 4.5.5](https://docs.opencv.org/4.5.5/d4/da8/group__imgcodecs.html)

The external documentation informs a candidate evaluation only. It does not
replace version-recorded upstream oracle evidence or a reproducible
dependency/supply-chain review.
