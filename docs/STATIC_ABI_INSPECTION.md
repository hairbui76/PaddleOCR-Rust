# Parse-only Static Program ABI Inspection

Roadmap items: MOD-001, RT-003
Status: The exact local static pair's program structure is recorded; no runtime,
artifact, or numerical equivalence is selected or established
Inspection date: 2026-08-04
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose and boundary

[`tools/inspect_static_candidate.py`](../tools/inspect_static_candidate.py) is a
developer-only, parse-only inspection tool for the `inference.json` program of
an explicitly provisioned local Paddle static inference package. It completes
the static half of the `MOD-001` graph/ABI requirement that the ONNX half
already records in [`ONNX_ABI_INSPECTION.md`](ONNX_ABI_INSPECTION.md).

The tool never downloads, converts, writes, or executes a model. It never
imports Paddle, PaddleX, ONNX, or an inference runtime: a PIR `inference.json`
is plain JSON, so the Python standard library is sufficient and no package
needs to be installed. The companion `inference.pdiparams` weight file is only
measured and hashed; its bytes are never parsed, decoded, or retained.

The tool is intentionally outside the Cargo build and test graph. Normal Rust
builds, tests, documentation builds, and CI require no Python, no Paddle, and no
model artifact.

## Input bounds and output boundary

Before parsing, the tool rejects a symlink, a non-regular or empty file, and a
file over the explicit `--max-program-bytes` limit (4 MiB by default). It
re-opens the checked path with `O_NOFOLLOW`, compares the `lstat` and `fstat`
device/inode/size to detect path replacement, reads the exact declared length,
and computes the SHA-256 before parsing. The optional `--parameters` file is
bounded the same way by `--max-parameter-bytes` (256 MiB by default) and is
streamed only through SHA-256.

It accepts one region containing one block and rejects any other program shape
rather than silently summarizing part of a graph.

The emitted JSON is structural metadata only:

- program file name, byte count, and SHA-256;
- declared PIR magic, version, and trainable flag;
- region/block/operation counts and a sorted operator histogram;
- parameter count, element count, declared tensor bytes, and dtype counts;
- declared input and fetched output names, dtypes, layouts, and dimensions;
- the direct producer index/operator of each fetched output, plus only its
  `axis` and `struct_name` attributes; and
- the optional parameter file's byte count, SHA-256, and container overhead.

It never emits a parameter name, a parameter value, a dictionary entry, a model
input/output tensor, or a user-machine path.

## Inspection method and replay

For an explicitly provisioned local static package:

~~~sh
python3 -B tools/inspect_static_candidate.py --role detector --expect-m2-static \
  --parameters <detector-inference.pdiparams> <detector-inference.json>
python3 -B tools/inspect_static_candidate.py --role recognizer --expect-m2-static \
  --parameters <recognizer-inference.pdiparams> <recognizer-inference.json>
~~~

`--expect-m2-static` does not identify an artifact by name or hash. It verifies
only the declared program-level ABI for the M2 candidate role: exactly one
program input and one fetched output, the `x` and `fetch_name_0` names, the
`float32` element type, the `NCHW` input layout, the exact declared dimension
vector, and the direct terminal operator (with `axis` for the recognizer).

The recorded tool source SHA-256 is
`e5a9cc47fbe12268c6e0bb0d43ecae565663f2858191e9ddf4a73230a1607561`. On CPython
3.12.3 without a third-party package, two consecutive runs of each role were
byte-identical: the detector result SHA-256 was
`b6a7851b9d4cbfc4a36674b6fcab334e2b8bc31b2057ea43d89443173cfc96ba` and the
recognizer result SHA-256 was
`f73d63ba48c11a8ccc7919e1858bc16c92030f14e47b661c18f33d504db53677`.

## Exact local results

The inspected files were independently rehashed by the tool and match the
already recorded candidate hashes, including the two `inference.pdiparams`
digests in the external provisioning record.

