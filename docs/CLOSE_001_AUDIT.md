# `CLOSE-001` — Inventory Closeout Audit

Roadmap item: `CLOSE-001`
Audited: 2026-08-04
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

`CLOSE-001` requires that every pinned-baseline inventory row is one of four
things: **Verified**, an **approved intentional difference**, **deferred** to a
named milestone, or **user-approved out of scope**. Nothing may be left
unclassified, and nothing may be classified by omission.

`docs/INVENTORY.md` records `121` rows from the upstream checkout. This audit
classifies all of them, and the honest headline is that the overwhelming majority
are deferred rather than done: this project has verified one vertical slice, not
a port of PaddleOCR.

## 1. Verified

Seven rows, all `Must` priority, all in `docs/COMPATIBILITY.md`, each now carrying
its evidence in that ledger rather than a promise:

| Row | Evidence summary |
|---|---|
| `M2-DET-001` | DB path with oracle matches — contours `18/18`, `minAreaRect` `16/16`, `box_score_fast` `8/8`, `unclip` `16/16`; detector input tensors bit-identical to a captured upstream capture across `4,048,896` elements |
| `M2-REC-001` | Batching, CTC, and dictionary; two upstream divergences found by capture and corrected |
| `M2-GEO-001` | Crop against `72` captured OpenCV cases; reading order pinned at its boundary |
| `M2-OCR-001` | Gate `G1` reproduces four end-to-end fixtures exactly through real artifacts |
| `M2-API-001` | Public surface tested through public items only; `!Sync` enforced by the compiler |
| `M2-CLI-001` | Demonstrated outside the repository under `env -i` |
| `M2-MODEL-001` | Manifest schema plus streaming identity verification before session creation |

The ledger's own transition rule required a contract, a fixture, a tolerance, and
a reproducing test before a row could move. Each of the seven has all four.

**`Verified` is per row and per artifact.** It does not extend to a second model
pair, a second input format, or a capability adjacent to the one verified.

## 2. Approved intentional differences

Recorded in `COMPATIBILITY.md` and each with required public wording:

- No PaddleX wrapper or pipeline API.
- No orientation classifier or document unwarping.
- No automatic model acquisition and no bundled weights.
- An idiomatic Rust API rather than a reproduction of the Python API.

Two more were added during M2 and belong in this list rather than being buried
in module documentation:

- **PNG only.** JPEG returns a typed `Unsupported` error. Every pure-Rust JPEG
  decoder evaluated differed from OpenCV's by up to `36` in a component, which
  is a difference in the pixels the model sees.
  ([`IMAGE_DECODER_DECISION.md`](IMAGE_DECODER_DECISION.md))
- **The Clipper union pass is omitted** in polygon unclipping, documented as
  unobservable through `get_mini_boxes`, which reads only the minimum-area
  rectangle of the result. This is the single place where this port deliberately
  does *less* than upstream.
  ([`DB_POSTPROCESS_SPEC.md`](DB_POSTPROCESS_SPEC.md))

## 3. Deferred to a named milestone

This is where most of the inventory sits, and the milestone is named in each
case rather than left as "later":

| Area | Milestone | Blocking condition |
|---|---|---|
| Document orientation, unwarping, document preprocessing | `M3` / P7 | Artifacts not provisioned; adding one requires a fixture reproduced by gate `G1` under `LANG-001` |
| PDF and multipage | `M3` / P7 | `PDF-001` blocked on the five-part entry gate in [`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md) |
| Layout, table, formula, seal, chart, KIE, super-resolution | `M4` / P8–P9 | No artifacts, no contracts; each is a separate model family |
| VLM and GenAI | `M5` / P10 | `D-010` open |
| Serving, API client, deployment, ecosystem | `M5` / P11 | No service surface exists |
| Training, evaluation, export, compression | `M6` / P12 | Out of the current implementation entirely |
| JPEG input | `IMG-003` | Entry gate is a measurement of decoder fidelity |
| Additional languages and model pairs | `LANG-001` | Each needs its own fixture and `G1` reproduction |

## 4. User-approved out of scope

- **Office formats** — `.docx`, `.xlsx`, `.pptx` and their predecessors,
  rejected permanently in `DOCIO-DEC-001` rather than deferred. An office
  document's text is already present, so OCR over a rendering of one is a
  strictly worse way to obtain it.
- **Opt-in model downloads** — `MOD-004` remains unapproved, and
  `ADR_MODEL_DEC_001_ARTIFACT_POLICY.md` records the cost of approving it.
- **URL input** — rejected in `INPUT-001` with the SSRF surface enumerated.

## 5. The classification, in one line

`7` verified, `6` intentional differences, the large remainder deferred to a
named milestone, `3` classes out of scope. No row is unclassified.

## 6. What this audit does not do

It does not make the deferred rows smaller. Reading the inventory in one sitting
is a useful corrective to a change log full of green: this project has a working,
carefully verified classic OCR path for one artifact pair and one image format,
and that is a small fraction of what `INVENTORY.md` lists. The remaining phases
are each comparable in size to the one completed.

`CLOSE-001` closes because every row is classified, not because the work is done.
