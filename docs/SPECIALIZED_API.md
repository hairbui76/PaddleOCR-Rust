# Specialized Module API

Roadmap item: `SPECAPI-001`
Status: **typed API and schema for the table pipeline**; layout deliberately
unexposed, with the reason below

`docs/API_CONTRACT.md` covers the classic OCR surface. This document covers the
specialized modules — what each one exposes, what it does not, and why.

## 1. What is reachable

| Module | Public surface | Reachable |
|---|---|---|
| Table classification | via `TableEngine::classify` | **yes** |
| Table cell detection | via `TableEngine::detect_cells` | **yes** |
| Table structure | via `TableEngine::recognize_structure` | **yes** |
| Table pipeline | `TableEngine::recognize_table` → `TableResult` | **yes** |
| Layout detection | none | **no** — see §4 |
| Document preprocessing | `DocumentPreprocessing` types only | partial |

Everything in the table column is behind the `onnxruntime` feature, which is off
by default. The composition logic in `paddleocr_rust::table_pipeline` is not: it
needs no runtime, so it is reachable, testable, and reviewable without one.

## 2. The typed surface

```rust
use paddleocr_rust::table_engine::{TableArtifacts, TableEngine, TableImage};
use paddleocr_rust::table_pipeline::TableRoute;

let engine = TableEngine::load(&TableArtifacts::new(
    "libonnxruntime.so",
    "PP-LCNet_x1_0_table_cls/inference.onnx",
    "RT-DETR-L_wired_table_cell_det/inference.onnx",
    "SLANeXt_wired/inference.onnx",
    TableRoute::Wired,
))?;

let result = engine.recognize_table(
    &rgb_crop, &bgr_crop, table_box, &ocr_boxes, &ocr_texts,
)?;
println!("{}", result.to_json(width, height, Some("table-0")));
```

Three deliberate shapes in that signature:

- **Two images, two channel orders.** The classifier and cell detector read
  `RGB`; the structure model reads `BGR`. Taking both rather than converting
  internally keeps the conversion where a caller can see it — a silent swap is
  the bug `docs/TABLE_CELLS_CONTRACT.md` records finding in `LAY-001`.
- **`TableImage`, not a re-export.** `InterleavedImage` is internal, and
  exposing it here would expose every other module's use of it too.
- **OCR comes in, not out.** `TableEngine` does not run text detection or
  recognition; `OcrEngine` already does, and duplicating its artifact handling
  would mean two ways to load a detector.

`TableEngine` is `!Sync` by construction, so one thread owns one set of
sessions — the same position `docs/CONC_001_EVIDENCE.md` records for
`OcrEngine`, enforced by the compiler.

## 2b. Detection without recognition

`MODAPI-001`. `OcrEngine::detect_png` runs the detector and the reading-order
sort, and stops. Cropping, orientation, and recognition do not run, which makes
it cheaper than `recognize_png` by the recognizer's whole cost — on a dense page,
most of the run.

```rust
let regions = engine.detect_png(png, &OcrOptions::default())?;
println!("{}", DetectedRegion::slice_to_json(&regions, width, height, Some("page")));
```

Two deliberate refusals in that method:

- **`drop_score` is not applied.** It filters on *recognition* confidence, and
  there is no recognition here. Reusing it against the detector's score would
  silently mean something else. `box_threshold` and `unclip_ratio` do apply,
  because they are the detector's own.
- **Unwarping is refused**, for the same reason `recognize_png` refuses it: the
  returned coordinates would describe an image the caller never supplied.

`DetectedRegion::score` is the **detector's** mean probability, not a
recognition confidence. The schema names the field `detector_score` rather than
`confidence` so the two cannot be compared by accident.

Verified against real artifacts: on the committed benchmark page, detection and
the full pipeline agree on **six regions**, the same boxes, in the same order —
both run the same reading-order sort.

## 3. Result schemas

Two frozen names, separate from the classic one:

