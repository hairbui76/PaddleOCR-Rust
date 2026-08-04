#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Capture deterministic unclip and box-score evidence for DB postprocessing.

Behavioral reference: `ppocr/postprocess/db_postprocess.py` at commit
2661c7c0ef5c613e8f93c6e93b2e052399f0f854, specifically `unclip` and
`box_score_fast`.

This developer-only tool needs `pyclipper`, `shapely`, `cv2`, and `numpy`,
which are **not** repository dependencies: install them in a disposable
environment. It does not import, execute, or write to PaddleOCR, writes one
JSON document to stdout, and never writes fixture files.

The two steps are captured together because they consume the same box and
because the upstream pipeline scores the untruncated corners while offsetting
the truncated ones; recording both from one input keeps that difference
visible.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import platform
import sys
from typing import Any, Sequence


SCHEMA_VERSION = "paddleocr-rust/unclip-score-oracle/v1"
UPSTREAM_COMMIT = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854"

# Self-authored boxes in the frozen upstream corner order, paired with the
# probability-map size they were derived from.
CASES: tuple[tuple[str, str, tuple[tuple[float, float], ...], int, int], ...] = (
    ("axis-small", "A small axis-aligned box.", ((1.0, 1.0), (9.0, 1.0), (9.0, 5.0), (1.0, 5.0)), 16, 12),
    ("axis-thin", "A thin box whose offset distance is small.", ((2.0, 4.0), (14.0, 4.0), (14.0, 7.0), (2.0, 7.0)), 20, 12),
    ("axis-tall", "A tall box.", ((3.0, 1.0), (7.0, 1.0), (7.0, 15.0), (3.0, 15.0)), 12, 20),
    ("fractional", "Fractional corners, which AddPath truncates toward zero.", ((1.7, 1.4), (9.6, 1.9), (9.2, 6.8), (1.3, 6.2)), 16, 12),
    ("touching-origin", "A box at the origin, where truncation and clipping meet.", ((0.0, 0.0), (6.0, 0.0), (6.0, 4.0), (0.0, 4.0)), 12, 10),
    ("slanted", "A slanted box.", ((2.0, 3.0), (11.0, 1.0), (13.0, 8.0), (4.0, 10.0)), 20, 16),
    ("near-edge", "A box against the right and bottom edges.", ((8.0, 6.0), (15.0, 6.0), (15.0, 11.0), (8.0, 11.0)), 16, 12),
    ("one-pixel-thin", "A one-pixel-thin box, the degenerate score case.", ((2.0, 5.0), (12.0, 5.0), (12.0, 6.0), (2.0, 6.0)), 16, 12),
)

UNCLIP_RATIOS = (1.5, 2.0)


def probability_map(width: int, height: int, seed: int, numpy: Any) -> Any:
    """Build a deterministic self-authored probability map in [0, 1]."""

    state = seed & 0xFFFFFFFF
    values = []
    for _ in range(width * height):
        state = (state * 1_664_525 + 1_013_904_223) & 0xFFFFFFFF
        values.append(((state >> 16) & 0xFFFF) / 65535.0)
    return numpy.array(values, dtype=numpy.float32).reshape(height, width)


