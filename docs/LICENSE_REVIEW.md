# M2 Artifact License and Provenance Review

Roadmap item: `LIC-001`  
Status: Blocked — preliminary remote and local package evidence is recorded, but no durable artifact-specific terms source has been located; no artifact is approved
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

On 2026-08-02, a second direct read-only immutable-revision audit confirmed
that both ONNX repositories are public and ungated, display `author:
PaddlePaddle`, and have `cardData.license: apache-2.0`. The platform's
`author` field is useful repository metadata, not proof of the rightsholder or
of a grant covering every package file. Each card data object contains only
`language`, `library_name`, `license`, `pipeline_tag`, and `tags`; there is no
`license_name`, `license_link`, `datasets`, or `base_model` field.

The recursive immutable tree for each revision contains exactly five package
files: `.gitattributes`, `README.md`, `inference.json`, `inference.onnx`, and
`inference.yml`. It contains no `LICENSE`, `NOTICE`, or third-party notice.
The corresponding immutable `resolve/<revision>/LICENSE` URLs each returned
HTTP `404`. The local canonical text assets (`.gitattributes`, `README.md`,
`inference.json`, and `inference.yml`) matched their exact remote revision
bytes; every canonical local package file is a regular file rather than a
symlink. The detector and recognizer README SHA-256 values are respectively
`3046e3aab0194a2291bb3941c93b980c2b3a938a24a5be88354968f6d6187ac8` and
`ebce8d28436623ecab4952e24935aed86b3f8ecaf8f8736b92d5544f60fae1e9`.

Both README files declare `license: apache-2.0` in front matter and link a
badge to `./LICENSE`, but that link is dangling at the immutable revision.
They contain no other copyright, notice, terms, dataset/training-data,
third-party, or attribution text. The recognizer `inference.yml` embeds a
`character_dict` with 18,708 entries and no legal/provenance text, so it is an
additional asset-specific terms gap. Hugging Face documents the model-card
fields as display/filtering metadata and expects authors to describe details
such as datasets and training information; it does not supply the absent
package-level terms here.

- Detector revision API: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_det_onnx/revision/61323801669c338b7891481ec7bac61ce31b576a
- Recognizer revision API: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_rec_onnx/revision/50c7eacafc52fa7bcf4194e8cd08e46f8558504b
- Detector recursive tree: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_det_onnx/tree/61323801669c338b7891481ec7bac61ce31b576a?recursive=true&expand=true
- Recognizer recursive tree: https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_rec_onnx/tree/50c7eacafc52fa7bcf4194e8cd08e46f8558504b?recursive=true&expand=true
- Detector missing license: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_onnx/resolve/61323801669c338b7891481ec7bac61ce31b576a/LICENSE
- Recognizer missing license: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_onnx/resolve/50c7eacafc52fa7bcf4194e8cd08e46f8558504b/LICENSE
- Hugging Face model-card documentation: https://huggingface.co/docs/hub/model-cards

## Additional publisher and dictionary trace

This follow-up is provenance evidence only. It does not turn metadata into a
weight license, approve a model, or establish a release/distribution right.

### Publisher and storage path

At immutable PaddleX v3.7.0 source commit
`e0068ce0bfe75b2992e5b38d06a0393c70f887f7`, the official-model mapping lists
both `PP-OCRv6_medium_det` and `PP-OCRv6_medium_rec` as ONNX-supported models.
The same source derives `<model>_onnx` for the ONNX request and its Hugging
Face hoster constructs `PaddlePaddle/<model_name>`. This is stronger
publisher-path evidence than a model-card `author` field, but the hoster does
not pin an immutable revision and it does not state terms for model weights or
exports.

- PaddleX model mapping: https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/utils/official_models.py#L515-L571
- PaddleX Hugging Face hoster: https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/utils/official_models.py#L773-L784

The public Hugging Face LFS commit pages also record the exact ONNX object
identities already observed locally: detector object
`eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1`
(62,032,837 bytes) and recognizer object
`9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba`
(76,554,979 bytes). The UI marks those storage commits verified, which is
useful chain-of-custody evidence for the bytes; it is not a grant of rights or
proof of a legal rightsholder.

