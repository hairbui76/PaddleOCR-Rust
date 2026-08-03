# Local ONNX Candidate Inspection

Roadmap items: MOD-001, LIC-001  
Status: User-authorized external ONNX candidates were inventoried, parsed, and
given source-level recognizer CTC index-construction and Unicode-structure
inspections backed by an aggregate-only replay tool. `LIC-001` now records
terms evidence for this exact pair, but neither candidate is selected,
supported, converted, redistributed, or bundled

Initial inspection date: 2026-08-02

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

The replayable inspection also records a sorted graph-wide count for every
operator, totals nodes and initializers across graph-valued node attributes,
and reports the embedded-subgraph count. Both exact local candidates have zero
embedded subgraphs. The exact per-operator counts and their runtime
qualification boundary are recorded in
[ONNX_ABI_INSPECTION.md](ONNX_ABI_INSPECTION.md).

These are graph facts for runtime qualification. They do not select a runtime
or demonstrate that any particular backend supports the graphs correctly.

## Direct terminal-output ABI inspection

The reusable developer-only `tools/inspect_onnx_candidate.py` rechecked the
two exact local ONNX files with `onnx` 1.22.0.
It first refuses a symlink, a non-regular/empty file, a file larger than its
explicit 128 MiB default limit. It streams SHA-256, parses with external data
disabled, rejects graph-declared external tensor data before checker
validation or shape inference, and emits JSON structural metadata. It creates
no ONNX Runtime session, executes no model, retains no tensor output, and does
not write a file.

The exact recorded hashes remained unchanged. With `--expect-m2-onnx`, the
tool requires the following graph-level terminal ABI:

| Role | Direct producer of `fetch_name_0` | Structural consequence |
|---|---|---|
| Detector | Node 275, `Sigmoid`, no attributes | The candidate's one-channel dynamic NCHW output is structurally a terminal sigmoid map. A conforming runtime should produce values in the sigmoid range, but actual numerical output and DB semantics still require runtime validation. |
| Recognizer | Node 507, `Softmax` with `axis=2` | The candidate's dynamic `[batch, time, 18,710]` output is structurally a terminal class-axis softmax. Under ONNX opset 11's flattening rule, `axis=2` becomes `[batch * time, 18,710]`, so each time-step row is normalized across classes. Actual runtime values, finite/range behavior, decoder safety, and score semantics remain unverified. |

For a locally provisioned candidate, replay the tool with a local `onnx`
parser:

~~~sh
.oracle-venv/bin/python tools/inspect_onnx_candidate.py --role detector --expect-m2-onnx <detector-inference.onnx>
.oracle-venv/bin/python tools/inspect_onnx_candidate.py --role recognizer --expect-m2-onnx <recognizer-inference.onnx>
~~~

The tool output contains only names, dtypes, dimensions, graph/subgraph and
operator counts, terminal operators, input byte count, and file digest. It
emits no initializer, dictionary entry, raw model output, or user-machine
path. It is a reproducible parse-only ABI check, not an accepted artifact
manifest, runtime qualification, or artifact-terms decision.

## Serialized preprocessing declaration audit

This is a read-only comparison of the two exact local `inference.yml` files
with the pinned PaddleOCR transform implementations. It neither instantiates
those Python operators nor processes an image. The YAML is a package metadata
lead, not proof that a future ONNX runtime invokes the same operator path or
receives the same tensor bytes.

