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
INVERSE_MAPPING_SCHEMA_VERSION = "paddleocr-rust/crop-inverse-mappings/v1"
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


def lcg_bgr_rows(
    width: int, height: int, seed: int
) -> tuple[tuple[tuple[int, int, int], ...], ...]:
    """Build high-variation self-authored BGR bytes from a fixed 32-bit LCG."""

    state = seed & 0xFFFFFFFF
    values: list[int] = []
    for _ in range(width * height * 3):
        state = (state * 1_664_525 + 1_013_904_223) & 0xFFFFFFFF
        values.append((state >> 24) & 0xFF)

    iterator = iter(values)
    return tuple(
        tuple(
            (next(iterator), next(iterator), next(iterator))
            for _ in range(width)
        )
        for _ in range(height)
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
    CropCase(
        identifier="classic-v1-crop-oracle-phase-projective-bgr-8x8",
        description=(
            "A fractional interior quadrilateral with eighth-pixel phases exercises "
            "OpenCV cubic coordinate quantization away from replicated borders."
        ),
        rows=patterned_bgr_rows(8, 8, 211),
        points=((0.125, 0.375), (6.875, 0.625), (6.625, 6.875), (0.375, 6.625)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-single-pixel-bgr-3x3",
        description=(
            "A one-by-one fractional crop records cubic sampling and uint8 rounding "
            "at the minimum non-zero output extent."
        ),
        rows=patterned_bgr_rows(3, 3, 37),
        points=((0.49, 0.49), (1.49, 0.49), (1.49, 1.49), (0.49, 1.49)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-tall-thin-projective-bgr-3x9",
        description=(
            "A one-pixel-wide tall fractional crop exercises cubic sampling before "
            "the exact counter-clockwise rotation of a thin result."
        ),
        rows=patterned_bgr_rows(3, 9, 101),
        points=((0.4, 0.1), (1.8, 0.2), (1.6, 7.9), (0.2, 7.6)),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-cubic-rounding-bgr-8x10",
        description=(
            "A high-variation self-authored BGR crop crosses fractional cubic "
            "rounding near a half-byte boundary before tall-result rotation."
        ),
        rows=lcg_bgr_rows(8, 10, 162),
        points=(
            (1.8328327, -0.8944577),
            (8.7014475, -0.5864337),
            (8.67722, 11.502462),
            (2.2030663, 11.573961),
        ),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-cubic-weight-order-bgr-5x10",
        description=(
            "A high-variation tall BGR crop catches the f32 cubic-weight "
            "construction order at a uint8 rounding boundary."
        ),
        rows=lcg_bgr_rows(5, 10, 847_333),
        points=(
            (0.9, -0.6666667),
            (5.142857, -0.8466667),
            (5.142857, 9.526316),
            (1.29, 9.676315),
        ),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-sampling-matrix-bgr-12x11",
        description=(
            "A high-variation BGR crop distinguishes the OpenCV-style "
            "source-to-warp matrix inversion and f32 sampler evaluation "
            "from a direct f64 inverse-coordinate path."
        ),
        rows=lcg_bgr_rows(12, 11, 3_130_585_584),
        points=(
            (1.672916054725647, 1.0145947933197021),
            (12.913864135742188, 1.7000665664672852),
            (14.252660751342773, 12.014155387878418),
            (1.5553478002548218, 12.413676261901855),
        ),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-perspective-lu-bgr-12x13",
        description=(
            "A high-variation BGR crop distinguishes OpenCV "
            "getPerspectiveTransform float32 coefficient construction and "
            "default LU solving from a generic f64 homography solve."
        ),
        rows=lcg_bgr_rows(12, 13, 384_875_819),
        points=(
            (1.9549548625946045, -1.7108573913574219),
            (12.976837158203125, 0.12843433022499084),
            (10.688947677612305, 12.063116073608398),
            (-0.5748963952064514, 14.746376991271973),
        ),
    ),
    CropCase(
        identifier="classic-v1-crop-oracle-ties-even-bgr-4x7",
        description=(
            "A high-variation projective BGR crop catches nearest-even uint8 "
            "rounding at an isolated cubic half-byte component."
        ),
        rows=lcg_bgr_rows(4, 7, 4_072_061_695),
        points=(
            (-2.1361637115478516, 0.2802700996398926),
            (4.024559497833252, -0.2929045557975769),
            (4.804234504699707, 7.8116960525512695),
            (-1.6474021673202515, 9.107187271118164),
        ),
    ),
)


def scalar_grid_cases() -> tuple[CropCase, ...]:
    """Build a broad, deterministic BGR corpus for scalar OpenCV comparison.

    The cases vary source dimensions, border crossings, affine-like shear, and
    genuinely projective lower-edge perturbations. The four rotation groups
    deliberately cover wide, tall, balanced, and narrow crop extents without
    using random input or externally sourced image data.
    """

    source_sizes = (
        (3, 3),
        (4, 7),
        (5, 11),
        (6, 4),
        (7, 13),
        (8, 5),
        (9, 15),
        (10, 6),
        (11, 16),
        (12, 8),
        (13, 14),
        (14, 9),
        (15, 12),
        (16, 10),
        (3, 16),
        (4, 12),
        (5, 15),
        (6, 9),
        (7, 14),
        (8, 3),
        (9, 11),
        (10, 4),
        (11, 13),
        (12, 6),
    )
    origins = (
        (-1.125, -0.875),
        (-0.375, 0.125),
        (0.25, -0.5),
        (0.625, 0.375),
        (-0.75, 0.625),
        (0.125, -1.0),
    )
    shears = (-0.625, -0.25, 0.125, 0.5, 0.75, -0.5)
    cases: list[CropCase] = []

    for index, (width, height) in enumerate(source_sizes):
        origin_x, origin_y = origins[index % len(origins)]
        shear_x = shears[index % len(shears)]
        shear_y = shears[(index * 3 + 1) % len(shears)]
        perspective_x = shears[(index * 5 + 2) % len(shears)] * 0.5
        perspective_y = shears[(index * 7 + 4) % len(shears)] * 0.5

        # The grid uses four bounded extent profiles. Every source side is at
        # least three pixels, while the spans and perturbations keep each
        # quadrilateral strictly convex and at least one output pixel wide/high.
        match index % 4:
            case 0:
                span_x = max(1.25, width * 0.55 + 0.125)
                span_y = height + 0.375
            case 1:
                span_x = width + 0.5
                span_y = max(1.25, height * 0.55 + 0.25)
            case 2:
                span_x = width * 0.8 + 0.375
                span_y = height * 0.8 + 0.125
            case _:
                span_x = max(1.25, width * 0.45 + 0.5)
                span_y = height * 0.65 + 0.625

        points = (
            (origin_x, origin_y),
            (origin_x + span_x, origin_y + shear_y),
            (
                origin_x + span_x + shear_x + perspective_x,
                origin_y + span_y + shear_y + perspective_y,
            ),
            (origin_x + shear_x, origin_y + span_y),
        )
        seed = (0xC0FF_EE00 + index * 0x9E37_79B9) & 0xFFFF_FFFF
        cases.append(
            CropCase(
                identifier=(
                    f"classic-v1-crop-scalar-grid-{index:02d}-bgr-{width}x{height}"
                ),
                description=(
                    "A deterministic scalar OpenCV coverage-grid crop with "
                    "high-variation self-authored BGR bytes and a bounded "
                    "projective quadrilateral."
                ),
                rows=lcg_bgr_rows(width, height, seed),
                points=points,
            )
        )

    return tuple(cases) + scalar_edge_cases()


def scalar_edge_cases() -> tuple[CropCase, ...]:
    """Return targeted scalar cases outside the regular 3--16 pixel grid.

    These cases keep the same self-authored BGR/strictly-convex constraints as
    the grid while covering one- and two-pixel source dimensions, far replicated
    borders, low binary phases, larger extents, and an output aspect ratio just
    below the classic counter-clockwise rotation boundary.
    """

    return (
        CropCase(
            identifier="classic-v1-crop-scalar-grid-24-bgr-1x1",
            description=(
                "A one-pixel source with a far exterior quadrilateral exercises "
                "replicated cubic taps when every sample resolves to one pixel."
            ),
            rows=lcg_bgr_rows(1, 1, 0x1020_3040),
            points=((-2.75, -1.25), (3.375, -1.0), (3.125, 2.875), (-3.0, 2.625)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-25-bgr-1x7",
            description=(
                "A one-column source crosses both horizontal borders before a "
                "tall scalar crop rotates counter-clockwise."
            ),
            rows=lcg_bgr_rows(1, 7, 0x5566_7788),
            points=((-0.625, -1.25), (0.875, -1.0), (0.625, 8.75), (-0.875, 8.5)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-26-bgr-7x1",
            description=(
                "A one-row source crosses both vertical borders in a wide scalar "
                "crop without post-warp rotation."
            ),
            rows=lcg_bgr_rows(7, 1, 0x99AA_BBCC),
            points=((-1.25, -0.625), (8.75, -0.875), (9.0, 0.625), (-1.0, 0.875)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-27-bgr-2x2",
            description=(
                "A two-pixel source uses phases immediately around integral "
                "coordinates to exercise scalar f32 cubic phase construction."
            ),
            rows=lcg_bgr_rows(2, 2, 0xDDEEFF00),
            points=(
                (-0.00000011920928955078125, 0.00000011920928955078125),
                (1.9999998807907104, -0.00000011920928955078125),
                (2.000000238418579, 2.000000238418579),
                (-0.0000002384185791015625, 1.9999998807907104),
            ),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-28-bgr-2x9",
            description=(
                "A narrow two-column source combines far replicated borders, "
                "perspective skew, and tall-result rotation."
            ),
            rows=lcg_bgr_rows(2, 9, 0x1357_9BDF),
            points=((-2.375, 0.25), (3.75, -0.5), (3.25, 10.875), (-2.0, 9.75)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-29-bgr-9x2",
            description=(
                "A short nine-column source combines far replicated borders and "
                "a wide projective crop."
            ),
            rows=lcg_bgr_rows(9, 2, 0x2468_ACED),
            points=((0.25, -2.125), (10.75, -1.875), (10.125, 3.25), (-0.5, 2.75)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-30-bgr-17x19",
            description=(
                "A larger interior scalar crop uses thirty-second phases and a "
                "non-affine lower edge without relying on replicated borders."
            ),
            rows=lcg_bgr_rows(17, 19, 0x3141_5926),
            points=((0.03125, 0.96875), (15.96875, 0.65625), (16.34375, 18.21875), (-0.3125, 18.6875)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-31-bgr-31x3",
            description=(
                "A wide larger source exercises a high horizontal output extent "
                "with cubic samples beyond every source edge."
            ),
            rows=lcg_bgr_rows(31, 3, 0x2718_2818),
            points=((-2.5, -0.75), (33.875, -0.125), (32.625, 3.875), (-1.75, 3.25)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-32-bgr-3x31",
            description=(
                "A tall larger source exercises a high vertical output extent, "
                "replicated borders, and post-warp rotation."
            ),
            rows=lcg_bgr_rows(3, 31, 0x1618_0339),
            points=((-0.75, -2.5), (3.875, -1.75), (3.25, 33.625), (-0.125, 32.875)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-33-bgr-16x16",
            description=(
                "A balanced all-side exterior crop carries strong but bounded "
                "projective skew across a square high-variation source."
            ),
            rows=lcg_bgr_rows(16, 16, 0x0BAD_C0DE),
            points=((-2.125, -1.375), (17.5, -0.25), (16.125, 18.75), (-1.5, 16.875)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-34-bgr-13x17",
            description=(
                "A skewed medium source stresses a bounded non-affine lower edge "
                "with mixed interior and replicated cubic coordinates."
            ),
            rows=lcg_bgr_rows(13, 17, 0xC001_D00D),
            points=((1.375, -1.625), (14.625, 0.875), (11.25, 18.5), (-1.875, 15.625)),
        ),
        CropCase(
            identifier="classic-v1-crop-scalar-grid-35-bgr-12x12",
            description=(
                "An exact five-by-seven pre-rotation crop stays just below the "
                "height-to-width 1.5 counter-clockwise rotation threshold."
            ),
            rows=lcg_bgr_rows(12, 12, 0xFACE_CAFE),
            points=((2.0, 1.0), (7.0, 1.0), (7.0, 8.0), (2.0, 8.0)),
        ),
    )


SCALAR_GRID_CASES = scalar_grid_cases()


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments without importing optional oracle packages."""

    parser = argparse.ArgumentParser(
        description=(
            "Capture self-authored OpenCV crop oracle cases as one stdout JSON document."
        )
    )
    parser.add_argument(
        "--suite",
        choices=("baseline", "scalar-grid"),
        default="baseline",
        help="select the deterministic case suite (default: baseline)",
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
        "--inverse-mapping-oracle",
        action="store_true",
        help=(
            "emit the line-oriented pre-rotation warp-to-source mapping oracle "
            "instead of the crop JSON document"
        ),
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


def suite_cases(suite: str) -> tuple[CropCase, ...]:
    """Return the deterministic suite selected by the command line."""

    if suite == "baseline":
        return CASES
    if suite == "scalar-grid":
        return SCALAR_GRID_CASES
    raise ValueError(f"unknown crop suite {suite!r}")


def selected_cases(
    case_ids: Sequence[str] | None, available_cases: Sequence[CropCase]
) -> tuple[CropCase, ...]:
    """Return the requested deterministic cases or report unknown identifiers."""

    if case_ids is None:
        return tuple(available_cases)

    by_identifier = {case.identifier: case for case in available_cases}
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


def capture_document(
    cases: Sequence[CropCase],
    cv2: Any,
    numpy: Any,
    *,
    opencv_optimized: bool | None = None,
) -> dict[str, Any]:
    """Create a single reviewable capture document for selected cases."""

    build_information = cv2.getBuildInformation()
    environment: dict[str, Any] = {
        "python": sys.version,
        "opencv": cv2.__version__,
        "opencv_distribution": installed_opencv_distribution(),
        "opencv_build_information_sha256": hashlib.sha256(
            build_information.encode("utf-8")
        ).hexdigest(),
        "numpy": numpy.__version__,
        "platform": platform.platform(),
    }
    if opencv_optimized is not None:
        environment["opencv_optimized"] = opencv_optimized

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
        "environment": environment,
        "cases": [capture_case(case, cv2, numpy) for case in cases],
    }


def inverse_mapping_oracle(cases: Sequence[CropCase], cv2: Any, numpy: Any) -> str:
    """Capture deterministic OpenCV inverse mappings in a Rust-testable format."""

    lines = [
        "# PaddleOCR-Rust crop inverse-mapping oracle",
        f"# schema_version: {INVERSE_MAPPING_SCHEMA_VERSION}",
        "# producer: tools/capture_crop_oracle.py --inverse-mapping-oracle",
        "# sample_coordinates: destination corners (0, 0), (width, 0), (width, height), (0, height), and (0.375 * width, 0.625 * height)",
        "# fields: fixture_id,source_x0,source_y0,source_x1,source_y1,source_x2,source_y2,source_x3,source_y3,pre_rotation_width,pre_rotation_height,warp_x,warp_y,expected_source_x,expected_source_y",
    ]
    for case in cases:
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
        inverse_transform = cv2.getPerspectiveTransform(destination, points)
        warp_samples = numpy.float32(
            (
                (0.0, 0.0),
                (float(crop_width), 0.0),
                (float(crop_width), float(crop_height)),
                (0.0, float(crop_height)),
                (float(crop_width) * 0.375, float(crop_height) * 0.625),
            )
        )
        expected_sources = cv2.perspectiveTransform(
            warp_samples.reshape(1, -1, 2), inverse_transform
        )[0]
        source_coordinates = [repr(float(value)) for value in points.reshape(-1)]
        for warp, expected_source in zip(warp_samples, expected_sources, strict=True):
            fields = [
                case.identifier,
                *source_coordinates,
                str(crop_width),
                str(crop_height),
                repr(float(warp[0])),
                repr(float(warp[1])),
                repr(float(expected_source[0])),
                repr(float(expected_source[1])),
            ]
            lines.append(",".join(fields))

    return "\n".join(lines) + "\n"


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
    available_cases = suite_cases(parsed.suite)
    if parsed.list:
        for case in available_cases:
            print(case.identifier)
        return 0

    try:
        cases = selected_cases(parsed.case, available_cases)
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

    if parsed.disable_optimized:
        cv2.setUseOptimized(False)
        if cv2.useOptimized():
            print("OpenCV did not disable optimized code paths", file=sys.stderr)
            return 1

    try:
        if parsed.inverse_mapping_oracle:
            inverse_mappings = inverse_mapping_oracle(cases, cv2, numpy)
        else:
            document = capture_document(
                cases,
                cv2,
                numpy,
                opencv_optimized=False if parsed.disable_optimized else None,
            )
    except (ValueError, cv2.error) as error:
        print(f"crop oracle capture failed: {error}", file=sys.stderr)
        return 1

    if parsed.inverse_mapping_oracle:
        sys.stdout.write(inverse_mappings)
    else:
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