| Role | Program SHA-256 | Program bytes | Declared input | Fetched output | Direct output producer |
|---|---|---:|---|---|---|
| Detector | `0f1a7ec35da36173529c7a60238b7f7919e3831929c3f700ad90ad4896adecd5` | 312,150 | `x`: `float32`, `NCHW`, `[-1, 3, -1, -1]` | `fetch_name_0`: `float32`, `[-1, 1, -1, -1]`, column `0` | Operation 846, `1.sigmoid`, `struct_name` `/DBHead/Head/` |
| Recognizer | `0b2e25e990bd072f1bf77d59d67d508bce6c4bd44af6624e0fb27d6da2cd00e8` | 221,814 | `x`: `float32`, `NCHW`, `[-1, 3, 48, -1]` | `fetch_name_0`: `float32`, `[-1, -1, 18710]`, column `0` | Operation 579, `1.softmax` with `axis=2`, `struct_name` `/MultiHead/CTCHead/` |

Both programs declare `magic: pir`, `version: 3`, `trainable: true`, one region,
and one block. The `trainable` flag is a serialization field of the exported
program; it is not evidence that this repository may train, fine-tune, or
redistribute the artifact.

The two `struct_name` values are exporter-generated module-path strings. They
are the first direct artifact-side record that the pinned detector's terminal
operation was emitted under a DB head and the recognizer's under a CTC head
inside a multi-head module; they confirm the exporter's naming, not head
semantics. They agree with the frozen
M2 classic DB + CTC contract in
[`CLASSIC_OCR_CONTRACT.md`](CLASSIC_OCR_CONTRACT.md), but they are declared
structure only: DB thresholding, unclipping, scoring, CTC decoding, and score
semantics all remain gated `DET-00x`/`REC-00x` work.

## Parameter inventory and weight-file consistency

| Role | Parameter operations | Declared elements | Declared tensor bytes | `inference.pdiparams` bytes | Container overhead | `inference.pdiparams` SHA-256 |
|---|---:|---:|---:|---:|---:|---|
| Detector | 257 | 15,488,353 | 61,953,412 | 61,960,476 | 7,064 | `85218d2e3d98f5a21c58b4220627be923a97aee5db3cc71f39536ab31ac53960` |
| Recognizer | 161 | 19,115,158 | 76,460,632 | 76,465,087 | 4,455 | `1b01c79a914587933f615569e75de54f2e638ebb5d3f3b3c1b38c24ede8c7319` |

Every declared parameter in both programs is `float32` with a fully static
shape; neither program declares a dynamic-shape parameter. The declared tensor
bytes are the exact `element_count * 4` arithmetic of the program, and the
remaining 7,064 and 4,455 bytes are the unparsed serialization container.

This is a size-consistency observation between two files of the same package.
It does not decode, validate, or compare any weight value, and it does not show
that the ONNX export carries the same numerical weights.

## Exact static operator inventory

| Operator | Detector | Recognizer |
|---|---:|---:|
| `p` (parameter) | 257 | 161 |
| `0.combine` | 2 | 2 |
| `1.add` | 161 | 85 |
| `1.batch_norm_` | 4 | 3 |
| `1.concat` | 2 | 1 |
| `1.conv2d` | 101 | 47 |
| `1.conv2d_transpose` | 2 | 0 |
| `1.data` | 1 | 1 |
| `1.depthwise_conv2d` | 21 | 15 |
| `1.dropout` | 0 | 8 |
| `1.fetch` | 1 | 1 |
| `1.flatten` | 0 | 1 |
| `1.full` | 2 | 14 |
| `1.full_int_array` | 125 | 86 |
| `1.gelu` | 13 | 14 |
| `1.hardsigmoid` | 5 | 6 |
| `1.layer_norm` | 0 | 5 |
| `1.matmul` | 0 | 13 |
| `1.mean` | 5 | 6 |
| `1.multiply` | 5 | 6 |
| `1.nearest_interp` | 6 | 0 |
| `1.pool2d` | 1 | 2 |
| `1.relu` | 16 | 11 |
| `1.reshape` | 117 | 64 |
| `1.scale` | 0 | 2 |
| `1.shape64` | 0 | 1 |
| `1.sigmoid` | 1 | 0 |
| `1.slice` | 0 | 7 |
| `1.softmax` | 0 | 3 |
| `1.squeeze` | 0 | 1 |
| `1.stack` | 0 | 1 |
| `1.swish` | 0 | 5 |
| `1.transpose` | 0 | 9 |
| Total operations | 848 | 581 |

