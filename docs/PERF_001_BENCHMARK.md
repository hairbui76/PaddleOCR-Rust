# `PERF-001` / `PERF-002` — Reproducible Benchmark Record

Roadmap items: `PERF-001` (record), `PERF-002` (verdict against budgets)
Measured: 2026-08-04
Status: every predeclared budget met; none amended

`PERF-001` requires a benchmark that records hardware, OS and toolchain, artifact
hashes, corpus, warmup, threads, latency, throughput, startup, peak memory, and
binary and model size, together with the limits of the comparison.
[`G3_RESOURCE_EVIDENCE.md`](G3_RESOURCE_EVIDENCE.md) already covers latency,
memory, and size. This adds the two dimensions it did not measure — **startup**
and **throughput** — and states the verdict `PERF-002` asks for.

## Configuration

| Field | Value |
|---|---|
| CPU | Intel Xeon E5-2696 v3 @ 2.30 GHz, `72` logical CPUs available |
| Threads used | `1` intra-op, `1` inter-op; a cold run measured at `99%` of one CPU |
| Memory | `131,871,608 kB` |
| Kernel | `Linux 7.0.0-28-generic x86_64` |
| Toolchain | `rustc 1.94.0 (4a4ef493e 2026-03-02)`, `--release` |
| Backend | ONNX Runtime `1.28.0` CPU, `ort` `=2.0.0-rc.13`, `load-dynamic` |
| Runtime library | SHA-256 `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa`, `31,428,768` bytes |
| Detector | SHA-256 `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1`, `62,032,837` bytes |
| Recognizer | SHA-256 `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba`, `76,554,979` bytes |
| Dictionary | SHA-256 `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d`, `18,708` entries |
| Corpus | `tests/fixtures/classic-v1-benchmark-page/input.png`, `1280x720`, SHA-256 `c68821c1…fa9caa`, six text lines |
| Warmup | in-process latency discards one run; the CLI figures below include no warmup and say so |
| Network | none |

## Throughput and startup

One CLI process, one engine, the same page repeated. This is the measurement the
multi-image mode exists for, and it is the one that separates startup from
steady-state cost.

| Pages | Wall clock | Peak RSS | Marginal cost per page |
|---|---|---|---|
| `1` | `3.51 s` | `448,556 kB` | — |
| `5` | `17.63 s` | `608,964 kB` | `3.53 s` |
| `20` | `56.69 s` | `607,488 kB` | `2.80 s` |

**Startup** is `56.69 - (20 × 2.80) = 0.69 s`: runtime initialisation, artifact
streaming and hashing, and two session creations. It agrees with the `0.534 s`
in-process engine load measured separately, the difference being process start
and dictionary parsing.

**Throughput** at steady state is `1 / 2.80 = 0.357 pages per second`, single
threaded, on this page. The `20`-page marginal cost matches the independently
measured warm median of `2.840 s`, which is the cross-check that the two
measurements describe the same work.

The `5`-page figure is *higher* than the `20`-page one because amortising a fixed
`0.69 s` over four marginal pages leaves more of it in the average. Reporting
both is deliberate: quoting only the `5`-page number would understate throughput,
and quoting only the `20`-page number would hide how long the fixed cost takes to
disappear.

### Memory does not grow with the page count

Peak RSS is `608,964 kB` at five pages and `607,488 kB` at twenty. It rises from
the single-image figure — the runtime's arenas grow to their working size — and
then **plateaus**. Twenty pages through one engine do not accumulate memory,
which is the property that matters for a caller processing a directory and the
one a single-image measurement cannot show.

### Determinism across a batch

All twenty emitted JSON documents are byte-identical (`sort -u` yields one line).
This is the determinism budget observed across a long single-process run, in
addition to the three separate cold processes and the twelve concurrent documents
recorded in [`CONC_001_EVIDENCE.md`](CONC_001_EVIDENCE.md).

## `PERF-002` — verdict against the predeclared budgets

`QUALITY_PROFILE.md` fixed these before implementation. None was amended, and
none needed to be.

| Budget | Declared | Measured | Verdict |
|---|---|---|---|
| Cold CLI end-to-end latency | at most `15 s` | `4.23 s` | Pass |
| Warm end-to-end latency, median | at most `5 s` | `2.840 s` | Pass |
| Warm end-to-end latency, p95 | at most `10 s` | `2.923 s` | Pass |
| Peak resident memory | at most `2 GiB` | `608,964 kB` = `594.7 MiB` worst observed | Pass |
| Stripped CLI binary | at most `100 MiB` | `812,144 bytes` | Pass |
| Model artifacts in package | `0 bytes` | `0 bytes` | Pass |
| Encoded input rejection | over `64 MiB` | enforced during the read | Pass |
| Determinism | byte-identical repeats | identical across `20` runs in one process, `3` processes, and `12` concurrent runs | Pass |

The peak-memory row uses the **worst** figure observed anywhere in this record,
which is the multi-image run rather than the single-image one G3 reported. A
budget should be judged against the least favourable measurement taken, not the
most convenient.

Startup and throughput have **no predeclared budget**. They are recorded here as
a baseline for future comparison, and deliberately not retrofitted with a budget
now: writing a threshold after seeing the number is exactly the practice
`QUALITY_PROFILE.md` forbids, and a budget invented to fit a passing measurement
proves nothing.

## Limits of this comparison

- **One host, one page, one artifact pair.** Nothing here transfers to another
  CPU, OS, runtime build, or model without its own measurement, and
  `PLAT-001` remains open precisely because Rust portability is not platform
  evidence.
- **The page is synthetic** — three composed renderings on white, with no
  photographic noise, skew, compression artifacts, or font variety. A real scan
  detects more regions and costs more. Every figure here is a lower bound on
  realistic input.
- **Single threaded by construction.** No parallel throughput was measured, and
  raising intra-op threads would change the numerical-variation position recorded
  in `CONC-001` before it changed the timings.
- **No comparison against upstream PaddleOCR.** Upstream runs a different
  runtime through a Python process; a wall-clock comparison between them would
  measure the two runtimes and the interpreter, not this port's arithmetic. That
  comparison would need its own controlled setup and is not claimed here.
