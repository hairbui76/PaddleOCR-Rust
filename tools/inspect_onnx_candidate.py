#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Inspect a locally provisioned ONNX graph without executing inference.

This developer-only tool never downloads, converts, writes, or executes a
model. It accepts one regular, non-symlinked ONNX file under an explicit size
limit, verifies its SHA-256 while streaming, parses its protobuf graph with
onnx, and emits structural ABI metadata as JSON on stdout.

The optional M2 assertion is intentionally specific to the currently recorded
PP-OCRv6 medium ONNX candidates. It validates only graph-level input/output
and direct terminal-operator facts; it does not select a runtime, establish
numerical equivalence, or approve the artifact's terms.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import stat
import sys
from pathlib import Path
from typing import Any, Sequence


SCHEMA_VERSION = "paddleocr-rust/onnx-abi-inspection/v1"
DEFAULT_MAX_MODEL_BYTES = 128 * 1024 * 1024
HASH_CHUNK_BYTES = 1024 * 1024


def positive_integer(value: str) -> int:
    """Parse one strictly positive command-line integer."""

    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be an integer") from error
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line options without importing the optional onnx package."""

    parser = argparse.ArgumentParser(
        description=(
            "Parse one local ONNX graph and emit bounded structural ABI metadata."
        )
    )
    parser.add_argument(
        "--role",
        required=True,
        choices=("detector", "recognizer"),
        help="Model role recorded in the inspection result.",
    )
    parser.add_argument(
        "--expect-m2-onnx",
        action="store_true",
        help=(
            "Reject a graph that does not match the recorded PP-OCRv6 medium "
            "ONNX terminal ABI for the selected role."
        ),
    )
    parser.add_argument(
        "--max-model-bytes",
        type=positive_integer,
        default=DEFAULT_MAX_MODEL_BYTES,
        help=(
            "Maximum accepted file size before parsing "
            f"(default: {DEFAULT_MAX_MODEL_BYTES})."
        ),
    )
    parser.add_argument(
        "model",
        type=Path,
        help="Explicit local path to one ONNX file.",
    )
    return parser.parse_args(arguments)


def require_regular_bounded_file(path: Path, maximum_bytes: int) -> int:
    """Reject missing, symlinked, non-regular, empty, or oversized files."""

    try:
        metadata = path.lstat()
    except OSError as error:
        raise ValueError(f"cannot inspect {path}: {error}") from error

    if stat.S_ISLNK(metadata.st_mode):
        raise ValueError(f"refusing symlinked model path: {path}")
    if not stat.S_ISREG(metadata.st_mode):
        raise ValueError(f"model path is not a regular file: {path}")
    if metadata.st_size <= 0:
        raise ValueError(f"model file is empty: {path}")
    if metadata.st_size > maximum_bytes:
        raise ValueError(
            f"model file exceeds {maximum_bytes} byte limit: {metadata.st_size}"
        )
    return metadata.st_size


def sha256_file(path: Path) -> str:
    """Return the SHA-256 of one already validated regular file."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(HASH_CHUNK_BYTES):
            digest.update(chunk)
    return digest.hexdigest()


def dimension_value(dimension: Any) -> int | str | None:
    """Render an ONNX dimension without evaluating a graph."""

    if dimension.HasField("dim_value"):
        return int(dimension.dim_value)
    if dimension.HasField("dim_param"):
        return "$" + dimension.dim_param
    return None


def tensor_record(value_info: Any, tensor_proto: Any) -> dict[str, Any]:
    """Return a JSON-safe tensor signature record."""

    tensor = value_info.type.tensor_type
    try:
        data_type = tensor_proto.DataType.Name(tensor.elem_type)
    except ValueError:
        data_type = f"UNRECOGNIZED_{tensor.elem_type}"
    return {
        "name": value_info.name,
        "data_type": data_type,
        "dimensions": [dimension_value(dimension) for dimension in tensor.shape.dim],
    }


def direct_terminal_outputs(graph: Any) -> list[dict[str, Any]]:
    """Find the direct graph-output producer for each declared graph output."""

    producers = {
        output: (index, node)
        for index, node in enumerate(graph.node)
        for output in node.output
        if output
    }
    terminals: list[dict[str, Any]] = []
    for output in graph.output:
        producer = producers.get(output.name)
        if producer is None:
            terminals.append(
                {
                    "output_name": output.name,
                    "producer_index": None,
                    "operator": None,
                    "attributes": {},
                }
            )
            continue

        index, node = producer
        attributes: dict[str, Any] = {}
        for attribute in node.attribute:
            if attribute.name == "axis":
                attributes["axis"] = int(attribute.i)
        terminals.append(
            {
                "output_name": output.name,
                "producer_index": index,
                "operator": node.op_type,
                "attributes": attributes,
            }
        )
    return terminals


