# Local ONNX Candidate Inspection

Roadmap items: MOD-001, LIC-001  
Status: User-authorized external ONNX candidates were inventoried, parsed, and
given a source-level recognizer CTC index-construction inspection; neither
candidate is accepted, supported, converted, redistributed, or bundled
Inspection date: 2026-08-02  
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Scope and boundary

The project user explicitly authorized a one-time Hugging Face CLI download of
the two revision-pinned ONNX candidates into a user-chosen directory outside
this repository and the read-only PaddleOCR checkout. The complete external
record, including its local root, is retained beside those files as
provisioning-evidence.json. It is intentionally not a project resolver,
environment-variable, cache, CLI option, or distribution contract.

The download used hf 1.26.0. Every expected package-root file was verified as
a regular non-symlink file. Hugging Face local metadata recorded the requested
immutable revision for each downloaded file. No file was copied into this
repository, test fixture, Cargo package, or build output.

This inspection uses onnx 1.22.0 only as an isolated developer-side protobuf
parser. It performed onnx.checker.check_model with full_check disabled and
never loaded ONNX Runtime or executed model inference.

## File inventory and hash verification

| Candidate key | Repository and revision | File | Bytes | SHA-256 | Result |
|---|---|---|---:|---|---|
| m2-onnx-det-v6-medium | PaddlePaddle/PP-OCRv6_medium_det_onnx at 61323801669c338b7891481ec7bac61ce31b576a | .gitattributes | 1,519 | 11ad7efa24975ee4b0c3c3a38ed18737f0658a5f75a0a96787b576a78a023361 | Recorded locally |
| m2-onnx-det-v6-medium | Same | README.md | 16,089 | 3046e3aab0194a2291bb3941c93b980c2b3a938a24a5be88354968f6d6187ac8 | Recorded locally |
| m2-onnx-det-v6-medium | Same | inference.json | 312,150 | 0f1a7ec35da36173529c7a60238b7f7919e3831929c3f700ad90ad4896adecd5 | Matches candidate metadata |
| m2-onnx-det-v6-medium | Same | inference.onnx | 62,032,837 | eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1 | Matches candidate metadata |
| m2-onnx-det-v6-medium | Same | inference.yml | 886 | 7298d5ead546584af2504d03355f881ac7a7bc0eb1e282d3e159277c1d0af871 | Matches candidate metadata |
| m2-onnx-rec-v6-medium | PaddlePaddle/PP-OCRv6_medium_rec_onnx at 50c7eacafc52fa7bcf4194e8cd08e46f8558504b | .gitattributes | 1,519 | 11ad7efa24975ee4b0c3c3a38ed18737f0658a5f75a0a96787b576a78a023361 | Recorded locally |
| m2-onnx-rec-v6-medium | Same | README.md | 16,587 | ebce8d28436623ecab4952e24935aed86b3f8ecaf8f8736b92d5544f60fae1e9 | Recorded locally |
| m2-onnx-rec-v6-medium | Same | inference.json | 221,814 | 0b2e25e990bd072f1bf77d59d67d508bce6c4bd44af6624e0fb27d6da2cd00e8 | Matches candidate metadata |
| m2-onnx-rec-v6-medium | Same | inference.onnx | 76,554,979 | 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba | Matches candidate metadata |
| m2-onnx-rec-v6-medium | Same | inference.yml | 150,580 | 991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129 | Matches candidate metadata |

The locally present inference.json files match the previously recorded static
candidate metadata hashes. That is file-identity evidence only; it does not
make either ONNX package graph-, numerical-, license-, or runtime-equivalent
to a static package.

## Parse-only graph inspection

| Role | ONNX opset | Input | Output | Nodes / initializers | External tensor data |
|---|---:|---|---|---:|---|
| Detector | 14 | x: FLOAT [dynamic, 3, dynamic, dynamic] | fetch_name_0: FLOAT [dynamic, 1, dynamic, dynamic] | 276 / 226 | None |
| Recognizer | 11 | x: FLOAT [dynamic, 3, 48, dynamic] | fetch_name_0: FLOAT [dynamic, dynamic, 18,710] | 508 / 254 | None |

The detector uses Add, Concat, Conv, ConvTranspose, Div, Erf, HardSigmoid,
MaxPool, Mul, ReduceMean, Relu, Resize, and Sigmoid. The recognizer uses Add,
AveragePool, BatchNormalization, Concat, Conv, Div, Erf, HardSigmoid,
Identity, MatMul, MaxPool, Mul, Pow, ReduceMean, Relu, Reshape, Shape,
Sigmoid, Slice, Softmax, Sqrt, Squeeze, Sub, Transpose, and Unsqueeze.

These are graph facts for runtime qualification. They do not select a runtime
or demonstrate that any particular backend supports the graphs correctly.

## Recognizer CTC dictionary index inspection

This is a read-only, source-level ABI inspection of the exact local recognizer
configuration and ONNX graph. It does not create a Rust dictionary, invoke a
model session, or retain any dictionary content in this repository.

