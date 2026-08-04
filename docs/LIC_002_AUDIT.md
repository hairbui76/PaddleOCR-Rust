# `LIC-002` — Licensing Audit

Roadmap item: `LIC-002`
Audited: 2026-08-04
Scope: this repository's own source, its dependency graph, native libraries,
model weights, dictionaries, fixtures, fonts, conversion tools, notices, and
distribution

`LIC-002` requires that every asset's licence is resolved and that unresolved
assets are excluded. Two findings came out of the audit and both were fixed
during it; they are recorded below rather than quietly corrected.

## 1. This repository's own content

`LICENSE` is Apache-2.0. `NOTICE` states that this is an independent port, not
an official PaddlePaddle or PaddleOCR release, and not affiliated with or
endorsed by either.

Every committed file is text. There is no binary blob of any kind:

```
git ls-files | grep -E '\.ttf|\.otf|\.so|\.dll|\.dylib|\.onnx|\.pdmodel|\.tar|\.zip'
(no matches)
```

Committed file types are `.md`, `.json`, `.rs`, `.py`, `.csv`, `.png`, `.txt`,
`.toml`, `.yml`, plus `LICENSE`, `NOTICE`, and `Cargo.lock`. The five PNGs are
the self-authored fixtures; all twenty fixture inputs declare `Apache-2.0` and
every one names a provenance and a consuming test, which the integrity gate
enforces.

### Finding 1 — eight source files carried no SPDX header

Thirty-one of thirty-nine Rust files had `SPDX-License-Identifier: Apache-2.0`;
eight did not — `src/lib.rs`, `src/main.rs`, the example, and the five test
files. Every one is project-authored and unambiguously covered by `LICENSE`, so
this was hygiene rather than exposure, but a per-file identifier is what makes an
automated licence scan agree with the repository's own claim. All thirty-nine now
carry the header.

### Finding 2 — the notice under-attributed

`NOTICE` credited OpenCV for `src/contour.rs` only. In fact nine modules derive
observable behaviour from a third-party implementation, and one of them from a
different project entirely:

- **OpenCV** — `contour.rs`, `min_area.rs`, `resize.rs`, `crop.rs`,
  `geometry.rs`, `score.rs`, `image.rs`
- **Clipper, via pyclipper** (Boost Software License 1.0) — `unclip.rs`
- **PaddleOCR** (Apache-2.0) — the pipeline order, constants, and semantics
  throughout

No third-party source is copied into this repository in any of these cases; what
was taken is behaviour, constants, and edge-case rules, which is exactly what an
attribution is for. Under-attributing a behavioural derivation is a real defect
in a project whose entire method is reading other implementations closely. The
notice now lists each module against the specific upstream file or algorithm it
was derived from.

## 2. Dependencies

Full counts are in [`SAFE_001_AUDIT.md`](SAFE_001_AUDIT.md). The licensing result:

| Licence | Crates |
|---|---|
| MIT OR Apache-2.0 | `png`, `fdeflate`, `cfg-if`, `bitflags`, `crc32fast`, `flate2`, `ort`, `ort-sys`, `smallvec`, and the three dev-only crates |
| MIT OR Zlib OR Apache-2.0 | `miniz_oxide` |
| 0BSD OR MIT OR Apache-2.0 | `adler2` |
| MIT | `simd-adler32` |
| ISC | `libloading` |

**No copyleft anywhere in the graph**, in either feature configuration. Every
crate can be used under Apache-2.0 or a compatible permissive licence. That is
what makes the distribution position in
[`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md)
tenable rather than aspirational, and it is also why the PDF decision in
[`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md)
rejects the AGPL and GPL renderers: adding one would be the first copyleft
dependency in the project.

`Cargo.lock` is committed and every version is pinned with `=`, so the audited
graph is the built graph.

## 3. Native libraries

One: ONNX Runtime, MIT-licensed, and **not distributed by this project**. It is
opened at runtime from a caller-supplied path. `RT-004` chose `load-dynamic`
precisely so this project neither bundles nor links it, which means this project
does not redistribute it and therefore does not become a licensor of it.

Its *supply chain* remains gate `G2` and is open. Licensing and provenance are
different questions, and only the first is settled here.

## 4. Model weights, dictionaries, and datasets

Zero bytes of any of them are in this repository. `/models/` is ignored. The
pinned candidates' terms review is recorded under `LIC-001`, and every fixture
that names an artifact also names that review, which the integrity gate checks.

The dictionary is a special case worth stating: `ppocrv6_dict.txt` ships inside
the upstream Python package and is **not** committed here. It is recorded by
SHA-256 and entry count only. A user provisions it themselves, and the licence
that applies is upstream's.

## 5. Fonts

None. The synthetic fixture inputs were rendered by an external `cv2.putText`
generator using `FONT_HERSHEY_SIMPLEX`, which is glyph data compiled into OpenCV
rather than a font file. No font binary, and no rendered glyph outline, enters
this repository — only the resulting raster images, which are self-authored
output.

This matters more than it looks: a committed `.ttf` would carry its own licence
and would be the kind of asset `LIC-002` exists to catch.

## 6. Conversion and capture tools

Fifteen Python files under `tools/`, all project-authored and Apache-2.0. Two
categories:

- **Self-contained captures** using `cv2` and `numpy` at the caller's machine —
  they import nothing from the upstream checkout.
- **`tools/capture_preprocess_oracle.py`**, which *does* import upstream
  operator classes by path from the read-only reference checkout, with bytecode
  writing disabled. It executes upstream code; it does not copy it, and its
  output is the numeric capture rather than the source.

Neither category redistributes anything. The tools are development-time only and
are not part of the crate's build or test path.

## 7. Distribution

What a release would contain: one stripped `812,144`-byte binary and the crate
source. What it would not contain: model weights, dictionaries, the ONNX Runtime
library, fonts, or datasets — all of which are `0` bytes of any package this
project produces.

That is the structural reason this audit is short. Most licensing risk in an OCR
project comes from bundling assets, and this project bundles none.

## 8. Unresolved assets

None. Every asset in the repository is either project-authored under Apache-2.0
or a permissively licensed dependency resolved above. The two findings were
corrected during the audit rather than deferred.

Two things this audit does **not** cover:

- **The terms of the model weights themselves**, which are `LIC-001`'s subject
  and apply to the user who provisions them, not to this repository.
- **Gate `G2`**, which is about how the ONNX Runtime binary was produced, not
  about the licence it carries.
