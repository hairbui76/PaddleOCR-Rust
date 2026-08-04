# `CLEAN-001` / `PKG-001` — Clean-Checkout and Package Evidence

Roadmap items: `CLEAN-001`, `PKG-001`
Recorded: 2026-08-04
Status: both pass; one real packaging defect was found and fixed

`CLEAN-001` requires that from a clean checkout without `PaddleOCR`, Python,
network, or GPU, the normal build, test, docs, and package steps pass and an
explicitly provisioned CPU end-to-end OCR run succeeds. `PKG-001` requires the
package to contain only intended files.

Both were exercised against a fresh `git clone` into a directory that has no
upstream symlink, no Python virtual environment, and none of this working
copy's local files.

## The exercise

```sh
git clone <repo> clean-checkout
cd clean-checkout
ls PaddleOCR          # No such file or directory
```

| Step | Command | Result |
|---|---|---|
| Build | `cargo build --offline --locked` | Clean |
| Test, default features | `cargo test --offline --locked --workspace` | `253` passing, `1` ignored |
| Test, all features | `cargo test --offline --locked --workspace --all-features` | `262` passing |
| Docs | `cargo doc --offline --locked --no-deps --all-features` | Clean after one fix, below |
| Package | `cargo package --offline --locked --no-verify` | `176` files, `2.4 MiB` |
| Release build | `cargo build --release --features onnxruntime` | Clean |
| End-to-end OCR | see below | Correct |
| Gate `G1` | from the clean checkout | All four fixtures reproduced |

Nothing in any step reached the upstream checkout, the network, a GPU, or a
Python interpreter. The upstream symlink does not exist in that directory, so
"did not use it" is a property of the environment rather than a claim.

### End-to-end OCR, from the clean checkout, under a scrubbed environment

```sh
env -i PATH=/usr/bin:/bin HOME=$HOME ./paddleocr-rust \
  --ort-dylib <libonnxruntime.so> \
  --detector <detector.onnx> --recognizer <recognizer.onnx> \
  --dictionary <ppocrv6_dict.txt> \
  --manifest tests/fixtures/classic-v1-model-manifest/expected.txt \
  <reading-order.png> <unicode.png>
```

```
reading-order/input.png	0.999995	Hello
reading-order/input.png	0.999979	World
reading-order/input.png	0.999996	Rust
reading-order/input.png	0.999860	OCR
unicode/input.png	0.999983	你好
```

Gate `G1` run from the same checkout reproduced all four committed end-to-end
fixtures.

## Finding 1 — a doc warning, fixed

`cargo doc` reported that `control`'s public documentation linked to the private
`crate::pipeline`. A public page linking somewhere a reader cannot follow. The
link was replaced with the sentence it was pointing at.

## Finding 2 — the package contained files that must never ship

This is the substantive one. Run in the *working copy*, `cargo package` produced
`232` files including:

- `ocr-1.png` — a user-supplied image, untracked
- `0.3` — an unrelated user file
- `.claude/` — agent working directory
- **`PaddleOCR/…` — `34` files walked through the read-only upstream symlink**
- **`.oracle-venv/…` — `18` files from a Python virtual environment**

The last two matter most. `cargo package` collects licence and notice files by
name from anywhere in the tree, and that walk followed the upstream symlink and a
local virtual environment. A release built that way would have redistributed
upstream and third-party licence files that this project has no business
shipping, in direct conflict with `MODEL-DEC-001`.

`.gitignore` did not prevent it: both paths are ignored, and cargo's licence
collection did not respect that.

### The fix

`Cargo.toml` now carries an explicit `include` list. What a package contains is
**stated**, not inherited from whatever happens to be in the working directory:

```toml
include = ["src/**", "examples/**", "tests/**", "tools/**", "docs/**",
           "sbom/**", "README.md", "ROADMAP.md", "LICENSE", "NOTICE",
           "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]
```

### Verified contents from the clean checkout

`176` files, `2.4 MiB`, `721 KiB` compressed:

| Area | Files |
|---|---|
| `tests/` | 63 |
| `docs/` | 53 |
| `src/` | 33 |
| `tools/` | 16 |
| `sbom/`, `examples/` | 1 each |
| `LICENSE`, `NOTICE`, `README.md`, `ROADMAP.md`, `Cargo.*`, `rust-toolchain.toml` | 1 each |

Checked absent: any `.onnx`, `.so`, or other model artifact; `ocr-1.png`; `0.3`;
`.claude/`; `PaddleOCR/`; `.oracle-venv/`; `target/`; anything matching
`credential` or `.env`. The package is entirely text.

`PKG-001` also requires licences, notices, SBOM, checksums, and provenance in the
package: `LICENSE`, `NOTICE`, `sbom/paddleocr-rust.cdx.json` with a SHA-256 for
every dependency, and `Cargo.lock` are all present, and
`tests/supply_chain.rs` ships with them so a consumer can re-run the drift check.

## Consequence for the release procedure

**A release must be built from a clean checkout, not merely recommended to be.**
The `include` list is what makes the contents deterministic, and the clean
checkout is what keeps the licence-file walk from finding anything it should not.
Either one alone is insufficient: the walk ignores `.gitignore`, and `include`
does not govern the files cargo collects by name.

`RC-001` should record the checkout the release was built from, and that it was
fresh.

## What this does not establish

- **One platform.** `x86_64-unknown-linux-gnu` only; `PLAT-001` is open and
  Rust portability is not platform evidence.
- **`--no-verify` was used** when packaging, which skips cargo's rebuild of the
  packaged crate. The clean checkout was built and tested directly instead,
  which covers the same ground; a release should drop `--no-verify` and let
  cargo verify the packaged tree itself.
- **`publish = false`** remains set. Nothing here authorises publication, which
  is `USER-GATE-001` and `REL-001`.