def graph_tree(graph: Any, attribute_proto: Any):
    """Yield one graph and every graph embedded in a node attribute."""

    yield graph
    for node in graph.node:
        for attribute in node.attribute:
            if attribute.type == attribute_proto.GRAPH:
                yield from graph_tree(attribute.g, attribute_proto)
            elif attribute.type == attribute_proto.GRAPHS:
                for nested_graph in attribute.graphs:
                    yield from graph_tree(nested_graph, attribute_proto)


def tensor_uses_external_data(tensor: Any, tensor_proto: Any) -> bool:
    """Return whether one TensorProto refers to an external payload."""

    return tensor.data_location == tensor_proto.EXTERNAL


def sparse_tensor_uses_external_data(sparse_tensor: Any, tensor_proto: Any) -> bool:
    """Return whether either storage tensor of a SparseTensorProto is external."""

    return (
        tensor_uses_external_data(sparse_tensor.values, tensor_proto)
        or tensor_uses_external_data(sparse_tensor.indices, tensor_proto)
    )


def has_external_tensor_data(
    graphs: Sequence[Any], attribute_proto: Any, tensor_proto: Any
) -> bool:
    """Return whether an inspected graph declares any external tensor payload."""

    sparse_tensor_kind = getattr(attribute_proto, "SPARSE_TENSOR", None)
    sparse_tensors_kind = getattr(attribute_proto, "SPARSE_TENSORS", None)
    for graph in graphs:
        if any(
            tensor_uses_external_data(initializer, tensor_proto)
            for initializer in graph.initializer
        ):
            return True
        if any(
            sparse_tensor_uses_external_data(initializer, tensor_proto)
            for initializer in graph.sparse_initializer
        ):
            return True
        for node in graph.node:
            for attribute in node.attribute:
                if (
                    attribute.type == attribute_proto.TENSOR
                    and tensor_uses_external_data(attribute.t, tensor_proto)
                ):
                    return True
                if attribute.type == attribute_proto.TENSORS and any(
                    tensor_uses_external_data(tensor, tensor_proto)
                    for tensor in attribute.tensors
                ):
                    return True
                if (
                    sparse_tensor_kind is not None
                    and attribute.type == sparse_tensor_kind
                    and sparse_tensor_uses_external_data(
                        attribute.sparse_tensor, tensor_proto
                    )
                ):
                    return True
                if (
                    sparse_tensors_kind is not None
                    and attribute.type == sparse_tensors_kind
                    and any(
                        sparse_tensor_uses_external_data(
                            tensor, tensor_proto
                        )
                        for tensor in attribute.sparse_tensors
                    )
                ):
                    return True
    return False


def operator_counts(graphs: Sequence[Any]) -> dict[str, int]:
    """Return a deterministic operator histogram across inspected graphs."""

    counts: dict[str, int] = {}
    for graph in graphs:
        for node in graph.node:
            counts[node.op_type] = counts.get(node.op_type, 0) + 1
    return dict(sorted(counts.items()))


def require_tensor(
    value_info: Any,
    tensor_proto: Any,
    *,
    name: str,
    static_dimensions: dict[int, int],
    dynamic_dimensions: frozenset[int],
) -> None:
    """Validate the M2 graph-level tensor facts without evaluating the graph."""

    tensor = value_info.type.tensor_type
    if value_info.name != name:
        raise ValueError(
            f"expected tensor name {name!r}, received {value_info.name!r}"
        )
    if tensor.elem_type != tensor_proto.FLOAT:
        received = tensor_proto.DataType.Name(tensor.elem_type)
        raise ValueError(f"expected {name!r} to have FLOAT data type, got {received}")
    dimensions = tensor.shape.dim
    if len(dimensions) != 4:
        raise ValueError(f"expected {name!r} to have rank 4, got {len(dimensions)}")
    for position, expected in static_dimensions.items():
        if position >= len(dimensions) or not dimensions[position].HasField("dim_value"):
            raise ValueError(f"expected dimension {position} of {name!r} to be {expected}")
        if dimensions[position].dim_value != expected:
            raise ValueError(
                f"expected dimension {position} of {name!r} to be {expected}, "
                f"got {dimensions[position].dim_value}"
            )
    for position in dynamic_dimensions:
        if position >= len(dimensions) or dimensions[position].HasField("dim_value"):
            raise ValueError(f"expected dimension {position} of {name!r} to be dynamic")


