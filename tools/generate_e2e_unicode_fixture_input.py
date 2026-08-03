#!/usr/bin/env python3
"""Generate the self-authored Unicode PNG fixture.

This maintainer tool requires the recorded external Pillow/OpenCV/NumPy and
Noto CJK font environment. It is not used by Rust tests or application builds.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


EXPECTED_BGR_SHA256 = "37c63cdf220706ab8c9808e9f257399a6ce32d6a1eb9d72f9457b761cd9a2d0c"
EXPECTED_PNG_SHA256 = "17ce44aad0a8ce5a3db571fc6d7ca57fa22e1dec979326ce02ff37d77157c94c"
EXPECTED_PNG_BYTE_LENGTH = 9151
EXPECTED_OPENCV_VERSION = "4.11.0"
EXPECTED_PILLOW_VERSION = "12.3.0"
FONT_PATH = Path("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc")
FONT_SHA256 = "b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a"
FONT_LICENSE_RECORD = Path("/usr/share/doc/fonts-noto-cjk/copyright")
FONT_LICENSE_RECORD_SHA256 = (
    "849f4ea9c214fa4ac3593b770c699f387534b11ce671264c1b10d85bdcb5997b"
)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Write the reviewed self-authored Unicode PNG fixture. "
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


def sha256_file(path: Path) -> str:
    try:
        with path.open("rb") as file:
            return hashlib.file_digest(file, "sha256").hexdigest()
    except OSError as error:
        raise SystemExit(f"cannot read required external file {path}: {error}") from error


def import_image_modules() -> tuple[object, object, object, object, object]:
    try:
        import PIL
        import cv2
        import numpy
        from PIL import Image, ImageDraw, ImageFont
    except ImportError as error:
        raise SystemExit(
            "Pillow, OpenCV, and NumPy are required only to regenerate this developer fixture"
        ) from error
    if cv2.__version__ != EXPECTED_OPENCV_VERSION:
        raise SystemExit(
            f"expected OpenCV {EXPECTED_OPENCV_VERSION}, got {cv2.__version__}"
        )
    if PIL.__version__ != EXPECTED_PILLOW_VERSION:
        raise SystemExit(
            f"expected Pillow {EXPECTED_PILLOW_VERSION}, got {PIL.__version__}"
        )
    if sha256_file(FONT_PATH) != FONT_SHA256:
        raise SystemExit("external Noto CJK font does not match the reviewed fixture")
    if sha256_file(FONT_LICENSE_RECORD) != FONT_LICENSE_RECORD_SHA256:
        raise SystemExit("external Noto CJK license record does not match the reviewed fixture")
    return cv2, numpy, Image, ImageDraw, ImageFont


def render_bgr(
    cv2: object,
    numpy: object,
    image_type: object,
    image_draw_type: object,
    image_font_type: object,
) -> object:
    image = image_type.new("RGB", (800, 320), (255, 255, 255))
    draw = image_draw_type.Draw(image)
    font = image_font_type.truetype(FONT_PATH, 128, index=0)
    draw.text((40, 45), "你好", font=font, fill=(0, 0, 0))
    return cv2.cvtColor(numpy.asarray(image), cv2.COLOR_RGB2BGR)


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
    cv2, numpy, image_type, image_draw_type, image_font_type = import_image_modules()
    image = render_bgr(cv2, numpy, image_type, image_draw_type, image_font_type)
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
        "font_sha256": FONT_SHA256,
        "opencv": cv2.__version__,
        "output": str(output_path),
        "pillow": EXPECTED_PILLOW_VERSION,
        "png_sha256": png_sha256,
    }
    print(json.dumps(record, ensure_ascii=False, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
