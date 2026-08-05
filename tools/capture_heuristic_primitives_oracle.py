#!/usr/bin/env python3
"""Capture the leaf geometry primitives of the xycut_enhanced heuristics.

Roadmap item: `STRUCT-001`, second slice.

The label-aware ordering layer above the XY-cut primitives rests on four leaf
functions that need no `LayoutBlock` object model: a projection overlap ratio,
a weighted nearest-edge distance, the per-label weight table, and the
normal-block sort key. They are pure over `[x1, y1, x2, y2]` boxes and labels,
so they are capturable now; the object model above them is the next unit.

Executed from the pinned `paddlex` 3.7.2, not transcribed.

Usage:
    python3 tools/capture_heuristic_primitives_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from paddlex.inference.pipelines.layout_parsing.utils import (
    calculate_projection_overlap_ratio,
)
from paddlex.inference.pipelines.layout_parsing.xycut_enhanced.utils import (
    _get_weights,
    _manhattan_distance,
    get_nearest_edge_distance,
    sort_normal_blocks,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/heuristic-primitives-oracle-capture/v1"

BOX_PAIRS = [
    ("overlapping", [0, 0, 10, 10], [5, 5, 15, 15]),
    ("disjoint_diagonal", [0, 0, 10, 10], [20, 20, 30, 30]),
    ("right_of", [0, 0, 10, 10], [20, 2, 30, 8]),
    ("left_of", [20, 2, 30, 8], [0, 0, 10, 10]),
    ("below", [0, 0, 10, 10], [2, 20, 8, 30]),
    ("above", [2, 20, 8, 30], [0, 0, 10, 10]),
    ("touching_edges", [0, 0, 10, 10], [10, 0, 20, 10]),
    ("contained", [0, 0, 100, 100], [10, 10, 20, 20]),
    ("same", [3, 4, 13, 24], [3, 4, 13, 24]),
]

LABELS = ["doc_title", "paragraph_title", "abstract", "image", "text",
          "vision_footnote", "table"]

WEIGHTED_CASES = [
    ("below_doc_title", "doc_title", "horizontal", [0, 0, 10, 10], [2, 20, 8, 30]),
    ("below_text", "text", "horizontal", [0, 0, 10, 10], [2, 20, 8, 30]),
    ("right_paragraph_title", "paragraph_title", "horizontal",
     [0, 0, 10, 10], [20, 2, 30, 8]),
    ("diagonal_doc_title_vertical", "doc_title", "vertical",
     [0, 0, 10, 10], [20, 20, 30, 30]),
]


class Block:
    def __init__(self, bbox):
        self.bbox = bbox

    def get_centroid(self):
        x1, y1, x2, y2 = self.bbox
        return ((x1 + x2) / 2, (y1 + y2) / 2)


SORT_CASES = {
    # Same visual row within line-height quantization, ordered by column.
    "single_row": [[0, 2, 10, 12], [40, 0, 50, 10], [20, 1, 30, 11]],
    # Two rows; the second's leftmost block first.
    "two_rows": [[40, 0, 50, 10], [0, 30, 10, 40], [20, 2, 30, 12], [30, 31, 44, 41]],
    # Quantization tie broken by centroid distance from the origin.
    "quantized_tie": [[5, 0, 15, 10], [5, 4, 15, 14], [5, 8, 15, 18]],
}


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    overlaps = []
    for name, a, b in BOX_PAIRS:
        overlaps.append({
            "case": name, "first": a, "second": b,
            "horizontal": float(calculate_projection_overlap_ratio(a, b, "horizontal")),
            "vertical": float(calculate_projection_overlap_ratio(a, b, "vertical")),
            "edge_distance_unit": float(get_nearest_edge_distance(a, b)),
        })

    weights = [{"label": label, "direction": direction,
                "weights": _get_weights(label, direction)}
               for label in LABELS for direction in ("horizontal", "vertical")]

    weighted = []
    for name, label, direction, a, b in WEIGHTED_CASES:
        weighted.append({
            "case": name, "label": label, "direction": direction,
            "first": a, "second": b,
            "edge_distance": float(
                get_nearest_edge_distance(a, b, _get_weights(label, direction))
            ),
        })

    manhattan = [{
        "point1": [1.0, 2.0], "point2": [4.0, 6.0],
        "weight_x": 2.0, "weight_y": 0.5,
        "distance": float(_manhattan_distance((1.0, 2.0), (4.0, 6.0), 2.0, 0.5)),
    }]

    sorts = []
    for name, boxes in SORT_CASES.items():
        for direction in ("horizontal", "vertical"):
            blocks = [Block(list(b)) for b in boxes]
            ordered = sort_normal_blocks(blocks, 10, 10, direction)
            sorts.append({
                "case": f"{name}/{direction}",
                "boxes": boxes,
                "text_line_height": 10, "text_line_width": 10,
                "direction": direction,
                "order": [boxes.index(b.bbox) for b in ordered],
            })

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 layout_parsing/utils.py and "
        "xycut_enhanced/utils.py leaf primitives",
        "overlaps": overlaps,
        "weights": weights,
        "weighted_distances": weighted,
        "manhattan": manhattan,
        "sorts": sorts,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(overlaps)} overlaps, {len(sorts)} sorts)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
