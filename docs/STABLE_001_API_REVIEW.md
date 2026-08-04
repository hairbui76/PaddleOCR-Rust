# `STABLE-001` — Public Surface Stability Review

Roadmap item: `STABLE-001`
Reviewed: 2026-08-04
Version reviewed: `0.1.0`, unreleased

`STABLE-001` asks for a review of public API, schema, and CLI names, defaults,
ownership, errors, feature flags, backend neutrality, semver policy,
deprecations, and experimental surfaces. The review found one systemic problem
and fixed it; everything else is recorded as a position.

## 1. The finding: additive changes were breaking changes

Three public types grew during this milestone:

- `Error` gained `TimedOut` when `OCR-003` added the time policy.
- `OcrOptions` gained `control` in the same change.
- `result_to_json` gained two parameters, `id` and `model`.

Every one of those would have been a **breaking change** after `1.0`, because a
public enum without `#[non_exhaustive]` cannot gain a variant and a public struct
without it cannot gain a field. The API had no mechanism to grow, and the only
reason nothing broke is that nothing depends on it yet. That is luck, not design,
and it would have expired at the first release.

### Fixed

`#[non_exhaustive]` added to `Error`, `InputViolation`, `ModelProblem`,
`TextLine`, `OcrOptions`, `Artifacts`, `ModelManifest`, `ArtifactEntry`, and
`DictionaryEntry`. `Script` already had it, which is the precedent the rest now
follow.

For the two types callers construct, `#[non_exhaustive]` forbids struct literals,
so builders were added rather than leaving callers stuck:

```rust
let options = OcrOptions::default()
    .with_drop_score(0.6)
    .with_control(RunControl::unbounded().with_time_budget(budget));

let artifacts = Artifacts::new(library, detector, recognizer)
    .with_detector_sha256("eb13b44b…d086e1")
    .with_recognizer_sha256("9c09abf0…b673ba");
```

`Artifacts::new` takes exactly the three paths a run always needs, and its
documentation states what omitting the digests means rather than letting the
absence look like a default.

## 2. Names and defaults

| Aspect | Position |
|---|---|
| `recognize_png` | Names the format because that is the only one supported. If JPEG is ever accepted it will be `recognize_image`, and `recognize_png` will be kept as a deprecated alias rather than silently widened |
| `recognize_path` / `recognize_reader` | Named for the input shape, not the format, because the format is detected from content in both |
| `OcrOptions` defaults | The frozen upstream values — `0.6`, `1.5`, `0.5` — asserted by `tests/end_to_end.rs`, so a silent drift fails a test |
| `Dictionary::len` | Entries excluding blank and any appended space; `class_count` is the model-facing number. Two names because they are two different quantities and conflating them is how a class-count mismatch happens |
| `TextLine::score` | `score` in Rust, `confidence` in JSON. An inconsistency, deliberately kept: the JSON name is the frozen schema and the Rust name matches the internal `RecognizedLine`. Renaming either now would break one of two contracts to tidy a word |

## 3. Ownership

Inputs are borrowed (`&[u8]`, `&str`, `impl AsRef<Path>`); outputs are owned
(`Vec<TextLine>`, `String`). `OcrEngine` owns its sessions and dictionary — it
clones the dictionary at load, because the recognizer's output contract is built
from its class count and an engine whose dictionary could change underneath it
would validate against a contract that no longer describes it.

`OcrOptions` is passed by reference to `OcrEngine::recognize_*` since it now
carries a cancellation flag a caller usually keeps.

## 4. Errors

One enum, `Error`, with typed variants and no string-typed failures. `Io` carries
a `&'static str` operation name and the OS error, deliberately **not** the path,
so a library caller embedding this project does not leak a filesystem layout into
a structured error they might forward. The CLI prints the path itself.

`Cancelled` and `TimedOut { stage }` are separate because an explicit request
from the caller says more about why a run stopped than running out of time does.

## 5. Feature flags and backend neutrality

| Flag | Default | Meaning |
|---|---|---|
| `onnxruntime` | **off** | The only backend. Off by default means the crate has no native dependency and its tests run offline |
| `fuzzing` | off | Exposes `crate::fuzz` for an external driver. **Not covered by semver** |

Backend neutrality is structural, not aspirational: `InferenceBackend` is the
trait, no implementation surfaces a backend type through it, and no public item
mentions ONNX Runtime. `RT-004` records that replacing the backend costs one
trait implementation and no public type changes.

One asymmetry is worth naming: `Artifacts` and `OcrEngine` exist only with the
`onnxruntime` feature, so the public surface differs by configuration. That is
honest — without a backend there is nothing to load — but it means a caller
cannot write backend-agnostic code against this crate today.

## 6. Semver policy

For `0.x`, any release may break. From `1.0`:

- **Covered**: `api`, `error`, `types`, `control`, `manifest`, `script`, `input`,
  and the two schema constants `RESULT_SCHEMA_VERSION` and
  `MANIFEST_SCHEMA_VERSION`. A schema change gets a new version string, never a
  redefinition of an existing one.
- **Not covered**: `fuzz`, which exists for a fuzzer; `digest`, which is an
  implementation detail exposed because the adapter takes a digest by trait; and
  `result_json`'s function signature, which has grown twice and will grow again
  as the schema fills. The stable serialization contract is the **document
  shape** and the CLI's `--json`, not the Rust function that writes it.
- Adding a variant or field to a `#[non_exhaustive]` type is **minor**.
- Raising the minimum supported Rust version is **minor**, and will be recorded.
- The `onnxruntime` feature changing which backend it selects is **major**.

## 7. Deprecations and experimental surfaces

None yet — nothing has been released, so nothing can be deprecated. The policy
for when there is: a deprecated item keeps working for one minor cycle with
`#[deprecated]` naming the replacement, and is removed only in a major release.

No item is marked experimental. If one is needed, it will be gated behind a
feature named `unstable-*` rather than documented as unstable, because a
documentation-only warning is not a mechanism.

## 8. CLI

The CLI is a stable contract in its own right: flag names, the `score<TAB>text`
default output, the leading path column when more than one image is given, the
JSONL shape, and the exit codes. Changing any of them is a breaking change to a
scripting interface even when no Rust signature moves.

`--json` output is the schema-versioned document, which is what makes it the
recommended machine-readable surface rather than the text format.

## 9. What this review does not settle

- **`Score` and `Polygon` are public but unused by the OCR API**, which returns
  `f64` and `Quadrilateral`. They are foundation types from P1 that the classic
  path did not end up needing. Either the API should use them or they should not
  be public; that decision belongs with the first release, not with this review.
- **`digest` being public** is a wart, kept because removing it would require
  changing how `ModelArtifact` takes its hasher. Recorded rather than fixed.
- **Platform coverage.** `PLAT-001` is open, so the API is reviewed on one
  target only.
