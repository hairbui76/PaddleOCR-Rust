# `DEPLOY-DEC-001` evidence — what each deployment target actually costs

Roadmap item: `DEPLOY-DEC-001`, resolving `D-012`
Recorded: 2026-08-05
Status: **evidence, not a decision.** `D-012` is a scope decision and belongs to
the user.

This document does for `D-012` what `docs/IMG_003_DELTA_MEASUREMENT.md` does for
`IMG-003` and `docs/P8_ARTIFACT_AVAILABILITY.md` does for P8: it measures what is
actually true, so the decision is made against facts rather than impressions.
Nothing here classifies a target as in or out of scope.

## 1. What was measured

Every number below is from a command run on 2026-08-05 against this commit, not
an estimate.

These are also **checked continuously**. `tools/gate.sh` runs
`cargo check --target` for both cross targets on every gate run, so the
portability below is a standing property rather than a measurement that was true
once. Each check is skipped when its target is not installed, so a clean checkout
with only the host target still passes.

| Fact | Value |
|---|---|
| Release CLI binary | `361,288` bytes |
| Dependencies, default features | `8` crates: `png` and its seven transitive |
| Dependencies, `onnxruntime` | `16` entries in the tree |
| `x86_64-pc-windows-msvc`, default features | **type-checks** |
| `wasm32-unknown-unknown`, default features | **type-checks** |
| `wasm32-unknown-unknown`, `onnxruntime` | **fails to compile** |
| Public items | `75` fn, `60` const, `39` struct, `18` mod, `14` enum, `2` type |

## 2. WASM is nearer than it looks, and blocked on one thing

`cargo check --target wasm32-unknown-unknown` **succeeds** with default
features. Everything this port does without a model — PNG decoding, geometry, DB
postprocessing, CTC decoding, table composition, reading order, Markdown
formatting, evaluation metrics, manifest resolution, observability — is already
WASM-portable, and nothing had to be done to make it so.

With `--features onnxruntime` it fails, and the failures name the cause exactly:

```
error[E0425]: cannot find function `init_from` in crate `ort`
error[E0599]: no method named `commit_from_file` found for `SessionBuilder`
error[E0599]: no method named `run` found for `RefMut<'_, Session>`
```

`ort`'s `load-dynamic` feature — chosen in `docs/ADR_RT004_RUNTIME_SELECTION.md`
precisely so that nothing is downloaded or linked at build time — has no
`wasm32` implementation. There is no dynamic library to load in a browser.

So the WASM question is **not** "is this crate portable". It is: *is there an
acceptable second runtime path for WASM*, which would mean a second backend to
verify against the same oracles, and `MODEL-DEC-001`'s reasoning about
conversion steps applies to backends too.

## 3. Windows type-checks, and that is not the same as working

`cargo check --target x86_64-pc-windows-msvc` succeeds with default features.
That establishes there is no Unix-only code in the pure-Rust core, and the gate
now keeps it true: introducing a single `std::os::unix` call was verified to fail
both cross checks and the gate's exit code.

It establishes nothing else. `PLAT-001` asks for tests **run** on each promised
platform, and a type-check is not a test run — the ONNX Runtime library loading,
the file paths, and the thread configuration are all untested there. This is
recorded as a starting point, not as evidence for a Windows claim.

## 4. The C ABI question is about `String`, not about effort

The public surface is `75` functions and `39` structs. The obstacle to a C ABI
is not its size; it is what those types carry:

| Type crossing the boundary | Count |
|---|---|
| `String` | `13` |
| `PathBuf` | `3` |
| `Vec<String>` | `2` |
| `Vec<TextLine>`, `Vec<TableBox>` | `1` each |

Every one needs an ownership decision — who allocates, who frees, what happens
to a partially consumed result — and `docs/STABLE_001_API_REVIEW.md`'s
`#[non_exhaustive]` growth mechanism does not survive a C header. A C ABI is
therefore a **second API to keep correct**, not a projection of the first.

The recognized text is the awkward one: it is UTF-8 of arbitrary length, and the
dictionary contract preserves exact scalars. A C API that returns it must either
allocate or make the caller size a buffer twice.

## 5. A small binary is a real asset here

`361 KB` for the whole CLI, and `8` dependencies at default features. That is a
consequence of decisions already recorded — no regex crate, no serialisation
framework, hand-rolled JSON, hand-rolled SHA-256 — and it is what makes the
container and mobile targets plausible at all.

It is also the thing most easily lost. Each of `SERVER-001`, `CLIENT-001`,
`ECO-001`, and `OBS-001`'s eventual logging backend brings a dependency tree
larger than this crate's, and the trade should be made deliberately rather than
arrived at.

## 6. What this does not decide

Every one of the eight targets `D-012` names — cloud client, local service, C
ABI, WASM, mobile, containers, accelerators, ecosystem integrations. No
recommendation is offered, because the choice is about what this project is for,
and that is not a technical question.

What the measurements do say is that the targets are **not equally distant**:

| Target | Nearest blocker |
|---|---|
| Containers | none measured; the binary is small and static-friendly |
| WASM (no inference) | none — it compiles today |
| WASM (with inference) | `ort` has no `wasm32` backend |
| Windows | untested at runtime; `PLAT-001` |
| C ABI | `13` `String`s and `3` `PathBuf`s need an ownership contract |
| Mobile | a C ABI first, or a target-native decision |
| Accelerators | hardware this project does not have |
| Local service, cloud client, ecosystem | dependency weight against a `361 KB` binary |
