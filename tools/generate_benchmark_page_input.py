#!/usr/bin/env python3
"""Compose the `G3` benchmark page from the committed end-to-end fixture inputs.

Roadmap item: `OCR-003` (post-decision gate `G3`).

`docs/QUALITY_PROFILE.md` states the latency and memory budgets against a
1280x720 fixture. No such fixture existed, so this script builds one by placing
the already-committed, already-reviewed end-to-end fixture inputs onto one white
page. Composing beats rendering fresh text here for three reasons: the pixel
provenance is unchanged (the same self-authored `cv2.putText` renderings, no new
font binary or upstream asset enters the repository), the page carries a
realistic multi-region detection load rather than one synthetic line, and the
output is a pure function of files this repository already pins by hash.

The page is a benchmark input, not a semantic oracle. It has no expected text
and no capture, because a resource measurement does not need one and claiming a
correctness expectation this script cannot justify would be worse than having
none.

Only the Python standard library is used, so this runs without OpenCV, NumPy, or
any part of the upstream project.

Usage:
    python3 tools/generate_benchmark_page_input.py <output.png>
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path

PAGE_WIDTH = 1280
PAGE_HEIGHT = 720

# The page background. The fixture inputs are rendered dark-on-white, so a white
# page keeps the composed seams invisible to the detector.
BACKGROUND = (255, 255, 255)

FIXTURES = Path("tests/fixtures")

# Each entry places one fixture input at a top-left offset, optionally cropped to
# fit. The layout is chosen so no two placements overlap and every placement is
# fully inside the page; `compose` asserts both.
PLACEMENTS = [
    # (fixture directory, x, y, cropped width, cropped height)
    ("classic-v1-e2e-reading-order", 16, 16, 800, 320),
    ("classic-v1-e2e-unicode", 16, 368, 800, 320),
    ("classic-v1-e2e-tall-crop", 840, 24, 360, 672),
]


def read_png_rgb(path: Path) -> tuple[int, int, bytearray]:
    """Decodes an 8-bit truecolour PNG to a flat RGB buffer.

    Only the subset the fixture inputs actually use is supported; anything else
    raises rather than guessing, because a silent misread here would corrupt a
    committed fixture.
    """
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path}: not a PNG")

    width = height = None
    idat = bytearray()
    offset = 8
    while offset < len(data):
        (length,) = struct.unpack(">I", data[offset : offset + 4])
        kind = data[offset + 4 : offset + 8]
        body = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour, compression, filt, interlace = struct.unpack(
                ">IIBBBBB", body
            )
            if (depth, colour, compression, filt, interlace) != (8, 2, 0, 0, 0):
                raise SystemExit(
                    f"{path}: expected 8-bit non-interlaced truecolour, got "
                    f"depth={depth} colour={colour} interlace={interlace}"
                )
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break

    if width is None or height is None:
        raise SystemExit(f"{path}: no IHDR")

    raw = zlib.decompress(bytes(idat))
    stride = width * 3
    if len(raw) != (stride + 1) * height:
        raise SystemExit(f"{path}: unexpected decompressed length {len(raw)}")

    out = bytearray(stride * height)
    previous = bytearray(stride)
    position = 0
    for row in range(height):
        method = raw[position]
        position += 1
        line = bytearray(raw[position : position + stride])
        position += stride
        unfilter(method, line, previous, 3)
        out[row * stride : (row + 1) * stride] = line
        previous = line
    return width, height, out


def unfilter(method: int, line: bytearray, previous: bytearray, bpp: int) -> None:
    """Applies one PNG row filter in place, per RFC 2083 section 6."""
    if method == 0:
        return
    for index in range(len(line)):
        left = line[index - bpp] if index >= bpp else 0
        up = previous[index]
        upper_left = previous[index - bpp] if index >= bpp else 0
        if method == 1:
            line[index] = (line[index] + left) & 0xFF
        elif method == 2:
            line[index] = (line[index] + up) & 0xFF
        elif method == 3:
            line[index] = (line[index] + ((left + up) >> 1)) & 0xFF
        elif method == 4:
            estimate = left + up - upper_left
            distance_left = abs(estimate - left)
            distance_up = abs(estimate - up)
            distance_upper_left = abs(estimate - upper_left)
            if distance_left <= distance_up and distance_left <= distance_upper_left:
                predictor = left
            elif distance_up <= distance_upper_left:
                predictor = up
            else:
                predictor = upper_left
            line[index] = (line[index] + predictor) & 0xFF
        else:
            raise SystemExit(f"unsupported PNG filter {method}")


def write_png_rgb(path: Path, width: int, height: int, pixels: bytearray) -> None:
    """Writes an 8-bit truecolour PNG with one fixed filter for determinism."""
    stride = width * 3
    raw = bytearray()
    for row in range(height):
        raw.append(0)
        raw += pixels[row * stride : (row + 1) * stride]

    def chunk(kind: bytes, body: bytes) -> bytes:
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    # A fixed compression level keeps the output byte-identical across runs, so
    # the fixture hash in metadata.json stays reproducible.
    body = zlib.compress(bytes(raw), 9)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", body) + chunk(b"IEND", b"")
    )


def compose() -> bytearray:
    page = bytearray(
        bytes(BACKGROUND) * (PAGE_WIDTH * PAGE_HEIGHT)
    )
    occupied: list[tuple[int, int, int, int]] = []
    for name, x, y, crop_width, crop_height in PLACEMENTS:
        source_path = FIXTURES / name / "input.png"
        width, height, pixels = read_png_rgb(source_path)
        if crop_width > width or crop_height > height:
            raise SystemExit(
                f"{name}: crop {crop_width}x{crop_height} exceeds {width}x{height}"
            )
        if x + crop_width > PAGE_WIDTH or y + crop_height > PAGE_HEIGHT:
            raise SystemExit(f"{name}: placement leaves the page")
        for other_name, ox, oy, ow, oh in occupied:
            if x < ox + ow and ox < x + crop_width and y < oy + oh and oy < y + crop_height:
                raise SystemExit(f"{name} overlaps {other_name}")
        occupied.append((name, x, y, crop_width, crop_height))

        for row in range(crop_height):
            source_start = row * width * 3
            target_start = ((y + row) * PAGE_WIDTH + x) * 3
            page[target_start : target_start + crop_width * 3] = pixels[
                source_start : source_start + crop_width * 3
            ]
    return page


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])
    page = compose()
    write_png_rgb(output, PAGE_WIDTH, PAGE_HEIGHT, page)
    print(f"wrote {output} ({PAGE_WIDTH}x{PAGE_HEIGHT}, {output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
