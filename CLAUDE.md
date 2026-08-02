# CLAUDE.md

## Project context

This repository will become a native-Rust implementation of selected PaddleOCR
capabilities. The `PaddleOCR` entry at the repository root is a symlink to the
original Python project in `../PaddleOCR`. It is provided only as a read-only
reference oracle.

Read [AGENTS.md](AGENTS.md) before working here; it is the canonical engineering
policy for this repository.

## Language policy

Communicate with the user in Vietnamese by default, even when the user prompts
in English. Use another response language only when explicitly requested. Keep
code, identifiers, comments, documentation, commands, generated messages, and
logs in English. Preserve raw technical output verbatim and explain it in
Vietnamese. See `AGENTS.md` for the authoritative policy and exceptions.

## Roadmap discipline

Read [ROADMAP.md](ROADMAP.md) before work that changes this repository. Until
that roadmap is formally complete, every change must map to a roadmap item and
respect its dependencies, decision gates, and acceptance criteria. If a request
is absent from or conflicts with the roadmap, update the roadmap and record the
reason before implementing it; do not silently bypass or reinterpret the plan.

Keep item status and validation evidence current in the same change. Mark an
item `Done` only after all of its acceptance criteria are met and the required
checks actually run. Material scope, compatibility-target, or Definition-of-Done
changes require a recorded user decision. `AGENTS.md` remains authoritative if
this summary and the roadmap ever disagree.

## Non-negotiable boundary

Do not change anything through `PaddleOCR/`. In particular, do not run
formatters, generators, test suites, package installs, `git` mutations, or
cleanup commands against it. Do not make this project's build or runtime depend
on that symlink—the repository must stand alone after the port is built.

## Working approach

- Port observable behaviour in narrow, tested slices rather than copying the
  Python architecture or creating placeholder support for all features.
- Consult the upstream implementation for the complete data path: defaults,
  preprocessing, tensor/image conventions, postprocessing, errors, and output
  order all matter as much as inference.
- Keep Rust APIs idiomatic and strongly typed. Document any intentional
  incompatibility and reject unsupported features clearly.
- Treat image and document data as hostile input. Add bounds and error-path
  tests; avoid unbounded memory use, automatic network downloads, and hidden
  Python fallbacks.
- Keep models, caches, credentials, and generated artifacts out of version
  control. Verify the separate licenses of model weights and datasets before
  bundling them.

## Validation

The Cargo workspace now exists. Use its `Cargo.toml`, locked toolchain, and
the root [ROADMAP.md](ROADMAP.md) as the source of truth for the current crate
layout and validation commands. Supporting contracts and evidence are indexed
in [docs/README.md](docs/README.md). The normal Rust quality baseline is
formatting, warning-free Clippy, and targeted plus workspace tests. Tests must
run without the upstream checkout, network access, a GPU, or large model
downloads unless explicitly marked as optional integration tests.

For every compatibility claim, preserve a small legal fixture or expected result
in this repo and identify the upstream revision/release used as the reference.
