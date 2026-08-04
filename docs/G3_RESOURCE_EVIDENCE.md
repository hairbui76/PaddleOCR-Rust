# Gate G3 — Resource and Performance Evidence

Roadmap item: `OCR-003` (post-decision gate `G3` from
[`ADR_RT004_RUNTIME_SELECTION.md`](ADR_RT004_RUNTIME_SELECTION.md))
Measured on: 2026-08-04
Status: All four measurable M2 resource budgets pass on the reference host.

This document records the first actual measurement of the resource and
performance budgets that [`QUALITY_PROFILE.md`](QUALITY_PROFILE.md) declared
before implementation. Until this measurement the profile's own status line read
"Approved P0 budgets; not yet measured", and the ADR listed the budgets as an
open gate. A budget that is never measured is not a budget.

## What was measured

| Dimension | Budget | Measured | Verdict |
|---|---|---|---|
| Cold CLI end-to-end latency | at most `15 s` | `4.23 s`, `4.17 s`, `4.87 s` | Pass |
| Warm end-to-end latency (median) | at most `5 s` | `2.840 s` | Pass |
| Warm end-to-end latency (p95) | at most `10 s` | `2.923 s` | Pass |
| Peak resident memory | at most `2 GiB` | `475,436 KiB` = `464.3 MiB` | Pass |
| Stripped CLI binary | at most `100 MiB` | `812,144 bytes` = `0.77 MiB` | Pass |
| Model artifacts in package | `0 bytes` | `0 bytes` | Pass |
| Determinism of serialized output | byte-identical repeat runs | byte-identical across three runs | Pass |

Every figure is from the same binary, the same artifacts, and the same input in
the same session. They are not assembled from separate runs.

## Reference host and configuration

| Field | Value |
|---|---|
| CPU | Intel Xeon E5-2696 v3 @ 2.30 GHz |
| Logical CPUs available | `72` |
| Threads actually used | `1` intra-op, `1` inter-op; the cold run reported `99%` of one CPU |
| Memory | `131,871,608 kB` total |
| Kernel | `Linux 7.0.0-28-generic x86_64` |
| Target | `x86_64-unknown-linux-gnu` |
| Toolchain | `rustc 1.94.0 (4a4ef493e 2026-03-02)` |
| Profile | `--release`, default release settings, feature `onnxruntime` |
| Backend | ONNX Runtime `1.28.0` CPU through `ort` `=2.0.0-rc.13`, `load-dynamic` |
| ONNX Runtime library | `libonnxruntime.so.1.28.0`, `31,428,768` bytes, SHA-256 `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa` |
| Detector artifact | SHA-256 `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1`, verified before session creation |
| Recognizer artifact | SHA-256 `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba`, verified before session creation |
| Dictionary | `ppocrv6_dict.txt`, SHA-256 `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d`, `18,708` entries |
| Input | `tests/fixtures/classic-v1-benchmark-page/input.png`, `1280x720`, SHA-256 `c68821c155fca558752dbfc263ce08529549d0f219367b4f9e385de132fa9caa` |
| Thresholds | box `0.6`, unclip `1.5`, drop score `0.5` |
| Network | none used; every artifact is an explicit local path |

The host is a measurement point, not a performance claim. `QUALITY_PROFILE.md`
requires a different CPU, OS, runtime, or artifact to be reported as a separate
measurement, and nothing here may be inherited by another target without its own
`PLAT-001` evidence.

## The benchmark input

`QUALITY_PROFILE.md` states the latency and memory budgets against a "1280×720
approved fixture". No such fixture existed, so
`tests/fixtures/classic-v1-benchmark-page` was added for this gate. It is
composed by `tools/generate_benchmark_page_input.py` from three inputs this
repository already commits and pins by hash — `classic-v1-e2e-reading-order`,
`classic-v1-e2e-unicode`, and `classic-v1-e2e-tall-crop` — placed without
overlap on one white page. No new pixels, font binary, OpenCV code, upstream
image, or model asset enters the repository, and regenerating the page from the
same three inputs reproduces it byte for byte.

The fixture is a resource-measurement input, not a semantic oracle. It carries no
expected text and no oracle capture. Its only recorded expectation is
decoder-level: the SHA-256 of the decoded `1280x720` BGR buffer, checked by
`image::tests::the_benchmark_page_decodes_to_the_recorded_bgr_digest`.

The page yields six text lines. That is the correct count — four from the
reading-order source, one from the unicode source, one from the tall-crop
source — and the pipeline recovered all six with confidences from `0.999873` to
`0.999996`:

```
0.999994  Hello
0.999983  World
0.999995  Rust
0.999996  Rust
0.999873  OCR
0.999978  你好
```