| Role | Exact serialized declaration | Pinned source-level implication | M2 disposition |
|---|---|---|---|
| Detector | `DecodeImage` declares BGR/HWC; `DetResizeForTest` is `null`; `NormalizeImage` declares `scale: 1./255.`, HWC `mean=[0.485, 0.456, 0.406]`, and `std=[0.229, 0.224, 0.225]`; `ToCHWImage` follows. | A no-argument `DetResizeForTest` constructor at the pinned baseline chooses `limit_side_len=736`, `limit_type="min"`, and `max_side_limit=4000`. It pads inputs with `height + width < 64` before resize. The normalization and CHW transpose agree structurally with the M2 detector rule, but the implicit resize mode does not. | The frozen M2 profile uses `limit_side_len=960`, `limit_type="max"`, and the same `4000` secondary limit. The candidate YAML must not be consumed as M2 preprocessing without a contract amendment or tensor/output evidence that validates an explicit M2 preprocessing path. |
| Recognizer | `DecodeImage` declares BGR/HWC and `RecResizeImg` declares only `image_shape=[3, 48, 320]`; the YAML does not serialize `infer_mode`, `eval_mode`, `character_dict_path`, `padding`, or interpolation. | With the pinned constructor defaults alone, `RecResizeImg` uses `resize_norm_img`: aspect-ratio resize to height 48 with `ceil`, width capped at 320, OpenCV linear interpolation, BGR-to-CHW transpose, `/255`, `(value - 0.5) / 0.5`, and zero right padding. A caller can change that branch by injecting omitted arguments. | The base shape and default normalization path resemble the M2 single-crop rule, but batching, dynamic width, actual caller options, interpolation bytes, and model-output consequences remain unverified. No recognizer preprocessing compatibility claim follows. |

The detector default is a material manifest conflict in addition to the
already recorded DB threshold conflict. It follows directly from the exact
`DetResizeForTest: null` YAML declaration and the `else` branch of
`ppocr/data/imaug/operators.py:DetResizeForTest.__init__` at baseline
`2661c7c0ef5c613e8f93c6e93b2e052399f0f854`; it is not inferred from a model
run. The recognizer observations similarly follow only from the serialized
fields and source defaults in `ppocr/data/imaug/rec_img_aug.py:RecResizeImg`
and `resize_norm_img`.

Before any Rust tensor implementation may use either path, `IMG-DEC-001`,
`IMG-002`, `TEN-001`, `PRE-001`, artifact terms, and raw-tensor validation must
establish the real selected-runtime input contract. In particular, a runtime
must not silently substitute the candidate's detector `736/min` defaults for
the public M2 `960/max` profile.

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

### Recognizer dictionary Unicode structural audit

On 2026-08-03, a disposable external Python 3.12.3 (Unicode 15.0.0)
standard-library harness audited only aggregate Unicode properties of the exact
local recognizer `inference.yml`. It first required a bounded regular,
non-symlink file, the recorded YAML SHA-256, the recorded ordered-entry-stream
SHA-256, 18,708 entries, and no exact duplicate. It emitted one sorted JSON
object of counts only: no dictionary entry, decoded text, ONNX tensor, or model
output was printed, retained, or added to the repository. That historical v1
harness source SHA-256 was
`cfe5fb23f85ad34d6cf53e05a9adca7e36219ae3e92587908028395497bb970b`; its
aggregate result SHA-256 was
`6a6b76ee7b118b3453e4914da47162662a767f257c7bdaa0a3e9d3cb87f909e9`.

