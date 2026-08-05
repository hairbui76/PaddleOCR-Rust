#!/usr/bin/env python3
"""Capture `box_score_slow` and the `fillPoly` masks it depends on.

Roadmap item: `DET-003`, the slow score mode — its one remaining item.

`box_score_slow` differs from `box_score_fast` in what it is handed: the raw
**contour** from `findContours` rather than the four-corner minimum-area box.
Contours are integer points and can be **concave** — an L-shaped text region
produces one — and that breaks the reuse this port would otherwise reach for:
`src/score.rs` fills each row from its leftmost to its rightmost marked pixel,
which is correct for the convex quadrilaterals `box_score_fast` sees and
overfills a concavity.

So this capture records two things per case:

  * the **mask** `cv2.fillPoly` produces, bit for bit — because the score alone
    under-constrains the fill, and a wrong fill can produce a right mean;
  * the **score** `box_score_slow` returns over a synthetic map.

`box_score_slow` itself is **executed** from the pinned checkout, loaded by file
path with `paddle` stubbed: `db_postprocess.py` imports `paddle` at module level
for a `.numpy()` call this capture never reaches, and installing Paddle to avoid
a stub would be a larger distortion than the stub.

Needs `numpy` and `cv2`. Nothing is downloaded and no model is run.

Usage:
    python3 tools/capture_slow_score_oracle.py <output.json>
"""

from __future__ import annotations

import base64
import importlib.util
import json
import sys
import types
from pathlib import Path

import numpy as np

import cv2

CHECKOUT = Path(__file__).resolve().parent.parent / "PaddleOCR"
CAPTURE_SCHEMA_VERSION = "paddleocr-rust/slow-score-oracle-capture/v1"


def load_db_postprocess():
    """Loads `DBPostProcess` by path, with `paddle` stubbed.

    The stub provides only the name the module dereferences at import time.
    `box_score_slow` touches none of it, so the executed code is upstream's.
    """
    if "paddle" not in sys.modules:
        stub = types.ModuleType("paddle")
        stub.Tensor = type("Tensor", (), {})
        sys.modules["paddle"] = stub
    path = CHECKOUT / "ppocr" / "postprocess" / "db_postprocess.py"
    spec = importlib.util.spec_from_file_location("_db_postprocess", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.DBPostProcess


# Contours chosen to cover what `findContours` can actually emit: convex,
# concave (L and U), thin, degenerate, and border-touching. All integer, all
# simple (contours never self-intersect).
CONTOURS = {
    "axis_rect": [(3, 3), (12, 3), (12, 9), (3, 9)],
    "triangle": [(5, 2), (14, 8), (2, 10)],
    "slanted_quad": [(4, 2), (15, 5), (13, 12), (2, 8)],
    "l_shape": [(2, 2), (12, 2), (12, 6), (7, 6), (7, 12), (2, 12)],
    "u_shape": [(2, 2), (14, 2), (14, 12), (10, 12), (10, 5), (6, 5), (6, 12), (2, 12)],
    "thin_diagonal": [(2, 2), (13, 11), (14, 12), (3, 3)],
    "single_point": [(7, 7)],
    "two_points": [(3, 4), (11, 9)],
    "touches_border": [(0, 0), (9, 0), (9, 7), (0, 7)],
    "full_map": [(0, 0), (19, 0), (19, 13), (0, 13)],
    "notched_rect": [(2, 2), (16, 2), (16, 11), (11, 11), (9, 7), (7, 11), (2, 11)],
}

MAP_WIDTH, MAP_HEIGHT = 20, 14


def star_polygons(count: int) -> dict:
    """Deterministic angle-sorted polygons: always simple, rarely convex.

    The eleven named cases **derived** the fill rules; these exist to test the
    rules on shapes that played no part in deriving them. A linear congruential
    generator rather than `random`, so the corpus is reproducible by seed.
    """
    import math

    cases = {}
    state = 0x2545F491
    def next_value(limit):
        nonlocal state
        state = (state * 1103515245 + 12345) % (1 << 31)
        return state % limit
    for index in range(count):
        vertex_count = 3 + next_value(6)
        centre_x = 4 + next_value(MAP_WIDTH - 8)
        centre_y = 4 + next_value(MAP_HEIGHT - 8)
        points = []
        for vertex in range(vertex_count):
            angle = 2.0 * math.pi * vertex / vertex_count
            radius = 1 + next_value(6)
            x = int(round(centre_x + radius * math.cos(angle)))
            y = int(round(centre_y + radius * math.sin(angle)))
            points.append((max(0, min(MAP_WIDTH - 1, x)), max(0, min(MAP_HEIGHT - 1, y))))
        # Collapsed duplicates are fine: upstream accepts them too.
        cases[f"star_{index:02}"] = points
    return cases


def synthetic_map() -> np.ndarray:
    """A deterministic probability map, reproduced in the Rust test."""
    y = np.arange(MAP_HEIGHT, dtype=np.float32)[:, None]
    x = np.arange(MAP_WIDTH, dtype=np.float32)[None, :]
    return ((x * 7.0 + y * 13.0) % 29.0) / 29.0


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    DBPostProcess = load_db_postprocess()
    post = DBPostProcess(score_mode="slow")
    bitmap = synthetic_map()

    contours = dict(CONTOURS)
    contours.update(star_polygons(40))

    records = []
    for name, points in contours.items():
        contour = np.asarray(points, dtype=np.int32).reshape(-1, 1, 2)

        # The mask exactly as `box_score_slow` builds it, reproduced here only
        # to *extract* it — the score below comes from the executed upstream
        # method, and the mask is what pins the fill.
        flat = contour.reshape(-1, 2).copy()
        xmin = int(np.clip(np.min(flat[:, 0]), 0, MAP_WIDTH - 1))
        xmax = int(np.clip(np.max(flat[:, 0]), 0, MAP_WIDTH - 1))
        ymin = int(np.clip(np.min(flat[:, 1]), 0, MAP_HEIGHT - 1))
        ymax = int(np.clip(np.max(flat[:, 1]), 0, MAP_HEIGHT - 1))
        mask = np.zeros((ymax - ymin + 1, xmax - xmin + 1), dtype=np.uint8)
        shifted = flat.copy()
        shifted[:, 0] -= xmin
        shifted[:, 1] -= ymin
        cv2.fillPoly(mask, shifted.reshape(1, -1, 2).astype("int32"), 1)

        score = float(post.box_score_slow(bitmap, contour))

        records.append(
            {
                "case": name,
                "contour": [[int(x), int(y)] for x, y in points],
                "clip": [xmin, ymin, xmax, ymax],
                "mask_shape": [int(mask.shape[0]), int(mask.shape[1])],
                "mask_base64": base64.b64encode(mask.tobytes()).decode("ascii"),
                "mask_filled": int(mask.sum()),
                "score": score,
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "PaddleOCR ppocr/postprocess/db_postprocess.py box_score_slow",
        "map": {
            "width": MAP_WIDTH,
            "height": MAP_HEIGHT,
            "formula": "((x*7 + y*13) % 29) / 29, f32",
        },
        "environment": f"cv2 {cv2.__version__}, numpy {np.__version__}",
        "records": records,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
