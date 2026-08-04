# M2 Model Artifact Candidate Record

Roadmap item: `MOD-001`
Status: In progress; remote and user-authorized local provenance/ABI discovery
plus bounded external runtime diagnostic evidence. `LIC-001` has confined
terms evidence for the exact ONNX pair and static `RT-003` oracle pair, but no
artifact or runtime is selected or supported
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Evidence captured: 2026-08-02

## Scope and non-decision

This record identifies reproducible official candidate artifacts for the
selected M2 family, `PP-OCRv6_medium_det` plus `PP-OCRv6_medium_rec`. It does
not select a backend, permit a model download, approve redistribution, add a
runtime dependency, or establish a compatibility claim.

The candidates were located through the pinned upstream module documentation
and then inspected as remote metadata/text configuration only. No model weight,
dictionary, cache, conversion output, or remote content was stored in this
repository. Normal builds and tests remain entirely offline.

The local artifact rule remains unchanged: a user must explicitly provision an
approved artifact outside version control, and future code must verify its
identity and hash before use. It must never silently download a candidate from
any URL below.

## Official source candidates

The upstream documentation links the PaddleX BOS inference archives. The
official PaddlePaddle Hugging Face repositories below expose static-package
candidates and separately published ONNX exports. Their byte-for-byte
relationship to the BOS archives is unverified. For the exact ONNX detector and
recognizer revisions, the publisher-hosted model-card `license: apache-2.0`
declarations close the confined `LIC-001` terms-evidence scopes described in
[LICENSE_REVIEW.md](LICENSE_REVIEW.md): runtime/evidence work for the ONNX
pair and a non-retained independent static/Paddle raw-tensor oracle for the
static pair. Neither scope selects an artifact/runtime or approves bundling any
file. P3/P13 still require the remaining provenance, runtime, release-policy,
and final audit work.

| Role | Static Paddle candidate (pinned revision) | `inference.pdiparams` SHA-256 / bytes | Official ONNX candidate (pinned revision) | `inference.onnx` SHA-256 / bytes |
|---|---|---|---|---|
| Detector | [`PaddlePaddle/PP-OCRv6_medium_det`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det) @ `8e0f56fb2ef86b461d99cfc7ac5c137738985f61` | `85218d2e3d98f5a21c58b4220627be923a97aee5db3cc71f39536ab31ac53960` / `61,960,476` | [`PaddlePaddle/PP-OCRv6_medium_det_onnx`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_onnx) @ `61323801669c338b7891481ec7bac61ce31b576a` | `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` / `62,032,837` |
| Recognizer | [`PaddlePaddle/PP-OCRv6_medium_rec`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec) @ `e5a92bcbc5cc1b494628e458d267778f0704fd7c` | `1b01c79a914587933f615569e75de54f2e638ebb5d3f3b3c1b38c24ede8c7319` / `76,465,087` | [`PaddlePaddle/PP-OCRv6_medium_rec_onnx`](https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_onnx) @ `50c7eacafc52fa7bcf4194e8cd08e46f8558504b` | `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` / `76,554,979` |

The pinned upstream sources also point at these BOS archive locations, which
are source locators rather than automatic-download endpoints:

- `https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_medium_det_infer.tar`
- `https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_medium_rec_infer.tar`

The BOS objects are not yet acceptable as reproducible M2 artifacts because no
immutable revision/content hash has been reviewed for them. A later evidence
record may choose one verified source representation; static and ONNX files
must not be treated as interchangeable merely because their model names match.

### Publisher linkage and immutable storage trace

At PaddleX v3.7.0 source commit
`e0068ce0bfe75b2992e5b38d06a0393c70f887f7`, the official-model mapping names
both M2 family identifiers as ONNX-supported, derives `<model>_onnx`, and its
Hugging Face hoster forms `PaddlePaddle/<model_name>`. This strengthens the
publisher path for the candidate locators, but the code does not pin a
revision or provide model/export terms. The two public Hugging Face LFS commit
records identify the exact ONNX objects in the table above; their UI verified
marker is chain-of-custody evidence for the bytes, not legal or numerical
equivalence evidence.