| Aggregate check | Result |
|---|---|
| Pinned input and ordered stream | 150,580-byte YAML and 18,708-entry stream reverified as `991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129` and `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |
| Scalar shape | Every one of 18,708 entries is exactly one Unicode scalar; total scalar count is 18,708 |
| Whitespace and non-printable entries | Exactly one each, both `U+3000`; the serialized list still contains no literal `U+0020` |
| Excluded scalar classes | No entry contains a combining mark, control, format, private-use, unassigned, or noncharacter scalar |
| General categories | `Ll=386`, `Lm=11`, `Lo=16,150`, `Lt=4`, `Lu=287`, `Nd=20`, `Nl=29`, `No=73`, `Pc=2`, `Pd=7`, `Pe=22`, `Pf=3`, `Pi=3`, `Po=47`, `Ps=21`, `Sc=38`, `Sk=6`, `Sm=273`, `So=1,325`, `Zs=1` |
| NFC transform | `4` entries change; transformed values form `4` collision groups containing `8` entries |
| NFKC transform | `290` entries change; transformed values form `160` collision groups containing `376` entries |
| Case-fold transform | Transformed values form `290` collision groups containing `595` entries |

The same bounded harness rejected the detector YAML before parsing because its
digest did not equal the recognizer's pinned digest (controlled exit `1`). This
checks the audit's input identity; it does not make the disposable harness a
project model-inspection tool or artifact manifest.

The source-controlled
[`tools/audit_recognizer_dictionary.py`](../tools/audit_recognizer_dictionary.py)
now makes the same input-specific structural audit replayable without a Python
package dependency. It only accepts the exact recognizer YAML fingerprint; it
uses `lstat`, no-follow opening, and descriptor metadata to reject a symlink
and detect path replacement, bounds the read to `512 KiB`, validates the YAML
digest before its narrow parser, and emits aggregate JSON to stdout only. It
neither creates a dictionary file nor exposes an input path, entry, text,
tensor, or model output.

Replay it against an explicitly provisioned local recognizer configuration:

```sh
python3 -B tools/audit_recognizer_dictionary.py <recognizer-inference.yml>
```

The recorded v2 tool source SHA-256 is
`fbdbf4db44bda21af63481e1c3a41df6284dc1bc088a51de050da6597a79095a`. On the
same Python 3.12.3 / Unicode 15.0.0 environment, two positive runs were
byte-identical; each emitted result SHA-256
`1c002119e48e1d1a4e0f3296d02630281224a7209af825d6dc4d151ce4344802` under
schema `paddleocr-rust/dictionary-unicode-audit/v2`. Controlled detector-digest,
symlink, and 524,289-byte oversized-input probes each exited `2` before any
dictionary output. The tool is developer-only evidence machinery, not a model
resolver, runtime, asset approval, or normal Rust build dependency.

For this candidate's future decoder, the rule is therefore exact-scalar
preservation: map each class to its original scalar in index order, append
only the literal `U+0020` at class `18,709`, and preserve the resulting UTF-8
unchanged. In particular, default NFC/NFKC normalization, case folding, or
whitespace cleanup must not run before or after the CTC mapping. The serialized
`U+3000` and appended `U+0020` are distinct class values even though both are
space-like characters.

This is source-level dictionary structure only. It neither establishes model
language coverage, runtime score semantics, CTC decoding safety, text-output
compatibility, terms for the embedded data, nor support for the candidate.

This does **not** prove that a selected runtime emits semantically correct
scores for every class, that the ONNX export is behaviorally identical to the
PaddleX source path, that out-of-range/malformed runtime outputs are handled
safely, or that every language-specific decode behavior is compatible. The
exact-pair package terms decision is recorded separately under `LIC-001`, but
runtime/output/decoder safety and distribution policy remain `RT-003`,
`REC-001`/`REC-002`, and later model-decision work.

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
but no separate legal/provenance text. The publisher-hosted, same-revision
`license: apache-2.0` declaration is nevertheless the accepted `LIC-001`
terms record for this exact ONNX pair; its scope and downstream distribution
conditions are in [LICENSE_REVIEW.md](LICENSE_REVIEW.md).

## Remaining gates

1. Validate the source-level recognizer index construction against selected
   runtime outputs, safe Rust decoder bounds/errors, and any required
   language-specific behavior. The structural blank/space/count evidence above
   is necessary but not sufficient.
2. Run bounded runtime candidate proofs and raw tensor comparisons before
   selecting a backend under RT-002 through RT-004.
3. Resolve the artifact lifecycle and local-path policy under MODEL-DEC-001
   and MOD-002 through MOD-004.
4. Obtain legal offline input fixtures and differential results before making
   a detector, recognizer, pipeline, API, CLI, model, or compatibility claim.
