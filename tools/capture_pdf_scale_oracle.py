#!/usr/bin/env python3
"""Capture the PDF render-scale planner from the pinned paddlex.

Roadmap item: `PDF-001`, the first executable slice.

The renderer itself — pdfium via pypdfium2 — stays behind the five-part entry
gate in `docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`. But the *scale planner*
in `paddlex/inference/utils/pdf_rendering.py` is pure arithmetic: given a PDF
page size in points and a pixel budget, it either keeps the requested scale,
raises when even the minimum scale exceeds the budget, or **bisects 32
iterations** between the minimum scale and an analytic upper bound. Whatever
renderer eventually satisfies the gate, the planned scale must be this one,
so it is pinned now by execution.

Also recorded, as data rather than as an assertion: the defaults around the
planner — requested scale `2.0`, minimum `0.1`, budget `178,956,970` — and the
quirk that `PDFReaderBackend.__init__` defaults `max_pixels=None`, which
bypasses the budget entirely unless a caller passes one.

Needs the pinned `paddlex` 3.7.2. Nothing is downloaded and no PDF is opened.

Usage:
    python3 tools/capture_pdf_scale_oracle.py <output.json>
"""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

from paddlex.inference.utils.pdf_rendering import (
    DEFAULT_MAX_IMAGE_PIXELS,
    PDFRenderSizeError,
    estimate_pdf_render_pixels,
    get_pdf_render_scale_within_pixel_limit,
)
from paddlex.utils.flags import PDF_MIN_RENDER_SCALE, PDF_RENDER_SCALE

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/pdf-scale-oracle-capture/v1"


def bits(value: float) -> str:
    """The exact f64, as hex bits — a decimal round-trip is not an oracle."""
    return struct.pack(">d", float(value)).hex()


# (name, page_size_pt, requested_scale, min_scale, max_pixels)
CASES = [
    # A4 at the default scale: fits the default budget, scale kept.
    ("a4_default", (595.0, 842.0), 2.0, 0.1, DEFAULT_MAX_IMAGE_PIXELS),
    # US Letter, same shape.
    ("letter_default", (612.0, 792.0), 2.0, 0.1, DEFAULT_MAX_IMAGE_PIXELS),
    # A tight budget forces the bisection.
    ("a4_tight_budget", (595.0, 842.0), 2.0, 0.1, 1_000_000),
    ("letter_tiny_budget", (612.0, 792.0), 2.0, 0.1, 200_000),
    # A absurdly large page: bisection from a large requested scale.
    ("poster", (10_000.0, 10_000.0), 2.0, 0.1, DEFAULT_MAX_IMAGE_PIXELS),
    # Requested below minimum still returns requested when it fits.
    ("small_requested", (595.0, 842.0), 0.05, 0.1, DEFAULT_MAX_IMAGE_PIXELS),
    # Fractional page sizes exercise the ceil in the estimator.
    ("fractional", (595.276, 841.89), 2.0, 0.1, 1_500_000),
    # Exactly at the budget boundary: kept, not bisected.
    ("exact_fit", (100.0, 100.0), 2.0, 0.1, 40_000),
    # One pixel under the boundary: bisected.
    ("one_under", (100.0, 100.0), 2.0, 0.1, 39_999),
    # Budget smaller than the minimum scale can reach: the error path.
    ("min_scale_exceeds", (10_000.0, 10_000.0), 2.0, 0.1, 500_000),
    # Non-integer minimum scale interacting with the bisection lower bound.
    ("high_min_scale", (595.0, 842.0), 4.0, 1.5, 2_000_000),
]

INVALID = [
    ("zero_width", (0.0, 842.0), 2.0, 0.1, 1_000_000),
    ("negative_height", (595.0, -1.0), 2.0, 0.1, 1_000_000),
    ("zero_scale", (595.0, 842.0), 0.0, 0.1, 1_000_000),
    ("zero_min_scale", (595.0, 842.0), 2.0, 0.0, 1_000_000),
    ("zero_budget", (595.0, 842.0), 2.0, 0.1, 0),
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    records = []
    for name, size, requested, minimum, budget in CASES:
        try:
            scale = get_pdf_render_scale_within_pixel_limit(
                size,
                page_index=1,
                requested_scale=requested,
                min_scale=minimum,
                max_pixels=budget,
            )
            w, h, pixels = estimate_pdf_render_pixels(size, scale)
            outcome = {
                "scale": scale,
                "scale_bits": bits(scale),
                "width": int(w),
                "height": int(h),
                "pixels": int(pixels),
            }
        except PDFRenderSizeError as error:
            outcome = {
                "error": "render_size",
                "width": int(error.width),
                "height": int(error.height),
                "pixel_count": int(error.pixel_count),
            }
        records.append(
            {
                "case": name,
                "page_size": [size[0], size[1]],
                "page_size_bits": [bits(size[0]), bits(size[1])],
                "requested_scale": requested,
                "min_scale": minimum,
                "max_pixels": budget,
                "outcome": outcome,
            }
        )

    invalid = []
    for name, size, requested, minimum, budget in INVALID:
        kind = None
        try:
            get_pdf_render_scale_within_pixel_limit(
                size,
                page_index=1,
                requested_scale=requested,
                min_scale=minimum,
                max_pixels=budget,
            )
        except ValueError:
            kind = "value_error"
        except PDFRenderSizeError:
            kind = "render_size"
        invalid.append({"case": name, "raises": kind})

    estimates = []
    for name, size, scale in [
        ("a4_2x", (595.0, 842.0), 2.0),
        ("ceil_engages", (100.4, 200.6), 1.0),
        ("ceil_exact", (100.0, 200.0), 1.5),
        ("tiny", (0.3, 0.7), 1.0),
    ]:
        w, h, pixels = estimate_pdf_render_pixels(size, scale)
        estimates.append(
            {
                "case": name,
                "page_size": [size[0], size[1]],
                "scale": scale,
                "width": int(w),
                "height": int(h),
                "pixels": int(pixels),
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/utils/pdf_rendering.py",
        "defaults": {
            "requested_scale": float(PDF_RENDER_SCALE),
            "min_scale": float(PDF_MIN_RENDER_SCALE),
            "max_pixels": int(DEFAULT_MAX_IMAGE_PIXELS),
            "backend_max_pixels_default": None,
            "bisection_iterations": 32,
        },
        "cases": records,
        "invalid": invalid,
        "estimates": estimates,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
