#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture deterministic OpenCV contour evidence for DB postprocessing.

Behavioral reference: `ppocr/postprocess/db_postprocess.py:boxes_from_bitmap`
at commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854, which calls
`cv2.findContours((bitmap * 255).astype(np.uint8), cv2.RETR_LIST,
cv2.CHAIN_APPROX_SIMPLE)`.

This developer-only tool does not import, execute, or write to PaddleOCR. It
writes one JSON document to stdout and never downloads assets, loads models, or
writes fixture files. Contour order matters: the upstream code truncates to
`max_candidates` by index, so the recorded order is part of the contract.
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


SCHEMA_VERSION = "paddleocr-rust/contour-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"


@dataclass(frozen=True)
class ContourCase:
    """One self-authored binary bitmap described as row strings."""

    identifier: str
    description: str
    rows: tuple[str, ...]


def case(identifier: str, description: str, rows: Sequence[str]) -> ContourCase:
    return ContourCase(identifier, description, tuple(rows))


CASES: tuple[ContourCase, ...] = (
    case("single-pixel", "One isolated foreground pixel.", ["000", "010", "000"]),
    case("single-rect", "One solid rectangle away from the border.", [
        "000000",
        "011110",
        "011110",
        "011110",
        "000000",
    ]),
    case("two-blobs", "Two separate blobs; contour order is part of the contract.", [
        "0000000",
        "0110110",
        "0110110",
        "0000000",
    ]),
    case("diagonal-touch", "Two blobs touching only diagonally.", [
        "00000",
        "01100",
        "00110",
        "00000",
    ]),
    case("ring", "A hollow ring, which yields an outer and an inner contour.", [
        "0000000",
        "0111110",
        "0100010",
        "0100010",
        "0111110",
        "0000000",
    ]),
    case("nested-rings", "A ring inside a ring.", [
        "000000000",
        "011111110",
        "010000010",
        "010111010",
        "010101010",
        "010111010",
        "010000010",
        "011111110",
        "000000000",
    ]),
    case("border-touching", "A blob flush against every image border.", [
        "1111",
        "1001",
        "1001",
        "1111",
    ]),
    case("full", "A fully foreground bitmap.", ["111", "111", "111"]),
    case("horizontal-line", "A one-pixel-tall horizontal run.", [
        "00000",
        "01110",
        "00000",
    ]),
    case("vertical-line", "A one-pixel-wide vertical run.", [
        "000",
        "010",
        "010",
        "010",
        "000",
    ]),
    case("l-shape", "A concave L-shaped blob.", [
        "000000",
        "011000",
        "011000",
        "011110",
        "011110",
        "000000",
    ]),
    case("staircase", "A stepped diagonal blob.", [
        "0000000",
        "0110000",
        "0111000",
        "0011100",
        "0001100",
        "0000000",
    ]),
    case("checkerboard", "Alternating pixels: many single-pixel contours.", [
        "10101",
        "01010",
        "10101",
        "01010",
        "10101",
    ]),
    case("empty", "No foreground at all.", ["0000", "0000", "0000"]),
    case("plus", "A plus sign with four concave corners.", [
        "0000000",
        "0001000",
        "0001000",
        "0111110",
        "0001000",
        "0001000",
        "0000000",
    ]),
    case("u-shape", "A U-shaped blob whose opening reaches the border.", [
        "0000000",
        "0110110",
        "0110110",
        "0111110",
        "0000000",
    ]),
    case("thin-diagonal", "A one-pixel diagonal stroke.", [
        "000000",
        "010000",
        "001000",
        "000100",
        "000010",
        "000000",
    ]),
    case("two-holes", "One blob containing two separate holes.", [
        "000000000",
        "011111110",
        "010101010",
        "011111110",
        "000000000",
    ]),
)


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture self-authored OpenCV findContours cases as one stdout JSON document."
    )
    parser.add_argument(
        "--disable-optimized",
        action="store_true",
        help="call cv2.setUseOptimized(False) before capture and record the setting",
    )
    parser.add_argument("--indent", type=int, default=2, help="JSON indentation width")
    return parser.parse_args(arguments)


def capture_case(item: ContourCase, cv2: Any, numpy: Any) -> dict[str, Any]:
    """Execute one findContours call and return JSON-safe evidence."""

    height = len(item.rows)
    width = len(item.rows[0])
    for row in item.rows:
        if len(row) != width:
            raise ValueError(f"case {item.identifier!r} has ragged rows")
    raw = bytes(1 if value == "1" else 0 for row in item.rows for value in row)
    bitmap = numpy.frombuffer(raw, dtype=numpy.uint8).reshape(height, width)

    contours, _hierarchy = cv2.findContours(
        (bitmap * 255).astype(numpy.uint8), cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE
    )
    recorded = [
        [[int(point[0][0]), int(point[0][1])] for point in contour] for contour in contours
    ]
    return {
        "fixture_id": item.identifier,
        "description": item.description,
        "bitmap": {
            "shape": [height, width],
            "rows": list(item.rows),
            "sha256": hashlib.sha256(raw).hexdigest(),
            "base64": base64.b64encode(raw).decode("ascii"),
        },
        "contours": recorded,
        "contour_count": len(recorded),
    }


def installed_opencv_distribution() -> dict[str, str] | None:
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
        "purpose": "developer-only OpenCV findContours oracle; not a normal Rust test dependency",
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_paths": ["ppocr/postprocess/db_postprocess.py:boxes_from_bitmap"],
        },
        "algorithm": {
            "call": "cv2.findContours((bitmap * 255).astype(uint8), RETR_LIST, CHAIN_APPROX_SIMPLE)",
            "order": "recorded in returned order; upstream truncates by index to max_candidates",
        },
        "environment": environment,
        "cases": [capture_case(item, cv2, numpy) for item in CASES],
    }
    indent = parsed.indent if parsed.indent > 0 else None
    print(json.dumps(document, indent=indent, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
