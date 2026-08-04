# `RC-001` — Release Candidate Evidence

Roadmap item: `RC-001`
Produced: 2026-08-04
Source: commit `86857e72574c473643503fc222701614f1a22eb5`, built from a fresh
clone with a clean working tree
Status: evidence complete; **not authorised for release** — `USER-GATE-001` and
`REL-001` are open by design

Everything below was produced from one checkout in one session. Where a figure
differs from an earlier document, the earlier one is superseded and the reason is
stated.

## 1. Toolchain and target

| Field | Value |
|---|---|
| `rustc` | `1.94.0 (4a4ef493e 2026-03-02)` |
| `cargo` | `1.94.0 (85eff7c80 2026-01-15)` |
| Target | `x86_64-unknown-linux-gnu` (host) |
| Edition / MSRV | `2024` / `1.94` |
| Host | Intel Xeon E5-2696 v3, `Linux 7.0.0-28-generic x86_64` |
| Network during build | none; every command used `--offline --locked` |

**One target only.** `PLAT-001` is open, and the roadmap's own standard is that
Rust portability is not platform evidence.

## 2. Artifacts and hashes

| Artifact | SHA-256 | Bytes |
|---|---|---|
| `paddleocr-rust-0.1.0.crate` | `6064d7dddfe4292c36ff56dd13dc78c2753db684980ce10381e5e3920f6b3d8d` | `741,075` |
| `paddleocr-rust` (release binary, `--features onnxruntime`) | `3997289ad44e4e95fe80aea163288fcde1a25723830dd82d25a98ad7c387fd07` | `853,104` |
| `sbom/paddleocr-rust.cdx.json` | `f0207b0970f9cf3b4124e3bcaa24c0837175a00a85661fb235f8cf06ac749986` | — |

The package contains `177` files and was produced **with** cargo's verification
build, which rebuilds and tests the packaged tree. `CLEAN_001_EVIDENCE.md` noted
that an earlier run used `--no-verify`; that caveat is now closed.

The binary is `853,104` bytes, up from the `812,144` recorded in
`G3_RESOURCE_EVIDENCE.md`. The growth is the manifest, script, input, and control
modules added since. Still `0.8 %` of the `100 MiB` budget.

### Externally provisioned, not part of any artifact

| Artifact | SHA-256 |
|---|---|
| `libonnxruntime.so.1.28.0` | `1c04ac4162d45e9cdf3a7f979770f1e1d96fcbc1ea4a09379fa63e75672742fa` |
| detector `inference.onnx` | `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` |
| recognizer `inference.onnx` | `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` |
| `ppocrv6_dict.txt` | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |

## 3. Commands and results

### Offline — no artifacts, no network, no upstream checkout

| Command | Result |
|---|---|
| `cargo build --offline --locked` | clean |
| `cargo test --offline --locked --workspace` | `253` passing |
| `cargo test --offline --locked --workspace --all-features` | `262` passing |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | clean |
| …`--all-features` | clean |
| …`--features fuzzing` | clean |
| `cargo fmt --all --check` | clean |
| `cargo doc --offline --locked --no-deps --all-features` | clean |
| `cargo package --offline --locked` | `177` files, verified |

### Provisioned gates — and their ordering constraints

This is the part a release runner needs stated exactly, because running them the
obvious way **fails**.

```sh
# 1. The loader test must run in its own process, before anything initialises
#    the runtime successfully. ort's environment is process-global.
cargo test --features onnxruntime --lib -- --ignored --exact \
  backend_ort::tests::an_invalid_library_path_is_mapped_to_a_backend_error

# 2. The remaining lib gates must run single-threaded, for the same reason.
cargo test --features onnxruntime --lib -- --ignored --test-threads=1 \
  --skip an_invalid_library_path

# 3. The public-surface suite, also single-threaded.
cargo test --features onnxruntime --test end_to_end -- --ignored --test-threads=1
```

| Gate | Result |
|---|---|
| `G1` end-to-end fixtures | all four reproduced exactly |
| `G3` warm latency | within budget |
| `PRE-001` full-element tensor comparison | bit-identical |
| `API-001` engine reuse | identical to reloading |
| adapter fingerprint cross-check | reproduced |
| loader error mapping | typed error, run alone |
| `E2E-001` provisioned suite | `6` passing |

