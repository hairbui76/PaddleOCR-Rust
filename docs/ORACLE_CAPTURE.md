# Isolated Oracle Capture Procedure

Roadmap item: ORACLE-001
Status: Procedure complete; one narrow component crop capture and two narrow
model-backed classic ONNX captures are committed
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose

This procedure produces reviewable expected results for the selected classic
M2 behavior without making PaddleOCR, Python, PaddleX, a network connection,
or a GPU a normal Rust test dependency.

It applies only to the legacy classic source paths named in
`CLASSIC_OCR_CONTRACT.md`. Modern wrapper/pipeline captures remain blocked
until a separate exact PaddleX resolver/oracle is chosen.

## Non-negotiable isolation rules

1. Never run Python, package installation, tests, formatters, downloads, or
   model conversion inside `./PaddleOCR` or its symlink target.
2. Create a separate immutable checkout at the pinned commit outside this
   repository. Record its origin URL, commit, and cleanliness before execution.
3. Create an ephemeral virtual environment and model-cache directory outside
   both repositories. Set cache/home paths explicitly so a command cannot write
   through the developer's upstream checkout.
4. Supply only pre-reviewed local fixture inputs and pre-reviewed local model
   artifacts. Do not let capture code automatically fetch assets.
5. Network access, GPU use, and non-deterministic accelerators are disabled for
   the capture unless a separately approved evidence record requires them.
6. Copy only small JSON/JSONL expected-result data and metadata back into this
   repository after review. Never copy a virtual environment, cache, model,
   binary, upstream checkout, or generated build artifact.

## Required capture metadata

Each capture record must include:

- fixture identifier, SHA-256, provenance, and license review;
- upstream Git remote/ref/commit and clean status;
- exact Python, Paddle/Paddle Inference, OpenCV, NumPy, and operating-system
  versions;
- exact detector/recognizer artifact names, local file hashes, format, and
  dictionary hash;
- legacy option profile from `CLASSIC_OCR_CONTRACT.md`;
- CPU model, thread count, provider/backend configuration, and whether a GPU
  was disabled;
- command template with credential-free paths redacted or made relative;
- raw detector-map/recognizer-output captures when necessary for P3 tolerance
  diagnosis, stored only when their asset terms allow it;
- expected JSON/JSONL schema version, output SHA-256, and tolerance rule.

## Capture workflow

1. Confirm that the fixture and artifact are legally approved for local use;
   otherwise stop before running an oracle.
2. Create the isolated checkout and virtual environment outside this project.
   Verify the checkout commit and `git status --short` before and after.
3. Configure legacy `TextDetector`/`TextRecognizer` options from the frozen
   M2 profile, with CPU execution and no automatic downloads.
4. Run exactly one approved fixture at a time. Capture detector quadrilaterals,
   crop ordering/rotation diagnostics, recognizer raw outputs when needed, and
   final filtered results.
5. Normalize only explicitly documented nondeterministic fields such as elapsed
   time. Do not round coordinates/scores or rewrite text merely to make a
   comparison pass.
6. Store the expected result and metadata under `tests/fixtures/` using the
   policy in `tests/fixtures/README.md`. Review it as source data.
7. Run the Rust differential test offline. A mismatch creates or updates a
   contract/tolerance investigation; it must not silently replace the golden.

## Component crop oracle

Before a model artifact is available, a maintainer may capture narrow,
self-authored OpenCV crop evidence with
[`CROP_ORACLE_CAPTURE.md`](CROP_ORACLE_CAPTURE.md). Its tool runs outside both
repositories, records the exact OpenCV/NumPy environment, writes only stdout,
and has no model or upstream-checkout dependency. It does not replace this
procedure for end-to-end capture or prove decoder/model compatibility.

The reviewed component capture at
[`tests/fixtures/classic-v1-crop-oracle/`](../tests/fixtures/classic-v1-crop-oracle/)
is intentionally limited to four self-authored BGR crop cases. It does not
meet the model, input-image, or end-to-end metadata requirements above.

## Model-backed end-to-end oracles

The reviewed
[`classic-v1-e2e-no-text`](../tests/fixtures/classic-v1-e2e-no-text/) fixture
is a deliberately small exception to the previous absence of model-backed
captures. It uses the exact revision-pinned ONNX detector and recognizer whose
terms evidence is recorded in [`LICENSE_REVIEW.md`](LICENSE_REVIEW.md), checks
their SHA-256 values before loading, and records the matching dictionary hash.

The capture ran in a separate clean checkout of the pinned classic source on
CPU with ONNX Runtime, one intra/inter-op thread, no GPU, no automatic model
fetch, and two fresh processes whose compact stdout digests match. The source
was passed the 3-by-2 BGR byte array already recorded by
`classic-v1-image-inputs`; it did not decode the committed `input.png` during
this capture. The final classic result is exactly `lines: []`. The capture
document records package versions, option values, thread/cache environment,
temporary-harness hash, candidate revisions/hashes, and its source-result
digest.

This is fixture evidence only. It does not select an image decoder, native
Rust inference backend, public OCR API, CLI behavior, model manifest, cache or
download policy. It also provides no text, quadrilateral, score, reading-order,
threshold, malformed-input, or resource-limit evidence. No model bytes,
dictionary entries, raw tensors, virtual environment, external checkout, cache,
or capture harness were committed.

The separate
[`classic-v1-e2e-reading-order`](../tests/fixtures/classic-v1-e2e-reading-order/)
fixture uses the same exact candidate pair, isolated clean classic checkout,
CPU ONNX Runtime settings, and two-fresh-process reproducibility rule. It
records a 800-by-320 BGR canvas with the self-authored words `Hello`, `World`,
`Rust`, and `OCR`, rendered by an external `cv2.putText` call using
`FONT_HERSHEY_SIMPLEX`. The committed PNG is self-authored project fixture
material; no font binary, OpenCV source/binary, upstream image, model byte,
dictionary entry, raw tensor, or capture harness is retained.

The oracle passed the rendered BGR pixels directly to classic `TextSystem`,
rather than decoding the committed PNG. Its exact result captures four text
values, confidence values, quadrilaterals, and the observed top-to-bottom,
left-to-right order. It is one synthetic arrangement only. It does not prove a
Rust decoder, geometry implementation, runtime/backend, score tolerance,
Unicode behavior, threshold behavior, error behavior, or a functional Rust OCR
path.

## Prohibited shortcuts

- Do not invoke the `PaddleOCR/` symlink as a test oracle.
- Do not use a live model URL as a test fixture.
- Do not capture a modern wrapper/pipeline as if it represented classic
  behavior.
- Do not accept output solely because visible text appears plausible.
- Do not treat a regenerated golden file as validation without reviewing the
  provenance, artifact hash, output difference, and tolerance.

## Completion condition

`ORACLE-001` is complete as a procedure. `FIX-001` and `TOL-001` remain
incomplete: narrow no-text and synthetic reading-order fixtures exist, but
tall-crop, Unicode, threshold, malformed-input, resource-limit, decoder,
raw-tensor, and actual Rust differential coverage has not been completed.
