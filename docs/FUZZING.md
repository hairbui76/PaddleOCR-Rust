# Bounded Primitive Fuzzing

Roadmap item: `FUZZ-001`; status: In progress.

## Purpose and boundary

`src/fuzz.rs` provides a developer-only, byte-driven entry point for the
current checked pure kernels. The `fuzz-primitives` binary reads one input
from standard input and can be driven by a compatible external mutator or
fuzzing harness.

This is deliberately not an OCR, model, decoder, or image-input fuzz target.
It does not load a model, decode an image, access the network, execute
Python, inspect the `PaddleOCR/` symlink, or serialize a result. The feature
is disabled by default and introduces no dependency.

## Resource bounds

The driver consumes at most `16 KiB` per invocation. It derives every
secondary allocation from fixed small limits:

- DB probability maps are at most `32 x 32`.
- CTC score matrices have at most 32 time steps and 32 classes.
- Geometry inputs use at most `32 x 32` image dimensions.
- Crop sources are at most `16 x 16` pixels with one through four channels.

The same input also reaches checked public constructors for encoded-image
size validation, image dimensions, points, scores, model identities, and
image transforms. Invalid shapes, non-finite floats, zero/large dimensions,
and malformed identity bytes are expected to produce typed errors rather
than a panic.

## Current coverage

The target exercises the currently implemented pure surfaces:

- checked public domain types and coordinate transforms;
- DB thresholding and bounded connected-component scanning;
- CTC matrix validation and greedy-index decoding;
- resize, quadrilateral ordering/clipping/rescaling, polygon metrics,
  minimum-area candidates, and crop-coordinate transforms;
- bounded perspective crops over generated interleaved pixel buffers.

The deterministic seed test is a smoke/regression check, not a fuzzing
campaign. It exercises empty input, 128 fixed byte streams, and an input
larger than the `16 KiB` consumption limit.

## Running it

Run the deterministic test:

```sh
cargo test --locked --features fuzzing fuzz::tests::byte_driven_fuzz_driver_handles_bounded_seed_corpus
```

Run one stdin-oriented target invocation:

```sh
cargo run --locked --features fuzzing --bin fuzz-primitives < corpus-input
```

A mutator or fuzzing engine is intentionally not bundled or installed by
this repository. Any future engine integration must preserve the feature
gate, fixed input/resource limits, offline normal tests, and the boundary
that native decoder fuzzing uses a separately reviewed malformed corpus.

## Limitations and next work

`FUZZ-001` is not complete. Manifest/config/schema/document parsers and
native decoder boundaries do not exist in the Rust implementation yet, so
they are not covered. No randomized campaign duration, corpus coverage
metric, crash-free claim, or decoder/runtime/model safety claim is made by
this target.