**Finding.** Running the ignored lib gates the default way — all of them, in
parallel — produces five failures inside `ort`'s mutex, because several tests
initialise the process-global ONNX Runtime environment concurrently. Every
earlier run in this project's history invoked them individually by name, which
hid the constraint. It is a property of the runtime, not a defect in the tests,
and the commands above are now the recorded way to run them.

### End-to-end OCR from the release checkout

```
image: tests/fixtures/classic-v1-benchmark-page/input.png (1280x720 PNG)
wall 4.29 s   peak 446,984 kB
```

Two-image run under `env -i`, from the clean checkout, produced correct text for
both including the CJK line.

## 4. Compatibility summary

Seven `Must` rows in `docs/COMPATIBILITY.md`, all **Verified**, each with a
frozen contract, a committed fixture, a recorded tolerance, and a reproducing
test. `docs/CLOSE_001_AUDIT.md` classifies all `121` inventory rows: `7`
verified, `6` intentional differences, the large remainder deferred to a named
milestone, `3` classes out of scope.

Per-stage oracle agreement: contours `18/18`, `minAreaRect` `16/16`,
`box_score_fast` `8/8`, `unclip` `16/16`, resize `34/34` plus `600` randomised,
crop `72`, PNG decode `5/5`, and detector plus recognizer input tensors
**bit-identical** to captured upstream tensors.

## 5. Benchmarks

| Dimension | Budget | Measured | Verdict |
|---|---|---|---|
| Cold CLI latency | `15 s` | `4.29 s` | Pass |
| Warm median | `5 s` | `2.840 s` | Pass |
| Warm p95 | `10 s` | `2.923 s` | Pass |
| Startup | none declared | `0.69 s` | Recorded |
| Throughput | none declared | `0.357` pages/s, single threaded | Recorded |
| Peak resident | `2 GiB` | `594.7 MiB` worst observed | Pass |
| Stripped binary | `100 MiB` | `853,104` bytes | Pass |
| Model artifacts in package | `0` | `0` | Pass |
| Determinism | byte-identical | `20` runs in one process, `3` processes, `12` concurrent | Pass |

No budget was amended. Startup and throughput carry no budget deliberately:
writing a threshold after seeing the number is what `QUALITY_PROFILE.md` forbids.

## 6. Security and licence review

| Review | Document | Outcome |
|---|---|---|
| Threat model | `THREAT_MODEL.md` | Two trust boundaries; most classes have no surface; two residuals recorded |
| Unsafe and native boundary | `SAFE_001_AUDIT.md` | No `unsafe` in this crate; one build script in the whole graph; `ort-sys` has none |
| Robustness | `ROB_001_EVIDENCE.md` | `4,000` fuzz cases clean in release and debug-with-overflow-checks |
| Licensing | `LIC_002_AUDIT.md` | All permissive, no copyleft; two findings fixed |
| Supply chain | `SUPPLY_001_POLICY.md` | Generated SBOM, drift enforced by test |
| Concurrency | `CONC_001_EVIDENCE.md` | `!Sync` compiler-enforced; twelve concurrent documents byte-identical |

## 7. Known limitations

Stated plainly, because a release candidate that hides these is worse than none.

1. **One artifact pair, one image format.** PNG only; `PP-OCRv6_medium` only.
2. **Gate `G2` is open.** The ONNX Runtime binary has no hermetic rebuild and no
   SBOM, and cannot fully close on this host because the source-tag signature's
   public key is not available locally. This blocks distribution, not use.
3. **`PLAT-001` is open.** One platform measured.
4. **Digest checking is optional**, and a swapped detector and recognizer load
   without complaint because they are not distinguishable by shape.
5. **The runtime library is not verified** by this project, only the models.
6. **Fuzzing is generation-driven**, not coverage-guided.
7. **Dependency `unsafe` was counted, not reviewed** — `375` sites in `ort`.
8. **No signing.** Deliberate; there is no distribution boundary yet.
9. **The benchmark page is synthetic.** Every timing is a lower bound on real
   scans.
10. **P7–P12 are unimplemented.** No document preprocessing, PDF, multipage,
    structure, VLM, serving, or training. Each is comparable in size to what has
    been delivered.

## 8. What is deliberately not done here

`USER-GATE-001` requires explicit user confirmation of the final release scope
and whether this meets the requested meaning of finished. `REL-001` requires
explicit authorization before any external effect — publishing, tagging, or
packaging for distribution.

Neither can be satisfied by this document, and closing them without the user
would violate the exact clause they exist to enforce. `publish = false` remains
set in `Cargo.toml`.
