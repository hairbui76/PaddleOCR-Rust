#!/usr/bin/env python3
"""Measure PDF rendering fidelity between a candidate and a reference renderer.

Roadmap item: `PDF-001`, entry gate part 2
(`docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md` section 5).

The gate asks for "the maximum per-component pixel difference recorded — the
same shape of evidence `IMG-003` requires for JPEG". `IMG-003` also established
what that number is worth on its own: its recorded delta of `36` components
turned out to change **no decoded character**, so a raw maximum is a prompt to
measure the consequence, not a verdict.

So this reports both halves. The pixel half: per-component maximum, mean, and the
share of components differing by more than a series of thresholds. And the
structural half: the share of components where the two renderers disagree about
*ink* — one placing coverage above a mid-grey where the other places none. Two
renderers with different antialiasing will differ by `255` on some edge pixel and
agree on every glyph and every rule; the ink disagreement is what separates that
from a renderer that dropped a shading, a font, or a form.

Inputs are pairs of same-sized PNGs, `ref_<name>.png` and `cand_<name>.png`, in
one directory. Needs `cv2` and `numpy`; nothing is downloaded.

Usage:
    python3 tools/measure_pdf_fidelity.py <directory> [output.json]
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import cv2
import numpy as np

MEASUREMENT_SCHEMA_VERSION = "paddleocr-rust/pdf-fidelity-measurement/v1"

# Component-difference thresholds reported as a share of all components. `1` is
# the smallest difference that exists at 8 bits; `128` is half the range and
# cannot be reached by antialiasing alone on a flat region.
THRESHOLDS = (0, 1, 8, 32, 64, 128, 254)

# A component at or above this is treated as "no ink" for the structural half.
INK_LEVEL = 128


def measure(reference: np.ndarray, candidate: np.ndarray) -> dict:
    if reference.shape != candidate.shape:
        return {
            "comparable": False,
            "reason": f"shape {reference.shape} vs {candidate.shape}",
        }

    a = reference.astype(np.int32)
    b = candidate.astype(np.int32)
    difference = np.abs(a - b)
    total = int(difference.size)

    shares = {
        f"share_above_{threshold}": float((difference > threshold).sum()) / total
        for threshold in THRESHOLDS
    }

    # Ink disagreement, on the grey projection rather than per component: a
    # renderer that puts a glyph where the other puts nothing.
    reference_grey = reference.mean(axis=2)
    candidate_grey = candidate.mean(axis=2)
    reference_ink = reference_grey < INK_LEVEL
    candidate_ink = candidate_grey < INK_LEVEL
    pixels = int(reference_ink.size)
    both = int((reference_ink & candidate_ink).sum())
    either = int((reference_ink | candidate_ink).sum())

    return {
        "comparable": True,
        "shape": list(reference.shape),
        "components": total,
        "max_component_difference": int(difference.max()),
        "mean_component_difference": float(difference.mean()),
        **shares,
        "reference_ink_pixels": int(reference_ink.sum()),
        "candidate_ink_pixels": int(candidate_ink.sum()),
        "ink_intersection_over_union": (float(both) / either) if either else 1.0,
        "ink_disagreement_share": float(either - both) / pixels,
    }


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    directory = Path(sys.argv[1])
    output = Path(sys.argv[2]) if len(sys.argv) == 3 else None

    names = sorted(
        path.name[len("ref_") : -len(".png")]
        for path in directory.glob("ref_*.png")
    )
    if not names:
        print(f"no ref_*.png in {directory}", file=sys.stderr)
        return 1

    cases = []
    for name in names:
        reference_path = directory / f"ref_{name}.png"
        candidate_path = directory / f"cand_{name}.png"
        if not candidate_path.exists():
            cases.append({"case": name, "comparable": False, "reason": "no candidate"})
            continue
        reference = cv2.imread(str(reference_path), cv2.IMREAD_COLOR)
        candidate = cv2.imread(str(candidate_path), cv2.IMREAD_COLOR)
        record = {"case": name}
        record.update(measure(reference, candidate))
        cases.append(record)

    document = {
        "schema_version": MEASUREMENT_SCHEMA_VERSION,
        "ink_level": INK_LEVEL,
        "thresholds": list(THRESHOLDS),
        "cases": cases,
    }

    for case in cases:
        if not case.get("comparable"):
            print(f"{case['case']:18} NOT COMPARABLE: {case.get('reason')}")
            continue
        print(
            f"{case['case']:18} max={case['max_component_difference']:3d} "
            f"mean={case['mean_component_difference']:7.3f} "
            f">32={case['share_above_32'] * 100:6.3f}% "
            f">128={case['share_above_128'] * 100:6.3f}% "
            f"inkIoU={case['ink_intersection_over_union']:.4f} "
            f"inkDisagree={case['ink_disagreement_share'] * 100:6.3f}%"
        )

    if output is not None:
        output.write_text(
            json.dumps(document, indent=1, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