| Schema | Produced by |
|---|---|
| `paddleocr-rust/ocr-result/v1` | `result_to_json` |
| `paddleocr-rust/detection-result/v1` | `DetectedRegion::slice_to_json` |
| `paddleocr-rust/table-result/v1` | `TableResult::to_json` |
| `paddleocr-rust/layout-result/v1` | frozen, **not reachable** — §4 |

A table document is not a page of text lines with extra fields: it has no
`lines` array and its coordinates describe a **crop**, not a page. Widening
`ocr-result/v1` would give consumers a document where half the fields are `null`
on any input, with no way to tell "this producer does not do tables" from "this
page had none".

```json
{"schema_version":"paddleocr-rust/table-result/v1",
 "input":{"id":"table-0","width":480,"height":320},
 "route":{"label":"wired_table","confidence":0.9506700039},
 "html":"<html><body><table>…</table></body></html>",
 "structure_tokens":["<html>","<body>","<table>","<tr>","<td></td>",…],
 "cells":[[0.0000000000,0.0000000000,50.0000000000,20.0000000000]]}
```

The tokens travel **alongside** the HTML rather than only inside it, because
they are what the model actually produced: the HTML is an assembly step over
them and over the OCR text, and a consumer checking a structure prediction
should not have to parse HTML to see it.

These documents carry **no model manifest block**, and the field is absent
rather than `null`. The classic result carries one because `MODEL-DEC-001` ties
a text result to its artifacts; the specialized modules have no equivalent
manifest type, and inventing one per module would freeze a shape before there is
a second module to check it against.

## 4. Layout is implemented and deliberately not exposed

`src/layout.rs` is complete and its oracle matches on two of three cases. The
third is the cubic resize's open divergence, recorded in
`docs/LAYOUT_CONTRACT.md`.

Exposing a layout API built on an operator that is knowingly one 8-bit step off
at some scales would sell a precision this port does not have. The schema name
is frozen so it does not move later; the function that produces it is
`pub(crate)` and tested, and it becomes public when the operator is exact.

That is the same standard `src/unwarp.rs` and `src/document_pipeline.rs` are
held to, for a different reason in each case.

## 5. Compatibility position

| Capability | Position |
|---|---|
| Table classification preprocessing and `Topk` | Verified bit-identical against a captured oracle |
| Table cell preprocessing | Three of four tensors bit-identical; one bounded by the cubic defect |
| Table structure preprocessing and token decode | Verified bit-identical |
| Table composition (geometry, matching, HTML) | Verified, HTML byte for byte |
| Three-model orchestration | Verified end to end against real artifacts |
| Detection accuracy of any model | **Not claimed.** No fixture in this repository asserts what a model detects, only what this port feeds it and does with its output. |

The last row is the one that matters most. Every oracle here pins
**preprocessing and postprocessing**. None of them is an accuracy claim, and a
consumer should read them as "this port agrees with upstream about what to send
the model and what to do with the answer", not "this port detects tables well".

## 6. CLI

Delivered, and shaped by the answer `STRUCT-001` produced: there are **three**
typed entry points — `OcrEngine`, `TableEngine`, `StructureEngine` — so the
binary grew two subcommands rather than a mode flag.

The classic invocation is unchanged. `paddleocr-rust --ort-dylib … page.png`
still means what it meant, still takes several images, and still honours every
flag it did; the recorded decision was to extend in place rather than
restructure, so no documented invocation broke. `structure` and `table` are
additional first words, sharing the same flag vocabulary — `--ort-dylib`, not a
second spelling of it — and adding only what they need: `--layout`, the table
trio, `--route`, `--format`, `--plain`.

Two boundaries are worth stating because they are choices, not omissions:

- **Exactly one image** for the two new commands. Joining pages is
  `concatenate_markdown_pages`, which needs `PDF-001`'s renderer gate; a
  subcommand that quietly concatenated would be inventing a document boundary.
- **The table trio is all-or-none.** Two of three would load an engine that
  cannot answer, and that is reported at parse time rather than after a session
  has opened.

The argument model is pure and unit-tested in the default build, without the
inference feature and without artifacts. That is where argument handling
belongs: a CLI's most common failure is a mistyped flag, not a mis-run model.