def box_score_fast(pred: Any, box: Any, numpy: Any, cv2: Any) -> float:
    """Reproduce the upstream scoring exactly."""

    height, width = pred.shape[:2]
    box = box.copy()
    xmin = numpy.clip(numpy.floor(box[:, 0].min()).astype("int32"), 0, width - 1)
    xmax = numpy.clip(numpy.ceil(box[:, 0].max()).astype("int32"), 0, width - 1)
    ymin = numpy.clip(numpy.floor(box[:, 1].min()).astype("int32"), 0, height - 1)
    ymax = numpy.clip(numpy.ceil(box[:, 1].max()).astype("int32"), 0, height - 1)

    mask = numpy.zeros((ymax - ymin + 1, xmax - xmin + 1), dtype=numpy.uint8)
    box[:, 0] = box[:, 0] - xmin
    box[:, 1] = box[:, 1] - ymin
    cv2.fillPoly(mask, box.reshape(1, -1, 2).astype("int32"), 1)
    score = cv2.mean(pred[ymin : ymax + 1, xmin : xmax + 1], mask)[0]
    return float(score), mask, (int(xmin), int(xmax), int(ymin), int(ymax))


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture self-authored unclip and box-score cases as stdout JSON."
    )
    parser.add_argument("--indent", type=int, default=2, help="JSON indentation width")
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    parsed = parse_arguments(arguments)
    try:
        import cv2
        import numpy
        import pyclipper
        from shapely.geometry import Polygon
    except ImportError as error:
        print(
            "error: this developer-only tool needs cv2, numpy, pyclipper and shapely "
            f"in a disposable environment: {error}",
            file=sys.stderr,
        )
        return 2

    cv2.setUseOptimized(False)

    cases = []
    for index, (identifier, description, corners, width, height) in enumerate(CASES):
        pred = probability_map(width, height, 0x2000 + 0x37 * index, numpy)
        box = numpy.array(corners, dtype=numpy.float32)
        score, mask, bounds = box_score_fast(pred, box.copy(), numpy, cv2)

        unclipped = []
        for ratio in UNCLIP_RATIOS:
            polygon = Polygon(corners)
            distance = polygon.area * ratio / polygon.length
            offset = pyclipper.PyclipperOffset()
            offset.AddPath(corners, pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
            expanded = offset.Execute(distance)
            unclipped.append(
                {
                    "unclip_ratio": ratio,
                    "shapely_area": float(polygon.area),
                    "shapely_length": float(polygon.length),
                    "distance": float(distance),
                    "path_count": len(expanded),
                    "paths": [[[int(x), int(y)] for x, y in path] for path in expanded],
                }
            )

        cases.append(
            {
                "fixture_id": f"classic-v1-unclip-score-{identifier}",
                "description": description,
                "box": [[float(x), float(y)] for x, y in corners],
                "probability_map": {
                    "shape": [height, width],
                    "sha256": hashlib.sha256(pred.tobytes(order="C")).hexdigest(),
                },
                "score": {
                    "value": score,
                    "bounds": {
                        "xmin": bounds[0],
                        "xmax": bounds[1],
                        "ymin": bounds[2],
                        "ymax": bounds[3],
                    },
                    "mask_shape": list(mask.shape),
                    "mask_set_pixels": int(mask.sum()),
                },
                "unclip": unclipped,
            }
        )

    def distribution(name: str) -> str | None:
        try:
            return importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            return None

    document = {
        "schema_version": SCHEMA_VERSION,
        "purpose": "developer-only unclip and score oracle; not a normal Rust test dependency",
        "upstream": {
            "commit": UPSTREAM_COMMIT,
            "reference_paths": [
                "ppocr/postprocess/db_postprocess.py:unclip",
                "ppocr/postprocess/db_postprocess.py:box_score_fast",
            ],
        },
        "algorithm": {
            "unclip": "Polygon(box).area * ratio / .length, then PyclipperOffset JT_ROUND/ET_CLOSEDPOLYGON",
            "score": "floor/ceil clip, fillPoly mask, cv2.mean over the ROI",
            "input_generator": "lcg-v1 scaled to [0, 1] by /65535",
        },
        "environment": {
            "python": sys.version,
            "platform": platform.platform(),
            "numpy": numpy.__version__,
            "opencv": cv2.__version__,
            "opencv_optimized": cv2.useOptimized(),
            "pyclipper": distribution("pyclipper"),
            "shapely": distribution("shapely"),
        },
        "cases": cases,
    }
    indent = parsed.indent if parsed.indent > 0 else None
    print(json.dumps(document, indent=indent, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
