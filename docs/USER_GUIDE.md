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

### Using a manifest

A manifest names the artifact pair a run is meant to use: family, version,
format, backend, each artifact's source revision, byte count and SHA-256, the
dictionary fingerprint, the licence review, and the upstream commit it was
verified against.

```sh
paddleocr-rust ... --manifest classic-ppocrv6-medium.txt page.png
```

Three things it does, and one it does not:

- Its digests become the expected ones when you did not pass
  `--detector-sha256` / `--recognizer-sha256`, so declaring a manifest is also
  declaring verification.
- It is checked against the dictionary you loaded; a manifest declaring a
  different entry count describes a different pairing and is refused.
- Its identity fields appear in `--json` output under `model`.
- It does **not** resolve paths, and it never fetches a URL. You still name each
  local file. The URLs in it are provenance for whoever provisions, not download
  instructions.

The schema is `paddleocr-rust/model-manifest/v1`; the committed example is
`tests/fixtures/classic-v1-model-manifest/expected.txt` and the policy behind it
is [`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md).
An unknown key is an error rather than being ignored, because a lenient parser
turns `detector.sha265` into an artifact that looks verified and is not.

### Correcting upside-down lines

Supply the text-line orientation classifier and each detected line is checked and
turned upright before recognition:

```sh
paddleocr-rust ... --orientation PP-LCNet_x1_0_textline_ori/inference.onnx page.png
```

On a page rotated 180 degrees, the difference is not subtle:

```
without    0.883741  Hello   0.899383  Word    0.966466  OCR
with       0.999995  Hello   0.999978  World   0.999919  OCR
```

Without it the recognizer reads upside-down text at low confidence and gets a
word wrong. With it, every line is correct.

Two things it does **not** do. It only decides `0` against `180` — a page rotated
`90` degrees is not handled, and document-level orientation is a different model
this project does not yet support. And it rotates *crops*, not the page, so the
reading order still follows the rotated layout: a 180-degree page reads
bottom-to-top because that is where its lines are.

The default is off, matching upstream. `--orientation-sha256` verifies the
classifier the same way the detector and recognizer digests do.

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

### Parsing a page's structure

```sh
paddleocr-rust structure \
  --ort-dylib   /path/to/libonnxruntime.so.1.28.0 \
  --detector    /path/to/det/inference.onnx \
  --recognizer  /path/to/rec/inference.onnx \
  --dictionary  /path/to/dict.txt \
  --layout      /path/to/PP-DocLayout_plus-L/inference.onnx \
  page.png
```

This is `PP-StructureV3`'s document-parsing path: layout detection, full-page
OCR, block assembly, the reading-order sort, and the Markdown rendering. The
default output is Markdown; `--format json` emits the versioned
`paddleocr-rust/parsing-result/v1` document and `--format text` emits block
contents one per line. `--plain` renders Markdown without upstream's HTML
wrappers.

Supplying `--table-classifier`, `--table-cells`, and `--table-structure` — all
three or none — turns table recognition on, so a table block carries recognized
HTML rather than an image reference. `--route wireless` selects the other model
pair; the classifier's verdict must agree with the pair you loaded, and a
mismatch is refused rather than run against a model that was not trained for
it.

```sh
paddleocr-rust table \
  --ort-dylib ... --detector ... --recognizer ... --dictionary ... \
  --table-classifier /path/to/cls/inference.onnx \
  --table-cells      /path/to/cell/inference.onnx \
  --table-structure  /path/to/str/inference.onnx \
  --format html crop.png
```

`table` recognizes one crop that is already a table, using the crop's own OCR
to fill the cells.

Both commands accept the same `--time-budget-ms`, `--orientation`, and digest
flags as the classic invocation, and both take **exactly one** image. The classic
invocation still takes as many images as you like. Joining several parsed pages
into one Markdown document is implemented and verified against upstream
(`concatenate_markdown_pages`), but is not reachable from a command yet: there is
no structure-over-PDF entry point, so nothing hands it more than one page.

What stays off: formula, seal, chart, and key-information extraction. Those
models have no published ONNX export, so this port has nothing to check itself
against; the configuration is upstream's own `use_*` flags set to `false`, not
a partial imitation of those stages.

### A whole PDF

Needs a build with `--features onnxruntime,pdf`; a build without it says so
rather than reading `pdf` as a filename.

```sh
paddleocr-rust pdf \
  --ort-dylib /path/to/libonnxruntime.so \
  --detector  /path/to/detector/inference.onnx \
  --recognizer /path/to/recognizer/inference.onnx \
  --dictionary /path/to/ppocrv6_dict.txt \
  [--json] [--time-budget-ms 30000] \
  [--first-page 3] [--pages 10] \
  scan.pdf
```

Page numbers are **one-based** on the command line and in the output, matching
what a PDF viewer shows. Text output gains a leading page column; `--json` emits
one frozen `ocr-result/v1` document per page, whose `input.id` is
`<document>#page=<n>` so a page's document survives being piped somewhere.

A page that cannot be read is reported on stderr and **does not stop the run**:

```text
1	0.999980	Hello World
page 2: unsupported capability: pdf.recursive_xobject
1 of 2 selected page(s) could not be read
```

The exit code is `1` when any selected page failed and `0` when none did, so a
script can tell a fully-read document from a partly-read one without parsing
anything. Two failures are still whole-document and exit `2`, because they leave
no pages to report against: a document that cannot be parsed, and one that is
encrypted.

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
| `model` | identity from `--manifest`: family, version, format, backend, and the three digests; `null` when no manifest was given |
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
    &Artifacts::new(
        "/path/to/libonnxruntime.so",
        "/path/to/detector/inference.onnx",
        "/path/to/recognizer/inference.onnx",
    )
    .with_detector_sha256("eb13b44b…d086e1")
    .with_recognizer_sha256("9c09abf0…b673ba"),
    &dictionary,
)?;

