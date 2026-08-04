# User Guide

Roadmap item: `DOC-USER-001`
Applies to: the classic `PP-OCRv6_medium` single-image OCR path
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

This is the practical document: how to build it, what to provision, what it
accepts, what it returns, and where it differs from upstream PaddleOCR. Nothing
here is aspirational. Every command was run, and every figure was measured on the
host recorded in [`G3_RESOURCE_EVIDENCE.md`](G3_RESOURCE_EVIDENCE.md).

If you only want to know whether a capability is supported, read
[`COMPATIBILITY.md`](COMPATIBILITY.md) instead — it is the authority, and this
guide is not.

## 1. What you need

Four files, all supplied by you, all local, none downloaded by this project:

| File | What it is |
|---|---|
| `libonnxruntime.so` | ONNX Runtime `1.28.0` CPU shared library |
| detector `.onnx` | `PP-OCRv6_medium` text detector |
| recognizer `.onnx` | `PP-OCRv6_medium` text recognizer |
| dictionary `.txt` | the recognizer's character dictionary, one entry per line |

This project never downloads a model, never reads a path from an environment
variable, and never consults a cache. If a file is not named on the command line
or passed to the API, it is not used. That is a deliberate constraint, not an
unfinished feature.

## 2. Building

```sh
cargo build --release --features onnxruntime --bin paddleocr-rust
```

The `onnxruntime` feature is **off by default**. Without it the crate has no
native dependency at all and its tests run offline, which is what keeps the
normal development loop independent of any runtime. A binary built without the
feature reports that it has no backend and exits `2` rather than pretending to
work.

The release binary is `812,144` bytes and is already stripped. The ONNX Runtime
library is *not* linked into it: it is loaded at runtime from the path you give,
which is why the same binary works against a library you built yourself.

## 3. Provisioning the artifacts

### ONNX Runtime

Any ONNX Runtime `1.28.0` CPU build works. The measured one is
`libonnxruntime.so.1.28.0`, `31,428,768` bytes, SHA-256
`1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa`.

A hermetic rebuild with a generated SBOM is gate `G2` and is **not** closed —
see [`ADR_RT004_RUNTIME_SELECTION.md`](ADR_RT004_RUNTIME_SELECTION.md). If you
are packaging this for distribution rather than running it yourself, that gap is
yours to close.

### Models

The pinned candidates, with the digests this project verifies against, are
recorded in [`MODEL_CANDIDATES.md`](MODEL_CANDIDATES.md):

| Artifact | SHA-256 | Bytes |
|---|---|---|
| detector `inference.onnx` | `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` | `62,032,837` |
| recognizer `inference.onnx` | `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` | `76,554,979` |
| `ppocrv6_dict.txt` | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` | `18,708` entries |

Verify what you downloaded before you trust it:

```sh
sha256sum detector/inference.onnx recognizer/inference.onnx ppocrv6_dict.txt
```

The dictionary ships inside the upstream Python package at
`ppocr/utils/dict/ppocrv6_dict.txt`. It is a plain text file, one entry per line,
and nothing else about the upstream project is needed.

Check what a dictionary can actually spell before trusting a language claim:

```sh
cargo run --example dictionary_census -- ppocrv6_dict.txt
```

See [`LANGUAGE_SUPPORT.md`](LANGUAGE_SUPPORT.md) for why that census is not a
support claim.

## 4. Running it

### One image

```sh
paddleocr-rust \
  --ort-dylib /path/to/libonnxruntime.so \
  --detector  /path/to/detector/inference.onnx \
  --recognizer /path/to/recognizer/inference.onnx \
  --dictionary /path/to/ppocrv6_dict.txt \
  page.png
