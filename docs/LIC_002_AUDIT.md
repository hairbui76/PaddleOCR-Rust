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

Counted from the generated SBOM, which is regenerated from `Cargo.lock` and
drift-checked by `supply_chain::the_sbom_describes_exactly_the_locked_dependencies`,
so this table cannot quietly fall behind the graph. **`83` components** across all
feature configurations as of 2026-08-05:

| Licence expression | Components |
|---|---|
| MIT OR Apache-2.0 (in any operand order) | 55 |
| MIT | 10 |
| Apache-2.0 | 4 |
| Unicode-3.0 | 4 |
| BSD-3-Clause OR Apache-2.0 | 3 |
| Zlib OR Apache-2.0 OR MIT (either order) | 4 |
| Unlicense OR MIT | 2 |
| 0BSD OR MIT OR Apache-2.0 | 1 |
| ISC | 1 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 |

**No copyleft anywhere in the graph**, in any feature configuration. Every
component can be used under Apache-2.0 or a compatible permissive licence. That
is what makes the distribution position in
[`ADR_MODEL_DEC_001_ARTIFACT_POLICY.md`](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md)
tenable rather than aspirational, and it is also why the PDF decision in
[`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md)
rejected the AGPL and GPL renderers: adding one would have been the first
copyleft dependency in the project.

**Updated 2026-08-05 for the `pdf` feature.** `PDF-001` added `hayro 0.4.0` and
`hayro-syntax 0.4.0` and, with them, `32` transitive components — the cost the
user accepted when resolving the entry gate, measured in
[`PDF_ENTRY_GATE_EVIDENCE.md`](PDF_ENTRY_GATE_EVIDENCE.md) section 3 before the
decision rather than discovered after it. Five licence expressions are new to
this project: `Apache-2.0` alone, `BSD-3-Clause OR Apache-2.0`, `Unicode-3.0`,
and two Zlib alternations. All five are permissive and Apache-2.0 compatible; the
Unicode licence appears through the ICU-derived `yoke`/`zerofrom` family, and
`Unicode-3.0` permits redistribution with its notice. The renderer itself is
`Apache-2.0`, which is why it was the candidate: no dual-licence argument is
needed to distribute it under this project's own terms.

The addition contains **no C or C++ source and no new native library**; the six
build scripts in the new subtree all do cfg detection, and five of the six were
already in the graph. That was a precondition of the choice, not a happy result:
`pdfium-render` was rejected precisely because it would have brought a C++
subsystem under gate `G2`.

### The finding this audit produced: bundled binary assets

Auditing the addition turned up something the licence expressions alone do not
show. `hayro`'s **default** feature set includes `embed-fonts`, which compiles
third-party binary assets into this project's binary:

- the **Foxit standard PDF fonts**, sixteen `.pfb` files extracted from PDFium
  and licensed **BSD 3-clause**;
- `CGATS001Compat-v2-micro.icc`, a compact CMYK profile released under
  **CC0-1.0**.

Both are redistributable, but the BSD 3-clause terms **require** the copyright
notice and disclaimer to be reproduced for binary redistribution. That
obligation was not previously met, because this project had never bundled a
third-party binary before — every model, dictionary, and font until now was
either caller-supplied or deliberately not committed. `NOTICE` now carries the
Foxit notice in full and records the ICC profile's provenance.

The feature is kept **on** rather than disabled, and the reason is a rendering
one rather than a licensing one: without it, a PDF that references a standard
font without embedding it renders blank. A silently blank page is precisely the
"plausible wrong page" that
[`ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md)
refused PDF over, and the entry gate's own worst-case measurement — non-embedded
Helvetica producing character-identical OCR — was taken with these fonts
supplying the substitution. Turning the feature off would invalidate that
measurement and replace a small licence obligation with a correctness hole.

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