for line in engine.recognize_png(&std::fs::read("page.png")?, &OcrOptions::default())? {
    println!("{:.6}\t{}", line.score, line.text);
}
```

`OcrOptions` carries `box_threshold` (`0.6`), `unclip_ratio` (`1.5`),
`drop_score` (`0.5`), and `control`, which holds the time budget and an
`Arc<AtomicBool>` cancellation flag. Build it with the `with_*` methods:

```rust
let options = OcrOptions::default()
    .with_drop_score(0.6)
    .with_control(RunControl::unbounded().with_time_budget(budget));
```

`OcrOptions` and `Artifacts` are `#[non_exhaustive]`, so future options can be
added without breaking your code — which is why the builders exist rather than
struct literals. See
[`STABLE_001_API_REVIEW.md`](STABLE_001_API_REVIEW.md).

### A whole PDF

Needs the `pdf` feature, which is off by default because it costs 32 packages
most callers do not want. The result has one entry per page, and reading it means
handling both cases — that is the point of it:

```rust
use paddleocr_rust::api::PdfPageRange;

let result = engine.recognize_pdf(
    &std::fs::read("scan.pdf")?,
    PdfPageRange::all(),
    &OcrOptions::default(),
)?;

println!("{} of {} pages read", result.recognized(), result.page_count);
for page in &result.pages {
    match &page.outcome {
        Ok(parsed) => {
            for line in &parsed.lines {
                println!("p{}\t{:.6}\t{}", page.index, line.score, line.text);
            }
        }
        // Not a document failure: this page and only this page.
        Err(error) => eprintln!("p{}\t{error}", page.index),
    }
}
```

`page.index` is the **document's** page number, not the position in `pages`, so a
range starting mid-document still tells you where each result came from.
`PdfPageRange::from(first, count)` selects a contiguous slice; the count clamps at
the last page, but a `first` past the end is an error rather than an empty
success. Pages are processed in document order and nothing here spawns a thread —
run one engine per thread over disjoint ranges if you want them in parallel.

**Concurrency:** an `OcrEngine` is `!Sync`, and the compiler enforces it rather
than the documentation. Load one engine per thread. A lock would turn that
compile error into a hidden queue without removing the serialisation, so this
project does not provide one.

## 7. Limits and failure behaviour

