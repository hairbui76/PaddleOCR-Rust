# PaddleOCR-Rust

PaddleOCR-Rust is an independent, native-Rust port in progress for selected,
useful PaddleOCR behavior. It is not an official PaddlePaddle or PaddleOCR
release, and it does not wrap Python, PaddleX, or the upstream project.

The project uses the upstream repository as a read-only behavioral reference
while designing an idiomatic, safe Rust library and CLI. Its public scope,
compatibility claims, and delivery order are governed by
[ROADMAP.md](ROADMAP.md).

## Status

`0.1.0` runs a complete classic OCR path end to end. It is still not a general
PaddleOCR replacement, and the table below says exactly where the line is.

| Area | Current state |
| --- | --- |
| Classic OCR path | **Working.** Decode, resize, detect, DB postprocess, reading order, perspective crop, recognize, CTC decode, score filter. Reproduces all four committed end-to-end fixtures exactly, text and confidence. |
| Input formats | **PNG only.** JPEG returns a typed `Unsupported` error rather than a near-miss result; see [docs/IMAGE_DECODER_DECISION.md](docs/IMAGE_DECODER_DECISION.md). |
| Models | One pinned pair: `PP-OCRv6_medium` detector and recognizer, ONNX. No other model, family, or language has been tried. |
| Backend | ONNX Runtime through `ort`, behind the off-by-default `onnxruntime` feature. The default build has no native dependency. |
| Numerical fidelity | Each stage matches a recorded OpenCV or Clipper oracle: contours 18/18, `minAreaRect` 16/16, `box_score_fast` 8/8, `unclip` 16/16, resize 34/34, crop 72 cases, PNG decode 5/5. |
| Resource budgets | **Measured and passing** on one reference host: cold CLI `4.2 s`, warm median `2.8 s`, peak memory `464 MiB`, stripped binary `812 KiB`, `0` bytes of model artifacts in the package. One synthetic 1280x720 page, single threaded; see [docs/G3_RESOURCE_EVIDENCE.md](docs/G3_RESOURCE_EVIDENCE.md) for what that does *not* establish. |
| Not measured | Photographic or scanned input beyond one page sample. Concurrency and throughput. Cancellation and timeouts. Multi-page, PDF, tables, formulas, orientation classification. |
| Not distributable yet | The supply-chain gate `G2` in [docs/ADR_RT004_RUNTIME_SELECTION.md](docs/ADR_RT004_RUNTIME_SELECTION.md) is open: the ONNX Runtime build is not hermetic and has no SBOM. |

Treat a capability as supported only when its row in
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) says so.

## Usage

You supply every artifact explicitly. Nothing is downloaded, cached, or read
from an environment variable.

You need four things:

1. an ONNX Runtime shared library, version 1.28.0;
2. the detector `inference.onnx`;
3. the recognizer `inference.onnx`;
4. a dictionary file, one entry per line, matching the recognizer.

The dictionary can be extracted from the recognizer package's `inference.yml`,
whose `character_dict` list holds one entry per line in order.

```sh
cargo run --release --features onnxruntime --bin paddleocr-rust -- \
  --ort-dylib   /path/to/libonnxruntime.so.1.28.0 \
  --detector    /path/to/det/inference.onnx \
  --recognizer  /path/to/rec/inference.onnx \
  --dictionary  /path/to/dict.txt \
  page.png
```

Output is one `score<TAB>text` line per detection, in reading order:

```text
0.987440	Cedric himself knew nothing
0.999855	whatever about it. It had never been
0.999944	even mentioned to him. He knew that
```

### Machine-readable output

`--json` emits the versioned `paddleocr-rust/ocr-result/v1` document with the
input extent, each quadrilateral in source coordinates, the text, and the
confidence. Field order and numeric formatting are fixed, so the bytes are
deterministic for a given result.

### Verifying the models you load

`--detector-sha256` and `--recognizer-sha256` check an artifact's SHA-256 by
streaming it **before** the model is loaded. A mismatch fails with a typed
error and the runtime never sees the file. Omitting them skips the check, which
is a choice you make rather than a silent default.

### Bounding a run

