# M2 Public API and JSON Contract

Roadmap items: `API-DEC-001`, `CTR-002`
Status: Frozen P2 contract; no OCR implementation or JSON serializer exists yet
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Applies to: the planned M2 classic DB + CTC OCR slice only

## Decision

`D-009` is resolved for M2 as follows:

- The library exposes a typed, idiomatic Rust result rather than reproducing a
  Python object graph or PaddleX wrapper API.
- M2 JSON is a native, versioned interchange format. Its exact identifier is
  `paddleocr-rust/ocr-result/v1`; it is not PaddleOCR/PaddleX JSON parity.
- One library call processes one encoded image. It returns one complete result
  or one structured error; it never returns fabricated or partially successful
  OCR output.
- The library accepts encoded bytes, not a user-controlled filesystem path or
  URL. A future CLI may read an explicitly supplied local file and pass its
  bytes to the same library boundary.
- Result metadata identifies the exact locally provisioned detector,
  recognizer, and recognizer dictionary. P3 must supply and validate those
  identities before a successful result can be emitted.

This decision does not select an inference backend, a model format, artifact
paths, model download mechanism, decoder crate, or a complete CLI syntax.
Those remain subject to the declared P3, P4, and P6 gates.

## Reference boundary and intentional differences

The selected classic implementation returns parallel `filter_boxes` and
`filter_rec_res` values from `tools/infer/predict_system.py`. Its script output
uses `transcription` and `points`. The current modern OCR documentation instead
describes fields such as `input_path`, `page_index`, `dt_polys`, `dt_scores`,
`rec_texts`, `rec_scores`, `rec_polys`, and `rec_boxes` in
`docs/version3.x/pipeline_usage/OCR.en.md`.

The native contract deliberately does not claim either schema as its own:

| Upstream observable surface | M2 native rule | Reason |
|---|---|---|
| Classic parallel boxes and recognition results | One `OcrLine` owns its quadrilateral, text, and confidence. | Parallel arrays can become length/order mismatches. |
| Modern `input_path` | No path is serialized. An opaque caller-provided `input.id` may be serialized. | Avoids path leakage and does not make paths part of the library API. |
| Modern `page_index` | `input.page_index` is always `null` in M2. | M2 supports a single image only; P7 must amend the contract before non-null pages are emitted. |
| Modern detector arrays and rectangular `rec_boxes` | No detector score or derived axis-aligned box is emitted in M2. | The selected classic final result does not expose detector scores, and duplicate geometry invites drift. |
| Modern document-preprocessor/orientation settings | Omitted. | Those capabilities are absent from M2. |
| Python `None`/ambiguous no-text paths | `lines: []` is the sole successful no-text representation. | Typed Rust output has one explicit empty result state. |

The classic geometry/order and CTC semantics are defined by
[`CLASSIC_OCR_CONTRACT.md`](CLASSIC_OCR_CONTRACT.md). This document defines the
public boundary around those semantics; it does not broaden them.

## Planned typed library surface

The following names and ownership model are the M2 public contract. Their Rust
implementation is deferred until P3–P6 have the model, decoder, geometry, and
pipeline evidence required to make them functional.

```rust
pub struct Ocr;

impl Ocr {
    pub fn recognize(&self, request: OcrRequest<'_>) -> Result<OcrResult>;
}

pub struct OcrRequest<'a> {
    pub image: EncodedImage<'a>,
    pub input_id: Option<&'a str>,
    pub options: OcrOptions,
}

pub struct OcrOptions {
    pub recognition_score_threshold: Score,
}

pub struct OcrResult {
    // Owned input metadata, model provenance, and reading-ordered lines.
}

pub struct OcrLine {
    // One source-image quadrilateral, UTF-8 text, and recognition confidence.
}
```

`OcrRequest` borrows only for the duration of `recognize`; every successful
result owns its text and metadata and may outlive the engine. Engine
construction from local artifacts is intentionally not shown above: P3 must
first specify hash validation, model/dictionary layout, tensor ABI, and backend
requirements. The resulting engine must not invoke Python, PaddleX, a network,
or the `PaddleOCR/` symlink.

