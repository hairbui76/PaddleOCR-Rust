# ADR — PDF and office input (`DOCIO-DEC-001`, resolving the `D-008` document portion)

Roadmap item: `DOCIO-DEC-001`
Decided: 2026-08-04
Status: Accepted — **office rejected, PDF deferred behind a stated entry gate**
Depends on: Gate P4, `SCOPE-001`

`D-008` has two halves. The image half was resolved on 2026-08-04 in
[`IMAGE_DECODER_DECISION.md`](IMAGE_DECODER_DECISION.md): PNG only, with JPEG
deferred behind a measured entry gate. This resolves the document half — which
formats, which renderer, which native dependencies, which licences, what
password and metadata behaviour, and what page, work, and resource limits.

## Evidence status

This decision is made from library licensing and architecture, not from
measurement. Nothing here was benchmarked, and this document does not pretend
otherwise. That is acceptable because the decision it reaches is **not to
implement yet**: a deferral needs enough evidence to justify waiting, which is a
lower bar than a selection needs, and the entry gate below is written so the
selection cannot later be made on this much evidence.

Where a claim is about a licence or a dependency's nature, it is checkable and
stated as fact. Where it is about behaviour or performance, it is marked as
unmeasured.

**Updated 2026-08-05**: the entry gate in section 5 has now been measured, and
[`PDF_ENTRY_GATE_EVIDENCE.md`](PDF_ENTRY_GATE_EVIDENCE.md) records the result.
Parts 1, 2, and 3 pass — `hayro 0.4.0` is a pure-Rust, Apache-2.0 rasterizer, the
scanned-page path reproduces the reference renderer **bit-identically**, and the
tree contains no C or C++ at the cost of `32` new packages. Part 4 **fails on one
case**: a self-referential form XObject exhausts memory where the reference
renderer bounds it. Part 5 remains a user decision, with a recommendation
recorded. So the deferral stands, but it is no longer a deferral for lack of
evidence — the sentence above about this decision resting on licensing rather
than measurement is now true only of the original decision, not of its gate.

## 1. Office formats — rejected

**Decision.** `.docx`, `.xlsx`, `.pptx`, and their legacy binary predecessors
are **out of scope**, permanently rather than deferred.

**Why.** An office document is not an image source; it is a layout problem. To
produce pixels for OCR you must lay out text, resolve fonts, apply styles, and
render — which is a word processor, not a decoder. Any implementation would
either embed a rendering engine of that size or shell out to one, and both make
this project depend on something far larger than itself for a capability whose
users already have the text without OCR.

**The asymmetry that decides it.** For an office file, the text is *already
there*. Running OCR over a rendering of a `.docx` is a strictly worse way of
getting text that could be read directly. The only case where it is not worse is
a scanned image embedded inside the document, and that image can be extracted
without rendering anything.

**Consequence.** A caller with office documents should extract embedded images
and pass those. This project will not add a format for a job it would do badly.

## 2. PDF — deferred, not rejected

**Decision.** PDF is in scope and is **not implemented yet**. `PDF-001` remains
open, blocked on the entry gate in §5.

**Why in scope.** Unlike office formats, a PDF frequently *is* a scan: pixels
with no text layer, which is exactly the input this project exists for. It is
also the format users most reliably have.

**Why deferred.** Every candidate available today fails at least one requirement
this project has already committed to, and the failures are structural rather
than a matter of maturity.

## 3. The candidate landscape

| Candidate | Nature | Licence | Blocking issue |
|---|---|---|---|
| `pdfium` | C++, via bindings | BSD-3-Clause | A large C++ dependency with its own build system, needing the same hermetic-rebuild and SBOM treatment as gate `G2` — which is still open for one much smaller library |
| MuPDF | C, via bindings | AGPL-3.0 or commercial | AGPL is incompatible with this project's Apache-2.0 distribution without a commercial licence per downstream user |
| Poppler | C++, via bindings | GPL-2.0 / GPL-3.0 | Same incompatibility as MuPDF |
| `pdf-render` / `pdfium-render` and pure-Rust renderers | Rust | permissive | Rendering completeness is unmeasured here, and completeness is the whole question: a renderer that silently drops a shading, a form XObject, or a CID font produces a *plausible wrong image*, which is the failure mode this project treats as worse than an error |
| Text extraction only, no rendering | Rust | permissive | Does not solve the problem. A scanned PDF has no text layer, and a PDF that does have one does not need OCR |