- Detector LFS commit: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_onnx/commit/dbee314685dad0b116a3d2faa7627776f936f085
- Recognizer LFS commit: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_onnx/commit/a7c2cc35beb94846af153645f5a619b6be905786

A separate `PaddlePaddle` ModelScope mirror also presents `apache-2.0`
metadata, but its mutable `master` inventories contain no `LICENSE`/`NOTICE`
and its `LicenseName`/`LicenseLink` metadata are empty. It is corroborating
absence evidence, not an immutable terms source.

- Detector mirror file list: https://www.modelscope.cn/api/v1/models/PaddlePaddle/PP-OCRv6_medium_det_onnx/repo/files?Revision=master
- Recognizer mirror file list: https://www.modelscope.cn/api/v1/models/PaddlePaddle/PP-OCRv6_medium_rec_onnx/repo/files?Revision=master

### Recognizer dictionary content trace

The pinned PaddleOCR baseline configuration
`configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml` names
`ppocr/utils/dict/ppocrv6_dict.txt` as its `character_dict_path`. The immutable
baseline dictionary has SHA-256
`b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` and
the baseline repository `LICENSE` has SHA-256
`3840c5c0c61c294264d2dd77b8777be6ddd90121ef4e0e64abcd22edea581d6e`.
The historical immutable commit
`b03f46425e8ff4442b268ce449e3eef758146cd4` introduced the v6 dictionary and
the v6-medium recognizer configuration.

For the exact local ONNX recognizer package, a read-only canonical comparison
extracted the YAML `PostProcess.character_dict` as one unescaped entry per LF.
That normalized entry stream has the same SHA-256 and an empty `diff` against
the pinned upstream dictionary. A later source-level inspection records the
PaddleX construction of blank, configured entries, and appended space for the
exact 18,710-class count in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md). This
establishes matching dictionary content and a structural index-map hypothesis,
not byte identity of `inference.yml`, runtime-output semantics, or permission
to copy/ship it.

The source-tree Apache-2.0 license is a concrete terms lead for the dictionary
source file, but `ppocr/utils/dict/README.md` gives only a general corpus
copyright caution and no dictionary-specific provenance or terms. The exact
ONNX package still lacks a revision-specific `LICENSE`/`NOTICE`, so neither the
dictionary trace nor the publisher path closes the terms gap for weights,
export, configuration, or distribution.

- v6 introduction commit: https://github.com/PaddlePaddle/PaddleOCR/commit/b03f46425e8ff4442b268ce449e3eef758146cd4
- Pinned recognizer configuration: https://github.com/PaddlePaddle/PaddleOCR/blob/2661c7c0ef5c613e8f93c6e93b2e052399f0f854/configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml#L14-L21
- Pinned dictionary: https://github.com/PaddlePaddle/PaddleOCR/blob/2661c7c0ef5c613e8f93c6e93b2e052399f0f854/ppocr/utils/dict/ppocrv6_dict.txt
- Pinned source license: https://github.com/PaddlePaddle/PaddleOCR/blob/2661c7c0ef5c613e8f93c6e93b2e052399f0f854/LICENSE
- Pinned dictionary README: https://github.com/PaddlePaddle/PaddleOCR/blob/2661c7c0ef5c613e8f93c6e93b2e052399f0f854/ppocr/utils/dict/README.md

### Official PP-OCRv6 publication-page check

The official PP-OCRv6 introduction confirms the public model-family context
for the selected medium tier, including its CTC/NRTR inference architecture
description and ONNX Runtime deployment mention. It does not state license or
usage terms for model parameters, the official ONNX exports, the embedded
dictionary, or derived artifacts. It is therefore availability and technical
context evidence only, not a revision-specific asset-terms source.

- Official PP-OCRv6 introduction: https://www.paddleocr.ai/latest/en/version3.x/algorithm/PP-OCRv6/PP-OCRv6.html

### Official issue-record check

