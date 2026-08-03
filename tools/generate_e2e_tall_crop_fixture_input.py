#!/usr/bin/env python3
"""Generate the self-authored tall-crop PNG fixture.

This maintainer tool requires the recorded external OpenCV/NumPy environment.
It is not used by Rust tests or application builds.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


EXPECTED_BGR_SHA256 = "c16f7c6e47c92d92d897fee5e7ecdf32e5847bff18335bb3600da7965e65204d"
EXPECTED_PNG_SHA256 = "95e9d9c3e198de854feb4c1b6b42cb8c6aedb3768313664879ba55c847683c20"
EXPECTED_PNG_BYTE_LENGTH = 6913
EXPECTED_OPENCV_VERSION = "4.11.0"


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Write the reviewed self-authored tall-crop PNG fixture. "
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
    horizontal = numpy.full((220, 760, 3), 255, dtype=numpy.uint8)
    cv2.putText(
        horizontal,
        "Rust",
        (20, 170),
        cv2.FONT_HERSHEY_SIMPLEX,
        4.0,
        (0, 0, 0),
        8,
        cv2.LINE_AA,
    )
    vertical = cv2.rotate(horizontal, cv2.ROTATE_90_CLOCKWISE)
    image = numpy.full((900, 360, 3), 255, dtype=numpy.uint8)
    image[70 : 70 + vertical.shape[0], 80 : 80 + vertical.shape[1]] = vertical
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