Licences are facts and are stated as such. Rendering completeness is unmeasured
and is stated as unmeasured.

**The recurring shape.** The two mature renderers are copyleft in a way that
conflicts with the distribution position already recorded in
[`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md).
The permissively licensed mature one is a C++ subsystem larger than this entire
project. The pure-Rust ones are the right shape and unproven at the only thing
that matters.

## 4. The policy any implementation must satisfy

Recording this now is the point of deciding early: it constrains the candidate
rather than being written to fit whichever candidate wins.

### Formats and features

- PDF only. No PostScript, no office, no XPS.
- Encrypted PDFs: a password may be supplied explicitly by the caller, exactly
  as artifacts are. No password guessing, no empty-password retry loop, no
  reading a password from the environment.
- A PDF whose encryption cannot be handled is a typed `Unsupported` error, never
  a partial render.
- Embedded JavaScript, embedded files, external streams, and remote references
  are **never executed, extracted, or fetched**. A PDF is a document, not a
  program, and this project will not treat it as one.

### Resource limits, all checked before allocation

| Dimension | Bound |
|---|---|
| Encoded document | at most `64 MiB`, the existing input bound |
| Pages per document | at most `1,000`, matching the existing work-unit budget |
| Rendered page pixels | the existing `40,000,000` pixel and `16,384` side limits, per page |
| Render DPI | caller-supplied, bounded so DPI times page size cannot exceed the pixel limit |
| Total rendered bytes | bounded across the document, not only per page |
| Time | the existing `RunControl` budget, checked at page boundaries |

A page that would exceed a limit is a typed error identifying the page. Pages are
independent, so this is the first place where whole-input failure semantics may
be reconsidered — and that reconsideration belongs to `MPAGE-001`, which owns
partial-failure semantics, not here.

### Determinism

The same document, DPI, and page range must produce byte-identical rendered
pixels across runs. A renderer whose output depends on system fonts, locale, or
time fails this, and font substitution is the most likely way to fail it.

## 5. Entry gate for `PDF-001`

PDF implementation may begin when **all** of the following are recorded, and not
before. This mirrors how `IMG-003` gates JPEG on a measurement rather than on
enthusiasm.

1. **A named candidate with a licence compatible with Apache-2.0 distribution.**
   Not "we could dual-licence" — compatible as-is.
2. **A rendering-fidelity measurement** against a small committed corpus that
   includes a scanned page, a vector page, an embedded CID font, and a page with
   a form XObject, comparing against a reference renderer, with the maximum
   per-component pixel difference recorded — the same shape of evidence
   `IMG-003` requires for JPEG.
3. **A supply-chain position** at least as strong as gate `G2` demands, if the
   candidate is not pure Rust.
4. **A malformed-input corpus** exercised without panic, unbounded allocation,
   or execution of anything embedded in the document.
5. **A recorded decision on partial-page failure**, since a document is the
   first input where "some of it worked" is meaningful.

Failing any of these means PDF stays deferred. A deferred capability that
returns a typed `Unsupported` error is strictly better than one that returns a
plausible wrong page.

## 6. What this unblocks and what it does not

- `PDF-001` is now blocked on a stated gate rather than on an unmade decision.
- `MPAGE-001`, `INPUT-001`, and `DOC-E2E-001` depend on `PDF-001` and stay
  blocked. `INPUT-001`'s non-PDF portion — bytes, path, and stream inputs — does
  not depend on it and may proceed.
- Office formats are closed. Reopening them requires a new decision with a new
  rationale, not an appeal to this one.

## 7. Reversal

This decision is reversed if any of the following becomes true:

1. A permissively licensed, pure-Rust PDF renderer publishes fidelity evidence
   of the kind §5 requires, removing the reason to wait.
2. A licensing review concludes that a copyleft renderer can be used in this
   project's distribution model without imposing terms downstream.
3. The project's distribution position changes such that a large C++ dependency
   is acceptable — which would also change gate `G2`'s cost, and should be
   decided there rather than here.

None holds today.
