# `ROB-001` — Robustness Evidence

Roadmap item: `ROB-001` (with `FUZZ-001`)
Recorded: 2026-08-04
Scope: corrupt models and configs, bad tensors, non-finite values, invalid token
indices, backend failures, cancellation, cleanup, and partial success

`ROB-001` asks that each of these is *verified*, not merely handled. Where the
verification already existed it is cited; where it did not, it was added for this
record, and the fuzz driver was extended because it was covering the wrong half
of the attack surface.

## 1. The fuzz driver was missing the parsers

`src/fuzz.rs` drove the checked public types and the DB, CTC, geometry, and crop
kernels. Those take *derived* values — tensors and polygons the pipeline
produced. It did not drive any surface that consumes caller-supplied bytes
directly, which is where a hostile document actually arrives.

Four surfaces were added:

- **the PNG decoder**, whose resource envelope is enforced from the declared
  header, so a hostile header is the interesting input rather than a hostile
  pixel stream;
- **the manifest parser**, reached through a lossy UTF-8 conversion so arbitrary
  bytes get past the encoding check instead of stopping there;
- **the bounded stream reader**, driven by a reader that hands out the input in
  small irregular pieces rather than one slice, because a single-slice reader
  never exercises the accumulation path;
- **the dictionary parser**, which decides every scalar a recognizer can emit.

## 2. Campaign

`4,000` inputs from four generators: uniform random bytes; bit-flip mutations of
the committed fixture PNGs and the manifest; truncations of the same at every
offset; and synthetic PNG headers with a valid signature and hostile declared
fields — dimensions of `0`, `1`, `16,385`, `65,535`, `2³¹−1`, and `2³²−1`, bit
depths including the invalid `7`, colour types including the invalid `5`, and
both interlace values.

Run twice over the same corpus:

| Build | Cases | Failures |
|---|---|---|
| `--release` | `4,000` | `0` |
| debug, with overflow checks and `debug_assertions` | `4,000` | `0` |

The debug run is the one that matters for arithmetic. A release build wraps on
integer overflow silently; the debug build panics, so a wrong-by-overflow index
computation in the header arithmetic would have surfaced there and not in the
release run. Both were clean.

The corpus is seeded deterministically, so the run is reproducible. It is not
committed: `4,000` mutation files are not a fixture, and the generator is a few
lines that belong in the record rather than in the repository.

## 3. Clause-by-clause verification

| Clause | Verified by |
|---|---|
| Corrupt model | `tests/end_to_end.rs` — a text file loaded as a model, a missing artifact, a digest mismatch |
| Corrupt config | `src/manifest.rs` — unknown key, duplicate key, missing key, malformed digest, zero byte count, wrong schema version, no separator, oversized, too many lines |
| Bad tensors | `src/backend.rs` — wrong output name, wrong output shape, inputs rejected before reaching the backend, budget rejection; `src/detector.rs` and `src/recognizer.rs` — wrong rank, wrong channel count, wrong extent, wrong row count, class-count mismatch |
| NaN and infinity | `run_validated` rejects non-finite output; `classic_db_boxes` rejects NaN and both infinities in the probability map; `CtcScoreMatrix` rejects non-finite scores |
| Invalid token index | `src/dictionary.rs` — an out-of-range class is a contract error, not a panic |
| Backend failure | `src/backend.rs` maps a failing backend to a typed error; observed end to end as `backend error: the ONNX Runtime session failed to run` with exit `2` |
| Cancellation | `src/control.rs` and `src/pipeline.rs` — a cancelled run returns `Error::Cancelled` **and no lines**; an exhausted budget reports the stage it stopped before |
| Cleanup | `tests/end_to_end.rs` — an engine keeps working after three rejected inputs; `PERF-001` shows peak memory plateaus across twenty pages rather than accumulating |
| No fabricated partial success | `run_classic_ocr` fails whole-input by construction, and the cancellation test asserts the empty result rather than a truncated one |

## 4. The one that deserves its own paragraph

**No fabricated partial success** is the clause most easily satisfied on paper
and most easily violated in practice, because returning what you have feels
helpful. This project cannot: the result document has no field marking a result
as incomplete, so four lines returned from a nine-line page would be
indistinguishable from a four-line page. Any error from any stage abandons the
whole request.

That is a deliberate cost. A caller who wants per-page recovery over a document
must run pages separately, and `MPAGE-001` owns revisiting it if multipage input
ever exists — with the recovery made visible in the output type, which is the
only way it could be safe.

## 5. What this does not establish

- **No coverage-guided fuzzing.** The campaign is generation and mutation
  driven, not feedback driven, so it explores shallowly. A `libFuzzer` or AFL
  run against the same driver would go deeper and is the obvious next step.
- **No fuzzing of the inference path.** The driver deliberately performs no
  model loading or inference, so the backend adapter's behaviour under hostile
  *model* bytes is covered by targeted tests rather than by fuzzing.
- **`4,000` cases is a smoke test, not a campaign.** It is enough to say the
  parsers do not panic on obvious hostile input; it is not enough to say they
  never will.
