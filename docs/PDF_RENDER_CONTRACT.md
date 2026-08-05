# PDF Rendering Contract (PDF-001)

Upstream reference: PaddleX `3.7.2` — `inference/utils/pdf_rendering.py`,
`inference/utils/io/readers.py` (`PDFReaderBackend`), `utils/flags.py`.
Decision context: [`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md),
whose five-part entry gate still blocks any renderer implementation.

This document pins two different kinds of fact and labels them apart:

1. the **scale planner**, which is pure arithmetic, captured by execution,
   and **implemented** in `src/pdf_render_plan.rs` against the fixture
   `tests/fixtures/classic-v1-pdf-scale/`;
2. the **renderer contract**, which is measured and recorded here so that a
   future candidate is evaluated against known figures — but deliberately
   **not implemented** and **not fixtured**, because a fixture without a
   consuming test is what the integrity gate exists to forbid.

## 1. The upstream data path

`PDFReaderBackend.read_file` opens the document with `pypdfium2`
(`pdfium.PdfDocument(path)`), calls **`doc.init_forms()`**, and renders each
page in document order under a process-wide lock:

```python
page.render(scale=scale, rotation=rotation).to_numpy()
```

- `zoom` (the requested scale) defaults to **`2.0`**
  (`PADDLE_PDX_PDF_RENDER_SCALE`); `rotation` defaults to `0`.
- The minimum scale is **`0.1`** (`PADDLE_PDX_PDF_MIN_RENDER_SCALE`).
- The module-level pixel budget is **`178,956,970`** — PIL's
  decompression-bomb threshold.
- **The reader bypasses its own budget.** `PDFReaderBackend.__init__`
  defaults `max_pixels=None`, and `render_pdf_page_to_numpy` disables the
  planner entirely when the budget is `None`. The budget applies only when a
  caller passes one. This port's planner makes the budget mandatory instead;
  the divergence is deliberate and recorded in the fixture's limitations.

## 2. The scale planner (implemented)

`get_pdf_render_scale_within_pixel_limit`, ported as
`pdf_render_plan::plan_render_scale`:

- extents are `ceil(points * scale)` per axis; a scale whose pixel count is
  `<=` the budget is **kept**, so a page exactly at the budget is not
  bisected;
- when the requested scale exceeds the budget but the minimum fits, the
  scale is found by **bisection, exactly 32 iterations**, between the
  minimum scale and the analytic bound `sqrt(max_pixels / (w * h))` capped
  by the requested scale, returning the lower edge of the final bracket;
- when even the minimum scale exceeds the budget, the page is refused
  (`PDFRenderSizeError` upstream, `Error::ResourceLimit` here, carrying the
  pixel count at minimum scale);
- non-positive sizes, scales, and budgets are refused (`ValueError`
  upstream, `Error::InvalidInput` here).

Eleven captured cases — kept scales, bisected scales including fractional
page sizes, the exact-boundary pair, and the refusal — reproduce
**bit for bit** (scales are compared as `f64` bits, not decimals). Capture:
`tools/capture_pdf_scale_oracle.py`.

## 3. The renderer contract (measured, not implemented)

Measured on 2026-08-05 by executing `pypdfium2 5.12.1` (pdfium build
`7947`, the version the pinned PaddleX resolves `pypdfium2>=4` to today)
against a hand-authored minimal two-page PDF — uncompressed content
streams, no fonts, one filled rectangle per page:

| Fact | Measured value |
|---|---|
| Output array | `(height, width, 3)`, `uint8`, **BGR** — no alpha channel, no conversion step in the reader |
| Background | white (`255,255,255`), pdfium's default fill colour |
| Geometry | a `40x30`pt page at scale `2.0` renders `80x60`px; at `1.3`, `52x39`px — `points * scale`, matching the planner's ceiling |
| Page order | document order, all pages, no range parameter in the upstream reader |
| Determinism | identical bytes across repeated renders in one process |
| Malformed input | `PdfiumError: Data format error` — a typed refusal, not a crash |

These figures are the **comparison target** for any candidate that attempts
the entry gate: a pure-Rust renderer must be measured against pdfium output
in exactly this configuration, the way `IMG_003_DELTA_MEASUREMENT.md`
measures JPEG decoders, with the delta stated rather than assumed.

## 4. What stays blocked, and why

The gate's five parts and their present status:

| Requirement | Status |
|---|---|
| Compatible licence | pdfium itself is permissive (BSD-3-Clause), but is a C++ subsystem requiring the gate `G2` hermetic-rebuild treatment — still open for a far smaller library |
| Fidelity measurement | the reference figures above exist now; no candidate has been measured against them |
| Supply chain (`G2`-strength) | not attempted for any candidate |
| Malformed-input corpus | not built; the single malformed probe above is a data point, not a corpus |
| Partial-page failure decision | not decided |

The renderer decision therefore remains exactly where
`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md` left it. What changed is that the
arithmetic above the renderer is no longer speculative, and the target a
candidate must hit is a table of measured numbers rather than a sentence.
