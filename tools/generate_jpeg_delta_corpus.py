#!/usr/bin/env python3
"""Generate the dense-text pages the `IMG-003` delta probe runs over.

Roadmap item: `IMG-003`.

`docs/IMG_003_DELTA_MEASUREMENT.md` measured a component delta of `36` through
the whole pipeline on one high-contrast page and named its own limits: one
page, and a perturbation whose shape does not match a real decoder
difference. These pages exist to close the first limit. Each targets a regime
where the detector's threshold is most sensitive:

    dense-small     many lines of 13px text — small strokes, tight spacing
    low-contrast    gray-on-gray at ~40 component separation
    thin-strokes    12px text on a bright ground

These are **generated inputs, not captures**: nothing upstream is executed,
and the pages are committed as PNGs so the probe does not depend on this
script or on the rendering font being installed. Regenerating requires
DejaVu Sans (the exact pixels depend on its version; the committed files are
the fixture, this script is its provenance).

Usage:
    python3 tools/generate_jpeg_delta_corpus.py <output-directory>
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

# (name, font_px, foreground, background, line_count)
SPECS = [
    ("dense-small", 13, 20, 255, 34),
    ("low-contrast", 16, 110, 150, 26),
    ("thin-strokes", 12, 90, 230, 36),
]

WORDS = [
    "invoice", "total", "2026", "amount", "Reference", "shipping", "042",
    "acknowledge", "quantity", "unit", "price", "94.50", "tax", "subtotal",
    "The", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "18%",
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=True)

    for name, size, foreground, background, line_count in SPECS:
        width = 640
        height = 20 + line_count * (size + 5)
        image = Image.new("L", (width, height), background)
        draw = ImageDraw.Draw(image)
        font = ImageFont.truetype(FONT, size)
        state = 12345
        y = 10
        for _ in range(line_count):
            words = []
            for _ in range(6):
                state = (state * 1103515245 + 12345) % (1 << 31)
                words.append(WORDS[state % len(WORDS)])
            draw.text((12, y), " ".join(words), fill=foreground, font=font)
            y += size + 5
        path = output / f"{name}.png"
        image.save(path, optimize=True)
        print(f"wrote {path} ({width}x{height})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
