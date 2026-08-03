#!/usr/bin/env python3
"""Generate the self-authored multi-line reading-order PNG fixture.

This maintainer tool requires the recorded external OpenCV/NumPy environment.
It is not used by Rust tests or application builds.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


EXPECTED_BGR_SHA256 = "eec2d2d8b45309575caf21d1ab59cf7763731410cd55f61d7af5e880a76f80b4"
EXPECTED_PNG_SHA256 = "1617b343fa384344a2b260bc4e57c836c93b9d3d35247dd5ea548df331042ea1"
EXPECTED_PNG_BYTE_LENGTH = 8988
EXPECTED_OPENCV_VERSION = "4.11.0"
TEXT_LAYOUT = (
    ("Hello", (40, 120), 2.0, 4),
    ("World", (510, 120), 2.0, 4),
    ("Rust", (40, 280), 2.0, 4),
    ("OCR", (510, 280), 2.0, 4),
)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Write the reviewed self-authored reading-order PNG fixture. "
            "Refuses to overwrite a file."
        )
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="new output file to create exclusively",
    )
    return parser.parse_args()


def import_image_modules() -> tuple[object, object]:
    try:
        import cv2
        import numpy
    except ImportError as error:
        raise SystemExit(
            "OpenCV and NumPy are required only to regenerate this developer fixture"
        ) from error
    if cv2.__version__ != EXPECTED_OPENCV_VERSION:
        raise SystemExit(
            f"expected OpenCV {EXPECTED_OPENCV_VERSION}, got {cv2.__version__}"
        )
    return cv2, numpy


def render_bgr(cv2: object, numpy: object) -> object:
    image = numpy.full((320, 800, 3), 255, dtype=numpy.uint8)
    for text, origin, scale, thickness in TEXT_LAYOUT:
        cv2.putText(
            image,
            text,
            origin,
            cv2.FONT_HERSHEY_SIMPLEX,
            scale,
            (0, 0, 0),
            thickness,
            cv2.LINE_AA,
        )
    return image


def encode_png(cv2: object, numpy: object, image: object) -> bytes:
    success, encoded = cv2.imencode(
        ".png", image, [cv2.IMWRITE_PNG_COMPRESSION, 9]
    )
    if not success:
        raise SystemExit("OpenCV PNG encoding failed")
    png = encoded.tobytes()
    decoded = cv2.imdecode(encoded, cv2.IMREAD_COLOR)
    if decoded is None or not numpy.array_equal(decoded, image):
        raise SystemExit("OpenCV PNG round-trip did not preserve the rendered BGR pixels")
    return png


def reject_upstream_target(repository_root: Path, output_path: Path) -> None:
    upstream_target = (repository_root / "PaddleOCR").resolve()
    try:
        output_path.relative_to(upstream_target)
    except ValueError:
        return
    raise SystemExit("refusing to write inside the read-only PaddleOCR upstream checkout")


def main() -> None:
    arguments = parse_arguments()
    cv2, numpy = import_image_modules()
    image = render_bgr(cv2, numpy)
    bgr_sha256 = hashlib.sha256(image.tobytes()).hexdigest()
    if bgr_sha256 != EXPECTED_BGR_SHA256:
        raise SystemExit("rendered BGR pixels do not match the reviewed fixture")
    png = encode_png(cv2, numpy, image)
    png_sha256 = hashlib.sha256(png).hexdigest()
    if len(png) != EXPECTED_PNG_BYTE_LENGTH or png_sha256 != EXPECTED_PNG_SHA256:
        raise SystemExit("encoded PNG does not match the reviewed fixture")

    repository_root = Path(__file__).resolve().parent.parent
    output_path = arguments.output
    if not output_path.is_absolute():
        output_path = Path.cwd() / output_path
    output_path = output_path.resolve(strict=False)
    reject_upstream_target(repository_root, output_path)
    if not output_path.parent.is_dir():
        raise SystemExit(f"output directory does not exist: {output_path.parent}")
    if output_path.exists():
        raise SystemExit(f"refusing to overwrite existing output: {output_path}")

    try:
        with output_path.open("xb") as output:
            output.write(png)
    except OSError as error:
        raise SystemExit(f"cannot create output: {error}") from error

    record = {
        "bgr_sha256": bgr_sha256,
        "bytes": len(png),
        "opencv": cv2.__version__,
        "output": str(output_path),
        "png_sha256": png_sha256,
    }
    print(json.dumps(record, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