- PaddleX model mapping: https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/utils/official_models.py#L515-L571
- PaddleX Hugging Face hoster: https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/utils/official_models.py#L773-L784
- Detector LFS object commit: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_det_onnx/commit/dbee314685dad0b116a3d2faa7627776f936f085
- Recognizer LFS object commit: https://huggingface.co/PaddlePaddle/PP-OCRv6_medium_rec_onnx/commit/a7c2cc35beb94846af153645f5a619b6be905786

The recognizer's canonical YAML `PostProcess.character_dict` entry stream is
also content-identical to the pinned upstream `ppocrv6_dict.txt` after YAML
unescaping and one-entry-per-LF normalization; the shared SHA-256 is
`b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d`.
This is dictionary-content provenance evidence only. A separate, exact-local
source-level CTC index-construction inspection now records how the 18,708
ordered entries, one blank, and one appended space structurally correspond to
the 18,710-class ONNX output. It does not establish runtime output semantics
or permission to retain or distribute a dictionary independently of the
selected ONNX package. The package-level terms decision and its narrow scope
are in [LICENSE_REVIEW.md](LICENSE_REVIEW.md).

## Captured package metadata

The static candidates have the following small metadata files. Their SHA-256
values were calculated from the remote text files at the pinned candidate
revisions; no binary model content was downloaded.

| Role | `inference.json` bytes / SHA-256 | `inference.yml` bytes / SHA-256 |
|---|---|---|
| Detector | `312,150` / `0f1a7ec35da36173529c7a60238b7f7919e3831929c3f700ad90ad4896adecd5` | `886` / `7298d5ead546584af2504d03355f881ac7a7bc0eb1e282d3e159277c1d0af871` |
| Recognizer | `221,814` / `0b2e25e990bd072f1bf77d59d67d508bce6c4bd44af6624e0fb27d6da2cd00e8` | `150,580` / `991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129` |

The separately published ONNX candidates also list an `inference.yml` file.
Streaming those text files from their pinned revisions with a 1 MiB transfer
limit, without storing them, produced the following fingerprints:

| Role | ONNX `inference.yml` bytes / SHA-256 | Relationship to static configuration |
|---|---|---|
| Detector | `886` / `7298d5ead546584af2504d03355f881ac7a7bc0eb1e282d3e159277c1d0af871` | Same configuration fingerprint as the static detector candidate. |
| Recognizer | `150,580` / `991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129` | Same configuration fingerprint as the static recognizer candidate. |

A matching configuration fingerprint is only file-identity evidence for that
small text file. It does not establish graph, tensor, numerical, dictionary,
license, conversion, or package equivalence between the static and ONNX
candidates.

Both `inference.json` files are Paddle program JSON, not ONNX. Each exposes one
input named `x` and one fetched output named `fetch_name_0`:

| Role | Input ABI observed in static graph | Output ABI observed in static graph | Status |
|---|---|---|---|
| Detector | `x`: `float32`, NCHW, `[-1, 3, -1, -1]` | `fetch_name_0`: `float32`, NCHW, `[-1, 1, -1, -1]` | Verified locally by a parse-only replayable inspection; runtime behaviour still unverified. |
| Recognizer | `x`: `float32`, NCHW, `[-1, 3, 48, -1]` | `fetch_name_0`: `float32`, NCHW, `[-1, -1, 18710]` | Verified locally by the same inspection; dictionary/index correspondence is recorded separately. |

