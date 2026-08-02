# Parse-only ONNX ABI Inspection

Roadmap items: MOD-001, RT-002
Status: Exact local graph structure rechecked; no runtime selected or accepted
Inspection date: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose and boundary

tools/inspect_onnx_candidate.py is a developer-only, parse-only inspection
tool for an explicitly provisioned local ONNX file. It never downloads,
converts, writes, or executes a model. It provides reproducible graph-level
ABI evidence without turning candidate artifacts into a project dependency, a
fixture, an accepted manifest, or a runtime selection.

The tool is intentionally outside the Cargo build and test graph. Its optional
onnx parser is installed only in an ignored developer environment. Normal Rust
builds, tests, documentation builds, and CI do not require Python, ONNX, or a
model artifact.

## Input bounds and output boundary

Before parsing, the tool requires a direct regular file and rejects a symlink,
a non-regular or empty file, or a file over the explicit --max-model-bytes
limit (128 MiB by default). It streams the file SHA-256 before parsing it with
external data disabled, then rejects a graph that declares external initializer
data before checker validation or shape inference.

The emitted JSON is structural metadata only:

- file name, byte count, and SHA-256;
- parser version and parse-only method flags;
- graph IR/opset versions;
- tensor names, dtypes, and declared dimensions;
- operator names/counts and direct terminal graph-output operators.

It never emits an initializer, a model input/output tensor, a dictionary
entry, a user-machine path, or raw model data.

## Inspection method

The recorded result used onnx 1.22.0 with the following operations:

1. onnx.load_model(..., load_external_data=False);
2. onnx.checker.check_model(..., full_check=False);
3. external-initializer rejection; and
4. protobuf shape inference.

No ONNX Runtime session or another inference backend was loaded. No model
inference was executed.

For an external local environment containing onnx, replay the inspection as:

~~~sh
.oracle-venv/bin/python tools/inspect_onnx_candidate.py \
  --role detector --expect-m2-onnx <detector-inference.onnx>
.oracle-venv/bin/python tools/inspect_onnx_candidate.py \
  --role recognizer --expect-m2-onnx <recognizer-inference.onnx>
~~~

--expect-m2-onnx does not identify an artifact by name or hash. It verifies
only the known graph-level ABI for the M2 candidate role and fails clearly on a
different signature or terminal activation.

## Exact local results

The files passed to the tool were independently rehashed at inspection time
and match the already recorded candidate hashes.

| Role | ONNX file SHA-256 | Byte count | Input | Output | Direct output producer |
|---|---|---:|---|---|---|
| Detector | eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1 | 62,032,837 | x: FLOAT [dynamic, 3, dynamic, dynamic] | fetch_name_0: FLOAT [dynamic, 1, dynamic, dynamic] | Node 275: Sigmoid with no attributes |
| Recognizer | 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba | 76,554,979 | x: FLOAT [dynamic, 3, 48, dynamic] | fetch_name_0: FLOAT [dynamic, dynamic, 18,710] | Node 507: Softmax(axis=2) |

The tool assertion additionally checks one input and one output, the x and
fetch_name_0 names, FLOAT dtype, all declared dynamic M2 axes, detector
channel dimension 1, recognizer height 48, recognizer class dimension 18,710,
and the direct terminal operator.

The detector graph terminal Sigmoid is a structural candidate-output property.
Under a conforming ONNX implementation, it is a sigmoid-range map, but actual
numerical values and DB-postprocessing semantics remain runtime validation
work.

The recognizer graph is ONNX opset 11. Its direct terminal Softmax(axis=2)
acts on the [batch, time, 18,710] output. In that opset, Softmax flattens
dimensions before and from its axis, so it normalizes [batch * time, 18,710]:
each time-step row is class-normalized. This is a graph-declared score-shape
property, not evidence that an unselected runtime will produce numerically
correct, finite, or safe values.

## Negative check

The detector command with --max-model-bytes 1 exited with status 2 before the
parser ran and reported:

~~~text
error: model file exceeds 1 byte limit: 62032837
~~~

This proves the tool file-size gate for the inspected file only. It is not a
model-loader resource qualification or a substitute for the future Rust
adapter's own bounds.

A separate invocation with the repository PaddleOCR symlink exited with status
2 and reported that it refused the symlinked model path. It used lstat only and
did not follow or access the read-only upstream checkout.

## Remaining gates

This record advances source-level MOD-001 ABI evidence only. It does not
resolve any of the following:

1. LIC-001 artifact-specific terms and distribution approval;
2. RT-002 physical baseline CPU, lifecycle, native supply-chain, and Rust
   adapter evidence;
3. RT-003 raw tensor/output comparison and conversion/runtime drift;
4. RT-004 backend decision; or
5. REC-001 through REC-003 runtime-output, dictionary, text, score, and
   language validation.

No detector, recognizer, model, decoder, backend, public API, CLI, or
compatibility support claim follows from this parse-only inspection.