The PaddleOCR project's official GitHub issue tracker contains a historical
license response that is relevant as a scope boundary, but not as a terms
source for M2. In issue
[`#8780`](https://github.com/PaddlePaddle/PaddleOCR/issues/8780), a project
collaborator answered on 2023-01-05 that the four PP-OCRv3 detector,
recognizer, slim recognizer, and classifier artifacts named in the question
were under Apache-2.0. The response is useful evidence that the project has
previously characterized specifically named pretrained OCR artifacts this way.
It predates PP-OCRv6, names no v6 artifact, revision, export, configuration,
or dictionary, and is not an artifact-specific grant for either M2 candidate.

An independent, unanswered 2026 request in the official PaddleX tracker also
shows why a repository-level license cannot be assumed to cover a model asset.
Issue [`PaddleX#5102`](https://github.com/PaddlePaddle/PaddleX/issues/5102)
asks PaddlePaddle to identify the license, commercial-use, redistribution, and
dataset terms for three other named pretrained model files. As checked on
2026-08-02, it has no maintainer answer. That issue concerns different models
and is not negative evidence about PP-OCRv6; it is recorded only because it
does not supply a general PaddlePaddle weight-license policy that could close
this review.

### Live publisher-status recheck (2026-08-03)

A subsequent live, read-only check did not locate a new terms source. The
official [`PaddleX#5102`](https://github.com/PaddlePaddle/PaddleX/issues/5102)
question remains open with its original request for artifact and commercial-use
terms; it still has no maintainer response. It concerns different named
artifacts, so it neither establishes nor disproves terms for PP-OCRv6, but it
continues not to supply a general publisher weight-license policy.

The public community pages for the exact
[`PP-OCRv6_medium_det_onnx`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_onnx/discussions)
and
[`PP-OCRv6_medium_rec_onnx`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_onnx/discussions)
repositories showed no open or closed discussion containing a publisher terms
statement. They still display the `apache-2.0` card metadata already recorded
above. These live, mutable repository pages are not an immutable
revision-specific license or notice for the candidate package files.

The current official [PaddleOCR repository page](https://github.com/PaddlePaddle/PaddleOCR)
announces PP-OCRv6 availability and identifies the source project as
Apache-2.0, but names no exact detector/recognizer artifact, ONNX/static
export, embedded dictionary, or distribution terms. It is therefore source
and availability context only, not a model-weight grant. No result from this
recheck meets either resumption condition below; `LIC-001` remains `Blocked`.

### Official safetensors cross-representation check (2026-08-03)

As an additional read-only check, the official `PaddlePaddle` safetensors
repositories were inspected without adding them as model candidates. The
current API records detector revision
`4236c2b61741a259c091fd879dcc4edc339e916c` and recognizer revision
`024cad6a831de75c2c3c26e711ba8c4a82ccd24b`; both cards display
`license: apache-2.0`. Each revision's reported file list contains only
`.gitattributes`, `README.md`, `config.json`, `inference.yml`,
`model.safetensors`, and `preprocessor_config.json`, with no `LICENSE` or
`NOTICE` file. Their `README.md` license badges link to `./LICENSE`, while the
corresponding revision-pinned `LICENSE` resolution returned HTTP 404 in this
check.

- Detector [card](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_safetensors) and [API](https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_det_safetensors)
- Recognizer [card](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_safetensors) and [API](https://huggingface.co/api/models/PaddlePaddle/PP-OCRv6_medium_rec_safetensors)

This is further metadata/absence evidence only. It does not make safetensors a
selected representation, does not establish its terms, and cannot supply the
artifact-specific grant missing for the distinct M2 static/ONNX candidates.
`LIC-001` remains `Blocked`.

## Current blocker and resumption condition

`LIC-001` is blocked on a durable, publisher/rightsholder-issued terms source
that applies to each exact M2 artifact file. The review has checked the
revision-pinned Hugging Face metadata and trees, the corresponding ModelScope
mirror metadata, the immutable PaddleX publisher path, the pinned PaddleOCR
source license/dictionary trace, the official PP-OCRv6 publication page, and
the official issue-records above. Those sources establish useful identity,
availability, historical context, and source-code context, but none supplies
the missing artifact-specific grant or notice set.

Resume this item only when one of the following can be preserved in the
evidence record for the selected immutable representation:

1. A publisher/rightsholder-issued license or terms document that explicitly
   covers the exact detector and recognizer files, their ONNX/static graph,
   configuration, and embedded dictionary; or
2. a publisher/rightsholder-issued statement that unambiguously ties an
   existing license to the exact revision-pinned package files, including any
   conversion/export and redistribution conditions.

The closure source must have a stable locator or archived copy permitted for
retention, identify the rightsholder/publisher relationship, and state the
applicable distribution, modification, attribution, and notice obligations.
It must be reviewed before accepting an artifact, producing a model-backed
fixture, or resolving `MODEL-DEC-001`. A user-provisioned local copy alone
does not satisfy this condition.

## Material-specific status

| Material | Evidence currently held | What remains required | Release status |
|---|---|---|---|
| PaddleOCR repository source | The read-only upstream repository identifies Apache License 2.0; the P0 record documents source-attribution and trademark boundaries. | If any non-trivial source is adapted, record file-level provenance, retained notices, modifications, and a source review. | No upstream source is currently included. |
| Static model parameters and graph/config files | Candidate revisions, checksums, sizes, preliminary card metadata, and an immutable PaddleX publisher path are recorded. | Obtain and preserve a revision-specific license/terms source; verify the publisher/rightsholder and terms for every selected file. | Unapproved. |
| Official ONNX exports | Separate pinned repository revisions/checksums, immutable LFS storage commits, and the PaddleX ONNX naming/hoster path are recorded. | Establish whether each export has its own applicable terms and its relationship to the static package; do not infer numerical or legal equivalence from a shared model name. | Unapproved. |
| Recognizer character dictionary and tokenizer behavior | The ONNX recognizer `inference.yml` embeds a 18,708-entry `character_dict`; its canonical entry stream matches the pinned upstream `ppocrv6_dict.txt` SHA-256. The exact-local source-level record maps it to blank at index `0`, ordered entries at `1..=18,708`, and appended space at `18,709`; this is not runtime-output validation. The source-tree Apache-2.0 license is a terms lead, while the dictionary README gives no file-specific provenance/terms. | Verify applicable terms, runtime index/blank/space behavior, decoder error handling, and any notice obligation before copying, extracting, or shipping it. | Unapproved. |
| Oracle and test fixtures | No model-backed input or expected-output fixture has been added. | Record original-author/source provenance, terms, privacy review, hashes, goldens, and applicable model evidence per [FIXTURE_AND_TOLERANCE_PLAN.md](FIXTURE_AND_TOLERANCE_PLAN.md). | Unapproved. |
| Conversion tools and generated outputs | No converter, conversion recipe, or converted output has been selected or run. | Record tool version/license, exact inputs and command, output hashes, notices, reproducibility, and tensor-differential evidence before any conversion is accepted. | Unapproved. |
| Rust/native dependencies | The bootstrap workspace has no third-party Cargo dependency. External-only `ort` spikes used `ort` 2.0.0-rc.13 with a temporary Python-wheel library and a separately source-built ONNX Runtime 1.28.0 library; see [RUNTIME_ORT_EVIDENCE.md](RUNTIME_ORT_EVIDENCE.md) and [RUNTIME_ORT_SOURCE_EVIDENCE.md](RUNTIME_ORT_SOURCE_EVIDENCE.md). Neither is a repository dependency or a distribution route. | Review the wrapper and complete native/transitive terms, notices, vulnerabilities, acquisition/build provenance, dynamic-loader behavior, `unsafe` boundary, and supported CPU/platform distribution targets under `RT-002` and `LIC-002`. | Unapproved; no runtime has been adopted for M2. |

## Explicit non-findings

This review does **not** establish any of the following:

- that the model-card license field covers every binary, embedded dictionary,
  configuration, or archive associated with a candidate;
- that a hosting-platform `author` field proves the publisher/rightsholder or
  grants model-weight rights;
- that the PaddleX publisher path, verified LFS storage commit, or
  dictionary-content match grants rights for ONNX weights, export,
  configuration, or redistribution;
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
   publisher/rightsholder relationship for every selected model, graph,
   configuration, and embedded dictionary file.
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