`EncodedImage` is the existing checked borrowed-byte foundation and represents
exactly one byte slice. It currently enforces only non-empty input and the
`64 MiB` encoded-byte bound; M2 intends to support content-detected PNG and
JPEG subject to the remaining bounds in
[`QUALITY_PROFILE.md`](QUALITY_PROFILE.md); the exact decoder implementation
and malformed-format behavior remain `IMG-DEC-001`/`IMG-001` work. The engine
does not interpret `input_id` as a path, URL, format hint, or command. When
present, it is an opaque UTF-8 identifier of at most 256 bytes with no control
characters.

`OcrOptions::default()` uses a recognition score threshold of `0.5`, retaining
scores equal to the threshold. A caller may lower but never bypass the hard
resource limits. Detector resize, DB thresholds, unclip ratio, orientation,
document unwarping, and model/language selection are not M2 public options;
they are fixed by the classic profile or absent as documented in
[`CLASSIC_OCR_CONTRACT.md`](CLASSIC_OCR_CONTRACT.md).

The M2 library API makes no thread-safety, cancellation, batch, document, or
multi-page promise beyond one synchronous image request. Those contracts are
introduced only when their owning roadmap items have implementation evidence.

## Result invariants

For a successful M2 call:

1. `OcrResult` represents exactly one decoded image with non-zero dimensions
   within the M2 resource limits.
2. `page_index` is `None`; M2 has no PDF, office-document, GIF, or multi-page
   input surface.
3. `lines` is in the stable reading order defined by the classic contract.
4. Every line has a strictly convex source-image quadrilateral ordered as
   top-left, top-right, bottom-right, bottom-left after the documented
   detector/filter steps. Its serialized coordinates are integral and clipped
   to the decoded source image's inclusive pixel range.
5. Every `text` value is preserved as decoded UTF-8. The API performs no
   Unicode normalization, case folding, whitespace cleanup, Arabic reversal,
   or language inference beyond the selected artifact's verified dictionary.
6. Every `confidence` is finite and in `[0.0, 1.0]`. It is the classic CTC
   recognition confidence, not a detector score or a calibrated probability.
7. `lines: []` is a successful no-text result. A missing/corrupt/wrong model,
   invalid tensor, malformed input, unsupported feature, or resource limit
   returns `Err(Error)` and no `OcrResult`.
8. Detector and recognizer provenance is complete and validated. A model family
   label without the approved artifact/dictionary hashes is insufficient.

An empty decoded string can only be retained when a caller intentionally sets a
threshold that permits its `0.0` CTC score. The default threshold excludes it.

## JSON result schema v1

The planned serializer emits one UTF-8 JSON object with this shape. The example
is illustrative only; it is not a golden result or evidence for any model.

```json
{
  "schema_version": "paddleocr-rust/ocr-result/v1",
  "input": {
    "id": "receipt-001",
    "page_index": null,
    "width": 640,
    "height": 480
  },
  "models": {
    "detector": {
      "family": "PP-OCRv6_medium_det",
      "version": "artifact-manifest-version",
      "artifact_sha256": "<64 lowercase hexadecimal characters>"
    },
    "recognizer": {
      "family": "PP-OCRv6_medium_rec",
      "version": "artifact-manifest-version",
      "artifact_sha256": "<64 lowercase hexadecimal characters>",
      "dictionary_sha256": "<64 lowercase hexadecimal characters>"
    }
  },
  "lines": [
    {
      "quad": [[12, 34], [210, 34], [210, 68], [12, 68]],
      "text": "Example",
      "confidence": 0.98
    }
  ]
}
```

All fields below are required unless their type explicitly permits `null`:

