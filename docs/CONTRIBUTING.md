# Contributing

## Scope discipline

Read `../AGENTS.md`, `../ROADMAP.md`, `P0_DECISIONS.md`, `API_CONTRACT.md`, and
`COMPATIBILITY.md` before changing behavior. Every change must map to a roadmap
item and must not broaden a compatibility claim beyond its evidence.

`../PaddleOCR/` is a read-only symlink to an upstream Python reference. Never
run a formatter, generator, package installation, test suite, Git mutation, or
any other write-capable command through that path. The Rust project must build
and test when that symlink target is absent.

## Development gate

Use Rust `1.94.0` on `x86_64-unknown-linux-gnu` for the initial support target.
Run the following from the repository root before submitting a change:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo doc --locked --workspace --no-deps
```

If the local `cc` command is a non-compiler wrapper, invoke the validation
commands with a real system C compiler first in `PATH`; do not commit a
machine-specific linker path as a workaround.

## Fixtures and assets

Read `../tests/fixtures/README.md` and `FIXTURE_AND_TOLERANCE_PLAN.md` before
adding a fixture. Normal tests must not download models, use a GPU, require
Python, or execute upstream. Do not add weights, fonts, datasets, dictionaries,
converted artifacts, or unclear third-party files without provenance and
license review.

## Source and documentation

Write code, identifiers, comments, tests, configuration, and repository
documentation in English. Keep public behavior, model identity, constraints,
and known differences precise. Do not present the project as an official
PaddlePaddle or PaddleOCR release.
