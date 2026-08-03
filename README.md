# PaddleOCR-Rust

PaddleOCR-Rust is an independent, native-Rust port in progress for selected,
useful PaddleOCR behavior. It is not an official PaddlePaddle or PaddleOCR
release, and it does not wrap Python, PaddleX, or the upstream project.

The project uses the upstream repository as a read-only behavioral reference
while designing an idiomatic, safe Rust library and CLI. Its public scope,
compatibility claims, and delivery order are governed by
[ROADMAP.md](ROADMAP.md).

## Status

`0.1.0` is an engineering bootstrap, not an OCR release. There is no supported
end-to-end OCR path yet.

| Area | Current state |
| --- | --- |
| Rust workspace and safety boundary | Present: stable Rust workspace, typed foundation values, structured errors, and `unsafe` forbidden. |
| Geometry and postprocessing foundations | In progress: private resize/geometry, reading-order, crop, DB-component, and CTC-greedy primitives have focused tests. |
| Model evidence | In progress: two exact `PP-OCRv6_medium` ONNX candidates have provenance and license evidence, but are not loaded or supported by this crate. |
| Image decoding | Not selected or implemented. |
| Inference runtime, detector, recognizer, API, and functional CLI | Not implemented. |

The `paddleocr-rust` binary intentionally exits with an unsupported-operation
message. Do not use this repository for OCR results until a capability is marked
`Verified` in [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## First delivery target

The first planned vertical slice is a classic single-image OCR flow on
`x86_64-unknown-linux-gnu`, with explicitly provisioned local
`PP-OCRv6_medium` detector and recognizer candidates. This is a roadmap target,
not a current feature or model-support claim. Runtime selection, image decoding,
artifact resolution, preprocessing, postprocessing, public API, and CLI
acceptance gates remain open.

The exact candidate identities and their current legal/provenance boundary are
recorded in [docs/MODEL_CANDIDATES.md](docs/MODEL_CANDIDATES.md),
[docs/CANDIDATE_PROVISIONING_LEDGER.md](docs/CANDIDATE_PROVISIONING_LEDGER.md),
and [docs/LICENSE_REVIEW.md](docs/LICENSE_REVIEW.md). Model weights are not
bundled, downloaded, or required by normal tests.

## Development

The bootstrap support profile is Rust `1.94.0` on
`x86_64-unknown-linux-gnu`. Normal development and test runs must not require
Python, PaddleOCR/PaddleX, a GPU, network access, or model weights.

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

At this stage, the binary documents its intentional limitation:

```sh
cargo run
# paddleocr-rust: OCR inference is not implemented yet; no model runtime or artifacts are available
```

`Cargo.lock` is version-controlled for reproducible workspace resolution. Add
or update it only as part of an intentional dependency change.

## Reference boundary

`PaddleOCR/` is a symbolic link to a local upstream checkout at the pinned
baseline recorded in [ROADMAP.md](ROADMAP.md). It is a read-only developer
reference only: this crate's build, tests, runtime, package, and CI must not
depend on that link or on Python. Consult it to understand observable behavior,
then preserve the relevant evidence in this repository using small,
redistributable fixtures.

The upstream project is substantially broader than this port's initial slice,
including document parsing, layouts, tables, formulas, training, services, and
deployment integrations. Those surfaces are not implied by this repository's
name; their classification and planned order are in
[docs/INVENTORY.md](docs/INVENTORY.md) and [ROADMAP.md](ROADMAP.md).

## Documentation and contributing

Start with these documents:

- [ROADMAP.md](ROADMAP.md) — canonical execution plan and acceptance criteria.
- [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) — the only place to check a compatibility claim.
- [docs/README.md](docs/README.md) — index of contracts, evidence, and design records.
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) — contributor workflow.
- [AGENTS.md](AGENTS.md) — repository boundaries, including the read-only upstream rule.

Every change must map to a roadmap item, retain the no-Python/no-upstream
runtime boundary, and include proportionate tests and documentation. See the
fixture and oracle records before changing compatibility behavior or numerical
expectations.

## License

Project-authored source code, documentation, and self-authored fixtures in this
repository are licensed under Apache-2.0 unless a file carries an explicit
third-party notice. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

This license does not grant rights to model weights, datasets, fonts,
dictionaries, converted artifacts, or other third-party materials. Those assets
remain excluded unless their separate provenance and license review is complete.