```

Results go to stdout as `score<TAB>text`, one line each, in reading order.
Diagnostics go to stderr, so `paddleocr-rust ... page.png > out.tsv` gives you a
clean file.

```
0.999994	Hello
0.999983	World
```

### Several images

```sh
paddleocr-rust ... page-01.png page-02.png page-03.png
```

The models load once and are reused. Session creation is roughly `1.4 s` of the
`4.2 s` cold run, so running the binary once per file is several times slower
than passing them together.

With more than one image the text output gains a leading path column, the way
`grep` prefixes filenames. A single image is unchanged.

### Verifying what you loaded

```sh
paddleocr-rust ... \
  --detector-sha256 eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1 \
  --recognizer-sha256 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba \
  page.png
```

The digest is streamed and checked **before** the model is handed to the
runtime. Omitting it is allowed and is a choice you are making, not a default
that happens to you.

This matters more than it looks. The detector and recognizer export the same
tensor names and leave the axes this port constrains dynamic, so **passing them
in the wrong order loads without complaint** and fails only on first use. With
digests declared, the swap is refused before the runtime sees a byte. Shape does
not tell two models apart; identity does.

### Bounding a run

```sh
paddleocr-rust ... --time-budget-ms 30000 page.png
```

The budget applies **per image**, not to the whole invocation, so a page list is
not silently truncated at the end. A run is abandoned only at a stage boundary —
before the detector, before cropping, before each recognition batch — because a
backend call cannot be interrupted without leaving the session undefined. So a
`1 ms` budget on a real page does not return in `1 ms`; it returns after the
detector finishes, with:

```
paddleocr-rust: time budget exhausted before crop
```

Overshoot is bounded by one detector run or one recognition batch of six crops.
If you need a hard wall-clock bound, enforce it out of process.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | success, including a page with no text |
| `2` | any failure: bad arguments, unreadable or unsupported input, missing or mismatched artifact, resource limit, cancellation, timeout |

There is no partial-success code, because there are no partial results: any
failure abandons the whole request. See §7.

## 5. Machine-readable output

```sh
paddleocr-rust ... --json page.png
```

One document per image, on one line. With several images that is JSONL.

```json
{
  "schema_version": "paddleocr-rust/ocr-result/v1",
  "input": { "id": null, "page_index": null, "width": 1280, "height": 720 },
  "lines": [
    { "quad": [[47,78],[212,78],[212,151],[47,151]],
      "text": "Hello",
      "confidence": 0.9999944568 }
  ]
}
```

| Field | Meaning |
|---|---|
| `schema_version` | frozen identifier; a breaking change gets a new one |
| `input.id` | the path you gave, when several images were passed; `null` for one |
| `input.page_index` | always `null`; multipage input does not exist yet |
| `input.width` / `height` | the decoded image's dimensions |
| `lines[].quad` | four corners in source-image pixels, in `get_mini_boxes` order |
| `lines[].text` | exact Unicode scalars, JSON-escaped and nothing else |
| `lines[].confidence` | mean of the per-timestep CTC maxima, ten decimals |

The output is byte-deterministic: fields appear in a fixed order with fixed
numeric formatting, and the writer is hand-rolled specifically so there is no
map-ordering or float-formatting behaviour to depend on. Three cold runs over the
same input produced byte-identical documents.

Text is escaped per JSON rules and **never otherwise transformed**. No NFC, no
NFKC, no case folding, no width folding. `U+3000` stays `U+3000`.

## 6. From Rust

```rust
use paddleocr_rust::api::{Artifacts, OcrEngine, OcrOptions, parse_dictionary};

let text = std::fs::read_to_string("ppocrv6_dict.txt")?;
let dictionary = parse_dictionary(&text, true)?;

let engine = OcrEngine::load(
    &Artifacts {
        library: "/path/to/libonnxruntime.so",
        detector: "/path/to/detector/inference.onnx",
        detector_sha256: Some("eb13b44b…d086e1"),
        recognizer: "/path/to/recognizer/inference.onnx",
        recognizer_sha256: Some("9c09abf0…b673ba"),
    },
    &dictionary,
)?;

