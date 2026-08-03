# M2 Image Decoder Candidate Source Review

- Roadmap items: `IMG-DEC-001`, `SAFE-001`, `SUPPLY-001`
- Decision: `D-008`
- Review date: 2026-08-03
- Status: pre-adoption evidence only; no decoder is selected

## Purpose and boundary

This review narrows the source and feature-closure risks of the temporary
hybrid image-control route:

- JPEG: `libjpeg-turbo-rs` `0.8.0` with `default-features = false` and
  `std` only;
- PNG: `png` `0.18.1` with its default feature closure.

It does not add either crate to PaddleOCR-Rust, resolve `D-008`, approve an
unsafe dependency, select an input contract, establish a vulnerability or
notice conclusion, or implement `IMG-001`. The project still forbids its own
unsafe code. If a later implementation uses a dependency containing unsafe
code, it must isolate the public boundary and complete its own targeted review
and tests.

The prior external hybrid control is described in
[`IMAGE_DECODER_EVIDENCE.md`](IMAGE_DECODER_EVIDENCE.md). It showed exact BGR
results for fourteen self-authored 8-bit PNG/JPEG controls, but finite fixture
fidelity is not a supply-chain or safety approval.

## Exact external review closure

A disposable external Rust 1.94.0 package contained only the two candidate
dependencies. Its lockfile was generated with `CARGO_NET_OFFLINE=true` and
resolved 17 package entries including the root package. `cargo fmt --check`,
locked Clippy with `-D warnings`, and a locked release build passed using
`/usr/bin/gcc`. The probe constructed the public JPEG `DecodeLimits` type and
PNG `Limits` type only; it did not decode model inputs, retain model output, or
become a repository dependency.

| External input | SHA-256 |
|---|---|
| Minimal review `Cargo.toml` | `897dcec3ad060abb754895838103e59fc28d2765b66c4a90ac726c1e5721f0ad` |
| Minimal review `Cargo.lock` | `56a1879472a347c363a12c47e88fcec12ed6decb52b02315be400c00b3eb7eba` |
| Minimal review source | `c03e08c07cb646304c852d681b92ae8858654c030e4952e403611d202126e680` |
| Minimal review release binary | `a201d1202c57fa02ef2d2aadd7038225f6ff90121fc6dd57b0d45469bad30740` |
| `libjpeg-turbo-rs` `Cargo.toml` | `ffb5eb9f241427cbab14a5a52c3ae11e0da18de6c327b3bb5930552d5223af2c` |
| `libjpeg-turbo-rs` `src/lib.rs` | `996eaaeeeba03316845a41a3abe8c4cd21c937aa7daa6aa54c8af191660aba26` |
| `libjpeg-turbo-rs` `src/common/types.rs` | `cda88b2ddb0b86cab62ac28214bfd4434b98fcddd3a3c6f28d6fb88f053c88b9` |
| `libjpeg-turbo-rs` `src/decode/pipeline.rs` | `293b02b393453cdba10537098f7c4a66a627e12aa8c6c7960b5972d5d43aca2a` |
| `libjpeg-turbo-rs` `src/simd/mod.rs` | `58832a4512c6926316e9d2f2d0a2c4be701b319a25d6bf59a72b25db0c1981a4` |
| `png` `Cargo.toml` | `642c5d3c2bf538404cf1d5c4aac254d033e0854af844610435fd13e910bf93aa` |
| `png` `src/decoder/mod.rs` | `8c15bb208e38ffcc55421fd3ece858dfa6460d0cc05be76d56d1640591669249` |
| `simd-adler32` `Cargo.toml` | `6981052c018f374476e9dd4186c0df3f4f5f0108a6c6b627409a2644b06e925b` |
| `simd-adler32` `src/imp/mod.rs` | `a2fbd6c32879af23658e7c53cb07a9e338c8b1760d5599ec6971cf057edd8121` |

The production-relevant resolved edges are:

```text
root
├── libjpeg-turbo-rs 0.8.0 [std]
│   └── thiserror 2.0.19
└── png 0.18.1 [default]
    ├── bitflags 2.13.1
    ├── crc32fast 1.5.0
    ├── fdeflate 0.3.7
    └── flate2 1.1.9 [rust_backend]
        └── miniz_oxide 0.8.9 [simd]
            └── simd-adler32 0.3.10 [std]
```

The review binary's direct ELF dependencies were only the Linux loader,
`libc.so.6`, and `libgcc_s.so.1`; it had no direct native JPEG or PNG shared
library dependency. This establishes only the observed test binary's linkage,
not a future distribution or platform guarantee.

## JPEG source findings

`libjpeg-turbo-rs` declares `MIT OR Apache-2.0`, Rust 1.87, and `build =
false`; the reviewed route adds its `std` feature only and does not resolve the
crate's optional `simd` feature. Its public `DecodeLimits` exposes maximum
width, height, pixel count, scan count, and optional memory fields. The
external hybrid control exercised the constructor with the project's candidate
16,384-side, 40,000,000-pixel, 100-scan, and 128 MiB limits. The crate also
returns a typed `JpegError::LimitExceeded` category used by that control.

