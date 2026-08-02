# PaddleOCR-Rust Agent Guide

## Mission and scope

PaddleOCR-Rust is an independent, native-Rust port of PaddleOCR. Its purpose is
to reproduce selected, useful PaddleOCR behaviour with a safe and idiomatic Rust
API and CLI. It is **not** a Python wrapper, a build-time dependency on Python,
or an FFI shell around the upstream project.

Feature compatibility is incremental. Do not imply that an upstream model,
pipeline, option, output field, or deployment target is supported until it has a
tested Rust implementation. When choosing a new feature, prefer a thin vertical
slice over broad, unverified API scaffolding.

## Conversation and artifact language

The user may write prompts in English; the prompt language does not select the
response language. Unless the user explicitly requests another response language,
write all user-facing conversational prose in Vietnamese. This includes progress
updates, questions, explanations, review findings, summaries, and final handoffs.
A one-turn language request applies only to that turn unless the user explicitly
changes this standing policy.

Keep repository and machine-oriented artifacts in English. This includes source
code, identifiers, comments, doc comments, repository documentation,
configuration, test names, generated messages, CLI help and output, error
messages, commit messages, commands, and agent-authored logs. Deliberately
language-specific OCR fixtures and test data are exempt when required by the
capability under test.

Within a Vietnamese response, keep code, paths, commands, API names, schemas,
quoted errors, and raw command or log output in their original form. Explain
those artifacts in Vietnamese; do not translate or rewrite them.

## Upstream reference: read-only, always

`./PaddleOCR` is a symbolic link to `../PaddleOCR`, the upstream Python checkout.
It is a **read-only behavioural reference**, not part of this repository.

- Inspect it to understand public APIs, model metadata, preprocessing,
  postprocessing, error handling, fixtures, and expected output.
- Never edit, format, generate files in, install into, stage, commit, reset, or
  otherwise mutate `PaddleOCR/` or its target. Do not run a command from that
  directory that might write cache, lock, build, or test artifacts.
- Never make the Rust build, tests, runtime, or package installation depend on
  `../PaddleOCR` being present. The symlink is a developer convenience only.
- Avoid recursive write-capable tools rooted at `.` unless they explicitly
  exclude `PaddleOCR/`; a symlink-following formatter or cleanup command can
  alter the upstream checkout.
- For a compatibility claim, record the upstream commit or release that was
  inspected and preserve a small, redistributable fixture or expected-result
  fixture in this repository. Do not require a live upstream checkout in CI.

The upstream project is Apache-2.0, but model weights, datasets, fonts, and
third-party assets can have different terms. Keep source notices when adapting
non-trivial code or data, add the appropriate attribution/license material, and
verify the terms of every bundled asset. Do not use upstream branding in a way
that suggests an official PaddlePaddle release.

## How to port behaviour

Treat upstream code as an oracle for externally observable semantics, not as a
template to transliterate line by line. For each capability:

1. Define the Rust-facing contract: inputs, units and coordinate convention,
   defaults, ordering, errors, resource bounds, and ownership/lifetime.
2. Trace the corresponding upstream path, including configuration defaults and
   preprocessing/postprocessing—not only the model invocation.
3. Add focused golden or differential tests using legal, small fixtures. Test
   edge cases such as empty images, invalid data, rotations, non-ASCII text,
   threshold boundaries, and multi-page/order stability as relevant.
4. Implement the narrowest complete slice, documenting an intentional semantic
   difference at the public boundary.
5. Validate the actual output and numerical tolerances. Never hide a mismatch
   by silently changing geometry, score thresholds, tensor layouts, or text
   normalization.

Exact Python API parity is not required: favor idiomatic typed Rust APIs. Keep
compatibility explicit, however—especially JSON schemas, bounding-box order,
confidence values, language/model selection, image orientation, and CLI output.
An unsupported upstream feature must fail clearly or remain absent; it must not
quietly return fabricated or partial success.

## Architecture principles

- Keep public pipeline orchestration separate from image geometry,
  preprocessing, inference backends, model metadata/download resolution, and
  task-specific postprocessing. This makes each layer testable without a large
  model download.
- Make model inference backend-neutral. Do not commit the project to ONNX,
  Candle, TensorRT, OpenVINO, a remote service, or another runtime until that
  decision is explicitly made and tested. Backend-specific code must not leak
  through the public API.
- Preserve numerical and geometric invariants deliberately: record pixel
  coordinate origin/order, inclusive/exclusive bounds, resize/pad transforms,
  channel order, tensor layout/dtype, batch semantics, and inverse transforms.