Every limit is checked before the memory it bounds is allocated. For file and
stream input that means during the read, not after it: a `200 MB` file is refused
without being loaded, and a stream that never ends is stopped at the bound
rather than exhausting memory. From Rust, `OcrEngine::recognize_path` and
`recognize_reader` apply the same bound.

There is no URL input, and that is a decision rather than an omission. Accepting
one makes this an HTTP client needing scheme and host allow-listing, redirect
handling that survives a redirect into a private network, DNS-rebinding
protection, response size and time limits, and content validation that does not
trust the server's own label. Fetch with a tool built for fetching and pass the
bytes; the network policy then stays where you can see it.

| Limit | Value |
|---|---|
| Encoded input | `64 MiB`, enforced *during* the read |
| Image side | `16,384` pixels |
| Image pixels | `40,000,000` |
| Decode envelope | `256 MiB` across both decode buffers |
| Detected regions | `1,000` |
| Recognition crops | `1,000` |
| Recognition batch | `6` crops per model call |
| PDF document | `256 MiB` |
| PDF pages | `4,096` |
| PDF page pixels | `178,956,970` |
| PDF XObject nesting | `16` levels, `4,096` nodes |

Measured resource use on the reference host, against a `1280×720` page: cold CLI
`4.23 s`, warm median `2.840 s`, peak resident `464.3 MiB`. Full conditions in
[`G3_RESOURCE_EVIDENCE.md`](G3_RESOURCE_EVIDENCE.md). That page is synthetic, so
treat those figures as a lower bound on real scans rather than a representative
one.

**Failure is whole-input, for one image.** Any error from any stage abandons the
entire request and returns that error; no partial line list is ever produced. The
result document has no field marking a result as incomplete, so four lines
returned from a nine-line page would be indistinguishable from a four-line page.
An engine remains usable after a rejected input.

**A PDF is the exception, deliberately.** A document is the first input where
"some of it worked" is meaningful, so `recognize_pdf` returns one outcome per
page: either that page's lines or a typed error naming the page index and the
reason. A broken page does not fail the document and is never silently missing.
The run control is checked **between** pages — a synchronous render cannot be
interrupted from outside — so an exhausted budget yields the pages that finished
plus one `TimedOut` entry rather than a failed document. Two failures are still
whole-document, because they leave no pages to attribute anything to: a document
that cannot be parsed, and one that is encrypted.

The PDF bounds above include one this port owns rather than borrows. The chosen
renderer does not bound recursive form XObjects, and a document that draws a
form from inside itself exhausted `2 GiB` and aborted the process when measured.
So before any page is handed over, its XObject reference graph is walked and a
cycle is refused as `Unsupported`. A cycle in a page you care about is therefore
a refusal, not a crash — and not a rendered page either.

## 8. Known differences from upstream PaddleOCR

These are behavioural, not cosmetic. [`COMPATIBILITY.md`](COMPATIBILITY.md)
carries the authoritative list; this is the part a user is most likely to hit.

- **JPEG decodes within a measured tolerance, not exactly.** PNG is exact
  against the captured OpenCV oracle. JPEG differs by at most `36` in a
  component on pathological few-pixel inputs and `1`–`3` on page-shaped
  content, a difference measured to change no decoded character; the recorded
  decision accepted it. CMYK and 12-bit JPEG return typed `Unsupported`
  errors. See [`IMAGE_DECODER_DECISION.md`](IMAGE_DECODER_DECISION.md) and
  [`IMG_003_DELTA_MEASUREMENT.md`](IMG_003_DELTA_MEASUREMENT.md).
- **PDF pixel fidelity is claimed only for the scan path.** A scanned page —
  an image XObject through `FlateDecode` or `DCTDecode` — reproduces the
  reference renderer bit-identically or within `4` components of 255. Vector
  and text-heavy pages agree closely but **not** exactly, because
  antialiasing and font substitution differ between renderers. A page using a
  standard font it does not embed is the widest divergence, and even there the
  recognized text was character-identical. See
  [`PDF_ENTRY_GATE_EVIDENCE.md`](PDF_ENTRY_GATE_EVIDENCE.md).