| Field | Type and constraints | Meaning |
|---|---|---|
| `schema_version` | Exact string `paddleocr-rust/ocr-result/v1` | Selects this native result contract. |
| `input.id` | UTF-8 string or `null`; no controls; at most 256 bytes | Opaque caller identifier. It is never inferred from a path. |
| `input.page_index` | Non-negative integer or `null` | Always `null` for M2. |
| `input.width`, `input.height` | Positive integer within M2 image limits | Decoded source-image dimensions in pixels. |
| `models.detector.family`, `models.recognizer.family` | Non-empty UTF-8 identifier | Validated model family identity. |
| `models.*.version` | Non-empty UTF-8 identifier | Version from the approved local manifest. |
| `models.*.artifact_sha256` | Exactly 64 lowercase hexadecimal characters | SHA-256 of the loaded model artifact. |
| `models.recognizer.dictionary_sha256` | Exactly 64 lowercase hexadecimal characters | SHA-256 of the loaded recognition dictionary. |
| `lines` | Array of at most 1,000 elements | Final retained OCR lines in reading order; an empty array means no text. |
| `lines[].quad` | Exactly four `[x, y]` integer pairs | Source-image points in top-left, top-right, bottom-right, bottom-left order. Each is clipped to `[0, width - 1] × [0, height - 1]`. |
| `lines[].text` | UTF-8 JSON string | Exact CTC-decoded text; no additional normalization. |
| `lines[].confidence` | Finite JSON number in `[0.0, 1.0]` | CTC recognition score for that line. |

The result deliberately contains neither an axis-aligned bounding box nor a
detector score. Both can be derived or are unavailable in the selected classic
final output, so emitting them would create a false compatibility surface.

## Serialization and JSONL rules

The implementation of `SCHEMA-001` must satisfy these rules before the schema
can be advertised:

- Serialize valid UTF-8 JSON without a byte-order mark. Non-ASCII text remains
  Unicode text rather than being deliberately ASCII-escaped.
- Emit root fields in this order: `schema_version`, `input`, `models`, `lines`.
  Emit nested fields in the order shown in the example. No timestamps, paths,
  timings, backend diagnostics, thread IDs, or process-specific values appear
  in a result record.
- Reject non-finite values before serialization. Do not serialize `NaN`,
  infinity, or out-of-range coordinates.
- The CLI's future JSONL mode emits one compact result object per successful
  input, terminated by exactly one `\n`; it never writes progress text to
  stdout. CLI batching/error continuation semantics remain `CLI-001` work.
- The default deterministic configuration must produce byte-identical JSONL
  for repeated same-build, same-artifact, single-thread runs as required by
  `QUALITY_PROFILE.md`. Pretty formatting, if later added, is not the
  byte-stability interchange mode.
- A breaking semantic or structural change requires a new `schema_version`.
  Writers must not silently change the meaning of a v1 field. Later pipeline
  types receive their own contracts instead of overloading M2 fields.

## Structured error boundary

Library callers receive the existing `Error` hierarchy. P6 may add an opt-in
machine-readable CLI error record, but it must map to the following stable
categories without including a user-controlled path, URL, document text, model
contents, credentials, or raw backend trace:

| `Error` variant | Stable category | Safe structured details |
|---|---|---|
| `InvalidInput` | `invalid_input` | `field`, `violation` |
| `ResourceLimit` | `resource_limit` | `resource`, `limit`, `actual` |
| `Model` | `model` | `problem` |
| `Backend` | `backend` | implementation-controlled stable diagnostic code only |
| `Io` | `io` | `operation` only |
| `Unsupported` | `unsupported` | `capability` |
| `Cancelled` | `cancelled` | no extra detail |

The exact CLI exit codes, stderr encoding, multi-input continuation behavior,
and any error-record envelope are intentionally deferred to `CLI-001`. They
must use this mapping and keep successful JSON/JSONL output separate from
diagnostics.

## Verification requirements

`API-DEC-001` and `CTR-002` are complete as design contracts only. Before a
serializer or public OCR API is marked implemented or verified:

1. P3 must provide exact artifact and dictionary provenance/hash validation.
2. P4/P5 must prove the geometry, preprocessing, DB, and CTC invariants.
3. P6 must add schema round-trip, malformed-value, Unicode, no-text,
   deterministic-order, and byte-stability tests using approved offline
   fixtures.
4. `COMPATIBILITY.md` must link the contract, fixture, tolerance, and actual
   validation evidence before its M2 API/CLI rows become `Verified`.