A deliberately narrow parser accepted only the observed
`PostProcess.character_dict` list syntax in the local `inference.yml`: plain
scalars and YAML single-quoted scalars (including doubled single-quote
escapes). It rejected a missing list, an empty item, double-quoted scalars, or
any unexpected line in that list. It found 18,708 entries, no duplicate entries,
and no literal U+0020 space entry. After unescaping, their ordered UTF-8 stream
with one LF per entry was byte-identical to the pinned PaddleOCR dictionary:

| Input / derived value | Result |
|---|---|
| Exact local `inference.yml` | 150,580 bytes; SHA-256 `991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129` |
| Ordered dictionary-entry stream | 18,708 entries; SHA-256 `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |
| Duplicate entries / literal space entries | `0` / `0` |
| Exact local ONNX output | `fetch_name_0`, `FLOAT`, dynamic × dynamic × `18,710` |
| Derived candidate class count | `1` blank + `18,708` entries + `1` space = `18,710` |
| Canonical index-map SHA-256 | `852ce132b49df2487fbf6985c2269dc77e77ef202683be1bfbbafed0ba7a6f08` |

The canonical index-map digest is over a domain-separated, non-asset byte
stream. The notation below describes the calculation; it does not embed or
redistribute the dictionary:

```text
"paddleocr-rust/ctc-index-map/v1\0blank\0"
  + entry_1_utf8 + "\0" + ... + entry_18708_utf8
  + "\0space"
```

The pinned training configuration names the same dictionary and enables
`use_space_char: true`. The corresponding immutable PaddleX source path builds
`CTCLabelDecode` by passing the configured `character_dict` without an explicit
space override; its default is therefore `use_space_char=True`. The base
decoder appends one literal space, and `CTCLabelDecode.add_special_char`
prepends `blank`. Its decoder treats index `0` as the ignored CTC blank.

- [PaddleOCR v6-medium configuration](https://github.com/PaddlePaddle/PaddleOCR/blob/2661c7c0ef5c613e8f93c6e93b2e052399f0f854/configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml#L17-L20)
- [PaddleX post-process construction](https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/models/text_recognition/predictor.py#L217-L221)
- [PaddleX default-space handling](https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/models/text_recognition/processors.py#L112-L123)
- [PaddleX CTC special token construction](https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/models/text_recognition/processors.py#L287-L319)
- [PaddleX blank-token filtering](https://github.com/PaddlePaddle/PaddleX/blob/e0068ce0bfe75b2992e5b38d06a0393c70f887f7/paddlex/inference/models/text_recognition/processors.py#L213-L232)

For that source-level construction, the structural index sequence is therefore
`0 = blank`, `1..=18,708 = character_dict entries in YAML order`, and
`18,709 = literal U+0020 space`. The ONNX output's final dimension agrees with
that count.

This does **not** prove that a selected runtime emits semantically correct
scores for every class, that the ONNX export is behaviorally identical to the
PaddleX source path, that out-of-range/malformed runtime outputs are handled
safely, or that every language-specific decode behavior is compatible. It also
does not establish terms for the configuration, dictionary, weights, export, or
distribution. Those remain `RT-003`, `REC-001`/`REC-002`, `LIC-001`, and later
model-decision work.

## Local package license observation

Both README.md files start with the model-card field `license: apache-2.0` and
display a badge pointing to `./LICENSE`. A later read-only immutable-revision
audit confirmed that the detector README SHA-256
`3046e3aab0194a2291bb3941c93b980c2b3a938a24a5be88354968f6d6187ac8` and the
recognizer README SHA-256
`ebce8d28436623ecab4952e24935aed86b3f8ecaf8f8736b92d5544f60fae1e9` match the
remote `resolve/<revision>/README.md` bytes. The same audit matched each
canonical non-weight text asset and found exactly five files in each recursive
remote tree: `.gitattributes`, `README.md`, `inference.json`,
`inference.onnx`, and `inference.yml`.

Neither downloaded package contains `LICENSE`, `NOTICE`, or a third-party
notice; each immutable `resolve/<revision>/LICENSE` URL returns HTTP `404`.
The README badge is therefore dangling. Neither README has another copyright,
notice, terms, dataset/training-data, third-party, or attribution statement.
The recognizer `inference.yml` embeds a `character_dict` with 18,708 entries
but no legal/provenance text. This strengthens the revision-specific evidence
gap; it does not resolve applicable terms for weights, embedded dictionary
data, or redistribution. See [LICENSE_REVIEW.md](LICENSE_REVIEW.md) for the
immutable API/tree URLs and remaining closure conditions.

## Remaining gates

1. Review durable revision-specific terms and publisher/rightsholder evidence
   for the selected representation under LIC-001.
2. Validate the source-level recognizer index construction against selected
   runtime outputs, safe Rust decoder bounds/errors, and any required
   language-specific behavior. The structural blank/space/count evidence above
   is necessary but not sufficient.
3. Run bounded runtime candidate proofs and raw tensor comparisons before
   selecting a backend under RT-002 through RT-004.
4. Resolve the artifact lifecycle and local-path policy under MODEL-DEC-001
   and MOD-002 through MOD-004.
5. Obtain legal offline input fixtures and differential results before making
   a detector, recognizer, pipeline, API, CLI, model, or compatibility claim.
