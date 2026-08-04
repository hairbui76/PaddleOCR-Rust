#!/usr/bin/env python3
"""Capture the XY-cut reading-order primitives.

Roadmap item: `STRUCT-001`, first slice.

`PP-StructureV3` orders layout blocks with `xycut_enhanced`, which is `1,830`
lines of heuristics layered on **four pure functions**:

    projection_by_bboxes      boxes -> a 1D occupancy histogram on one axis
    split_projection_profile  histogram -> segments, split at gaps
    recursive_yx_cut          project on Y, then X, then recurse
    recursive_xy_cut          the mirror

Those four *are* the reading-order algorithm; the rest is special handling for
titles, figures, and regions. They take integer boxes and return an ordering, so
they are capturable and matchable exactly — the same property that made the
table composition portable ahead of its plumbing.

Like every PaddleX capture in this project, this one **imports and executes**
the pinned functions rather than transcribing them.

Needs `numpy` and `paddlex` 3.7.2. Nothing is downloaded and no model is run.

Usage:
    python3 tools/capture_reading_order_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
import traceback
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.xycut_enhanced.utils import (
    projection_by_bboxes,
    recursive_xy_cut,
    recursive_yx_cut,
    split_projection_profile,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/reading-order-oracle-capture/v1"

# Page layouts chosen to exercise the recursion rather than one shape of page.
LAYOUTS = {
    # A single column: the Y cut separates every block and no X cut is needed.
    "single_column": [
        [10, 10, 190, 40],
        [10, 50, 190, 80],
        [10, 90, 190, 120],
    ],
    # Two columns: the Y cut finds one band, the X cut splits it, and reading
    # order must go down the left column before the right.
    "two_columns": [
        [10, 10, 90, 40],
        [110, 10, 190, 40],
        [10, 50, 90, 80],
        [110, 50, 190, 80],
    ],
    # A full-width header over two columns, which is where a naive top-to-bottom
    # sort and a real XY cut disagree.
    "header_over_two_columns": [
        [10, 10, 190, 30],
        [10, 40, 90, 70],
        [110, 40, 190, 70],
        [10, 80, 90, 110],
        [110, 80, 190, 110],
    ],
    # Blocks sharing a top edge, so the sort has ties to break.
    "tied_tops": [
        [10, 10, 90, 40],
        [110, 10, 190, 40],
        [210, 10, 290, 40],
    ],
    # Overlapping vertically, so the Y projection finds one band containing all.
    "overlapping_rows": [
        [10, 10, 90, 60],
        [110, 30, 190, 80],
    ],
    # One block only.
    "single_block": [[5, 5, 50, 50]],
    # Blocks touching edge to edge, where `min_gap` decides whether they split.
    "touching_columns": [
        [10, 10, 100, 40],
        [100, 10, 190, 40],
    ],
}

PROJECTION_CASES = [
    ("simple_x", [[0, 0, 10, 10], [20, 0, 30, 10]], 0),
    ("simple_y", [[0, 0, 10, 10], [0, 20, 10, 30]], 1),
    ("overlapping", [[0, 0, 10, 10], [5, 0, 15, 10]], 0),
    ("touching", [[0, 0, 10, 10], [10, 0, 20, 10]], 0),
]

SPLIT_CASES = [
    ("one_run", [0, 1, 1, 1, 0, 0], 0, 1),
    ("two_runs_wide_gap", [1, 1, 0, 0, 0, 1, 1], 0, 1),
    ("gap_equal_to_min_gap", [1, 1, 0, 1, 1], 0, 1),
    ("gap_below_min_gap", [1, 1, 0, 1, 1], 0, 2),
    ("all_zero", [0, 0, 0], 0, 1),
    ("threshold_excludes", [1, 2, 1], 1, 1),
]


def guarded(name: str, call) -> dict:
    try:
        return {"case": name, "ok": True, "value": call()}
    except Exception as error:  # noqa: BLE001 - recording upstream's behaviour
        return {
            "case": name,
            "ok": False,
            "error_type": type(error).__name__,
            "error": str(error),
            "traceback_tail": traceback.format_exc().strip().splitlines()[-1],
        }


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    projections = []
    for name, boxes, axis in PROJECTION_CASES:
        histogram = projection_by_bboxes(np.array(boxes), axis)
        projections.append(
            {
                "case": name,
                "boxes": boxes,
                "axis": axis,
                "projection": [int(v) for v in histogram],
            }
        )

    splits = []
    for name, values, min_value, min_gap in SPLIT_CASES:
        result = split_projection_profile(np.array(values), min_value, min_gap)
        splits.append(
            {
                "case": name,
                "values": values,
                "min_value": min_value,
                "min_gap": min_gap,
                "starts": None if result is None else [int(v) for v in result[0]],
                "ends": None if result is None else [int(v) for v in result[1]],
            }
        )

    orders = []
    for name, boxes in LAYOUTS.items():
        array = np.array(boxes)
        indices = list(range(len(boxes)))

        yx: list[int] = []
        yx_result = guarded(name, lambda: (recursive_yx_cut(array, indices, yx), yx)[1])
        xy: list[int] = []
        xy_result = guarded(name, lambda: (recursive_xy_cut(array, indices, xy), xy)[1])

        orders.append(
            {
                "case": name,
                "boxes": boxes,
                "yx_order": [int(v) for v in yx_result["value"]]
                if yx_result["ok"]
                else None,
                "yx_error": None if yx_result["ok"] else yx_result["error_type"],
                "xy_order": [int(v) for v in xy_result["value"]]
                if xy_result["ok"]
                else None,
                "xy_error": None if xy_result["ok"] else xy_result["error_type"],
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/layout_parsing/"
        "xycut_enhanced/utils.py",
        "note": "These four functions are the reading-order algorithm; the rest of "
        "xycut_enhanced is per-label heuristics layered on them. Boxes are integers "
        "because the projection uses them as array indices.",
        "projections": projections,
        "splits": splits,
        "orders": orders,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(
        f"wrote {output} ({len(projections)} projections, {len(splits)} splits, "
        f"{len(orders)} orders)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