- Keep model acquisition explicit and reproducible. Never commit large weights,
  credentials, caches, or generated build artifacts. Network access must be
  opt-in, checksummed where practical, and never required for normal unit tests.
- Image and document inputs are untrusted. Bound decoded dimensions, memory,
  recursion/page counts, and work; use secure temporary files; clean them up;
  and do not follow user-controlled paths or URLs without validation.
- Avoid `unsafe`. If a backend genuinely requires it, isolate it behind a small
  reviewed boundary with documented invariants and tests.

Do not prematurely create crates, traits, or feature flags. Introduce a boundary
only when an implemented responsibility needs one. Keep dependency choices in
the owning `Cargo.toml`, minimize default features, commit `Cargo.lock` for an
application/workspace, and use the stable Rust toolchain unless the repository
later pins a different one.

## Roadmap authority and governance

`ROADMAP.md` is the canonical execution plan for this port until it is formally
declared complete. All implementation, architecture, compatibility, packaging,
testing, documentation, and release work must follow its declared scope,
dependencies, decision gates, and acceptance criteria. Every repository change
must map to a roadmap item. Read-only investigation and user support may assist a
current item but must not silently change scope or status.

Read `ROADMAP.md` before beginning work. If a requested change is missing,
conflicts with the current plan, or depends on an unresolved decision, update
the roadmap before implementation or as the first part of the same change. A
direct user request may change roadmap scope or priority, but it does not bypass
the roadmap: record the change and its rationale. Independent items may proceed
in parallel only when their declared dependencies and decision gates permit it.

Treat the roadmap as a living plan, not as a substitute for verified evidence.
When it conflicts with Cargo metadata, implemented behaviour, tests, verified
upstream semantics, licensing, security constraints, or a direct user decision,
record the discrepancy and correct the roadmap rather than forcing the project
to match a stale plan. The roadmap never relaxes another repository policy,
especially the read-only upstream boundary.

Use roadmap statuses consistently:

- `Planned`: scoped but no implementation work has begun.
- `In progress`: implementation or validation has actually begun.
- `Blocked`: unfinished; record the blocker, its impact, and the condition or
  decision needed to resume.
- `Done`: all item acceptance criteria are satisfied, required code,
  documentation, and tests are present, and the validation actually run is
  recorded.
- `Deferred` or `Out of scope`: not completed; retain the item and record why it
  is excluded instead of presenting it as done.

Partial scaffolding, an unvalidated implementation, or an unrun required check
must not be marked `Done`. Routine status and validation-evidence updates do not
require a new product decision. Adding or removing a required deliverable,
changing the compatibility baseline, crossing an unresolved product decision,
or weakening the project Definition of Done is a scope change: surface it to
the user and record the resolution before proceeding.

The roadmap may be declared complete only when its explicit completion scope
and project-level Definition of Done are satisfied, every required item is
`Done`, final validation is recorded, and the targeted release scope has been
confirmed by the user. Roadmap completion must not be described as full
PaddleOCR parity unless that exact, versioned compatibility target was
explicitly declared and verified.

## Repository workflow

At the roadmap bootstrap on 2026-08-02, the repository has no Rust workspace.
Establish the workspace, package name, supported platforms, first model family,
runtime backend, and license through the decision gates in `ROADMAP.md`; do not
invent them merely because upstream has an equivalent Python component. Once
Cargo metadata exists, inspect it as the implementation source of truth and keep
the roadmap synchronized with it.

Before a non-trivial change:

1. Check the working tree and read the affected Rust code, tests, and public
   docs first.
2. Identify the upstream reference path and the intended compatibility level.
3. Keep the change scoped to one observable capability. Update its user-facing
   documentation and compatibility notes in the same change.
4. Run the narrowest relevant tests, then the applicable workspace gate. For a
   conventional Cargo workspace, that normally includes:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features
   ```

   Adjust features/targets only when the actual workspace defines a more
   appropriate gate; do not claim an unrun command passed.

For a new public surface, include tests for malformed input and resource/error
handling as well as successful output. Avoid tests that fetch models, depend on
GPU availability, or rely on the local upstream symlink unless they are clearly
marked as optional developer-only checks.

## Documentation and communication

- Follow the conversation/artifact language boundary above; do not translate
  technical artifacts merely because the user-facing explanation is Vietnamese.
- State decisions as decisions. Distinguish verified upstream behaviour from a
  Rust design choice and from an unimplemented aspiration.
- Report the changed surface, compatibility reference, validation actually run,
  and remaining limitations. Do not overstate parity or benchmark results.
- If a requested port needs a product decision (for example, model family,
  runtime, licensing/distribution, or public API compatibility level), surface
  the options before making an irreversible structural commitment.
