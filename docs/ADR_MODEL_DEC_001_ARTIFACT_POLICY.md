# ADR — Model artifact policy (`MODEL-DEC-001`, resolving `D-007`)

Roadmap item: `MODEL-DEC-001`
Decided: 2026-08-04
Status: Accepted
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

`D-007` asks for the exact artifact, conversion, distribution, cache, offline,
integrity, provenance, and licensing policy. Most of this policy was already
implemented — every artifact in this repository is an explicit local path with an
optional verified digest — but it was implemented as a series of local decisions
rather than recorded as one. This document is that record, and it exists now
because `MOD-002` must encode the policy in a manifest schema, and a schema
that encodes an unwritten policy freezes whatever the author happened to assume.

Where the decision merely confirms what the code does, this says so. Where it
decides something the code has not yet had to face, it says that too.

## 1. Artifact and conversion

**Decision.** The M2 artifacts are the official ONNX exports of
`PP-OCRv6_medium`, consumed as published. This project performs **no model
conversion** — no Paddle-to-ONNX step, no quantization, no graph rewriting, no
operator fusion of its own.

**Why.** A conversion step is a second implementation to verify. The comparison
this project actually makes is against upstream *inference*, and inserting a
conversion the upstream pipeline does not perform would mean any divergence has
two possible causes. Consuming the published export keeps the difference between
this port and upstream located entirely in this port's own code.

**Consequence.** If a needed model has no published ONNX export, that model is
out of scope until either an export exists or a conversion policy is written with
its own verification. "Convert it locally and hope" is not available.

The pinned exports, with the digests this project verifies:

| Artifact | Source revision | SHA-256 | Bytes |
|---|---|---|---|
| detector | `61323801669c338b7891481ec7bac61ce31b576a` | `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` | `62,032,837` |
| recognizer | `50c7eacafc52fa7bcf4194e8cd08e46f8558504b` | `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` | `76,554,979` |
| dictionary | upstream `ppocr/utils/dict/ppocrv6_dict.txt` | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` | `18,708` entries |

## 2. Distribution

**Decision.** This project distributes **no model bytes**. Model artifacts are
`0` bytes of any package it produces, and `/models/` is ignored by version
control. A manifest may *record* a URL; nothing in this project fetches it.

**Why.** Redistributing weights makes this project a licensor of them, which
requires a licensing position on every artifact for every downstream user. It
also makes the package tens of megabytes for a capability most builds do not use,
and turns a model update into a release of this project.

**Consequence.** Every user provisions artifacts themselves. That is friction,
and it is the friction that keeps the licensing question with the party who
actually chose the model.

## 3. Downloads, cache, and offline behaviour

**Decision.** No downloads. No cache. No search path. No environment variable
naming an artifact. An artifact is used if and only if the caller named its path.

**Why.** Each of these is a way for a run to depend on state the caller did not
state. A cache means two identical commands can use different bytes; a search
path means an artifact appearing elsewhere on the machine changes behaviour; an
environment variable means the result depends on the shell. The property this
buys is that a command line plus the named files fully determines the output —
which is also what makes the digest check meaningful, since there is nothing else
the run could have loaded.

**Consequence.** "Offline mode" is not a mode. There is no online mode to switch
off. `MOD-004` remains available if opt-in downloads are ever wanted, and it
would have to carry its own trusted-host, redirect, size, time, checksum, atomic
write, and cache-locking policy — which is the cost of the feature, stated up
front.

## 4. Integrity

**Decision.** Integrity is by SHA-256 over the exact artifact file, streamed
before the model is handed to the runtime, under a `512 MiB` read bound, with the
file type and size checked before any read. Declaring the digest is **optional**
and its absence is recorded rather than silently defaulted.

**Why optional.** A mandatory digest would make first use impossible without
first obtaining a digest from somewhere, and the obvious somewhere is the file
you are about to trust. Optionality is honest about what the check is: a
confirmation that the file is the one you decided on, not a proof it is good.

**Why it matters more than it looks.** The detector and recognizer export the
same tensor names and leave the axes this port constrains dynamic. Passing them
in the wrong order **loads without complaint** and fails only on first use, and
`tests/end_to_end.rs` pins that behaviour rather than wishing otherwise. Shape
does not distinguish two models; identity does. So the digest is the only
mechanism that catches a swapped, substituted, or truncated artifact before the
runtime sees it.

**Consequence.** Any recommended configuration declares digests. The user guide
states this where a user decides whether to bother.

## 5. Provenance

**Decision.** An artifact is identified by its source repository, its exact
revision, its SHA-256, and its byte count — all four, recorded together. A
version name alone is not provenance.

**Why.** "PP-OCRv6_medium" names a family that has been republished at several
revisions. `docs/MODEL_CANDIDATES.md` already records the revision-pinned
sources, and the end-to-end fixtures carry the same digests, which is what makes
a fixture's claim checkable years later.

**Consequence.** The `MOD-002` manifest carries all four fields per artifact, and
a manifest that omits the revision is incomplete rather than merely terse.

## 6. Licensing

**Decision.** Model weights and the dictionary are licensed separately from this
project's code, and that separation is preserved by not bundling them. The terms
review for the pinned candidates is recorded under `LIC-001`; every committed
fixture that names an artifact also names that review.

**Why.** The licence of a model is not the licence of the code that loads it, and
the two are routinely conflated by projects that vendor weights. Not shipping
them makes the distinction structural rather than documentary.

**Consequence.** A future artifact whose terms have not been reviewed cannot
appear in a committed fixture, because the fixture metadata has a field for the
review and the integrity gate checks it.

## 7. What this does not decide

- **Any second artifact pair.** This policy governs how artifacts are handled,
  not which are supported. `LANG-001` governs that, and it currently records one
  verified mapping.
- **Opt-in downloads.** `MOD-004` is unchanged and remains unapproved.
- **The runtime library.** `libonnxruntime.so` is provisioned by the same
  explicit-path rule, but its supply-chain position is gate `G2` in
  [`ADR_RT004_RUNTIME_SELECTION.md`](ADR_RT004_RUNTIME_SELECTION.md) and is
  still open.
- **Quantized or accelerated variants.** They would be different artifacts with
  different digests and would need their own verification, not an exemption from
  this one.

## 8. Reversal

This policy is reversed if any of the following becomes true, and each is
observable rather than a matter of taste:

1. A supported capability requires an artifact with no published export, making
   §1 a blocker rather than a simplification.
2. Provisioning friction is shown to cause users to disable the digest check,
   making §4's optionality a net loss.
3. A licensing review concludes that recording a URL in a manifest constitutes
   distribution, which would make §2's "record but never fetch" untenable.

None of the three holds today.
