#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture deterministic OpenCV `INTER_LINEAR` resize evidence.

Behavioral reference: the `cv2.resize(..., interpolation=cv2.INTER_LINEAR)`
call reached from `ppocr/data/imaug/rec_img_aug.py:resize_norm_img` and
`ppocr/data/imaug/operators.py:DetResizeForTest` at commit
2661c7c0ef5c613e8f93c6e93b2e052399f0f854. This developer-only tool does not
import, execute, or write to PaddleOCR. It writes one JSON document to stdout
and never downloads assets, loads models, or writes fixture files.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import importlib.metadata
import json
import platform
import sys
from dataclasses import dataclass
from typing import Any, Sequence


SCHEMA_VERSION = "paddleocr-rust/resize-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"


@dataclass(frozen=True)
class ResizeCase:
    """One self-authored BGR uint8 source and destination size."""

    identifier: str
    description: str
    source_width: int
    source_height: int
    target_width: int
    target_height: int
    seed: int


def lcg_bgr_bytes(width: int, height: int, seed: int) -> bytes:
    """Build deterministic self-authored BGR bytes from a fixed 32-bit LCG."""

    state = seed & 0xFFFFFFFF
    values = bytearray()
    for _ in range(width * height * 3):
        state = (state * 1_664_525 + 1_013_904_223) & 0xFFFFFFFF
        values.append((state >> 24) & 0xFF)
    return bytes(values)


def grid_cases() -> tuple[ResizeCase, ...]:
    """Build the deterministic linear-resize coverage grid.

    The sizes deliberately mix identity, pure upscale, pure downscale, mixed
    axes, single-pixel extents, and extreme aspect ratios, because OpenCV's
    fixed-point linear path has distinct edge behaviour when a destination
    sample falls before the first or after the last source centre.
    """

    sizes = (
        # identity and near-identity
        (4, 3, 4, 3),
        (5, 5, 5, 5),
        # pure upscale
        (2, 2, 5, 5),
        (3, 2, 8, 6),
        (4, 4, 16, 16),
        (2, 3, 9, 4),
        # pure downscale
        (8, 6, 3, 2),
        (16, 16, 4, 4),
        (15, 9, 4, 3),
        (13, 11, 5, 2),
        # mixed axes
        (12, 3, 4, 9),
        (3, 12, 9, 4),
        (7, 5, 11, 2),
        (11, 2, 7, 5),
        # single-pixel extents
        (1, 1, 1, 1),
        (1, 1, 4, 3),
        (5, 4, 1, 1),
        (1, 7, 3, 3),
        (7, 1, 3, 3),
        (3, 3, 1, 6),
        (3, 3, 6, 1),
        # extreme aspect ratios
        (31, 1, 5, 1),
        (1, 31, 1, 5),
        (2, 24, 6, 3),
        (24, 2, 3, 6),
        # non-integer ratios that exercise coefficient rounding
        (7, 7, 3, 3),
        (9, 9, 4, 4),
        (10, 6, 7, 5),
        (6, 10, 5, 7),
        (14, 9, 5, 6),
        # recognizer-shaped: fixed target height with varying width
        (10, 6, 13, 8),
        (20, 6, 26, 8),
        (5, 12, 3, 8),
        (17, 13, 8, 8),
    )
    return tuple(
        ResizeCase(
            identifier=(
                f"classic-v1-resize-linear-{index:02d}-bgr-"
                f"{source_width}x{source_height}-to-{target_width}x{target_height}"
            ),
            description=(
                f"A {source_width}x{source_height} self-authored BGR source resized to "
                f"{target_width}x{target_height} with OpenCV INTER_LINEAR."
            ),
            source_width=source_width,
            source_height=source_height,
            target_width=target_width,
            target_height=target_height,
            seed=0x1000 + 0x2D * index,
        )
        for index, (source_width, source_height, target_width, target_height) in enumerate(
            sizes
        )
    )