This is a candidate requirement inventory for a Paddle-executing reference
only. It is not a backend capability claim, and it does not prove that any
runtime executes these operators, attributes, or shapes correctly.

## Static-versus-ONNX structural comparison

The two `inference.json` files shipped inside the ONNX packages are
byte-identical to the two static-package programs: the detector pair both hash
to `0f1a7ec3…` and the recognizer pair both hash to `0b2e25e9…`. Running this
tool against the ONNX-package copies produced byte-identical inspection records
(`5073c0b0b4b5435efca4ce0ff82f59117ff7724a8e95c05cbf14d6dee4a459fa` for the
detector and `7f79f822482b4791bf38ff6e97be0e76fea7319ae27524d8e75567f59415bcaa`
for the recognizer, with `--parameters` omitted). Identical records follow
trivially from identical bytes and are not independent evidence. The two
packages therefore publish the same source program document; that is file
identity, not equality of the separately serialized weights.

The declared terminal ABI matches the separately recorded ONNX graph ABI in
every checked field:

| Checked field | Static program | ONNX graph |
|---|---|---|
| Detector input | `x`, `float32`, `[-1, 3, -1, -1]` | `x`, `FLOAT`, `[dynamic, 3, dynamic, dynamic]` |
| Detector output | `fetch_name_0`, `float32`, `[-1, 1, -1, -1]` | `fetch_name_0`, `FLOAT`, `[dynamic, 1, dynamic, dynamic]` |
| Detector terminal operator | `1.sigmoid` | `Sigmoid` |
| Recognizer input | `x`, `float32`, `[-1, 3, 48, -1]` | `x`, `FLOAT`, `[dynamic, 3, 48, dynamic]` |
| Recognizer output | `fetch_name_0`, `float32`, `[-1, -1, 18710]` | `fetch_name_0`, `FLOAT`, `[dynamic, dynamic, 18,710]` |
| Recognizer terminal operator | `1.softmax` with `axis=2` | `Softmax` with `axis=2` |

Several operator families also have equal counts across the two
representations under an assumed decomposition mapping. No ONNX node was traced
back to a static operation, so this is count-level agreement for a selected
subset, not a node-level correspondence:

| Static operators | ONNX operators | Detector | Recognizer |
|---|---|---|---|
| `1.conv2d` + `1.depthwise_conv2d` | `Conv` | 122 = 122 | 62 = 62 |
| `1.conv2d_transpose` | `ConvTranspose` | 2 = 2 | 0 = 0 |
| `1.relu` | `Relu` | 16 = 16 | 11 = 11 |
| `1.hardsigmoid` | `HardSigmoid` | 5 = 5 | 6 = 6 |
| `1.gelu` | `Erf` | 13 = 13 | 14 = 14 |
| `1.nearest_interp` | `Resize` | 6 = 6 | 0 = 0 |
| `1.pool2d` | `MaxPool` + `AveragePool` | 1 = 1 | 2 = 2 |
| `1.matmul` | `MatMul` | 0 = 0 | 13 = 13 |
| `1.softmax` | `Softmax` | 0 = 0 | 3 = 3 |
| `1.transpose` | `Transpose` | 0 = 0 | 9 = 9 |
| `1.mean` + 2 per `1.layer_norm` | `ReduceMean` | 5 = 5 | 16 = 16 |

The two representations are nevertheless not structurally identical. The
following differences are recorded rather than explained away; the list is
illustrative, not an exhaustive diff of the two graphs:

1. The detector's four `1.batch_norm_` operations have no `BatchNormalization`
   counterpart in the detector ONNX graph, while the recognizer's three
   `1.batch_norm_` operations do appear as three ONNX `BatchNormalization`
   nodes. Only the absence is observed here: because no weight value was
   decoded, this parse cannot distinguish constant folding from any other
   rewrite.
2. Shape-manipulation counts diverge sharply. The detector declares 117
   `1.reshape` against zero ONNX `Reshape`, and the recognizer declares 64
   against 8. The recognizer also declares 1 `1.concat` against 3 `Concat`, 1
   `1.squeeze` against 8 `Squeeze`, and 7 `1.slice` against 8 `Slice`, and the
   ONNX graph adds 4 `Shape` and 1 `Unsqueeze` nodes with no static
   counterpart.