The exact local static programs were re-parsed with the standard-library tool
[`tools/inspect_static_candidate.py`](../tools/inspect_static_candidate.py)
without importing Paddle or executing a graph. That record adds the declared
PIR version, the full operator histogram, a parameter inventory whose declared
`float32` bytes agree with each `inference.pdiparams` size to within a small
serialization container, the direct terminal operators `1.sigmoid` and
`1.softmax(axis=2)`, and their `/DBHead/Head/` and `/MultiHead/CTCHead/`
`struct_name` values. It also records that each ONNX package ships a
byte-identical copy of the corresponding static program, that several operator
families correspond exactly across the two representations, and that the
detector's four `1.batch_norm_` operations have no ONNX counterpart while the
recognizer's three do. See
[STATIC_ABI_INSPECTION.md](STATIC_ABI_INSPECTION.md); no weight value was
decoded and no representation was selected.

The recognizer metadata declares BGR decode semantics and `RecResizeImg` base
shape `[3, 48, 320]`. Its HPI metadata records static-engine dynamic shapes
from `[1, 3, 48, 160]` through `[8, 3, 48, 3200]`. The exact local parse finds
`fetch_name_0` directly produced by `Softmax(axis=2)`; with ONNX opset 11
semantics and the `[batch, time, 18,710]` output shape, that is a per-time-step
class-axis softmax declaration. The detector's corresponding one-channel
output is directly produced by terminal `Sigmoid`. These are parse-only graph
facts, not runtime numerical, detector/recognizer, or compatibility evidence;
see [ONNX_ABI_INSPECTION.md](ONNX_ABI_INSPECTION.md).

The inline recognizer `character_dict` has 18,708 serialized entries. The
exact local inspection confirms that it has no duplicates or literal space
entry, matches the pinned dictionary stream, and structurally maps with a
PaddleX CTC decoder's source-level construction to `0 = blank`, the 18,708
entries in order, and `18,709 = space`. That makes 18,710 classes and matches
the observed ONNX output dimension. The evidence, method, and canonical
non-asset index-map digest are recorded in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md).

It is not yet a verified runtime dictionary ABI. P3/P5 must still validate
actual output semantics, all relevant language behavior, malformed/out-of-range
runtime results, and the Rust decoder against an accepted artifact under
approved terms.

## Artifact metadata versus M2 behavior

The detector candidate's `inference.yml` declares BGR/HWC normalization with
the same ImageNet mean/std values used by the classic contract, but it also
declares `thresh: 0.2`, `box_thresh: 0.45`, `unclip_ratio: 1.4`, and
`max_candidates: 3000`. Those values conflict with the deliberately selected
M2 classic profile (`0.3`, `0.6`, `1.5`, and `1000`).

The same exact detector YAML declares `DetResizeForTest: null`. At the pinned
PaddleOCR baseline, applying that no-argument declaration through the named
operator's constructor selects its implicit `736/min` resize policy and a
`4000` maximum-side limit. The frozen M2 profile instead uses `960/max` with
the same `4000` secondary limit. This is a source-level configuration conflict,
not a model-output result: a later runtime may supply different operator
arguments, but Rust must never silently adopt either unverified path. The
read-only derivation and recognizer-default caveats are recorded in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md).

`CLASSIC_OCR_CONTRACT.md` remains the public M2 behavior authority. A runtime
must not silently consume the candidate manifest's thresholds or resize
defaults. Before the candidate can be accepted, P3/P5 must demonstrate that
it produces the documented detector map and that applying the M2 profile is
valid; otherwise the contract must be amended with reviewed fixture evidence
or the candidate must be rejected.

Similarly, the recognizer candidate is exported from a CTC+NRTR multi-head
architecture but exposes a CTC-shaped fetch. M2 supports only the validated
CTC fetch/decoder path. No NRTR head, word boxes, Arabic reversal, or generic
multilingual claim is accepted merely from this metadata.

## User-authorized local candidate inventory

On 2026-08-02, the project user authorized the two exact ONNX packages to be
downloaded into an external user-owned directory. Each expected file was a
regular file, the local Hugging Face metadata recorded the requested immutable
revision, and the runtime-relevant file hashes matched this record. A
parse-only `onnx.checker.check_model` inspection recorded actual ONNX opsets,
input/output signatures, operator sets, node counts, and the absence of
external tensor data. The detailed evidence is in
[LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md).