- **Formula, seal, chart, and key-information recognition are absent**, and
  not by choice: those models publish no ONNX export, and this project's
  artifact policy forbids converting one locally. Their labels still appear in
  a parsed page, formatted through the plain image handler exactly as upstream
  does when the corresponding `use_*` flag is off. See
  [`P8_ARTIFACT_AVAILABILITY.md`](P8_ARTIFACT_AVAILABILITY.md).
- **Office formats are rejected permanently**, because their text is already
  present and OCR over a rendering of one is strictly worse than reading it.
  See
  [`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md).
- **No training, no VLM, no service, and no C ABI.** All four are out of
  scope by recorded decision rather than unstarted; the release targets are
  the Rust library and the CLI on desktop.
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

## 9. Troubleshooting

Every message below is one this project actually emits. They are grouped by what
you should change, not by which module produced them.

### It exits 2 and says there is no backend

```
paddleocr-rust: this build has no inference backend compiled in.
Rebuild with `--features onnxruntime` to run the classic pipeline.
```

The feature is off by default. Rebuild with `--features onnxruntime`.

### The runtime library will not load

```
backend error: cannot load the ONNX Runtime library from the supplied path
```

`--ort-dylib` must name the shared library file itself, not its directory. The
library must be ONNX Runtime `1.28.0`; an older or newer major API will fail at
load rather than silently misbehave, because the `api-28` feature pins it.

### It says the artifact identity does not match

```
model error: model artifact identity does not match the manifest
```

The file at that path is not the one whose SHA-256 you declared. The three most
common causes, in order: a partial download, the detector and recognizer passed
in the wrong order, and a different revision of the same model family. Check with
`sha256sum` against the table in §3.

### It loads and then fails on the first image

```
backend error: the ONNX Runtime session failed to run
```

Almost always the detector and recognizer swapped. They export the same tensor
names, so the swap **loads** without complaint and only fails here. Declare the
digests and the swap is refused before the runtime sees a byte.

### It says the input is unsupported

```
unsupported capability: image format
```

PNG and JPEG are supported, and the format is decided by the file's content,
not its extension — so a `.png` that is really a bitmap produces this, while a
`.png` that is really a JPEG is decoded correctly. Other formats are refused
rather than guessed at.

### It says a resource limit was exceeded

```
resource limit exceeded for image.encoded_bytes: actual 200000000, limit 67108864
```

The named resource tells you which limit; §7 lists them all. `image.total_pixels`
and `image.width_pixels` come from the PNG header, so they fire on a declared
size before anything is decoded.

### It returns no lines for a page that clearly has text

Nothing is wrong with the models by default. Check, in this order:

1. **Contrast.** The detector runs on the image as-is; there is no binarisation
   or contrast normalisation step.
2. **`--time-budget-ms`**, if set. A budget that expires returns an error, not an
   empty result, so an empty result is not a timeout.
3. **The thresholds.** `box_threshold` `0.6` and `drop_score` `0.5` are the
   upstream defaults. Lowering `drop_score` shows you low-confidence lines that
   were found and filtered, which distinguishes "not detected" from "detected and
   dropped".

### The dictionary count looks wrong

```
dictionary: 18708 entries
```

That is entries, excluding the CTC blank and the appended space, so the model's
class count is `18,710`. If your recognizer expects a different class count,
loading fails with a contract error rather than decoding into the wrong
characters.

### Text comes out as the wrong script entirely

The dictionary decides every scalar the recognizer can emit. Run
`cargo run --example dictionary_census -- <dictionary.txt>` to see what yours
contains, and read [`LANGUAGE_SUPPORT.md`](LANGUAGE_SUPPORT.md) for why a
dictionary containing a script is not the same as that script being supported.

### It is slower than you expected

Session creation is roughly `0.7 s` of every process start. If you are running
the binary once per file, pass the files together instead — the models load once.
Measured figures and their conditions are in
[`PERF_001_BENCHMARK.md`](PERF_001_BENCHMARK.md).

## 10. Reporting a difference

A difference is only actionable with the input that produced it. The most useful
report is a small PNG, the exact command, the output you got, and the output you
expected with its source. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the
fixture rules — anything committed as evidence needs a recorded provenance and a
consuming test.