These API facts do not prove all allocations or work are bounded. In
particular, a future adapter must account for decoded bytes, BGR conversion,
orientation transforms, temporary buffers, and any codec-internal memory as a
single request envelope. It must map third-party messages to stable project
errors rather than expose raw error text.

The candidate is not an unsafe-free JPEG dependency. The reviewed crate root
declares `pub mod simd` and explicitly allows `unsafe_op_in_unsafe_fn` for that
module while noting a large unresolved mechanical safety sweep. Its decode
paths contain raw-pointer, unchecked-index, and unsafe helper operations; the
SIMD feature gates dispatch selection in `simd::detect`, but the source module
layout still includes architecture-specific code on applicable targets. A
feature-only inspection therefore cannot prove absence of unsafe instructions
or establish their invariants. The project must treat this as a broad
third-party unsafe boundary, even though it is neither a C FFI dependency nor
project-authored unsafe code.

`Image::apply_orientation` is useful for explicit JPEG EXIF handling in the
earlier control, but it must be included in future allocation and geometry
tests. The API alone does not settle whether unsupported metadata, CMYK,
lossless/arithmetic JPEG, malformed Exif, or progressive behavior belongs in
the first public contract.

## PNG source findings

The reviewed `png` crate declares `MIT OR Apache-2.0`, Rust 1.73, and exposes
a `Decoder::new_with_limits` route plus `Limits { bytes }`. Its own `src/`
tree had no `unsafe` keyword hit in this source review. That observation does
not make the resolved PNG closure unsafe-free: its manifest unconditionally
requests `miniz_oxide` with `simd`, which resolves `simd-adler32` with `std`.
The latter contains runtime CPU detection, `target_feature` functions, and
unsafe SIMD implementations for x86 and other architectures.

The `png` byte limit is not a sufficient public allocation policy. A future
adapter must first validate the parsed dimensions against the project side and
pixel limits, check arithmetic and output-buffer requirements before allocating
its BGR output, reject or bound APNG/multiple-frame behavior, and use fallible
allocation at its own boundary. The prior 32,000,000-pixel external control
showed why independently passing per-buffer checks is insufficient: retaining
separate decoder output and BGR buffers exceeded the process budget before its
in-place/fallible-output revision. That finding remains external harness
evidence, not proof about every `png` allocation path.

The candidate control intentionally rejected 16-bit PNG in normal operation;
the one successful diagnostic conversion does not authorize silent
normalization. Palette, transparency, grayscale, alpha, ICC, eXIf, APNG, and
non-8-bit policy must be explicit in the eventual public input contract.

## Review outcome and decision consequences

The hybrid route is the strongest currently recorded *fidelity* candidate for
the self-authored 8-bit corpus, and it avoids a C build/native dynamic library.
It is not currently qualified for project adoption because its source closure
contains broad third-party unsafe/runtime-dispatch surfaces and its resource,
format, metadata, and notice policies have not received the required review.

| Possible next decision | Benefit | Consequence that must be accepted and recorded |
|---|---|---|
| Provisional hybrid direction: `libjpeg-turbo-rs` `std` only + `png` | The existing external control is byte-exact for fourteen 8-bit JPEG/PNG fixtures. | Accept the need for a focused unsafe/supply-chain review and a deliberately narrow first-input policy before adding dependencies. |
| `image` with JPEG/PNG only | Smaller facade API and explicit orientation support. | Its recorded full JPEG replay diverges from the fixture by up to 36 components; an intentional-difference/tolerance and model-impact proof would be required. |
| Native OpenCV/libjpeg route | Most direct alignment with the upstream OpenCV source path and a finite byte-exact JPEG signal. | Select and distribute a native library, introduce a reviewed FFI boundary, and prove platform/resource behavior. |

No selection is made by this review. Before `D-008` can close, the selected
route still needs the complete `IMG-DEC-001` acceptance set in
[`IMAGE_DECODER_EVIDENCE.md`](IMAGE_DECODER_EVIDENCE.md): exact supported and
unsupported format/metadata rules, end-to-end resource enforcement, legal
fixtures, malformed-input and fuzz coverage, model-preprocessing comparison,
and full dependency/license/notice/advisory review.

## Commands actually run

All commands ran outside PaddleOCR-Rust and outside the read-only upstream
checkout:

```sh
cd /tmp/paddleocr-rust-decoder-review.wRyiLL
CARGO_NET_OFFLINE=true cargo generate-lockfile
CARGO_NET_OFFLINE=true \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc \
  cargo fmt --check
CARGO_NET_OFFLINE=true \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc \
  cargo clippy --locked --all-targets -- -D warnings
CARGO_NET_OFFLINE=true \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc \
  cargo build --release --locked
CARGO_NET_OFFLINE=true cargo tree --locked -e features --prefix none
```

The source review used only the exact crate sources resolved by that lockfile.
No model was loaded, no OCR inference was performed, and no external artifact
was added to the repository.
