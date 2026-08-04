#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Inspect a locally provisioned Paddle static program without executing it.

This developer-only tool reads one ``inference.json`` PIR program from an
explicitly provisioned static inference package. It never downloads, converts,
writes, or executes a model, and it never imports Paddle, PaddleX, ONNX, or an
inference runtime: the program is plain JSON, so the Python standard library is
sufficient.

The tool refuses symlinks, non-regular files, empty files, and files over an
explicit size limit. It streams the file SHA-256 before parsing, then emits one
aggregate JSON record on stdout: declared program version, operator histogram,
parameter counts, declared input/output tensor signatures, and the direct
producer of each fetched output.

The record never contains a parameter name, a parameter value, a dictionary
entry, a tensor, a model output, or a user-machine path. The optional
``--expect-m2-static`` assertion validates only the recorded PP-OCRv6 medium
static terminal ABI. It selects no runtime, approves no artifact, and
establishes no numerical equivalence with any exported representation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import sys
from pathlib import Path
from typing import Any, Sequence


SCHEMA_VERSION = "paddleocr-rust/static-program-abi-inspection/v1"
DEFAULT_MAX_PROGRAM_BYTES = 4 * 1024 * 1024
DEFAULT_MAX_PARAMETER_BYTES = 256 * 1024 * 1024
HASH_CHUNK_BYTES = 1024 * 1024

PARAMETER_OPERATOR = "p"
DATA_OPERATOR = "1.data"
FETCH_OPERATOR = "1.fetch"

# Element widths for the PIR tensor element types this tool is allowed to
# summarize. An unlisted type is reported by name with an unknown width; it is
# never silently treated as four bytes.
ELEMENT_TYPE_WIDTHS: dict[str, tuple[str, int]] = {
    "0.t_bool": ("bool", 1),
    "0.t_i8": ("int8", 1),
    "0.t_u8": ("uint8", 1),
    "0.t_i16": ("int16", 2),
    "0.t_f16": ("float16", 2),
    "0.t_bf16": ("bfloat16", 2),
    "0.t_i32": ("int32", 4),
    "0.t_f32": ("float32", 4),
    "0.t_i64": ("int64", 8),
    "0.t_f64": ("float64", 8),
}

# Scalar attribute payloads this tool renders directly. Any other attribute
# type is summarized by its type tag instead of its value.
SCALAR_ATTRIBUTE_TYPES = frozenset(
    {"0.a_str", "0.a_i32", "0.a_i64", "0.a_f32", "0.a_f64", "0.a_bool"}
)
INT_ARRAY_ATTRIBUTE_TYPE = "1.a_intarray"
DTYPE_ATTRIBUTE_TYPE = "1.a_dtype"

# Attributes reported for a direct terminal producer. They describe the
# declared output activation, not a runtime value.
REPORTED_TERMINAL_ATTRIBUTES = ("axis", "struct_name")


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
    """Parse command-line options without reading the supplied files."""

    parser = argparse.ArgumentParser(
        description=(
            "Parse one local Paddle static inference.json program and emit "
            "bounded structural ABI metadata."
        )
    )
    parser.add_argument(
        "--role",
        required=True,
        choices=("detector", "recognizer"),
        help="Model role recorded in the inspection result.",
    )
    parser.add_argument(
        "--expect-m2-static",
        action="store_true",
        help=(
            "Reject a program that does not match the recorded PP-OCRv6 medium "
            "static terminal ABI for the selected role."
        ),
    )
    parser.add_argument(
        "--max-program-bytes",
        type=positive_integer,
        default=DEFAULT_MAX_PROGRAM_BYTES,
        help=(
            "Maximum accepted program size before parsing "
            f"(default: {DEFAULT_MAX_PROGRAM_BYTES})."
        ),
    )
    parser.add_argument(
        "--parameters",
        type=Path,
        default=None,
        help=(
            "Optional explicit path to the companion inference.pdiparams file. "
            "It is only measured and hashed; its contents are never parsed."
        ),
    )
    parser.add_argument(
        "--max-parameter-bytes",
        type=positive_integer,
        default=DEFAULT_MAX_PARAMETER_BYTES,
        help=(
            "Maximum accepted parameter-file size before hashing "
            f"(default: {DEFAULT_MAX_PARAMETER_BYTES})."
        ),
    )
    parser.add_argument(
        "program",
        type=Path,
        help="Explicit local path to one static inference.json program file.",
    )
    return parser.parse_args(arguments)