for line in engine.recognize_png(&std::fs::read("page.png")?, &OcrOptions::default())? {
    println!("{:.6}\t{}", line.score, line.text);
}
```

`OcrOptions` carries `box_threshold` (`0.6`), `unclip_ratio` (`1.5`),
`drop_score` (`0.5`), and `control`, which holds the time budget and an
`Arc<AtomicBool>` cancellation flag.

**Concurrency:** an `OcrEngine` is `!Sync`, and the compiler enforces it rather
than the documentation. Load one engine per thread. A lock would turn that
compile error into a hidden queue without removing the serialisation, so this
project does not provide one.

## 7. Limits and failure behaviour

Every limit is checked before the memory it bounds is allocated.

| Limit | Value |
|---|---|
| Encoded input | `64 MiB` |
| Image side | `16,384` pixels |
| Image pixels | `40,000,000` |
| Decode envelope | `256 MiB` across both decode buffers |
| Detected regions | `1,000` |
| Recognition crops | `1,000` |
| Recognition batch | `6` crops per model call |

Measured resource use on the reference host, against a `1280×720` page: cold CLI
`4.23 s`, warm median `2.840 s`, peak resident `464.3 MiB`. Full conditions in
[`G3_RESOURCE_EVIDENCE.md`](G3_RESOURCE_EVIDENCE.md). That page is synthetic, so
treat those figures as a lower bound on real scans rather than a representative
one.

**Failure is whole-input.** Any error from any stage abandons the entire request
and returns that error; no partial line list is ever produced. The result
document has no field marking a result as incomplete, so four lines returned from
a nine-line page would be indistinguishable from a four-line page. An engine
remains usable after a rejected input.

## 8. Known differences from upstream PaddleOCR

These are behavioural, not cosmetic. [`COMPATIBILITY.md`](COMPATIBILITY.md)
carries the authoritative list; this is the part a user is most likely to hit.

- **PNG only.** JPEG returns a typed `Unsupported` error rather than a
  near-miss result. Every pure-Rust JPEG decoder evaluated differed from
  OpenCV's by up to `36` in a component, which is a difference in the pixels the
  model sees. See [`IMAGE_DECODER_DECISION.md`](IMAGE_DECODER_DECISION.md).
- **No orientation classification.** Upstream can run a document- or
  text-orientation model before recognition. This port does not, and the crop
  stage's tall-crop rotation is not a substitute.
- **No document preprocessing, PDF, tables, formulas, or structured output.**
  The classic detect-and-recognize path is the whole of it.
- **One model pair.** A different `PP-OCR` version or language pack is
  unverified, will load, and can produce wrong answers silently. Declare digests.
- **Reading order is the upstream sort**, top-to-bottom then left-to-right with
  a ten-pixel row tolerance. That is wrong for right-to-left and vertical text,
  as it is upstream.
- **The Clipper union pass is omitted** in polygon unclipping. It is documented
  as unobservable through `get_mini_boxes`, which only reads the minimum-area
  rectangle of the result. This is the one place where the implementation
  deliberately does less than upstream, and the reason it is safe is recorded in
  [`DB_POSTPROCESS_SPEC.md`](DB_POSTPROCESS_SPEC.md).
- **No Python, no PaddlePaddle, no upstream checkout at runtime.** The upstream
  project is used only to capture oracles during development.

Two upstream behaviours were found to differ from earlier versions of this port
and corrected; if you compared against an older build, expect small changes:
recognition now batches in groups of six with a per-batch padded width, and that
batch width truncates rather than rounding up. Both changes shift confidences in
roughly the fourth decimal on multi-line pages.

## 9. Reporting a difference

A difference is only actionable with the input that produced it. The most useful
report is a small PNG, the exact command, the output you got, and the output you
expected with its source. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the
fixture rules — anything committed as evidence needs a recorded provenance and a
consuming test.
