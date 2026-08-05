#!/usr/bin/env python3
"""Capture `xycut_enhanced` end to end over synthetic pages.

Roadmap item: `STRUCT-001`, the object-model slice.

This is the top-level oracle for the whole ordering layer: it constructs real
`LayoutBlock` and `LayoutRegion` objects from synthetic page specs, runs the
pinned `xycut_enhanced`, and records the final block order, every block's
`order_label`, and the parent/child assignments. A Rust port that reproduces
these end-to-end orderings has reproduced the layer; the unit fixtures beneath
it exist so a failure names a function rather than a page.

Executed, not transcribed. Needs `numpy` and `paddlex` 3.7.2.

Usage:
    python3 tools/capture_layout_order_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.layout_objects import (
    LayoutBlock,
    LayoutRegion,
)
from paddlex.inference.pipelines.layout_parsing.xycut_enhanced import xycut_enhanced
from paddlex.inference.pipelines.layout_parsing.xycut_enhanced.utils import (
    calculate_discontinuous_projection,
    find_local_minima_flat_regions,
    reference_insert,
    shrink_overlapping_boxes,
)
from paddlex.inference.pipelines.layout_parsing.utils import calculate_overlap_ratio

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/layout-order-oracle-capture/v1"

# Each block: [label, bbox, num_of_lines, text_line_height, text_line_width].
# Defaults chosen to look like a real page: ~10px lines, ~20px per glyph run.
PAGES = {
    "single_column": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 100, 380, 200], 5, 10, 300],
            ["text", [20, 220, 380, 320], 5, 10, 300],
            ["text", [20, 340, 380, 440], 5, 10, 300],
        ],
    },
    "two_column": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 100, 190, 300], 10, 10, 150],
            ["text", [210, 100, 380, 300], 10, 10, 150],
            ["text", [20, 320, 190, 520], 10, 10, 150],
            ["text", [210, 320, 380, 520], 10, 10, 150],
        ],
    },
    "doc_title_with_subtitle": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["doc_title", [60, 40, 340, 90], 1, 40, 280],
            ["text", [120, 96, 280, 112], 1, 12, 160],
            ["text", [20, 140, 380, 400], 15, 10, 300],
        ],
    },
    "title_chain": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["paragraph_title", [20, 100, 380, 124], 1, 18, 300],
            ["paragraph_title", [20, 130, 380, 150], 1, 14, 300],
            ["text", [20, 160, 380, 400], 12, 10, 300],
        ],
    },
    "vision_with_title_and_footnote": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["table_title", [80, 80, 320, 100], 1, 12, 200],
            ["image", [60, 110, 340, 360], 1, 10, 200],
            ["text", [150, 366, 250, 380], 1, 10, 100],
            ["text", [20, 420, 380, 560], 8, 10, 300],
        ],
    },
    "header_and_footer": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["header", [20, 10, 380, 30], 1, 10, 300],
            ["text", [20, 60, 380, 500], 20, 10, 300],
            ["footer", [20, 560, 380, 580], 1, 10, 300],
        ],
    },
    "seal_is_unordered": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 60, 380, 300], 12, 10, 300],
            ["seal", [280, 320, 380, 420], 1, 10, 80],
            ["text", [20, 320, 260, 560], 12, 10, 220],
        ],
    },
    "centered_pre_cut": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 40, 190, 200], 8, 10, 150],
            ["text", [210, 40, 380, 200], 8, 10, 150],
            ["text", [40, 260, 360, 300], 2, 10, 300],
            ["text", [20, 360, 190, 560], 8, 10, 150],
            ["text", [210, 360, 380, 560], 8, 10, 150],
        ],
    },
    "cross_layout_span": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 60, 380, 120], 3, 10, 300],
            ["text", [20, 160, 190, 480], 16, 10, 150],
            ["text", [210, 160, 380, 480], 16, 10, 150],
            ["image", [100, 250, 300, 380], 1, 10, 200],
        ],
    },
    "single_block": {
        "bbox": [0, 0, 400, 600],
        "blocks": [["text", [20, 60, 380, 500], 20, 10, 300]],
    },
    "abstract_between_titles": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["doc_title", [60, 30, 340, 80], 1, 40, 280],
            ["abstract", [40, 100, 360, 180], 4, 10, 300],
            ["paragraph_title", [20, 200, 380, 224], 1, 18, 300],
            ["text", [20, 234, 380, 500], 14, 10, 300],
        ],
    },
    "footnote_page": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["text", [20, 60, 380, 460], 20, 10, 300],
            ["footnote", [20, 500, 380, 560], 3, 8, 300],
        ],
    },
    # A reference section: two columns of references with one wide reference
    # spanning both — the only page shape that reaches `reference_insert`.
    "reference_columns": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["reference_title", [20, 40, 380, 64], 1, 18, 300],
            ["reference", [20, 80, 190, 400], 16, 10, 150],
            ["reference", [210, 80, 380, 400], 16, 10, 150],
            ["reference", [20, 420, 380, 560], 7, 10, 300],
        ],
    },
    "vertical_doc_title": {
        "bbox": [0, 0, 400, 600],
        "blocks": [
            ["doc_title", [300, 40, 360, 560], 1, 40, 500],
            ["text", [160, 40, 280, 560], 8, 12, 500],
            ["text", [30, 40, 150, 560], 8, 12, 500],
        ],
    },
    # Detector-style float coordinates: LayoutBlock truncates the bbox to
    # ints but freezes width/height/area — and the derived direction — from
    # the floats. The last block is 100.4 wide by 100.6 tall: vertical from
    # the floats, square from the truncated ints.
    "fractional_coordinates": {
        "bbox": [0, 0, 400, 700],
        "blocks": [
            ["text", [20.3, 60.7, 380.6, 120.2], 3, 10.5, 300.2],
            ["text", [20.5, 160.2, 190.8, 480.9], 16, 10.2, 150.4],
            ["text", [210.1, 160.6, 380.4, 480.3], 16, 10.8, 150.1],
            ["text", [150.2, 500.3, 250.6, 600.9], 5, 10.0, 90.7],
        ],
    },
}


def build_region(spec):
    blocks = []
    for label, bbox, lines, line_height, line_width in spec["blocks"]:
        block = LayoutBlock(label, bbox, content="x " * 8)
        block.num_of_lines = lines
        block.text_line_height = line_height
        block.text_line_width = line_width
        # Realistic segment coordinates: the text fills the block.
        block.seg_start_coordinate = bbox[0]
        block.seg_end_coordinate = bbox[2]
        blocks.append(block)
    return LayoutRegion(spec["bbox"], blocks)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    pages = []
    for name, spec in PAGES.items():
        region = build_region(spec)
        ordered = xycut_enhanced(region)
        pages.append(
            {
                "case": name,
                "page_bbox": spec["bbox"],
                "blocks": spec["blocks"],
                "region_direction": region.direction,
                "region_text_line_height": float(region.text_line_height),
                "region_text_line_width": float(region.text_line_width),
                "order": [int(block.index) for block in ordered],
                "order_labels": {
                    str(int(block.index)): block.order_label for block in ordered
                },
            }
        )

    # Unit fixtures for the helpers with subtle boundaries.
    projections = []
    for name, boxes, direction in [
        ("two_bands", [[0, 0, 10, 10], [0, 20, 10, 30], [0, 5, 10, 12]], "vertical"),
        ("touching", [[0, 0, 10, 10], [10, 0, 20, 10]], "horizontal"),
        ("nested", [[0, 0, 100, 10], [10, 0, 20, 10], [200, 0, 210, 10]], "horizontal"),
    ]:
        merged, nums = calculate_discontinuous_projection(
            np.array(boxes), direction=direction, return_num=True
        )
        projections.append(
            {
                "case": name,
                "boxes": boxes,
                "direction": direction,
                "intervals": [[int(a), int(b)] for a, b in merged],
                "counts": [int(n) for n in nums],
            }
        )

    shrinks = []
    for name, boxes, direction in [
        ("vertical_touching", [[0, 0, 100, 50], [0, 50, 100, 100]], "vertical"),
        ("vertical_overlap", [[0, 0, 100, 52], [0, 48, 100, 100]], "vertical"),
        ("horizontal_near", [[0, 0, 50, 100], [52, 0, 100, 100]], "horizontal"),
    ]:
        blocks = [LayoutBlock("text", list(b)) for b in boxes]
        shrunk = shrink_overlapping_boxes(blocks, direction)
        shrinks.append(
            {
                "case": name,
                "boxes": boxes,
                "direction": direction,
                "shrunk": [[int(v) for v in block.bbox] for block in shrunk],
            }
        )

    minima = []
    for name, values in [
        ("valley", [3, 3, 1, 1, 2, 2, 1, 3]),
        ("flat", [2, 2, 2]),
        ("empty", []),
        ("two_valleys", [5, 1, 5, 2, 5]),
    ]:
        result = find_local_minima_flat_regions(list(values))
        minima.append(
            {
                "case": name,
                "values": values,
                "regions": None if result is None else [[int(a), int(b)] for a, b in result],
            }
        )

    # `reference_insert` directly: no synthetic page reliably reaches its
    # `cross_reference` path end to end, and its stale-variable quirk (the
    # distance survives iterations that skip the assignment) needs pinning.
    # Every case keeps the first sorted block above the reference — upstream
    # raises `NameError` otherwise, and that crash is not worth porting.
    reference_inserts = []
    for name, sorted_boxes, block_box in [
        ("below_all", [[10, 10, 100, 50], [10, 60, 100, 90]], [10, 200, 100, 240]),
        (
            "stale_skips_middle",
            [[10, 10, 100, 50], [10, 300, 100, 340], [10, 60, 100, 90]],
            [10, 100, 100, 140],
        ),
        (
            "widest_wins",
            [[10, 10, 200, 50], [10, 60, 100, 90]],
            [10, 200, 200, 240],
        ),
        ("single", [[10, 10, 100, 50]], [10, 60, 100, 90]),
    ]:
        sorted_blocks = [LayoutBlock("text", list(b)) for b in sorted_boxes]
        for position, item in enumerate(sorted_blocks):
            item.index = position
        block = LayoutBlock("reference", list(block_box))
        block.index = len(sorted_blocks)
        result = reference_insert(block, list(sorted_blocks))
        reference_inserts.append(
            {
                "case": name,
                "sorted": sorted_boxes,
                "block": block_box,
                "result": [int(item.index) for item in result],
            }
        )

    overlaps = []
    for name, a, b, mode in [
        ("partial_union", [0, 0, 10, 10], [5, 5, 15, 15], "union"),
        ("partial_small", [0, 0, 10, 10], [5, 5, 15, 15], "small"),
        ("contained_small", [0, 0, 100, 100], [10, 10, 20, 20], "small"),
        ("disjoint", [0, 0, 10, 10], [20, 20, 30, 30], "union"),
    ]:
        overlaps.append(
            {
                "case": name,
                "first": a,
                "second": b,
                "mode": mode,
                "ratio": float(calculate_overlap_ratio(a, b, mode=mode)),
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 layout_parsing: layout_objects.py, "
        "xycut_enhanced/{xycuts,utils}.py",
        "pages": pages,
        "discontinuous_projections": projections,
        "shrinks": shrinks,
        "local_minima": minima,
        "overlap_ratios": overlaps,
        "reference_inserts": reference_inserts,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(pages)} pages)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