3. Composite Paddle operators (`1.gelu`, `1.swish`, `1.layer_norm`, `1.stack`)
   are decomposed by the export, and the recognizer ONNX graph contains 143
   `Identity` nodes with no static counterpart. At most eight of those could
   correspond to its evaluation-mode `1.dropout` operations; the remainder are
   unattributed exporter pass-through nodes.
4. Elementwise counts diverge accordingly: the detector declares 161 `1.add`
   against 59 ONNX `Add` and 5 `1.multiply` against 31 `Mul` plus 13 `Div`,
   while the recognizer declares 85 `1.add` against 109 `Add`.

### Disposition

This record establishes that the two published packages ship the same program
document and the same declared terminal ABI, and that their operator graphs are
consistent with one export but not node-for-node identical. It deliberately does
**not** dispose of the static-versus-ONNX choice:

- No weight value from either representation has been decoded or compared.
- The external raw-tensor comparison has since completed and passed the
  predeclared `m2-tensor-v1` rule with zero violations across 7,057,864
  elements for the six declared shapes, while nearly every element still
  differs in its `f32` bit pattern; see
  [`RUNTIME_PROOF_PLAN.md`](RUNTIME_PROOF_PLAN.md). That measures output
  agreement for those shapes, not weight equality or graph equivalence.
- The `LIC-001` terms record permits the static pair only as a non-retained
  external `RT-003` oracle, not as a selected or distributable artifact; see
  [`LICENSE_REVIEW.md`](LICENSE_REVIEW.md).

The backend/format decision remains `RT-004` / `D-006`, and the artifact
lifecycle decision remains `MODEL-DEC-001` / `D-007`.

## Negative checks

Each probe exited with status `2` and produced no inspection record:

| Probe | Reported error |
|---|---|
| Detector program inspected with `--role recognizer --expect-m2-static` | `expected 'x' shape [-1, 3, 48, -1], got [-1, 3, -1, -1]` |
| Detector program with `--max-program-bytes 1` | `program file exceeds 1 byte limit: 312150` |
| The repository `PaddleOCR` symlink as the program path | `refusing a symlinked program path` |
| The detector `inference.onnx` binary as the program path | `program file exceeds 4194304 byte limit: 62032837` |
| The same binary with `--max-program-bytes 134217728` | `program file is not valid UTF-8` |
| Detector parameters with `--max-parameter-bytes 1024` | `parameter file exceeds 1024 byte limit: 61960476` |
| A non-existent path | `cannot inspect program path` |
| The model root directory | `program path is not a regular file` |
| A 200,000-byte prefix of the detector program | `program file is not valid JSON: Expecting value` |
| A self-authored document declaring `magic: onnx` | `program document is not a PIR program` |
| A self-authored two-block program | `this inspection supports one region with one block; received 1 regions and 2 blocks` |
| A self-authored `null` document | `program document is not a JSON object` |

The last four probes used disposable self-authored documents outside the
repository; no model file was modified, copied, or retained.

The symlink probe used `lstat` only. It did not follow, open, read, or write
anything inside the read-only upstream checkout.

These prove the tool's own gates for the inspected inputs. They are not a model
loader resource qualification and not a substitute for the future Rust adapter's
own bounds.

## Remaining gates

This record advances source-level `MOD-001` static ABI evidence only. It does
not resolve any of the following:

1. `RT-003` raw tensor comparison, its predeclared tolerance metric, and the
   required second fresh process;
2. `RT-002` physical baseline CPU, lifecycle, native supply-chain, and Rust
   adapter evidence;
3. `RT-004` backend selection or `MODEL-DEC-001` artifact lifecycle policy;
4. `REC-001` through `REC-003` runtime-output, dictionary, text, score, and
   language validation; or
5. `DET-001` through `DET-003` detector preprocessing, DB postprocessing, and
   geometry behaviour.

No detector, recognizer, model, decoder, backend, public API, CLI, or
compatibility support claim follows from this parse-only inspection.
