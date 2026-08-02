#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture self-authored image-input evidence for the planned M2 decoder.

This developer-only tool creates small PNG and JPEG byte streams in memory and
uses OpenCV's ``cv2.imdecode(..., cv2.IMREAD_COLOR)`` as a version-recorded
comparison point. It never imports, executes, or writes to PaddleOCR, never
loads a model, and writes one JSON document to stdout only.

The resulting document is a finite input corpus, not proof of universal
OpenCV equivalence and not an authorization to add a decoder dependency.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import importlib.metadata
import json
import platform
import struct
import sys
import zlib
from collections.abc import Sequence
from typing import Any


SCHEMA_VERSION = "paddleocr-rust/image-input-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
VALID_CASE_IDENTIFIERS = (
    "classic-v1-image-input-png-rgb-3x2",
    "classic-v1-image-input-png-rgba-3x2",
    "classic-v1-image-input-png-grayscale-3x2",
    "classic-v1-image-input-png-indexed-trns-3x2",
    "classic-v1-image-input-png-grayscale16-3x2",
    "classic-v1-image-input-jpeg-baseline-3x2",
    "classic-v1-image-input-jpeg-progressive-3x2",
    "classic-v1-image-input-jpeg-exif-orientation-1",
    "classic-v1-image-input-jpeg-exif-orientation-2",
    "classic-v1-image-input-jpeg-exif-orientation-3",
    "classic-v1-image-input-jpeg-exif-orientation-4",
    "classic-v1-image-input-jpeg-exif-orientation-5",
    "classic-v1-image-input-jpeg-exif-orientation-6",
    "classic-v1-image-input-jpeg-exif-orientation-7",
    "classic-v1-image-input-jpeg-exif-orientation-8",
)
NEGATIVE_CASE_IDENTIFIERS = (
    "classic-v1-image-input-empty",
    "classic-v1-image-input-unknown-bytes",
    "classic-v1-image-input-truncated-png",
    "classic-v1-image-input-oversized-png-header",
    "classic-v1-image-input-content-name-confusion",
)


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments without importing oracle packages."""

    parser = argparse.ArgumentParser(
        description=(
            "Capture self-authored OpenCV image-input evidence as one stdout JSON document."
        )
    )
    parser.add_argument(
        "--case",
        action="append",
        metavar="ID",
        help="capture one named valid case; repeat to select multiple cases",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list valid and negative case identifiers without importing OpenCV or NumPy",
    )
    parser.add_argument(
        "--indent",
        type=int,
        default=2,
        help="JSON indentation width; use 0 for compact output",
    )
    return parser.parse_args(arguments)


def png_chunk(kind: bytes, data: bytes) -> bytes:
    """Encode one CRC-protected PNG chunk from self-authored bytes."""

    checksum = zlib.crc32(kind)
    checksum = zlib.crc32(data, checksum)
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", checksum & 0xFFFFFFFF)
    )


def png_bytes(
    *,
    width: int,
    height: int,
    bit_depth: int,
    color_type: int,
    rows: Sequence[bytes],
    palette: bytes | None = None,
    transparency: bytes | None = None,
) -> bytes:
    """Build one non-interlaced self-authored PNG stream with filter type zero."""

    if width < 1 or height < 1 or len(rows) != height:
        raise ValueError("invalid self-authored PNG dimensions or row count")
    if palette is not None and color_type != 3:
        raise ValueError("a PNG palette requires indexed color type 3")
    if transparency is not None and color_type not in (0, 2, 3, 4, 6):
        raise ValueError("unsupported PNG transparency color type")

    ihdr = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, 0)
    scanlines = b"".join(b"\x00" + row for row in rows)
    chunks = [PNG_SIGNATURE, png_chunk(b"IHDR", ihdr)]
    if palette is not None:
        chunks.append(png_chunk(b"PLTE", palette))
    if transparency is not None:
        chunks.append(png_chunk(b"tRNS", transparency))
    chunks.extend((png_chunk(b"IDAT", zlib.compress(scanlines)), png_chunk(b"IEND", b"")))
    return b"".join(chunks)


def png_rgb() -> bytes:
    """Return a small truecolor PNG with distinct self-authored RGB bytes."""

    return png_bytes(
        width=3,
        height=2,
        bit_depth=8,
        color_type=2,
        rows=(
            bytes((255, 0, 0, 0, 255, 0, 0, 0, 255)),
            bytes((16, 32, 48, 64, 96, 128, 240, 224, 208)),
        ),
    )


def png_rgba() -> bytes:
    """Return a small RGBA PNG whose alpha values must not trigger compositing."""

    return png_bytes(
        width=3,
        height=2,
        bit_depth=8,
        color_type=6,
        rows=(
            bytes((255, 0, 0, 0, 0, 255, 0, 64, 0, 0, 255, 128)),
            bytes((16, 32, 48, 192, 64, 96, 128, 255, 240, 224, 208, 1)),
        ),
    )


def png_grayscale() -> bytes:
    """Return a small 8-bit grayscale PNG with varied luminance values."""

    return png_bytes(
        width=3,
        height=2,
        bit_depth=8,
        color_type=0,
        rows=(bytes((0, 127, 255)), bytes((16, 64, 240))),
    )


def png_indexed_trns() -> bytes:
    """Return an indexed PNG with a palette and per-entry tRNS alpha values."""

    return png_bytes(
        width=3,
        height=2,
        bit_depth=8,
        color_type=3,
        rows=(bytes((0, 1, 2)), bytes((3, 2, 1))),
        palette=bytes(
            (
                255,
                0,
                0,
                0,
                255,
                0,
                0,
                0,
                255,
                16,
                32,
                48,
            )
        ),
        transparency=bytes((0, 64, 128, 255)),
    )


def png_grayscale_16() -> bytes:
    """Return a small 16-bit grayscale PNG to capture a deliberate depth boundary."""

    return png_bytes(
        width=3,
        height=2,
        bit_depth=16,
        color_type=0,
        rows=(
            struct.pack(">3H", 0, 32_768, 65_535),
            struct.pack(">3H", 4_096, 24_576, 61_440),
        ),
    )


def jpeg_source_bgr(numpy: Any) -> Any:
    """Return a distinct 3-by-2 BGR pattern used for all self-authored JPEGs."""

    return numpy.array(
        (
            ((0, 0, 255), (0, 255, 0), (255, 0, 0)),
            ((48, 32, 16), (128, 96, 64), (208, 224, 240)),
        ),
        dtype=numpy.uint8,
    )


def jpeg_bytes(numpy: Any, cv2: Any, *, progressive: bool) -> bytes:
    """Encode a self-authored baseline or progressive JPEG with fixed settings."""

    parameters = [cv2.IMWRITE_JPEG_QUALITY, 95]
    if progressive:
        parameters.extend((cv2.IMWRITE_JPEG_PROGRESSIVE, 1))
    success, encoded = cv2.imencode(".jpg", jpeg_source_bgr(numpy), parameters)
    if not success:
        raise ValueError("OpenCV could not encode the self-authored JPEG input")
    return encoded.tobytes()


def with_exif_orientation(jpeg: bytes, orientation: int) -> bytes:
    """Insert one minimal Exif APP1 orientation record after a JPEG SOI marker."""

    if not 1 <= orientation <= 8:
        raise ValueError("Exif orientation must be in 1..=8")
    if not jpeg.startswith(b"\xff\xd8"):
        raise ValueError("self-authored JPEG must begin with SOI")

    tiff = b"".join(
        (
            b"II",
            struct.pack("<H", 42),
            struct.pack("<I", 8),
            struct.pack("<H", 1),
            struct.pack("<HHI", 0x0112, 3, 1),
            struct.pack("<H", orientation),
            b"\x00\x00",
            struct.pack("<I", 0),
        )
    )
    payload = b"Exif\x00\x00" + tiff
    return jpeg[:2] + b"\xff\xe1" + struct.pack(">H", len(payload) + 2) + payload + jpeg[2:]


def valid_case_sources(numpy: Any, cv2: Any) -> list[tuple[str, str, str, bytes]]:
    """Return all legal self-authored valid image byte streams in stable order."""

    baseline_jpeg = jpeg_bytes(numpy, cv2, progressive=False)
    cases: list[tuple[str, str, str, bytes]] = [
        (
            "classic-v1-image-input-png-rgb-3x2",
            "png",
            "Truecolor 8-bit PNG with distinct RGB pixels.",
            png_rgb(),
        ),
        (
            "classic-v1-image-input-png-rgba-3x2",
            "png",
            "RGBA 8-bit PNG with transparent and opaque self-authored pixels.",
            png_rgba(),
        ),
        (
            "classic-v1-image-input-png-grayscale-3x2",
            "png",
            "8-bit grayscale PNG with varied luminance values.",
            png_grayscale(),
        ),
        (
            "classic-v1-image-input-png-indexed-trns-3x2",
            "png",
            "Indexed PNG with PLTE and tRNS records.",
            png_indexed_trns(),
        ),
        (
            "classic-v1-image-input-png-grayscale16-3x2",
            "png",
            "16-bit grayscale PNG retained as a supported-or-rejected policy boundary.",
            png_grayscale_16(),
        ),
        (
            "classic-v1-image-input-jpeg-baseline-3x2",
            "jpeg",
            "Baseline JPEG encoded from a fixed self-authored BGR pattern at quality 95.",
            baseline_jpeg,
        ),
        (
            "classic-v1-image-input-jpeg-progressive-3x2",
            "jpeg",
            "Progressive JPEG encoded from the same fixed self-authored BGR pattern.",
            jpeg_bytes(numpy, cv2, progressive=True),
        ),
    ]
    cases.extend(
        (
            (
                f"classic-v1-image-input-jpeg-exif-orientation-{orientation}",
                "jpeg",
                f"Baseline JPEG with self-authored Exif orientation {orientation}.",
                with_exif_orientation(baseline_jpeg, orientation),
            )
            for orientation in range(1, 9)
        )
    )
    if tuple(case[0] for case in cases) != VALID_CASE_IDENTIFIERS:
        raise AssertionError("valid image fixture identifiers changed without review")
    return cases


def oversized_png_header() -> bytes:
    """Return a CRC-valid PNG header above the project-side width boundary.

    It deliberately omits IDAT/IEND data because it is an input-limit probe,
    not a valid image fixture. A future decoder must reject the header before
    allocating project-owned pixel buffers.
    """

    return PNG_SIGNATURE + png_chunk(
        b"IHDR", struct.pack(">IIBBBBB", 16_385, 1, 8, 2, 0, 0, 0)
    )


def negative_case_sources() -> list[tuple[str, str, str, bytes, str]]:
    """Return bounded non-image and malformed byte inputs in stable order."""

    valid_png = png_rgb()
    cases = [
        (
            "classic-v1-image-input-empty",
            "none",
            "Empty byte input, rejected by EncodedImage before decoder selection.",
            b"",
            "invalid_input_empty",
        ),
        (
            "classic-v1-image-input-unknown-bytes",
            "none",
            "Non-image bytes with no recognized content signature.",
            b"not-a-png-or-jpeg\x00\xff",
            "unsupported_format",
        ),
        (
            "classic-v1-image-input-truncated-png",
            "png",
            "Prefix of the self-authored truecolor PNG, cut before its IDAT payload.",
            valid_png[:24],
            "malformed_input",
        ),
        (
            "classic-v1-image-input-oversized-png-header",
            "png",
            "CRC-valid PNG header whose width is one pixel above the project limit.",
            oversized_png_header(),
            "resource_limit_before_project_pixel_allocation",
        ),
        (
            "classic-v1-image-input-content-name-confusion",
            "png",
            "A valid PNG intentionally paired with a .jpg filename hint; future input policy uses content, not a filename.",
            valid_png,
            "content_detection_ignores_filename_hint",
        ),
    ]
    if tuple(case[0] for case in cases) != NEGATIVE_CASE_IDENTIFIERS:
        raise AssertionError("negative image fixture identifiers changed without review")
    return cases


def byte_payload(data: bytes) -> dict[str, Any]:
    """Return a JSON-safe identity record for arbitrary fixture bytes."""

    return {
        "byte_length": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "base64": base64.b64encode(data).decode("ascii"),
    }


def decoded_bgr_payload(data: bytes, numpy: Any, cv2: Any) -> dict[str, Any]:
    """Decode one valid fixture using the explicitly recorded OpenCV flag."""

    decoded = cv2.imdecode(numpy.frombuffer(data, dtype=numpy.uint8), cv2.IMREAD_COLOR)
    if decoded is None:
        raise ValueError("OpenCV rejected a self-authored valid image fixture")
    if decoded.dtype != numpy.uint8 or decoded.ndim != 3 or decoded.shape[2] != 3:
        raise ValueError("OpenCV IMREAD_COLOR did not return HWC BGR uint8")
    payload = byte_payload(decoded.tobytes(order="C"))
    payload.update(
        {
            "shape": [int(axis) for axis in decoded.shape],
            "channel_order": "BGR",
            "dtype": str(decoded.dtype),
        }
    )
    return payload


def capture_valid_case(
    case: tuple[str, str, str, bytes], numpy: Any, cv2: Any
) -> dict[str, Any]:
    """Capture one self-authored encoded input and its OpenCV BGR result."""

    identifier, format_name, description, encoded = case
    return {
        "fixture_id": identifier,
        "format": format_name,
        "description": description,
        "encoded_image": byte_payload(encoded),
        "opencv_imread_color": decoded_bgr_payload(encoded, numpy, cv2),
    }


def capture_negative_case(case: tuple[str, str, str, bytes, str]) -> dict[str, Any]:
    """Record a bounded raw negative input without assigning final Rust errors."""

    identifier, format_name, description, encoded, required_outcome = case
    record = {
        "fixture_id": identifier,
        "format_hint": format_name,
        "description": description,
        "encoded_input": byte_payload(encoded),
        "required_outcome": required_outcome,
    }
    if identifier == "classic-v1-image-input-content-name-confusion":
        record["filename_hint"] = "self-authored-input.jpg"
    return record


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


def select_cases(
    cases: Sequence[tuple[str, str, str, bytes]], identifiers: Sequence[str] | None
) -> list[tuple[str, str, str, bytes]]:
    """Return requested valid cases in caller order or all cases in source order."""

    if identifiers is None:
        return list(cases)
    by_identifier = {case[0]: case for case in cases}
    selected = []
    for identifier in identifiers:
        try:
            selected.append(by_identifier[identifier])
        except KeyError as error:
            available = ", ".join(by_identifier)
            raise ValueError(f"unknown image case {identifier!r}; available: {available}") from error
    return selected


def capture_document(
    selected: Sequence[tuple[str, str, str, bytes]], numpy: Any, cv2: Any
) -> dict[str, Any]:
    """Create the complete reviewable image-input evidence document."""

    return {
        "schema_version": SCHEMA_VERSION,
        "purpose": (
            "developer-only self-authored image-input oracle; not a normal Rust test "
            "dependency and not a universal OpenCV-equivalence claim"
        ),
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_paths": [
                "tools/infer/predict_det.py",
                "tools/infer/predict_rec.py",
                "tools/infer/predict_system.py",
                "tools/infer/utility.py",
            ],
        },
        "oracle": {
            "operation": "cv2.imdecode(encoded, cv2.IMREAD_COLOR)",
            "expected_output": "HWC BGR uint8 for each valid case",
            "filename_policy": "The oracle receives bytes only; filename hints are not passed to cv2.imdecode.",
        },
        "environment": {
            "python": sys.version,
            "opencv": cv2.__version__,
            "opencv_distribution": installed_opencv_distribution(),
            "opencv_build_information_sha256": hashlib.sha256(
                cv2.getBuildInformation().encode("utf-8")
            ).hexdigest(),
            "numpy": numpy.__version__,
            "platform": platform.platform(),
        },
        "cases": [capture_valid_case(case, numpy, cv2) for case in selected],
        "negative_cases": [
            capture_negative_case(case) for case in negative_case_sources()
        ],
    }


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the selected capture without writing files or fetching assets."""

    parsed = parse_arguments(arguments)
    if parsed.indent < 0:
        print("--indent must be non-negative", file=sys.stderr)
        return 2

    if parsed.list:
        for identifier in VALID_CASE_IDENTIFIERS:
            print(identifier)
        for identifier in NEGATIVE_CASE_IDENTIFIERS:
            print(identifier)
        return 0

    try:
        import cv2  # type: ignore[import-not-found]
        import numpy
    except ModuleNotFoundError as error:
        print(
            "image-input oracle requires an explicitly provisioned external Python "
            f"environment with OpenCV and NumPy: {error}",
            file=sys.stderr,
        )
        return 2

    all_cases = valid_case_sources(numpy, cv2)
    try:
        selected = select_cases(all_cases, parsed.case)
    except ValueError as error:
        print(f"capture configuration error: {error}", file=sys.stderr)
        return 2

    try:
        document = capture_document(selected, numpy, cv2)
    except (ValueError, cv2.error) as error:
        print(f"image-input oracle capture failed: {error}", file=sys.stderr)
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