GRID_CASES = grid_cases()


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments without importing optional oracle packages."""

    parser = argparse.ArgumentParser(
        description=(
            "Capture self-authored OpenCV INTER_LINEAR resize cases as one "
            "stdout JSON document."
        )
    )
    parser.add_argument(
        "--disable-optimized",
        action="store_true",
        help=(
            "call cv2.setUseOptimized(False) before capture and record the "
            "scalar setting in the JSON environment"
        ),
    )
    parser.add_argument(
        "--indent",
        type=int,
        default=2,
        help="JSON indentation width; use 0 for compact output",
    )
    return parser.parse_args(arguments)


def encoded_bytes(raw: bytes) -> dict[str, str]:
    """Return reviewable content identity and lossless bytes."""

    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "base64": base64.b64encode(raw).decode("ascii"),
    }


def capture_case(case: ResizeCase, cv2: Any, numpy: Any) -> dict[str, Any]:
    """Execute one OpenCV linear resize and return JSON-safe evidence."""

    raw = lcg_bgr_bytes(case.source_width, case.source_height, case.seed)
    source = numpy.frombuffer(raw, dtype=numpy.uint8).reshape(
        case.source_height, case.source_width, 3
    )
    output = cv2.resize(
        source,
        (case.target_width, case.target_height),
        interpolation=cv2.INTER_LINEAR,
    )
    return {
        "fixture_id": case.identifier,
        "description": case.description,
        "input": {
            "shape": [case.source_height, case.source_width, 3],
            "channel_order": "BGR",
            "dtype": "uint8",
            **encoded_bytes(raw),
        },
        "target_size": {"width": case.target_width, "height": case.target_height},
        "output": {
            "shape": list(output.shape),
            "channel_order": "BGR",
            "dtype": str(output.dtype),
            **encoded_bytes(numpy.ascontiguousarray(output).tobytes(order="C")),
        },
    }


def installed_opencv_distribution() -> dict[str, str] | None:
    """Return the installed OpenCV distribution name and version, if present."""

    for name in (
        "opencv-python-headless",
        "opencv-python",
        "opencv-contrib-python-headless",
        "opencv-contrib-python",
    ):
        try:
            return {"name": name, "version": importlib.metadata.version(name)}
        except importlib.metadata.PackageNotFoundError:
            continue
    return None


def main(arguments: Sequence[str] | None = None) -> int:
    """Capture the deterministic resize grid."""

    parsed = parse_arguments(arguments)
    try:
        import cv2
        import numpy
    except ImportError as error:
        print(f"error: this developer-only tool needs cv2 and numpy: {error}", file=sys.stderr)
        return 2

    if parsed.disable_optimized:
        cv2.setUseOptimized(False)

    environment: dict[str, Any] = {
        "python": sys.version,
        "platform": platform.platform(),
        "numpy": numpy.__version__,
        "opencv": cv2.__version__,
        "opencv_build_information_sha256": hashlib.sha256(
            cv2.getBuildInformation().encode("utf-8")
        ).hexdigest(),
    }
    distribution = installed_opencv_distribution()
    if distribution is not None:
        environment["opencv_distribution"] = distribution
    if parsed.disable_optimized:
        environment["opencv_optimized"] = cv2.useOptimized()

    document = {
        "schema_version": SCHEMA_VERSION,
        "purpose": "developer-only OpenCV linear-resize oracle; not a normal Rust test dependency",
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_paths": [
                "ppocr/data/imaug/rec_img_aug.py:resize_norm_img",
                "ppocr/data/imaug/operators.py:DetResizeForTest",
            ],
        },
        "algorithm": {
            "interpolation": "INTER_LINEAR",
            "dtype": "uint8",
            "note": "cv2.resize with an explicit destination size and no scale factors",
        },
        "environment": environment,
        "cases": [capture_case(case, cv2, numpy) for case in GRID_CASES],
    }
    indent = parsed.indent if parsed.indent > 0 else None
    print(json.dumps(document, indent=indent, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