## How each figure was produced

### Warm end-to-end latency

`src/pipeline.rs`, module `g3`, test
`warm_end_to_end_latency_stays_inside_the_declared_budget`. Ignored by default
because it needs artifacts this repository never ships.

```sh
PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
PADDLEOCR_RUST_DICTIONARY=<ppocrv6_dict.txt> \
  cargo test --release --offline --locked --features onnxruntime --lib \
  -- --ignored --nocapture g3
```

The models load once. One warmup run is executed and discarded, because the
runtime's first call performs allocation and lazy kernel setup that is cold
cost, and counting it would make the warm figure meaningless. Each of the twenty
measured iterations then times the whole in-process path — decode, detect, crop,
recognize, score-filter — because that is what the budget names.

Twenty samples, in run order:

```
3.059 2.898 2.923 2.842 2.870 2.833 2.775 2.728 2.843 2.853
2.814 2.791 2.780 2.798 2.857 2.724 2.840 2.797 2.896 2.860
```

```
runs=20  min=2.724s  median=2.840s  p95=2.923s  max=3.059s  lines=6
```

Percentiles use the nearest-rank definition, so every reported figure is a time
some run actually produced; with twenty samples the p95 is the nineteenth sorted
value. The test also asserts that all twenty runs recognized the same number of
lines, because a run that did different work is not a comparable sample.

### Cold CLI latency, peak resident memory, and determinism

Three full process invocations under `/usr/bin/time -v`:

```sh
/usr/bin/time -v target/release/paddleocr-rust \
  --ort-dylib <libonnxruntime.so> \
  --detector <detector.onnx> --detector-sha256 eb13b44b…d086e1 \
  --recognizer <recognizer.onnx> --recognizer-sha256 9c09abf0…b673ba \
  --dictionary <ppocrv6_dict.txt> --json \
  tests/fixtures/classic-v1-benchmark-page/input.png
```

| Run | Wall clock | Peak RSS |
|---|---|---|
| 1 | `0:04.23` | `475,436 kB` |
| 2 | `0:04.17` | `472,884 kB` |
| 3 | `0:04.87` | `458,548 kB` |

Run 1 reported `3.90 s` user, `0.32 s` system, and `99%` of one CPU, which is
what confirms the single-thread policy held end to end rather than being merely
requested. The measurement includes both SHA-256 artifact verifications, since
the budget is stated from process start.

The three JSON documents are byte-identical, which is the determinism budget
from the correctness table measured on the same run set.

Peak RSS is what the kernel reports for the process; it excludes OS file cache
as the budget requires, but it does include the mapped ONNX Runtime library and
both model sessions.

### Stripped CLI binary and package contents

```sh
cargo build --release --offline --locked --features onnxruntime --bin paddleocr-rust
```

The release binary is `812,144` bytes. Running `strip` on a copy left the size
unchanged, so the release profile already produces a stripped binary and there
is no debug-information tail to remove.

The `31,428,768`-byte ONNX Runtime library is deliberately **not** counted
against this budget and is deliberately not in the repository. `RT-004` selected
`load-dynamic` precisely so the native library is a caller-supplied local path,
never bundled, downloaded, or required to build or test. A distributor who
chooses to ship it must account for it separately.

Model artifacts in the package are `0` bytes: `/models/` is ignored by
`.gitignore`, and the two ONNX files used here live outside the repository at an
explicitly supplied path.

## What this gate does not establish

- **One input, one host, one artifact pair.** These are the budgets for the
  selected M2 `PP-OCRv6_medium` slice on one machine. They are not a general
  performance characterisation.
- **The page is synthetic.** It is three composed renderings on white, with no
  photographic noise, skew, compression artifacts, or font variety. A latency
  figure measured on it is a lower bound on realistic input, not a
  representative one. Real scans will detect more regions and cost more.
- **No concurrency measurement.** The budgets are single-threaded by
  construction, and `ort`'s session is not concurrently usable in this adapter.
  Throughput under parallel requests is unmeasured.
- **No cancellation or timeout policy.** `RT-005` still owes an explicit
  thread/cancellation policy; this gate measured resources, not the ability to
  bound a runaway request.
- **Not a supply-chain result.** Gate `G2` — hermetic rebuild and SBOM for the
  ONNX Runtime library — remains open and is independent of everything here.
- **Cold latency is dominated by session creation, not by OCR.** Cold `4.2 s`
  against warm `2.8 s` puts roughly `1.4 s` in runtime initialisation, artifact
  hashing, and session construction. Any caller doing more than one image should
  reuse the sessions; the CLI does not, by design, because it processes one
  image.
