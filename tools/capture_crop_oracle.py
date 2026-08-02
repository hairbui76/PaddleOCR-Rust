#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture deterministic OpenCV crop evidence for the private M2 crop path.

Behavioral reference: PaddleOCR `tools/infer/utility.py:get_rotate_crop_image`
at commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854. This developer-only tool
does not import, execute, or write to PaddleOCR. It writes one JSON document to
stdout and never downloads assets, loads models, or writes fixture files.
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


SCHEMA_VERSION = "paddleocr-rust/crop-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"


@dataclass(frozen=True)
class CropCase:
    """One self-authored BGR uint8 crop input and source quadrilateral."""

    identifier: str
    description: str
    rows: tuple[tuple[tuple[int, int, int], ...], ...]
    points: tuple[tuple[float, float], ...]


def patterned_bgr_rows(
    width: int, height: int, seed: int
) -> tuple[tuple[tuple[int, int, int], ...], ...]:
    """Build deterministic, self-authored BGR values with non-linear variation."""

    return tuple(
        tuple(
            (
                (seed + 31 * x + 17 * y + 7 * x * y) % 256,
                (seed + 11 * x + 47 * y + 13 * x * y + 53) % 256,
                (seed + 59 * x + 19 * y + 5 * x * y + 101) % 256,
            )
            for x in range(width)
        )
        for y in range(height)
    )


