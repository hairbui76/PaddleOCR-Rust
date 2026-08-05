# `PDF-001` Entry Gate: Measured Evidence

Roadmap item: `PDF-001`
Gate: [`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md)
section 5
Measured on: 2026-08-05
Corpus: `tests/fixtures/classic-v1-pdf-entry-gate`

## What this document is

`DOCIO-DEC-001` put PDF in scope and deferred it behind a five-part entry gate,
explicitly modelled on how `IMG-003` gated JPEG: *measure the thing, then decide*.
This is that measurement. It resolves four of the five parts with numbers and
leaves the fifth — the partial-page failure policy — as a decision with a
recommendation, because a policy is the user's to set.

**It is not an approval to implement, and no dependency was added.** Every
candidate-renderer number here was produced by a disposable harness built
outside the repository, the same way the static/Paddle raw-tensor reference was
taken. `Cargo.toml` is untouched.

## Summary

| Gate part | Verdict |
|---|---|
| 1. Named candidate, licence compatible as-is | **Pass** — `hayro 0.4.0`, Apache-2.0, pure Rust |
| 2. Rendering-fidelity measurement | **Pass, with the divergence located** — the scan path is bit-identical; every divergence is antialiasing or font substitution, and the worst page still yields character-identical OCR |
| 3. Supply-chain position | **Pass on kind, costly in size** — no C or C++ anywhere in the tree, but 32 new packages |
| 4. Malformed corpus without panic, unbounded allocation, or embedded execution | **Fail on one case** — a self-referential form XObject exhausts memory where the reference renderer bounds it |
| 5. Partial-page failure decision | **Open** — recommendation in section 6 |

One part fails, so by the gate's own terms **PDF stays deferred**. The failure is
specific and it is not a reason to reject the candidate: it tells the
implementation exactly which bound it must own rather than borrow.

## 1. The candidate

`hayro 0.4.0` — "A rasterizer for PDF files" — licensed **Apache-2.0**, with its
sibling crates `hayro-interpret 0.4.0`, `hayro-syntax 0.4.0`, and
`hayro-font 0.3.0` under the same licence, and its decoders
(`hayro-jbig2`, `hayro-jpeg2000`, `hayro-ccitt`) under `Apache-2.0 OR MIT`.
Compatible as-is with this project's recorded Apache-2.0 distribution position;
no dual-licensing argument is needed.

The alternatives, and why they are not the candidate:

| Candidate | Rasterizes | Licence | Disposition |
|---|---|---|---|
| `hayro 0.4.0` | yes | Apache-2.0 | **the candidate** |
| `pdfium-render 0.9.3` | yes | MIT OR Apache-2.0 wrapper | Rejected on gate part 3: wraps the pdfium C++ subsystem, which needs the hermetic-rebuild treatment of gate `G2` — still open for one much smaller library |
| `pdf_oxide 0.3.77` | optional feature | MIT OR Apache-2.0 | Not measured. Its own documentation positions it as a text-extraction library with rendering added behind a feature; a page renderer is the requirement |
| `lopdf`, `pdf-extract` | no | MIT | Not renderers: an object model and a text extractor |
| MuPDF, Poppler | yes | AGPL, GPL | Rejected by the ADR on licence, unchanged |

This satisfies the ADR's section 7 reversal condition 1 in part: a permissively
licensed pure-Rust renderer exists. The condition also asks for published
fidelity evidence "of the kind section 5 requires", which is what section 2
below now supplies — measured here rather than published upstream.

## 2. Rendering fidelity

**Reference renderer**: poppler `pdftoppm` 24.02.0 at `-r 144` (a 2x scale over
the 72 dpi default user space). **Candidate**: `hayro 0.4.0` at `x_scale =
y_scale = 2.0`. All seven documents rendered to identical pixel dimensions
under both, so every comparison is component-aligned with no resampling.

The corpus is generated, not collected — every byte including the embedded
TrueType font is authored by `tools/generate_pdf_fidelity_corpus.py`, so the
committed corpus carries this project's own licence. A real-world PDF corpus
would drag in licensing this project cannot clear.

Measured by `tools/measure_pdf_fidelity.py`:

| Page | Size | Max component Δ | Mean Δ | Share > 1 | Share > 32 | Share > 128 | Ink IoU |
|---|---|---|---|---|---|---|---|
| `scanned_flate` | 240x320 | **0** | 0.0000 | 0.0000% | 0.0000% | 0.0000% | **1.0000** |
| `scanned_jpeg` | 240x320 | 4 | 0.2838 | 7.8750% | 0.0000% | 0.0000% | 0.9997 |
| `shading` | 400x400 | 29 | 0.0890 | 0.0875% | 0.0000% | 0.0000% | **1.0000** |
| `form_xobject` | 400x400 | 32 | 0.0629 | 0.3406% | 0.0000% | 0.0000% | **1.0000** |
| `vector` | 400x400 | 80 | 0.1402 | 0.4338% | 0.0750% | 0.0000% | 0.9983 |
| `cid_font` | 400x400 | 198 | 0.6993 | 0.7100% | 0.6119% | 0.2331% | 0.9769 |
| `standard_font` | 800x800 | 255 | 1.7704 | 1.9577% | 1.3864% | 0.4984% | 0.8313 |

"Ink IoU" is the intersection over union of the two renderers' below-mid-grey
pixels. It is reported alongside the raw maximum for the reason `IMG-003`
established: a maximum difference on its own is a prompt to measure the
consequence, not a verdict. Two renderers with different antialiasing will
differ by `255` on some edge and agree about every glyph and rule; the ink
measure separates that from a renderer that dropped a feature.

### The three findings that matter

**The scan path is bit-identical.** `scanned_flate` — a full-page RGB image
XObject, the shape every scanned PDF actually has — reproduces with a maximum
component difference of **zero**. This is the case `PDF-001` exists to serve,
and on it the two renderers do not merely agree closely, they agree exactly.
`scanned_jpeg`, the same raster through `DCTDecode`, differs by at most **4**
components out of 255, which is an IDCT implementation difference and is
narrower than the JPEG decoder delta `IMG-003` already accepted.

**Nothing was dropped.** The ADR's stated worry is "a renderer that silently
drops a shading or a CID font produces a plausible wrong page". Measured: the
axial shading renders with ink IoU exactly `1.0000` and a maximum difference of
`29`, the form XObject with its own matrix and a second scaled invocation is
also exactly `1.0000`, and the embedded CID font draws every glyph — poppler
itself reports the font as `CID TrueType`, `Identity-H`, embedded. Every
divergence in the table is confined to edges.

**The worst page changes no character.** `standard_font` is the outlier at ink
IoU `0.8313`, and the cause is structural rather than a defect: the page uses
non-embedded Helvetica, so each renderer substitutes its own font and the glyph
outlines genuinely differ. Running this port's own OCR over both renderings,
with the provisioned `PP-OCRv6` medium pair:

| Line | Reference rendering | Candidate rendering |
|---|---|---|
| 1 | `Hello World` (0.999979) | `Hello World` (0.999980) |
| 2 | `Rust OCR 2026` (0.999919) | `Rust OCR 2026` (0.999918) |
| 3 | `PADDLE ocr test` (0.999921) | `PADDLE ocr test` (0.999879) |

Character-identical on every line, with scores agreeing to five decimals. This
is exactly the `IMG-003` outcome — a large raw pixel delta with no consequence
for the output the port actually produces — and it is the strongest single result
in this packet, because it was measured on the page chosen to be the worst.

## 3. Supply-chain position

The gate asks for a position "at least as strong as gate `G2` demands, **if the
candidate is not pure Rust**". Measured across the resolved tree of 53 packages:

- **No C or C++ source anywhere.** Zero `.c`, `.cc`, or `.cpp` files in any
  crate in the tree.
- **Six build scripts**, all from established pure-Rust crates doing cfg
  detection: `crc32fast`, `num-traits`, `proc-macro2`, `quote`, `serde`,
  `serde_core`. Five of the six are already in this project's tree.
- **Every licence is permissive**, with no copyleft: 23 `MIT OR Apache-2.0`, 6
  `MIT`, 4 `Unicode-3.0`, 4 `Apache-2.0 OR MIT`, 4 `Apache-2.0`, 3
  `BSD-3-Clause OR Apache-2.0`, and the remainder Zlib/0BSD/Unlicense
  alternatives. All compatible with Apache-2.0 distribution.
- **`unsafe` is rare in the renderer itself**: across the four `hayro` crates,
  about `47,750` lines of Rust contain **10** lines mentioning `unsafe`.

So the `G2` clause does not trigger. What the gate does not ask about, and what
this measurement makes visible anyway, is **size**:

| | Packages |
|---|---|
| This project's current resolved tree | 50 (including itself) |
| The candidate's resolved tree | 53 |
| Shared between them | 19 |
| **New packages the candidate would add** | **32** |

Thirty-two new packages, for a project whose release CLI is currently `361,288`
bytes over `8` dependencies at default features. That is the honest cost of the
candidate: not a licence problem and not a native-code problem, but roughly a
doubling of the reviewed surface. `SUPPLY-001` would have to cover all of it,
and the SBOM and drift check would grow accordingly.

## 4. The malformed corpus

Fourteen documents, one named lie each, generated by
`tools/generate_pdf_malformed_corpus.py`. Each ran in **its own process** under
`ulimit -v 2097152` (2 GiB) and a 20-second timeout, so a panic, an abort, or an
out-of-memory kill is attributable to exactly one input. The same corpus ran
through poppler under identical limits, because "the candidate handled it badly"
means little without knowing whether any renderer handles it well.

| Document | Candidate | Peak RSS | Time | Reference (poppler) | Peak RSS | Time |
|---|---|---|---|---|---|---|
| `truncated_header` | REFUSED `Invalid` | 3.6 MB | 0.00 s | refused | 14.3 MB | 0.01 s |
| `truncated_body` | REFUSED `Invalid` | 3.7 MB | 0.00 s | refused | 14.9 MB | 0.01 s |
| `missing_root` | REFUSED `Invalid` | 3.0 MB | 0.00 s | refused | 15.0 MB | 0.01 s |
| `encrypted_stub` | REFUSED `Decryption(MissingIDEntry)` | 3.1 MB | 0.00 s | refused | 14.3 MB | 0.01 s |
| `zero_pages` | no pages | 3.6 MB | 0.00 s | refused | 14.9 MB | 0.01 s |
| `circular_pages` | no pages | 3.0 MB | 0.00 s | rendered | 16.2 MB | 0.02 s |
| `no_xref` | rendered | 5.3 MB | 0.00 s | rendered | 16.7 MB | 0.03 s |
| `bad_xref_offsets` | rendered | 5.9 MB | 0.00 s | rendered | 16.7 MB | 0.03 s |
| `huge_declared_length` | rendered | 5.4 MB | 0.00 s | rendered | 16.8 MB | 0.03 s |
| `huge_image_dimensions` | rendered | 5.9 MB | 0.01 s | rendered | 16.7 MB | **15.95 s** |
| `negative_dimensions` | rendered 1000x1000 | 8.8 MB | 0.02 s | rendered | 19.0 MB | 0.08 s |
| `javascript_openaction` | rendered, **no side effect** | 4.8 MB | 0.00 s | rendered | 16.2 MB | 0.02 s |
| `embedded_file` | rendered, **nothing extracted** | 4.8 MB | 0.00 s | rendered | 16.2 MB | 0.02 s |
| `deep_nesting` | **ABORTED at the 2 GiB limit** | **2,084,736 KB** | 5.22 s | rendered | 16.2 MB | 0.02 s |

### What passes

**Nothing embedded was executed.** The JavaScript document carries an
`/OpenAction` whose script tries to launch a URL and export a data object; it
ran through the candidate with no file appearing at the path the script names
and no extra file written anywhere. The `/EmbeddedFile` document's payload was
never written out. Both were rendered as pages and nothing more.

**Declared sizes do not become allocations.** `huge_image_dimensions` declares a
60000x60000 image XObject — `10.8` GB of RGB — behind 300 bytes of compressed
data. The candidate finished in 0.01 s at 5.9 MB. The reference renderer took
**15.95 s** on the same file: each renderer has a different resource weakness,
and on this one the candidate is the better-behaved of the two.

**Refusals are typed and cheap.** Truncation, a missing catalogue, and a
declared `/Encrypt` all produce a structured error in under a millisecond at
under 4 MB. The encrypted document is refused rather than rendered, which is the
behaviour that matters: rendering an encrypted document as though it were plain
would be worse than failing.

### The one failure

`deep_nesting` is a form XObject that draws itself. The candidate consumed the
whole 2 GiB limit and aborted after 5.22 seconds. Poppler renders the same file
in 0.02 s at 16 MB, so **this is the candidate's own missing recursion bound**,
not an inherent hazard of the input.

That single result decides an implementation question the ADR's section 4 policy
already implies. This project requires "resource limits, all checked before
allocation", and a library that recurses without a depth bound cannot honour
that from the inside. So if `PDF-001` proceeds with this candidate, the page
budget cannot be delegated to it: the renderer has to run under a wall-clock and
memory bound this port enforces, in a form where exceeding it becomes a typed
error rather than an abort. A crash inside a library that owns the process is
not a failure mode this project's error contract can express.

## 5. Reproducing these measurements

Nothing here runs in the offline gate, and nothing should: the gate has no
poppler and no candidate renderer. The committed test
`contracts::the_pdf_entry_gate_corpus_matches_its_recorded_digests` enforces the
one property that can be enforced offline — that the 22 corpus files still have
the digests the numbers above were measured against, because a measurement whose
input silently changed is worse than no measurement.

To retake the measurements:

```sh
# 1. Generate the corpus (already committed; this checks it reproduces).
python3 tools/generate_pdf_fidelity_corpus.py  <dir>/fidelity
python3 tools/generate_pdf_malformed_corpus.py <dir>/malformed

# 2. Reference renderings, poppler at 2x.
for f in <dir>/fidelity/*.pdf; do
    pdftoppm -r 144 -png -singlefile "$f" "<dir>/fidelity/ref_$(basename "$f" .pdf)"
done

# 3. Candidate renderings, from a disposable project outside this repository
#    that depends on hayro 0.4.0. Do not add the dependency here.

# 4. Compare.
python3 tools/measure_pdf_fidelity.py <dir>/fidelity
```

The malformed run must give each document its own process under an address-space
limit and a timeout; running them in one process attributes a crash to the wrong
input, and running them without a limit turns the `deep_nesting` finding into a
machine-wide memory event rather than a measurement.

## 6. Part 5: partial-page failure, and what is recommended

The gate's fifth part asks for "a recorded decision on partial-page failure,
since a document is the first input where 'some of it worked' is meaningful".
That is a policy question, and it stays the user's. What this measurement adds is
that the question is now concrete: the corpus contains documents that render,
documents that refuse, and one that dies — and a multipage document can mix all
three.

Three coherent positions:

1. **All-or-nothing.** Any page that fails to render fails the document. Matches
   this port's existing habit of refusing rather than approximating — the
   `recognize_png`-refuses-unwarping precedent — and is the cheapest to reason
   about. A 400-page scan with one broken page yields nothing.
2. **Per-page results with typed per-page failures.** The document result carries
   one entry per page, each either a parsed page or a structured error naming the
   page index and the reason. Nothing is silently missing, and the caller decides
   whether a partial document is useful. Costs a wider result type and forces
   every consumer to handle the mixed case.
3. **Best-effort with a summary.** Return the pages that worked plus a count of
   those that did not. Cheapest for callers, and the one position this project
   should not take: a document that is quietly missing page 7 is exactly the
   "plausible wrong page" the ADR refused PDF over in the first place.

**Recommended: 2.** It is the only one of the three that keeps both properties
this port has been consistent about — a failure is typed and named rather than
inferred, and a result never looks complete when it is not. It also composes with
the `MPAGE-001` row's own requirement for "page metadata/order ... and
partial-failure semantics", which would otherwise have to invent the same answer
later.

Option 1 is defensible and cheaper; it is worse only because a document is
genuinely the first input where the caller, not this library, knows whether
partial output has value.

## 7. Consequences for `PDF-001` if it proceeds

Independently of part 5, this measurement fixes four things about any
implementation:

1. The renderer runs under a **wall-clock and memory bound owned by this port**,
   and exceeding it is a typed error. The `deep_nesting` result makes this
   non-optional.
2. A declared `/Encrypt` is refused before rendering. The candidate already does
   this; the port should not depend on it continuing to.
3. Embedded files, JavaScript actions, and launch actions are never executed or
   written. The candidate does not execute them today, and a test in this
   repository should assert it rather than trust it.
4. The scan path — an image XObject through `FlateDecode` or `DCTDecode` — is the
   one this port can claim fidelity on, with the measurements above behind it.
   Vector and text-heavy pages agree closely but not exactly, and no
   pixel-identity claim should be made for them.

`PDF-001` stays `Blocked`. Parts 1, 2, and 3 are recorded and pass; part 4 fails
on one measured case with a named remedy; part 5 needs a user decision.