def require_regular_bounded_file(
    path: Path, maximum_bytes: int, *, kind: str
) -> os.stat_result:
    """Return lstat metadata after rejecting unsafe input-file forms."""

    try:
        metadata = path.lstat()
    except OSError as error:
        raise ValueError(f"cannot inspect {kind} path") from error

    if stat.S_ISLNK(metadata.st_mode):
        raise ValueError(f"refusing a symlinked {kind} path")
    if not stat.S_ISREG(metadata.st_mode):
        raise ValueError(f"{kind} path is not a regular file")
    if metadata.st_size <= 0:
        raise ValueError(f"{kind} file is empty")
    if metadata.st_size > maximum_bytes:
        raise ValueError(
            f"{kind} file exceeds {maximum_bytes} byte limit: {metadata.st_size}"
        )
    return metadata


def open_regular_bounded_file(path: Path, maximum_bytes: int, *, kind: str):
    """Open one bounded regular file without following a replacement symlink."""

    checked_metadata = require_regular_bounded_file(path, maximum_bytes, kind=kind)
    no_follow = getattr(os, "O_NOFOLLOW", None)
    if no_follow is None:
        raise ValueError("platform does not provide no-follow file opening")
    flags = os.O_RDONLY | no_follow | getattr(os, "O_CLOEXEC", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise ValueError(f"cannot open {kind} file") from error

    source = os.fdopen(descriptor, "rb", closefd=True)
    try:
        opened_metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(opened_metadata.st_mode):
            raise ValueError(f"opened {kind} is not a regular file")
        if (
            checked_metadata.st_dev != opened_metadata.st_dev
            or checked_metadata.st_ino != opened_metadata.st_ino
        ):
            raise ValueError(f"{kind} path changed while opening")
        if opened_metadata.st_size <= 0 or opened_metadata.st_size > maximum_bytes:
            raise ValueError(f"{kind} size changed while opening")
    except BaseException:
        source.close()
        raise
    return source, opened_metadata.st_size


def read_bounded_text(path: Path, maximum_bytes: int, *, kind: str) -> tuple[bytes, str]:
    """Return the bytes and SHA-256 of one bounded regular file."""

    source, expected_bytes = open_regular_bounded_file(path, maximum_bytes, kind=kind)
    with source:
        try:
            raw = source.read(expected_bytes + 1)
        except OSError as error:
            raise ValueError(f"cannot read {kind} file") from error
    if len(raw) != expected_bytes:
        raise ValueError(f"{kind} size changed while reading")
    return raw, hashlib.sha256(raw).hexdigest()


def hash_bounded_file(path: Path, maximum_bytes: int, *, kind: str) -> tuple[int, str]:
    """Return the byte count and SHA-256 of one bounded file without parsing it."""

    source, expected_bytes = open_regular_bounded_file(path, maximum_bytes, kind=kind)
    digest = hashlib.sha256()
    seen = 0
    with source:
        try:
            while chunk := source.read(HASH_CHUNK_BYTES):
                seen += len(chunk)
                if seen > expected_bytes:
                    raise ValueError(f"{kind} grew while hashing")
                digest.update(chunk)
        except OSError as error:
            raise ValueError(f"cannot read {kind} file") from error
    if seen != expected_bytes:
        raise ValueError(f"{kind} size changed while hashing")
    return seen, digest.hexdigest()


def parse_program(raw: bytes) -> dict[str, Any]:
    """Parse the PIR program JSON without executing or importing Paddle."""

    try:
        document = json.loads(raw.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise ValueError("program file is not valid UTF-8") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"program file is not valid JSON: {error.msg}") from error
    except RecursionError as error:
        raise ValueError("program file nests beyond the parser limit") from error
    if not isinstance(document, dict):
        raise ValueError("program document is not a JSON object")
    return document


def require_pir_document(document: dict[str, Any]) -> dict[str, Any]:
    """Validate the declared program container and return its base code."""

    base_code = document.get("base_code")
    if not isinstance(base_code, dict):
        raise ValueError("program document has no base_code object")
    if base_code.get("magic") != "pir":
        raise ValueError("program document is not a PIR program")
    return base_code


def single_block_operations(document: dict[str, Any]) -> tuple[list[Any], int, int]:
    """Return the operations of the single expected program block."""

    program = document.get("program")
    if not isinstance(program, dict):
        raise ValueError("program document has no program object")
    regions = program.get("regions")
    if not isinstance(regions, list) or not regions:
        raise ValueError("program has no regions")
    blocks: list[Any] = []
    for region in regions:
        if not isinstance(region, dict):
            raise ValueError("program region is not an object")
        region_blocks = region.get("blocks")
        if not isinstance(region_blocks, list):
            raise ValueError("program region has no block list")
        blocks.extend(region_blocks)
    if len(regions) != 1 or len(blocks) != 1:
        raise ValueError(
            "this inspection supports one region with one block; received "
            f"{len(regions)} regions and {len(blocks)} blocks"
        )
    block = blocks[0]
    if not isinstance(block, dict):
        raise ValueError("program block is not an object")
    operations = block.get("ops")
    if not isinstance(operations, list):
        raise ValueError("program block has no operation list")
    return operations, len(regions), len(blocks)


def attribute_value(payload: Any) -> Any:
    """Render one attribute payload without evaluating the program."""

    if not isinstance(payload, dict):
        return None
    kind = payload.get("#")
    if kind in SCALAR_ATTRIBUTE_TYPES:
        return payload.get("D")
    if kind == DTYPE_ATTRIBUTE_TYPE:
        return payload.get("D")
    if kind == INT_ARRAY_ATTRIBUTE_TYPE:
        data = payload.get("D")
        if isinstance(data, list) and all(isinstance(item, int) for item in data):
            return list(data)
        return None
    return None


def attribute_map(operation: dict[str, Any]) -> dict[str, Any]:
    """Return the named scalar/int-array attributes of one operation."""

    attributes: dict[str, Any] = {}
    declared = operation.get("A")
    if not isinstance(declared, list):
        return attributes
    for entry in declared:
        if not isinstance(entry, dict):
            continue
        name = entry.get("N")
        if not isinstance(name, str):
            continue
        attributes[name] = attribute_value(entry.get("AT"))
    return attributes


def tensor_signature(tensor_type: Any) -> dict[str, Any]:
    """Return a JSON-safe signature for one declared dense tensor type."""

    if not isinstance(tensor_type, dict):
        raise ValueError("operation result has no tensor type")
    if tensor_type.get("#") != "0.t_dtensor":
        raise ValueError(
            f"unsupported result type {tensor_type.get('#')!r} in this inspection"
        )
    payload = tensor_type.get("D")
    if not isinstance(payload, list) or len(payload) < 3:
        raise ValueError("dense tensor type has no element/shape/layout payload")
    element = payload[0]
    element_tag = element.get("#") if isinstance(element, dict) else None
    dtype, width = ELEMENT_TYPE_WIDTHS.get(
        element_tag, (str(element_tag), None)
    )
    shape = payload[1]
    if not isinstance(shape, list) or not all(isinstance(item, int) for item in shape):
        raise ValueError("dense tensor type has no integer shape")
    layout = payload[2] if isinstance(payload[2], str) else None
    return {
        "data_type": dtype,
        "element_bytes": width,
        "dimensions": list(shape),
        "layout": layout,
    }


def operation_results(operation: dict[str, Any]) -> list[dict[str, Any]]:
    """Return the declared result records of one operation."""

    results = operation.get("O")
    if isinstance(results, dict):
        return [results]
    if isinstance(results, list):
        return [result for result in results if isinstance(result, dict)]
    return []


def result_producers(operations: Sequence[Any]) -> dict[int, int]:
    """Map each declared value identifier to the index of its producing op."""

    producers: dict[int, int] = {}
    for index, operation in enumerate(operations):
        if not isinstance(operation, dict):
            continue
        for result in operation_results(operation):
            identifier = result.get("%")
            if isinstance(identifier, int):
                producers.setdefault(identifier, index)
    return producers


def parameter_inventory(operations: Sequence[Any]) -> dict[str, Any]:
    """Summarize declared parameters without emitting a parameter name."""

    count = 0
    element_count = 0
    byte_count: int | None = 0
    dtype_counts: dict[str, int] = {}
    dynamic_parameters = 0
    for operation in operations:
        if not isinstance(operation, dict) or operation.get("#") != PARAMETER_OPERATOR:
            continue
        count += 1
        results = operation_results(operation)
        if len(results) != 1:
            raise ValueError("parameter operation does not declare one result")
        signature = tensor_signature(results[0].get("TT"))
        dtype = signature["data_type"]
        dtype_counts[dtype] = dtype_counts.get(dtype, 0) + 1
        dimensions = signature["dimensions"]
        if any(dimension < 0 for dimension in dimensions):
            dynamic_parameters += 1
            byte_count = None
            continue
        elements = 1
        for dimension in dimensions:
            elements *= dimension
        element_count += elements
        width = signature["element_bytes"]
        if byte_count is None or width is None:
            byte_count = None
        else:
            byte_count += elements * width
    return {
        "count": count,
        "element_count": element_count,
        "declared_bytes": byte_count,
        "data_type_counts": dict(sorted(dtype_counts.items())),
        "dynamic_shape_count": dynamic_parameters,
    }


def declared_inputs(operations: Sequence[Any]) -> list[dict[str, Any]]:
    """Return the declared program inputs in program order."""

    inputs: list[dict[str, Any]] = []
    for operation in operations:
        if not isinstance(operation, dict) or operation.get("#") != DATA_OPERATOR:
            continue
        attributes = attribute_map(operation)
        results = operation_results(operation)
        if len(results) != 1:
            raise ValueError("data operation does not declare one result")
        record = tensor_signature(results[0].get("TT"))
        record["name"] = attributes.get("name")
        record["declared_shape"] = attributes.get("shape")
        record["declared_dtype"] = attributes.get("dtype")
        inputs.append(record)
    return inputs


def declared_outputs(
    operations: Sequence[Any], producers: dict[int, int]
) -> list[dict[str, Any]]:
    """Return the fetched outputs and the direct producer of each one."""

    outputs: list[dict[str, Any]] = []
    for operation in operations:
        if not isinstance(operation, dict) or operation.get("#") != FETCH_OPERATOR:
            continue
        attributes = attribute_map(operation)
        results = operation_results(operation)
        if len(results) != 1:
            raise ValueError("fetch operation does not declare one result")
        record = tensor_signature(results[0].get("TT"))
        record["name"] = attributes.get("name")
        record["column"] = attributes.get("col")

        operands = operation.get("I")
        identifier = None
        if isinstance(operands, list) and operands and isinstance(operands[0], dict):
            identifier = operands[0].get("%")
        producer_index = producers.get(identifier) if isinstance(identifier, int) else None
        if producer_index is None:
            record["producer_index"] = None
            record["producer_operator"] = None
            record["producer_attributes"] = {}
        else:
            producer = operations[producer_index]
            producer_attributes = attribute_map(producer)
            record["producer_index"] = producer_index
            record["producer_operator"] = producer.get("#")
            record["producer_attributes"] = {
                name: producer_attributes[name]
                for name in REPORTED_TERMINAL_ATTRIBUTES
                if name in producer_attributes
            }
        outputs.append(record)
    return outputs


def operator_counts(operations: Sequence[Any]) -> dict[str, int]:
    """Return a deterministic operator histogram for the inspected block."""

    counts: dict[str, int] = {}
    for operation in operations:
        if not isinstance(operation, dict):
            raise ValueError("program operation is not an object")
        name = operation.get("#")
        if not isinstance(name, str):
            raise ValueError("program operation has no name")
        counts[name] = counts.get(name, 0) + 1
    return dict(sorted(counts.items()))


def require_tensor(
    record: dict[str, Any],
    *,
    name: str,
    expected_dimensions: Sequence[int],
) -> None:
    """Validate one declared M2 tensor signature exactly."""

    if record.get("name") != name:
        raise ValueError(f"expected tensor name {name!r}, received {record.get('name')!r}")
    if record.get("data_type") != "float32":
        raise ValueError(
            f"expected {name!r} to have float32 elements, got {record.get('data_type')!r}"
        )
    dimensions = record.get("dimensions")
    if list(expected_dimensions) != dimensions:
        raise ValueError(
            f"expected {name!r} shape {list(expected_dimensions)}, got {dimensions}"
        )


def require_m2_static_abi(role: str, record: dict[str, Any]) -> None:
    """Validate only the recorded declared terminal ABI for one M2 role."""

    inputs = record["inputs"]
    outputs = record["outputs"]
    if len(inputs) != 1 or len(outputs) != 1:
        raise ValueError(
            "expected exactly one program input and one fetched output for the "
            "M2 candidate"
        )
    if inputs[0].get("layout") != "NCHW":
        raise ValueError("expected the M2 program input to declare the NCHW layout")

    if role == "detector":
        require_tensor(inputs[0], name="x", expected_dimensions=(-1, 3, -1, -1))
        require_tensor(
            outputs[0], name="fetch_name_0", expected_dimensions=(-1, 1, -1, -1)
        )
        if outputs[0].get("producer_operator") != "1.sigmoid":
            raise ValueError(
                "expected the detector fetched output to be produced directly by "
                "1.sigmoid"
            )
        return

    require_tensor(inputs[0], name="x", expected_dimensions=(-1, 3, 48, -1))
    require_tensor(
        outputs[0], name="fetch_name_0", expected_dimensions=(-1, -1, 18_710)
    )
    if outputs[0].get("producer_operator") != "1.softmax":
        raise ValueError(
            "expected the recognizer fetched output to be produced directly by "
            "1.softmax"
        )
    if outputs[0].get("producer_attributes", {}).get("axis") != 2:
        raise ValueError("expected the recognizer terminal softmax axis to be 2")


def inspect_program(
    *,
    role: str,
    path: Path,
    maximum_bytes: int,
    parameters: Path | None,
    maximum_parameter_bytes: int,
    expect_m2_static: bool,
) -> dict[str, Any]:
    """Parse, check, and summarize one static program without executing it."""

    raw, digest = read_bounded_text(path, maximum_bytes, kind="program")
    document = parse_program(raw)
    base_code = require_pir_document(document)
    operations, region_count, block_count = single_block_operations(document)
    producers = result_producers(operations)

    record: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "role": role,
        "file_name": path.name,
        "bytes": len(raw),
        "sha256": digest,
        "parser": {
            "implementation": "python standard library json",
            "paddle_imported": False,
            "inference_executed": False,
            "parameters_parsed": False,
        },
        "program_format": {
            "magic": base_code.get("magic"),
            "version": base_code.get("version"),
            "trainable": base_code.get("trainable"),
        },
        "region_count": region_count,
        "block_count": block_count,
        "operation_count": len(operations),
        "operator_counts": operator_counts(operations),
        "parameters": parameter_inventory(operations),
        "inputs": declared_inputs(operations),
        "outputs": declared_outputs(operations, producers),
    }

    if parameters is not None:
        parameter_bytes, parameter_digest = hash_bounded_file(
            parameters, maximum_parameter_bytes, kind="parameter"
        )
        declared = record["parameters"]["declared_bytes"]
        record["parameter_file"] = {
            "file_name": parameters.name,
            "bytes": parameter_bytes,
            "sha256": parameter_digest,
            "declared_tensor_bytes": declared,
            "container_overhead_bytes": (
                None if declared is None else parameter_bytes - declared
            ),
        }

    if expect_m2_static:
        require_m2_static_abi(role, record)
    record["m2_terminal_abi_validated"] = expect_m2_static
    return record


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the bounded parse-only static program inspection."""

    parsed = parse_arguments(arguments)
    try:
        record = inspect_program(
            role=parsed.role,
            path=parsed.program,
            maximum_bytes=parsed.max_program_bytes,
            parameters=parsed.parameters,
            maximum_parameter_bytes=parsed.max_parameter_bytes,
            expect_m2_static=parsed.expect_m2_static,
        )
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(json.dumps(record, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