CASES: tuple[CropCase, ...] = (
    CropCase(
        identifier="classic-v1-crop-oracle-identity-bgr-3x2",
        description="Identity BGR crop preserves interleaved row-major bytes.",
        rows=(
            ((1, 2, 3), (4, 5, 6), (7, 8, 9)),
            ((10, 11, 12), (13, 14, 15), (16, 17, 18)),
        ),
        points=((0.0, 0.0), (3.0, 0.0), (3.0, 2.0), (0.0, 2.0)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-border-replicate-bgr-3x2",
        description="A left-shifted crop exercises BORDER_REPLICATE.",
        rows=(
            ((20, 40, 60), (80, 100, 120), (140, 160, 180)),
            ((21, 41, 61), (81, 101, 121), (141, 161, 181)),
        ),
        points=((-1.0, 0.0), (2.0, 0.0), (2.0, 2.0), (-1.0, 2.0)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-projective-bgr-4x3",
        description=(
            "Fractional projective coordinates exercise INTER_CUBIC and replicated borders."
        ),
        rows=(
            ((0, 7, 19), (31, 43, 59), (71, 83, 97), (101, 109, 127)),
            ((13, 29, 47), (61, 73, 89), (107, 131, 149), (151, 173, 191)),
            ((17, 37, 53), (67, 79, 103), (113, 137, 157), (179, 199, 223)),
        ),
        points=((-0.25, 0.25), (3.4, -0.1), (3.2, 2.5), (-0.5, 2.3)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-tall-rotation-bgr-2x3",
        description="A height-to-width ratio of 1.5 exercises the np.rot90 boundary.",
        rows=(
            ((0, 1, 2), (10, 11, 12)),
            ((20, 21, 22), (30, 31, 32)),
            ((40, 41, 42), (50, 51, 52)),
        ),
        points=((0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 3.0)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-interior-projective-bgr-7x6",
        description=(
            "A non-linear BGR pattern and fractional interior quadrilateral exercise "
            "INTER_CUBIC coordinate quantization without relying on a border."
        ),
        rows=patterned_bgr_rows(7, 6, 23),
        points=((0.35, 0.2), (5.7, 0.65), (5.25, 4.6), (0.6, 4.85)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-edge-projective-bgr-5x4",
        description=(
            "A non-linear BGR pattern and fractional quadrilateral crossing all four "
            "image sides exercise replicated-border cubic sampling."
        ),
        rows=patterned_bgr_rows(5, 4, 71),
        points=((-1.1, -0.6), (4.45, 0.3), (5.1, 4.15), (-0.75, 3.4)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-tall-projective-bgr-4x7",
        description=(
            "A fractional tall projective BGR crop exercises cubic sampling before "
            "the post-warp counter-clockwise rotation."
        ),
        rows=patterned_bgr_rows(4, 7, 149),
        points=((0.25, 0.1), (2.85, 0.45), (2.5, 5.9), (0.0, 6.2)),
    ),
)


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments without importing optional oracle packages."""

    parser = argparse.ArgumentParser(
        description=(
            "Capture self-authored OpenCV crop oracle cases as one stdout JSON document."
        )
    )
    parser.add_argument(
        "--case",
        action="append",
        metavar="ID",
        help="capture one named case; repeat to select multiple cases",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list available case identifiers without importing OpenCV or NumPy",
    )
    parser.add_argument(
        "--indent",
        type=int,
        default=2,
        help="JSON indentation width; use 0 for compact output",
    )
    return parser.parse_args(arguments)


def selected_cases(case_ids: Sequence[str] | None) -> tuple[CropCase, ...]:
    """Return the requested deterministic cases or report unknown identifiers."""

    if case_ids is None:
        return CASES

    by_identifier = {case.identifier: case for case in CASES}
    selected: list[CropCase] = []
    for case_identifier in case_ids:
        case = by_identifier.get(case_identifier)
        if case is None:
            available = ", ".join(by_identifier)
            raise ValueError(f"unknown crop case {case_identifier!r}; available: {available}")
        selected.append(case)
    return tuple(selected)


def crop_dimensions(points: Any, numpy: Any) -> tuple[int, int]:
    """Mirror the source contract's truncating crop width and height calculation."""

    width = int(
        max(
            numpy.linalg.norm(points[0] - points[1]),
            numpy.linalg.norm(points[2] - points[3]),
        )
    )
    height = int(
        max(
            numpy.linalg.norm(points[0] - points[3]),
            numpy.linalg.norm(points[1] - points[2]),
        )
    )
    if width < 1 or height < 1:
        raise ValueError("self-authored crop case produced a zero-sized destination")
    return width, height


def encoded_bytes(values: Any) -> dict[str, str]:
    """Return reviewable content identity and lossless bytes for one uint8 array."""

    raw_bytes = values.tobytes(order="C")
    return {
        "sha256": hashlib.sha256(raw_bytes).hexdigest(),
        "base64": base64.b64encode(raw_bytes).decode("ascii"),
    }


def capture_case(case: CropCase, cv2: Any, numpy: Any) -> dict[str, Any]:
    """Execute one source-equivalent OpenCV crop and return JSON-safe evidence."""

    image = numpy.array(case.rows, dtype=numpy.uint8)
    points = numpy.array(case.points, dtype=numpy.float32)
    crop_width, crop_height = crop_dimensions(points, numpy)
    destination = numpy.float32(
        (
            (0.0, 0.0),
            (float(crop_width), 0.0),
            (float(crop_width), float(crop_height)),
            (0.0, float(crop_height)),
        )
    )
    transform = cv2.getPerspectiveTransform(points, destination)
    output = cv2.warpPerspective(
        image,
        transform,
        (crop_width, crop_height),
        borderMode=cv2.BORDER_REPLICATE,
        flags=cv2.INTER_CUBIC,
    )
    rotates_counter_clockwise = output.shape[0] / output.shape[1] >= 1.5
    if rotates_counter_clockwise:
        output = numpy.rot90(output)

    return {
        "fixture_id": case.identifier,
        "description": case.description,
        "input": {
            "shape": list(image.shape),
            "channel_order": "BGR",
            "dtype": str(image.dtype),
            **encoded_bytes(image),
        },
        "points": points.astype(float).tolist(),
        "perspective_transform": transform.astype(float).tolist(),
        "pre_rotation_output": {
            "width": crop_width,
            "height": crop_height,
        },
        "rotates_counter_clockwise": rotates_counter_clockwise,
        "output": {
            "shape": list(output.shape),
            "channel_order": "BGR",
            "dtype": str(output.dtype),
            **encoded_bytes(output),
        },
    }


def capture_document(cases: Sequence[CropCase], cv2: Any, numpy: Any) -> dict[str, Any]:
    """Create a single reviewable capture document for selected cases."""

    build_information = cv2.getBuildInformation()
    return {
        "schema_version": SCHEMA_VERSION,
        "purpose": "developer-only OpenCV crop oracle; not a normal Rust test dependency",
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_path": "tools/infer/utility.py:get_rotate_crop_image",
        },
        "algorithm": {
            "interpolation": "INTER_CUBIC",
            "border_mode": "BORDER_REPLICATE",
            "post_warp_rotation": "numpy.rot90 when height / width >= 1.5",
        },
        "environment": {
            "python": sys.version,
            "opencv": cv2.__version__,
            "opencv_distribution": installed_opencv_distribution(),
            "opencv_build_information_sha256": hashlib.sha256(
                build_information.encode("utf-8")
            ).hexdigest(),
            "numpy": numpy.__version__,
            "platform": platform.platform(),
        },
        "cases": [capture_case(case, cv2, numpy) for case in cases],
    }


def installed_opencv_distribution() -> dict[str, str] | None:
    """Identify the installed OpenCV Python distribution when metadata exists."""

    for distribution in ("opencv-python-headless", "opencv-python", "opencv-contrib-python"):
        try:
            return {
                "name": distribution,
                "version": importlib.metadata.version(distribution),
            }
        except importlib.metadata.PackageNotFoundError:
            continue
    return None


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the selected capture without creating files or fetching assets."""

    parsed = parse_arguments(arguments)
    if parsed.indent < 0:
        print("--indent must be non-negative", file=sys.stderr)
        return 2
    if parsed.list:
        for case in CASES:
            print(case.identifier)
        return 0

    try:
        cases = selected_cases(parsed.case)
    except ValueError as error:
        print(f"capture configuration error: {error}", file=sys.stderr)
        return 2

    try:
        import cv2  # type: ignore[import-not-found]
        import numpy
    except ModuleNotFoundError as error:
        print(
            "crop oracle requires an explicitly provisioned external Python environment "
            f"with OpenCV and NumPy: {error}",
            file=sys.stderr,
        )
        return 2

    try:
        document = capture_document(cases, cv2, numpy)
    except (ValueError, cv2.error) as error:
        print(f"crop oracle capture failed: {error}", file=sys.stderr)
        return 1

    json.dump(
        document,
        sys.stdout,
        ensure_ascii=True,
        indent=None if parsed.indent == 0 else parsed.indent,
        sort_keys=True,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
