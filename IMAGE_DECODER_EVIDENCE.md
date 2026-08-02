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

## Decision options and current recommendation

| Option | Potential benefit | Evidence still required | Current disposition |
|---|---|---|---|
| Evaluate `image` with only `jpeg` and `png` features | Small explicit format surface, documented orientation API, and no intentional OpenCV/FFI commitment. | Exact dependency/supply-chain review; resource-limit spike; BGR/alpha/EXIF oracle comparison; malformed-input tests; MSRV and binary measurements. | First candidate to evaluate after gates; not selected. |
| Bind to an OpenCV-compatible native decoder | Could reduce a source-runtime difference for the classic path. | Exact library/version/distribution terms, FFI/unsafe audit, resource controls, CPU portability, and proof that it improves M2 oracle fidelity enough to justify the boundary. | Not evaluated; no dependency or implementation is authorized. |
| Evaluate another pure-Rust decoder | May offer different performance or allocation properties. | Equivalent public API, format, metadata, limits, safety, license, MSRV, and upstream-oracle evidence. | No candidate is currently accepted. |

Subject to the unresolved gates, the evidence supports evaluating the minimal
`image` feature configuration first. That is a prioritisation for a future
spike, not `D-008` resolution: exact behaviour and resource safety are more
important than the library name.

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
- [`image::Limits` 0.25.10](https://docs.rs/image/0.25.10/image/struct.Limits.html)
- [OpenCV image-codec documentation, version 4.5.5](https://docs.opencv.org/4.5.5/d4/da8/group__imgcodecs.html)

The external documentation informs a candidate evaluation only. It does not
replace version-recorded upstream oracle evidence or a reproducible
dependency/supply-chain review.