This inventory is not a local model resolver, accepted artifact manifest,
runtime decision, or support claim. The bounded external diagnostics allowed by
`RT-002` are recorded in
[`RUNTIME_TRACT_EVIDENCE.md`](RUNTIME_TRACT_EVIDENCE.md),
[`RUNTIME_ORT_EVIDENCE.md`](RUNTIME_ORT_EVIDENCE.md), and
[`RUNTIME_ORT_SOURCE_EVIDENCE.md`](RUNTIME_ORT_SOURCE_EVIDENCE.md); they do
not approve the artifacts for project adoption, conversion, distribution,
bundling, or model-derived fixture retention. The later `RT-003` static
supplement permits only one external, hash-verified, non-retained raw-tensor
oracle; it does not make the static candidate a supported artifact. The local
presence of `inference.json` files with the same hashes as static candidate
metadata does not make static and ONNX representations equivalent.

On 2026-08-04, the confined static-oracle terms supplement permitted the exact
static pair to be provisioned into the same external model root. Every expected
`.gitattributes`, `README.md`, `inference.json`, `inference.pdiparams`, and
`inference.yml` file was a regular non-symlink file and matched its recorded
byte count/SHA-256. A first external Paddle Inference versus ONNX Runtime
comparison is recorded in [RUNTIME_PROOF_PLAN.md](RUNTIME_PROOF_PLAN.md); it
is not a final static graph/runtime inspection, artifact acceptance, numerical
equivalence result, or support claim.

## Required acceptance work

`MOD-001` cannot become `Done` until all of the following exist:

1. A revision-specific provenance/license record for the exact chosen static
   or ONNX artifacts and every accompanying dictionary/configuration file.
2. A local provisioning manifest that names every required file, byte length,
   SHA-256, format, and location policy, with no automatic acquisition.
3. A safe, bounded local inspection that confirms the actual file hashes,
   tensor names/dtypes/layouts/shapes, required operators, and output order.
   The ONNX format/hash/signature/operator portion is recorded in
   [LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md) and
   the static portion in [STATIC_ABI_INSPECTION.md](STATIC_ABI_INSPECTION.md);
   backend-visible graph semantics still require runtime candidate validation.
4. A verified recognizer dictionary ABI, including CTC blank/space behavior
   and the 18,710-class output correspondence. The source-level index
   construction, structural count, and aggregate Unicode audit are recorded in
   [LOCAL_ONNX_CANDIDATE_INSPECTION.md](LOCAL_ONNX_CANDIDATE_INSPECTION.md),
   with a bounded developer-only replay tool that emits no dictionary content.
   The evidence includes the requirement to preserve exact scalars without
   default normalization, case folding, or whitespace cleanup. Runtime
   semantics and safe Rust decoder validation remain required.
5. A recorded disposition of the static-vs-ONNX choice. The structural half is
   recorded in [STATIC_ABI_INSPECTION.md](STATIC_ABI_INSPECTION.md): the two
   packages ship byte-identical program documents and the same declared terminal
   ABI, but their operator graphs are not node-for-node identical. The numerical
   half is recorded in [RUNTIME_PROOF_PLAN.md](RUNTIME_PROOF_PLAN.md): a
   completed two-process external capture found zero `m2-tensor-v1` violations
   across 7,057,864 elements for the six declared shapes, with nearly every
   element still differing in its `f32` bit pattern. Remaining for this item are
   the runtime dictionary behaviour and the written selection itself; the
   backend choice is `RT-004` and the artifact policy is `MODEL-DEC-001`.
6. Offline golden capture and runtime qualification before any compatibility
   ledger row is marked `Verified`.

Until then, the data in this file is discovery evidence and a rejection/acceptance
checklist—not a supported model list.
