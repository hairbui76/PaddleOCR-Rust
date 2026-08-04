# Gate P6 (`M2`) — Evidence

Roadmap phase: P6, milestone `M2`
Recorded: 2026-08-04
Status: **Passed**

Gate P6 requires the approved detector/recognizer path to run end to end through
both the Rust API and the CLI, on the baseline CPU, offline after explicit model
provisioning, without Python or the upstream checkout. Each clause is checked
below against something that was run, not against an intention.

## The clauses

### "the approved detector/recognizer path … end to end"

Gate `G1` reproduces all four committed end-to-end fixtures exactly, text and
confidence within `1e-5`, through the real `PP-OCRv6_medium` artifacts:

```
[reading-order] expected ["Hello", "World", "Rust", "OCR"]
[reading-order] actual   ["Hello", "World", "Rust", "OCR"]
[no-text]       expected []           actual []
[tall-crop]     expected ["Rust"]     actual ["Rust"]
[unicode]       expected ["你好"]      actual ["你好"]
```

A real 486×423 book page returns all twelve of its lines in reading order with
confidences from `0.9857` to `0.9999`.

### "through both Rust API and CLI"

The API path is `tests/end_to_end.rs`, which is restricted to public items so it
cannot pass by reaching inside the crate. Its provisioned half covers no-text,
multi-line reading order, a rotated tall crop, a non-Latin script, mixed scripts,
repeat-run identity, threshold boundaries, artifact failures, four threads with
one engine each, and survival after rejected input.

The CLI path is below.

### "on the baseline CPU"

`x86_64-unknown-linux-gnu`, Intel Xeon E5-2696 v3, `rustc 1.94.0`, single
threaded — `99%` of one CPU measured. Full configuration in
[`G3_RESOURCE_EVIDENCE.md`](G3_RESOURCE_EVIDENCE.md).

### "offline after explicit model provisioning"

Nothing is downloaded, cached, resolved from a search path, or read from an
environment variable. Every artifact is a path the caller names.
[`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md)
records the policy and its reasons.

### "without Python or the upstream checkout"

Demonstrated rather than asserted. The release binary was copied out of the
repository together with one PNG and one manifest, and run from that directory
with a scrubbed environment:

```sh
ldd paddleocr-rust
	linux-vdso.so.1
	libgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1
	libm.so.6     => /lib/x86_64-linux-gnu/libm.so.6
	libc.so.6     => /lib/x86_64-linux-gnu/libc.so.6
	/lib64/ld-linux-x86-64.so.2
```

No Python, and no ONNX Runtime either: the runtime is opened at the path given on
the command line, which is what `RT-004` chose `load-dynamic` for.

```sh
env -i PATH=/usr/bin:/bin HOME=$HOME ./paddleocr-rust \
  --ort-dylib  <libonnxruntime.so> \
  --detector   <detector.onnx> \
  --recognizer <recognizer.onnx> \
  --dictionary <ppocrv6_dict.txt> \
  --manifest manifest.txt --json input.png
```

```
manifest: PP-OCRv6_medium 2026-08-04 (onnx via onnxruntime)
dictionary: 18708 entries
image: input.png (800x320 PNG)
{"schema_version":"paddleocr-rust/ocr-result/v1","input":{…},"model":{…},"lines":[…]}
```

The working directory contained three files. The repository, the upstream
symlink, and every Python interpreter were irrelevant to the run.

## What the gate does not certify

Passing P6 means the classic path works and is documented. It does not mean the
project is releasable, and three specific things remain open:

- **Gate `G2`** — a hermetic ONNX Runtime rebuild with an SBOM. It blocks
  distribution, not use, and it cannot fully close on this host because the
  source-tag signature's public key is not available locally.
- **`MOD-003` and `MOD-004`** — manifest-driven path resolution, and opt-in
  downloads if ever approved. A manifest today records identity and provenance;
  the caller still names every path.
- **One artifact pair, one input format.** PNG only, `PP-OCRv6_medium` only, and
  the verified scripts are narrower than the dictionary's contents. See
  [`LANGUAGE_SUPPORT.md`](LANGUAGE_SUPPORT.md).

P7 onwards — document preprocessing, PDF and multipage, structure, VLM, serving,
training, release — is untouched.

## Test totals at the gate

`250` tests pass offline with no artifacts and no network, across the library,
the contract suite, the fixture-integrity gate, the foundation suite, and the
public-surface end-to-end suite. Eleven further tests are ignored by default and
need explicitly provisioned artifacts: gates `G1`, `G3`, `PRE-001`, engine reuse,
and the five provisioned `E2E-001` cases. All were run for this record.

`cargo fmt --check` is clean, and `cargo clippy -D warnings --all-targets` is
clean **both** with and without `--all-features` — a gate that runs one feature
set is not a gate for the other.