def require_recognizer_output(value_info: Any, tensor_proto: Any) -> None:
    """Validate the recognizer's three-dimensional M2 output signature."""

    tensor = value_info.type.tensor_type
    if value_info.name != "fetch_name_0":
        raise ValueError(
            f"expected tensor name 'fetch_name_0', received {value_info.name!r}"
        )
    if tensor.elem_type != tensor_proto.FLOAT:
        received = tensor_proto.DataType.Name(tensor.elem_type)
        raise ValueError(
            "expected 'fetch_name_0' to have FLOAT data type, "
            f"got {received}"
        )
    dimensions = tensor.shape.dim
    if len(dimensions) != 3:
        raise ValueError(
            "expected recognizer output 'fetch_name_0' to have rank 3, "
            f"got {len(dimensions)}"
        )
    if (
        not dimensions[2].HasField("dim_value")
        or dimensions[2].dim_value != 18_710
    ):
        raise ValueError(
            "expected recognizer output class dimension to be 18710"
        )
    for position in (0, 1):
        if dimensions[position].HasField("dim_value"):
            raise ValueError(
                f"expected recognizer output dimension {position} to be dynamic"
            )


def require_m2_terminal_abi(role: str, graph: Any, tensor_proto: Any) -> None:
    """Validate only the recorded graph-level terminal ABI for one M2 role."""

    if len(graph.input) != 1 or len(graph.output) != 1:
        raise ValueError(
            "expected exactly one graph input and one graph output for the M2 candidate"
        )
    terminal = direct_terminal_outputs(graph)[0]

    if role == "detector":
        require_tensor(
            graph.input[0],
            tensor_proto,
            name="x",
            static_dimensions={1: 3},
            dynamic_dimensions=frozenset({0, 2, 3}),
        )
        require_tensor(
            graph.output[0],
            tensor_proto,
            name="fetch_name_0",
            static_dimensions={1: 1},
            dynamic_dimensions=frozenset({0, 2, 3}),
        )
        if terminal["operator"] != "Sigmoid":
            raise ValueError(
                "expected detector graph output to be produced directly by Sigmoid"
            )
        return

    require_tensor(
        graph.input[0],
        tensor_proto,
        name="x",
        static_dimensions={1: 3, 2: 48},
        dynamic_dimensions=frozenset({0, 3}),
    )
    require_recognizer_output(graph.output[0], tensor_proto)
    if terminal["operator"] != "Softmax":
        raise ValueError(
            "expected recognizer graph output to be produced directly by Softmax"
        )
    if terminal["attributes"].get("axis") != 2:
        raise ValueError(
            "expected recognizer terminal Softmax axis to be 2"
        )


def inspect_model(
    *,
    role: str,
    path: Path,
    maximum_bytes: int,
    expect_m2_onnx: bool,
) -> dict[str, Any]:
    """Load, check, and summarize one graph without creating a runtime session."""

    byte_count = require_regular_bounded_file(path, maximum_bytes)
    digest = sha256_file(path)
    try:
        import onnx
        from onnx import AttributeProto, TensorProto, shape_inference
    except ImportError as error:
        raise ValueError(
            "onnx is required; install a reviewed parser in a local developer environment"
        ) from error

    model = onnx.load_model(path, load_external_data=False)
    source_graphs = tuple(graph_tree(model.graph, AttributeProto))
    if has_external_tensor_data(source_graphs, AttributeProto, TensorProto):
        raise ValueError(
            "refusing a graph with external tensor data"
        )
    onnx.checker.check_model(model, full_check=False)
    inferred = shape_inference.infer_shapes(model)
    graph = inferred.graph
    graphs = tuple(graph_tree(graph, AttributeProto))
    if expect_m2_onnx:
        require_m2_terminal_abi(role, graph, TensorProto)
    operators = operator_counts(graphs)

    return {
        "schema_version": SCHEMA_VERSION,
        "role": role,
        "file_name": path.name,
        "bytes": byte_count,
        "sha256": digest,
        "parser": {
            "onnx": onnx.__version__,
            "checker_full_check": False,
            "shape_inference": True,
            "inference_executed": False,
        },
        "ir_version": model.ir_version,
        "opsets": {
            imported.domain or "ai.onnx": imported.version
            for imported in model.opset_import
        },
        "inputs": [
            tensor_record(value_info, TensorProto) for value_info in graph.input
        ],
        "outputs": [
            tensor_record(value_info, TensorProto) for value_info in graph.output
        ],
        "direct_terminal_outputs": direct_terminal_outputs(graph),
        "operator_types": list(operators),
        "operator_counts": operators,
        "node_count": sum(len(candidate.node) for candidate in graphs),
        "initializer_count": sum(
            len(candidate.initializer) for candidate in graphs
        ),
        "subgraph_count": len(graphs) - 1,
        "external_initializer_data": False,
        "external_tensor_data": False,
        "m2_terminal_abi_validated": expect_m2_onnx,
    }


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the bounded parse-only inspection."""

    parsed = parse_arguments(arguments)
    try:
        record = inspect_model(
            role=parsed.role,
            path=parsed.model,
            maximum_bytes=parsed.max_model_bytes,
            expect_m2_onnx=parsed.expect_m2_onnx,
        )
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    except Exception as error:
        print(f"error: ONNX inspection failed: {error}", file=sys.stderr)
        return 2

    print(json.dumps(record, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
