# M2 Candidate Provisioning Ledger

Roadmap item: `MOD-001`  
Status: In progress — candidate-only local verification workflow; two
user-authorized external ONNX directories and two confined static-oracle
directories were inventoried, but no artifact, format, runtime, or
distribution policy is accepted
Prepared: 2026-08-02  
PaddleOCR baseline: `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and decision boundary

This ledger turns the discovery evidence in
[MODEL_CANDIDATES.md](MODEL_CANDIDATES.md) into a conservative manual
provisioning and verification procedure. It exists so a future artifact review
can prove exactly what was inspected without making normal development depend
on an upstream checkout, network access, a cache, or an unpinned URL.

It is not a model manifest, downloader specification, local-path API, runtime
selection, or permission to convert, redistribute, or bundle an artifact.
Those decisions remain with `MODEL-DEC-001`, `MOD-002` through `MOD-004`,
`RT-002` through `RT-004`, and `LIC-002`. `LIC-001` accepts terms evidence for
the exact ONNX pair and, separately, for a confined static `RT-003` raw-tensor
oracle; no current Rust code reads these candidate artifacts.

The 2026-08-02 user-authorized external inventory of the ONNX detector and
recognizer is summarized in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md).
Its full machine-readable record remains beside the external files rather than
in this repository. On 2026-08-04, the same external-only procedure verified
all five expected files in the static detector and recognizer packages after
their confined `RT-003` terms review. The record now lists all four candidate
directories; neither inventory is an artifact-selection record or alters the
boundary above.

`MODEL_CANDIDATES.md` is the canonical record for the pinned URLs, byte
lengths, SHA-256 values, and observed ABI. This ledger deliberately refers to
those values instead of creating a competing final manifest. A final,
versioned, machine-readable manifest is future `MOD-002` work and may exist
only after the required model and license decisions.

## Candidate identity sets

The static and ONNX packages are separate candidate representations. A shared
model name, matching `inference.yml` fingerprint, or matching task does not
make files, licenses, dictionary behavior, or numerical output equivalent. No
detector/recognizer pair is selected by this ledger; a later record must name
both exact package identities explicitly.

| Candidate key | Role and representation | Pinned candidate evidence | Runtime-relevant files that must be inventoried | Additional review boundary |
|---|---|---|---|---|
| `m2-static-det-v6-medium` | Detector, Paddle static package | Static-detector row and static metadata table in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md) | `inference.pdiparams`, `inference.json`, `inference.yml` | Confirm every graph/operator/ABI claim locally; inspect retained terms/notices. |
| `m2-static-rec-v6-medium` | Recognizer, Paddle static package | Static-recognizer row and static metadata table in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md) | `inference.pdiparams`, `inference.json`, `inference.yml` | The inline `character_dict` is data requiring its own ABI, provenance, and terms review; do not extract or ship it yet. |
| `m2-onnx-det-v6-medium` | Detector, official ONNX export | ONNX-detector row and ONNX configuration table in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md) | `inference.onnx`, `inference.yml` | Prove the graph and output contract locally; do not assume it is a conversion-equivalent static artifact. |
| `m2-onnx-rec-v6-medium` | Recognizer, official ONNX export | ONNX-recognizer row and ONNX configuration table in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md) | `inference.onnx`, `inference.yml` | Verify CTC/dictionary correspondence and every tensor class locally; no generic multilingual claim follows. |

For all four candidates, the recorded package listings also observed
`.gitattributes` and `README.md`. They are not yet hash-pinned runtime inputs,
but any terms, attribution, provenance, or usage statement they contain must
be retained and reviewed before an artifact is accepted. The observed absence
of a top-level `LICENSE` file is a gap, not evidence that no terms apply. See
[LICENSE_REVIEW.md](LICENSE_REVIEW.md).

## Required local placement and handling

An artifact can be supplied only by the user or an explicitly approved external
process. Until `MODEL-DEC-001` and `MOD-003` are complete, this project has no
artifact root, cache directory, environment variable, CLI option, download
command, or path-resolution behavior.

When a reviewer is authorized to inspect a candidate, the candidate package
must remain in an explicitly chosen directory outside both this Rust checkout
and the read-only `PaddleOCR/` reference checkout. The reviewer must preserve
the package boundary and inspect it read-only. The package must not be copied,
symlinked, generated, extracted, cached, or staged under this repository.

Before accepting a file for any later experiment, the reviewer must establish
all of the following:

1. The chosen root and every expected file are ordinary, readable local files;
   symlinks, device files, sockets, FIFOs, archives, and unbounded recursive
   trees are rejected or separately reviewed. A future resolver must define
   its exact no-following behavior rather than trusting a user-controlled path.
2. The package contains the exact expected runtime-relevant filenames for one
   candidate key above. Unexpected files are recorded as provenance evidence;
   they are never silently treated as model inputs.
3. Each listed runtime-relevant file has the byte length and SHA-256 recorded
   in `MODEL_CANDIDATES.md`. A mismatch is a hard rejection for that candidate,
   not a reason to update the expected hash from the local copy.
4. The reviewer records the candidate key, pinned revision, source locator,
   local byte count, SHA-256, inspection date, acquisition authority, and
   retained terms/notices in an external evidence record. That record is not
   committed here until the asset and disclosure review permits it.
5. An ONNX experiment matches the exact candidate/revision and package terms
   decision recorded in `LIC-001`. The named static pair may be executed only
   for the confined external `RT-003` raw-tensor oracle after its exact package
   inventory is complete; any other representation needs its own review before
   it is executed, converted, or used to create model-backed goldens. No
   candidate is described as supported before its later gates.

Illustrative read-only verification commands may calculate a byte count and
SHA-256 for a previously approved local path, for example:

```sh
wc -c -- "$candidate_root/inference.onnx"
sha256sum -- "$candidate_root/inference.onnx"
```

The commands are not a resolver contract and do not authorize a download. A
reviewer must first establish that `candidate_root` and the file are not
symlinks and that the path is outside both repositories.

## Prohibited behavior before the later lifecycle decisions

- Do not download any model, configuration, archive, dictionary, font, or
  conversion output automatically or as part of a test, build, package, CI job,
  or documentation example.
- Do not place a candidate artifact in Git, `models/`, a Cargo package, a test
  fixture, a source tree, a build output, or a cache owned by this project.
- Do not run an inspector, converter, or runtime from `PaddleOCR/`, and do not
  use the upstream checkout as a fallback source for a missing local file.
- Do not derive a new SHA-256, config, dictionary, threshold, or ABI from an
  unverified local artifact and present it as a replacement candidate.
- Do not pair static and ONNX files, reuse a recognizer dictionary, or infer a
  language/support matrix without a later exact manifest and validation record.

## Handoff to later gates

Passing this manual inventory step does not accept an artifact. The following
evidence remains required in its owning roadmap items:

| Required evidence | Owning work |
|---|---|
| Revision-specific model/config/dictionary/notice provenance and terms, plus a disposition for static versus ONNX | Exact ONNX terms: `LIC-001` Done; representation/distribution: `MODEL-DEC-001` |
| Bounded local graph inspection: format, operators, tensor names, dtype, layout, dynamic shapes, output order, and errors | `MOD-001`, `RT-002` |
| Source-level recognizer index construction is recorded for the exact local ONNX configuration: no duplicate/literal-space entries, blank at `0`, ordered dictionary entries at `1..=18,708`, appended space at `18,709`, and a matching 18,710-class output. Runtime output semantics, safe Rust decoder bounds/errors, and language behavior remain required. | `MOD-001`, `REC-001`, `REC-002` |
| Raw tensor and end-to-end differential evidence against approved legal fixtures | `TOL-001`, `RT-003`, P4–P5 |
| Measured backend selection and migration strategy | `RT-004` / `D-006` |
| Final versioned manifest and safe explicit local resolution | `MOD-002`, `MOD-003` |
| Any distribution, cache, conversion, or download behavior | `MODEL-DEC-001`, `MOD-004` / `D-007` |

Until every applicable row closes, all four packages remain discovery evidence
only. The project has no supported OCR model artifact.
