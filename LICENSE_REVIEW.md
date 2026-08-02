# M2 Artifact License and Provenance Review

Roadmap item: `LIC-001`  
Status: In progress — preliminary remote and local package evidence recorded; no artifact is approved  
Prepared: 2026-08-02  
PaddleOCR baseline: `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and decision boundary

This record tracks the artifact-specific evidence needed before the planned M2
model pair can be used, redistributed, converted, bundled, or advertised as
supported. It is a provenance and release-control record, not legal advice and
not a grant of rights.

`DEC-003` remains in force: normal builds and tests do not download, cache,
convert, bundle, or require model artifacts. `MODEL-DEC-001` is the only later
decision that may establish an exact artifact/distribution policy. Until then,
the candidate names and hashes in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md)
are discovery evidence only.

## Review method and scope

The initial review used two read-only evidence sources:

- the local upstream source checkout for its repository-level Apache-2.0
  license and source-notice boundaries; and
- remote model-card metadata and top-level file listings for the four
  revision-pinned candidates in `MODEL_CANDIDATES.md`.

No model binary, archive, dictionary, font, fixture, conversion output, or
third-party runtime was downloaded or stored in this repository. The linked
`PaddleOCR/` checkout was not executed or modified.

On 2026-08-02, the project user separately authorized a one-time external
download of the two exact ONNX candidates. This added a local package
inventory and parse-only graph inspection, recorded in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md), but
did not add an asset to this repository or change any approval/distribution
status.

The separately planned, user-local `RT-002` runtime diagnostics later executed
the verified external ONNX bytes without retaining raw outputs or a fixture in
this repository. Their results are recorded in
[RUNTIME_TRACT_EVIDENCE.md](RUNTIME_TRACT_EVIDENCE.md),
[RUNTIME_ORT_EVIDENCE.md](RUNTIME_ORT_EVIDENCE.md), and
[RUNTIME_ORT_SOURCE_EVIDENCE.md](RUNTIME_ORT_SOURCE_EVIDENCE.md). These narrow
diagnostics do not close a license gate or approve project adoption, conversion,
distribution, bundling, or model-derived fixture retention.

The remote card metadata displayed `license: apache-2.0` for each candidate
below. That is useful preliminary provenance evidence, but a hosting-platform
metadata field alone does not establish all rights needed to distribute weights,
their embedded data, a converted representation, or derivative fixtures.

## Candidate evidence matrix

| Role and representation | Pinned revision | Preliminary license evidence | Package-listing observation | Current disposition |
|---|---|---|---|---|
| Static detector, `PP-OCRv6_medium_det` | `8e0f56fb2ef86b461d99cfc7ac5c137738985f61` | The official model card displayed `apache-2.0`. | The recorded top-level listing contains `.gitattributes`, `README.md`, `inference.json`, `inference.pdiparams`, and `inference.yml`; no top-level file named `LICENSE` was observed. | Not approved for project adoption, conversion, distribution, bundling, or retained fixtures. |
| Static recognizer, `PP-OCRv6_medium_rec` | `e5a92bcbc5cc1b494628e458d267778f0704fd7c` | The official model card displayed `apache-2.0`. | The recorded top-level listing contains `.gitattributes`, `README.md`, `inference.json`, `inference.pdiparams`, and `inference.yml`; no top-level file named `LICENSE` was observed. The configuration embeds a character dictionary. | Not approved for project adoption, conversion, distribution, bundling, or retained fixtures. |
| ONNX detector, `PP-OCRv6_medium_det_onnx` | `61323801669c338b7891481ec7bac61ce31b576a` | The official model card displayed `apache-2.0`. | The recorded top-level listing contains `.gitattributes`, `README.md`, `inference.onnx`, and `inference.yml`; no top-level file named `LICENSE` was observed. | Not approved for project adoption, conversion, distribution, bundling, or retained fixtures. |
| ONNX recognizer, `PP-OCRv6_medium_rec_onnx` | `50c7eacafc52fa7bcf4194e8cd08e46f8558504b` | The official model card displayed `apache-2.0`. | The recorded top-level listing contains `.gitattributes`, `README.md`, `inference.onnx`, and `inference.yml`; no top-level file named `LICENSE` was observed. | Not approved for project adoption, conversion, distribution, bundling, or retained fixtures. |

The exact source URLs, file sizes, SHA-256 values, and observed tensor metadata
are maintained in [MODEL_CANDIDATES.md](MODEL_CANDIDATES.md). The companion
[CANDIDATE_PROVISIONING_LEDGER.md](CANDIDATE_PROVISIONING_LEDGER.md) maps that
canonical evidence to a candidate-only local verification workflow; neither
document is a final accepted artifact manifest.

## Local package verification

The two downloaded ONNX packages were checked as ordinary non-symlink files,
and every runtime-relevant byte count and SHA-256 matched the pinned candidate
record. The local README.md files begin with the model-card field
`license: apache-2.0` and display a link to `./LICENSE`; neither package
contains a top-level `LICENSE` file. This confirms, rather than closes, the
previously recorded terms evidence gap. The complete local file inventory and
parse-only graph inspection are in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md).

On 2026-08-02, direct read-only Hugging Face revision-API queries confirmed
the same immutable detector and recognizer revisions, each public and
ungated, with `cardData.license: apache-2.0`. Each API sibling list contains
only `.gitattributes`, `README.md`, `inference.json`, `inference.onnx`, and
`inference.yml`; it contains no `LICENSE`. The corresponding immutable
`resolve/<revision>/LICENSE` URLs each returned HTTP `404`. This makes the
model-card declaration and absent-file observation revision-specific remote
evidence, but it still does not provide the Apache-2.0 text or establish its
coverage of every model-package file.

- Detector API: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_det_onnx/revision/61323801669c338b7891481ec7bac61ce31b576a
- Recognizer API: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_rec_onnx/revision/50c7eacafc52fa7bcf4194e8cd08e46f8558504b

## Material-specific status

| Material | Evidence currently held | What remains required | Release status |
|---|---|---|---|
| PaddleOCR repository source | The read-only upstream repository identifies Apache License 2.0; the P0 record documents source-attribution and trademark boundaries. | If any non-trivial source is adapted, record file-level provenance, retained notices, modifications, and a source review. | No upstream source is currently included. |
| Static model parameters and graph/config files | Candidate revisions, checksums, sizes, and preliminary card metadata are recorded. | Obtain and preserve a revision-specific license/terms source; verify the publisher/rightsholder and terms for every selected file. | Unapproved. |
| Official ONNX exports | Separate pinned repository revisions and checksums are recorded. | Establish whether each export has its own applicable terms and its relationship to the static package; do not infer numerical or legal equivalence from a shared model name. | Unapproved. |
| Recognizer character dictionary and tokenizer behavior | The static recognizer configuration contains an inline dictionary; its ABI is not yet verified. | Verify exact provenance, terms, index mapping, blank/space behavior, and any notice obligation before copying, extracting, or shipping it. | Unapproved. |
| Oracle and test fixtures | No model-backed input or expected-output fixture has been added. | Record original-author/source provenance, terms, privacy review, hashes, goldens, and applicable model evidence per [FIXTURE_AND_TOLERANCE_PLAN.md](FIXTURE_AND_TOLERANCE_PLAN.md). | Unapproved. |
| Conversion tools and generated outputs | No converter, conversion recipe, or converted output has been selected or run. | Record tool version/license, exact inputs and command, output hashes, notices, reproducibility, and tensor-differential evidence before any conversion is accepted. | Unapproved. |
| Rust/native dependencies | The bootstrap workspace has no third-party Cargo dependency. External-only `ort` spikes used `ort` 2.0.0-rc.13 with a temporary Python-wheel library and a separately source-built ONNX Runtime 1.28.0 library; see [RUNTIME_ORT_EVIDENCE.md](RUNTIME_ORT_EVIDENCE.md) and [RUNTIME_ORT_SOURCE_EVIDENCE.md](RUNTIME_ORT_SOURCE_EVIDENCE.md). Neither is a repository dependency or a distribution route. | Review the wrapper and complete native/transitive terms, notices, vulnerabilities, acquisition/build provenance, dynamic-loader behavior, `unsafe` boundary, and supported CPU/platform distribution targets under `RT-002` and `LIC-002`. | Unapproved; no runtime has been adopted for M2. |

## Explicit non-findings

This review does **not** establish any of the following:

- that the model-card license field covers every binary, embedded dictionary,
  configuration, or archive associated with a candidate;
- that the static and ONNX candidates are equivalent, mutually convertible, or
  covered by identical terms;
- that a user-provisioned copy may be redistributed by this project;
- that a generated golden, cropped image, or derived model output is free of
  source, privacy, or dataset restrictions; or
- that a future runtime, decoder, font, or native library is license-compatible.

The absence of a top-level `LICENSE` file in the observed package listings is
an evidence gap, not proof that no terms apply.

## Required closure evidence

`LIC-001` remains incomplete until the selected M2 representation has all of
the following, reviewed at its immutable revision:

1. A durable source for the exact applicable license/terms and a recorded
   publisher/rightsholder relationship for the selected model files.
2. A file inventory covering parameters, graph, configuration, dictionary,
   tokenizer, archive wrapper, and every retained notice, with byte sizes and
   SHA-256 values tied to the local provisioning manifest.
3. A disposition for static versus ONNX artifacts, including the terms and
   provenance of any official or project-run conversion.
4. A fixture review record for every committed image, golden, dictionary
   fragment, font, or other asset, including privacy and redistribution terms.
5. The exact release behavior: local-only use, approved opt-in acquisition, or
   approved package distribution. Any behavior that copies or downloads an
   artifact must remain disabled until this record and `MODEL-DEC-001` approve
   it.
6. Required attribution and notices prepared for the chosen release form, with
   a final cross-check by `LIC-002` before a broad release.

Until those conditions close, unclear material is excluded rather than replaced
with an undocumented substitute or a fabricated compatibility claim.
