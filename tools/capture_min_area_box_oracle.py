#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture deterministic OpenCV minimum-area box evidence for DB postprocessing.

Behavioral reference: `ppocr/postprocess/db_postprocess.py:get_mini_boxes` at
commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854, which calls
`cv2.minAreaRect(contour)` then `cv2.boxPoints(...)`, sorts the four corners by
`x`, and reindexes them into the frozen `box[0..3]` order. `sside` is
`min(bounding_box[1])`.

This developer-only tool does not import, execute, or write to PaddleOCR. It
writes one JSON document to stdout and never downloads assets, loads models, or
writes fixture files.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import platform
import sys
from typing import Any, Sequence


SCHEMA_VERSION = "paddleocr-rust/min-area-box-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"


def polygon_cases() -> tuple[tuple[str, str, tuple[tuple[int, int], ...]], ...]:
    """Self-authored integer point sets covering the reachable rectangle forms."""

    return (
        ("axis-square", "An axis-aligned square.", ((0, 0), (0, 8), (8, 8), (8, 0))),
        ("axis-wide", "An axis-aligned wide rectangle.", ((2, 3), (2, 7), (20, 7), (20, 3))),
        ("axis-tall", "An axis-aligned tall rectangle.", ((5, 1), (5, 30), (9, 30), (9, 1))),
        ("diamond", "A 45-degree diamond.", ((10, 0), (20, 10), (10, 20), (0, 10))),
        ("slanted", "A slanted quadrilateral.", ((1, 1), (9, 3), (11, 12), (3, 10))),
        ("thin-slanted", "A thin slanted stroke.", ((0, 0), (30, 6), (30, 8), (0, 2))),
        ("triangle", "A triangle, whose minimum rectangle rests on one edge.", ((0, 0), (14, 2), (5, 11))),
        ("single-point", "A degenerate one-point set.", ((4, 4),)),
        ("two-points", "A degenerate segment.", ((2, 2), (12, 7))),
        ("collinear", "Three collinear points.", ((0, 0), (5, 5), (10, 10))),
        ("l-shape", "A concave L outline.", ((0, 0), (0, 10), (4, 10), (4, 4), (10, 4), (10, 0))),
        ("near-square", "A nearly square blob with a shaved corner.", ((0, 0), (0, 9), (8, 9), (9, 5), (9, 0))),
        ("shallow-slant", "A very shallow slant that stresses angle selection.", ((0, 0), (40, 1), (40, 5), (0, 4))),
        ("steep-slant", "A very steep slant.", ((0, 0), (1, 40), (5, 40), (4, 0))),
        ("hexagon", "A convex hexagon.", ((4, 0), (12, 0), (16, 7), (12, 14), (4, 14), (0, 7))),
        ("duplicate-points", "A rectangle with repeated vertices.", ((0, 0), (0, 0), (0, 6), (6, 6), (6, 6), (6, 0))),
    )


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture self-authored OpenCV minAreaRect/boxPoints cases as stdout JSON."
    )
    parser.add_argument(
        "--disable-optimized",
        action="store_true",
        help="call cv2.setUseOptimized(False) before capture and record the setting",
    )
    parser.add_argument("--indent", type=int, default=2, help="JSON indentation width")
    return parser.parse_args(arguments)


def capture_case(
    identifier: str, description: str, points: Sequence[tuple[int, int]], cv2: Any, numpy: Any
) -> dict[str, Any]:
    """Run the exact upstream mini-box sequence over one self-authored point set."""

    contour = numpy.array([[list(point)] for point in points], dtype=numpy.int32)
    rect = cv2.minAreaRect(contour)
    corners = cv2.boxPoints(rect)

    ordered = sorted(list(corners), key=lambda value: value[0])
    if ordered[1][1] > ordered[0][1]:
        index_1, index_4 = 0, 1
    else:
        index_1, index_4 = 1, 0
    if ordered[3][1] > ordered[2][1]:
        index_2, index_3 = 2, 3
    else:
        index_2, index_3 = 3, 2
    box = [ordered[index_1], ordered[index_2], ordered[index_3], ordered[index_4]]

    return {
        "fixture_id": f"classic-v1-min-area-box-{identifier}",
        "description": description,
        "points": [list(point) for point in points],
        "rect": {
            "center": [float(rect[0][0]), float(rect[0][1])],
            "size": [float(rect[1][0]), float(rect[1][1])],
            "angle": float(rect[2]),
        },
        "box_points": [[float(corner[0]), float(corner[1])] for corner in corners],
        "ordered_box": [[float(corner[0]), float(corner[1])] for corner in box],
        "sside": float(min(rect[1])),
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
        "purpose": "developer-only OpenCV minAreaRect oracle; not a normal Rust test dependency",
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_paths": ["ppocr/postprocess/db_postprocess.py:get_mini_boxes"],
        },
        "algorithm": {
            "call": "cv2.minAreaRect(contour) then cv2.boxPoints(rect)",
            "ordering": "corners sorted by x, then reindexed by the frozen upstream rule",
            "sside": "min(rect[1])",
        },
        "environment": environment,
        "cases": [
            capture_case(identifier, description, points, cv2, numpy)
            for identifier, description, points in polygon_cases()
        ],
    }
    indent = parsed.indent if parsed.indent > 0 else None
    print(json.dumps(document, indent=indent, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
