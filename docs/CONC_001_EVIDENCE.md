# `CONC-001` — Concurrency Evidence

Roadmap item: `CONC-001`
Recorded: 2026-08-04
Scope: the classic OCR path; there is no service, worker pool, or training loop
in this project yet

`CONC-001` asks for proof of documented thread safety, bounded queues and
workers, session reuse, deterministic order, allowed numerical variation,
cancellation, and clean shutdown. Several of those have an answer that is
"there is none, deliberately", and saying so is more useful than inventing a
mechanism to describe.

## 1. Thread safety — enforced by the compiler, not by documentation

`OcrEngine` holds each backend's session in a `RefCell`, so it is `!Sync`. Rust
will not compile a program that shares one engine across threads behind a shared
reference. The documented position — **one engine per thread** — is therefore not
a convention a caller can violate by accident.

This is asserted rather than assumed. `api::concurrency_position` uses an
auto-trait probe to check the property, because a normal test cannot assert
`!Sync`: a type that *is* `Sync` would simply compile. If a future change adds a
lock to make the engine shareable, that test fails. That is the intended outcome
— a lock would convert a compile error into a hidden queue rather than removing
the serialisation, and a caller would silently lose the parallelism they thought
they had bought.

## 2. Bounded queues and workers — there are none

This project spawns no threads. There is no worker pool, no queue, no scheduler,
and no async runtime. A call to `recognize_png` runs to completion on the calling
thread.

Both ONNX Runtime sessions are created with intra-op and inter-op thread counts
of `1`. The measured cold run used `99%` of one CPU, which is what confirms the
setting took effect rather than merely being requested.

Concurrency is therefore entirely the caller's to arrange, and the only bound
that matters is the one they choose. That is a smaller promise than a worker pool
would be, and it is one this project can actually keep.

## 3. Session reuse

`OcrEngine::load` creates both sessions once; every subsequent `recognize_png`
reuses them. Measured load is `0.534 s` warm, against roughly `1.4 s` of the
`4.2 s` cold CLI run.

`api::engine_reuse` verifies that reuse changes cost and nothing else: three
pages return results identical to reloading the models per page, and one page run
twice through the same engine agrees with itself. No state carries between
images.

## 4. Determinism under concurrency — measured at full precision

`tests/end_to_end.rs::concurrent_runs_are_byte_identical` runs four threads, each
loading its own engine and performing three runs, and compares the **serialized
result document** against a single-threaded baseline. Twelve concurrent documents,
all byte-identical to the baseline.

Comparing documents rather than text is deliberate. Two runs can agree on every
recognized string while differing in a polygon coordinate or the last digit of a
confidence; the serialized form compares geometry and scores at full precision,
which is the property a caller writing results to disk actually depends on.

`one_engine_per_thread_runs_concurrently_and_agrees` covers the same shape at the
text level with four threads.

## 5. Allowed numerical variation — none

The budget is exact equality, not a tolerance. Nothing in the classic path
introduces run-to-run variation: there is no thread-count-dependent reduction
order in this crate's own arithmetic, no random initialisation, no time or
locale input, and the backend is pinned to one intra-op thread.

Three cold CLI processes over the same input also produced byte-identical JSON,
which is the same property observed across processes rather than threads.

If a future configuration enables multiple intra-op threads, this section stops
being true — floating-point reduction order becomes thread-count dependent — and
the tolerance would have to be declared before that change, not after seeing what
it produces.

## 6. Cancellation and time budget

`src/control.rs` provides an `Arc<AtomicBool>` cancellation flag that any thread
may set, and a wall-clock budget. The flag is read and never written by this
project, so one flag may cancel several runs.

Checked at named stage boundaries: before the detector, before cropping, and
before each recognition batch. **A run is abandoned only at a stage boundary**,
because a backend call cannot be interrupted without leaving the session
undefined, so overshoot is bounded by one detector run or one recognition batch
of six crops rather than being immediate.

Demonstrated end to end: a `1 ms` budget on a real page returns `time budget
exhausted before crop`, having completed the detector run it could not interrupt.
A cancelled run returns the error and **no lines at all**; a partial list would be
indistinguishable from a complete one.

Cancellation is reported ahead of a timeout, because an explicit request from the
caller says more about why the run stopped than running out of time does.

## 7. Clean shutdown

An engine owns its sessions and drops them when it drops; there is nothing to
join, flush, or await, because nothing was spawned. The concurrency test drops
four engines across four threads on every run and the process exits normally.

The one process-global piece is ONNX Runtime's environment, initialised once per
process by `initialize_runtime`. It is not re-initialised per engine, and no test
has observed a failure to shut down.

## 8. What this does not establish

- **Throughput.** No parallel benchmark was run. This records that concurrent
  execution is correct, not that it scales.
- **Multi-threaded inference.** Intra-op threading is pinned to `1`. Raising it
  is a separate change that would need its own numerical-variation position
  first.
- **Service-level concerns.** Backpressure, admission control, and graceful
  drain belong to P11 and have no implementation here to evaluate.