`--time-budget-ms <n>` gives the whole request a wall-clock budget. The Rust API
exposes the same policy plus a cancellation flag through `OcrOptions::control`.

A run is abandoned only at a stage boundary. A backend call, once started, runs
to completion, so a one-millisecond budget on a real page does not return in one
millisecond — it returns after the detector finishes:

```
paddleocr-rust: time budget exhausted before crop
```

Overshoot is bounded by one backend call: one detector run, or one recognition
batch of at most six crops. A caller needing a hard wall-clock bound must
enforce it out of process. Cancelling or running out of time is a typed error
and never a partial line list, because nothing in the result document marks a
result as truncated.

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Ran; recognized lines, if any, are on stdout. |
| `2` | Bad arguments, unreadable input, unsupported format, or a failed identity check. |

Without the `onnxruntime` feature the binary reports that no backend is
compiled in and exits `2`, rather than pretending to work.

## First delivery target

The first vertical slice — a classic single-image OCR flow on
`x86_64-unknown-linux-gnu` with explicitly provisioned local `PP-OCRv6_medium`
detector and recognizer candidates — now runs. Runtime selection, image
decoding, preprocessing, postprocessing, the public API, and the CLI are
implemented and gated by tests.

What remains before this is a *release* rather than a working path: the
supply-chain gate `G2`, the latency and memory budgets `G3`, wider behavioural
coverage for the detector and recognizer, and the artifact manifest work in
`MOD-002`. Those are tracked in [ROADMAP.md](ROADMAP.md).

The exact candidate identities and their current legal/provenance boundary are
recorded in [docs/MODEL_CANDIDATES.md](docs/MODEL_CANDIDATES.md),
[docs/CANDIDATE_PROVISIONING_LEDGER.md](docs/CANDIDATE_PROVISIONING_LEDGER.md),
and [docs/LICENSE_REVIEW.md](docs/LICENSE_REVIEW.md). Model weights are not
bundled, downloaded, or required by normal tests.

## Development

The bootstrap support profile is Rust `1.94.0` on
`x86_64-unknown-linux-gnu`. Normal development and test runs must not require
Python, PaddleOCR/PaddleX, a GPU, network access, or model weights.

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

At this stage, the binary documents its intentional limitation:

```sh
cargo run
# paddleocr-rust: OCR inference is not implemented yet; no model runtime or artifacts are available
```

`Cargo.lock` is version-controlled for reproducible workspace resolution. Add
or update it only as part of an intentional dependency change.

## Reference boundary

`PaddleOCR/` is a symbolic link to a local upstream checkout at the pinned
baseline recorded in [ROADMAP.md](ROADMAP.md). It is a read-only developer
reference only: this crate's build, tests, runtime, package, and CI must not
depend on that link or on Python. Consult it to understand observable behavior,
then preserve the relevant evidence in this repository using small,
redistributable fixtures.

The upstream project is substantially broader than this port's initial slice,
including document parsing, layouts, tables, formulas, training, services, and
deployment integrations. Those surfaces are not implied by this repository's
name; their classification and planned order are in
[docs/INVENTORY.md](docs/INVENTORY.md) and [ROADMAP.md](ROADMAP.md).

## Documentation and contributing

Start with these documents:

- [ROADMAP.md](ROADMAP.md) — canonical execution plan and acceptance criteria.
- [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) — the only place to check a compatibility claim.
- [docs/README.md](docs/README.md) — index of contracts, evidence, and design records.
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) — contributor workflow.
- [AGENTS.md](AGENTS.md) — repository boundaries, including the read-only upstream rule.

Every change must map to a roadmap item, retain the no-Python/no-upstream
runtime boundary, and include proportionate tests and documentation. See the
fixture and oracle records before changing compatibility behavior or numerical
expectations.

## License

Project-authored source code, documentation, and self-authored fixtures in this
repository are licensed under Apache-2.0 unless a file carries an explicit
third-party notice. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

This license does not grant rights to model weights, datasets, fonts,
dictionaries, converted artifacts, or other third-party materials. Those assets
remain excluded unless their separate provenance and license review is complete.
