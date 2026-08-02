# PaddleOCR-Rust

PaddleOCR-Rust is an independent native-Rust port of selected PaddleOCR
behavior. It is not an official PaddlePaddle or PaddleOCR release.

The project targets the complete pinned PaddleOCR baseline described in
`ROADMAP.md`. Its first delivery milestone is a carefully verified classic OCR
slice for explicitly provisioned `PP-OCRv6_medium` artifacts on
`x86_64-unknown-linux-gnu`; it does not yet implement OCR inference.

## Current status

The Rust workspace has been bootstrapped. No inference runtime, model artifact,
model download, Python wrapper, PaddleX dependency, or FFI integration exists.
Do not rely on this repository for OCR results until an implementation is
explicitly documented as verified in `COMPATIBILITY.md`.

## Development

The supported bootstrap toolchain is Rust `1.94.0` on
`x86_64-unknown-linux-gnu`.

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

Normal development and tests must not require Python, PaddleOCR/PaddleX, a GPU,
network access, or model weights. Model files are intentionally local-only and
are ignored by Git.

`Cargo.lock` is version-controlled for reproducible application/workspace
resolution. Add or update it only through an intentional dependency change.

## Reference and scope

`PaddleOCR/` is a read-only symlink to an upstream reference checkout. It is
never a build, test, runtime, package, or CI dependency. Read
`AGENTS.md`, `ROADMAP.md`, `P0_DECISIONS.md`, `API_CONTRACT.md`,
`COMPATIBILITY.md`, and (for model work) `MODEL_CANDIDATES.md` plus
`CANDIDATE_PROVISIONING_LEDGER.md`; use `M2_CONTRACT_COVERAGE.md` to check the
contract/start gate for an M2 Must surface before changing capability scope or
making compatibility claims. Decoder selection and input-limit research is
recorded separately in `IMAGE_DECODER_EVIDENCE.md`; it does not indicate that
image decoding is implemented or selected.

## License

This repository is licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
Weights, datasets, fonts, dictionaries, and third-party artifacts require
separate provenance and license review before use or distribution.
