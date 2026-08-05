#!/usr/bin/env python3
"""Capture the nested region ordering of `sort_layout_parsing_blocks`.

Roadmap item: `STRUCT-001`, the orchestration slice (phase C).

PP-StructureV3 orders a document twice: `xycut_enhanced` over a page whose
blocks are whole `LayoutRegion` objects (the `region` label, matched by
`euclidean_insert` and sorted among children by `euclidean_distance`), then
`xycut_enhanced` again inside each region. This tool mirrors
`get_layout_parsing_objects` + `sort_layout_parsing_blocks`: it builds real
`LayoutBlock`s, groups them into `LayoutRegion`s, unions the page box with
`update_region_box`, runs both ordering passes, and records the region order,
each region's derived attributes, and the flattened `(region, block)` order.

Executed, not transcribed. Needs `numpy` and `paddlex` 3.7.2.

Usage:
    python3 tools/capture_region_order_oracle.py <output.json>
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
from paddlex.inference.pipelines.layout_parsing.utils import update_region_box
from paddlex.inference.pipelines.layout_parsing.xycut_enhanced import xycut_enhanced

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/region-order-oracle-capture/v1"

# Each region: {"bbox": [...], "blocks": [[label, bbox, lines, height, width]]}.
PAGES = {
    # Two full-width regions stacked: the region page pre-cuts at every gap.
    "two_regions_stacked": [
        {
            "bbox": [0, 0, 400, 280],
            "blocks": [
                ["text", [20, 20, 190, 260], 12, 10, 150],
                ["text", [210, 20, 380, 260], 12, 10, 150],
            ],
        },
        {
            "bbox": [0, 300, 400, 600],
            "blocks": [
                ["text", [20, 320, 380, 440], 6, 10, 300],
                ["text", [20, 460, 380, 580], 6, 10, 300],
            ],
        },
    ],
    # Three regions side by side: more than one primary band, so the region
    # page skips the cross-layout pass and keeps the cut order.
    "three_region_columns": [
        {
            "bbox": [0, 0, 130, 600],
            "blocks": [["text", [10, 20, 120, 580], 30, 10, 100]],
        },
        {
            "bbox": [140, 0, 270, 600],
            "blocks": [["text", [150, 20, 260, 580], 30, 10, 100]],
        },
        {
            "bbox": [280, 0, 400, 600],
            "blocks": [["text", [290, 20, 390, 580], 30, 10, 100]],
        },
    ],
    # A small region inside a large one: `update_region_child_blocks` makes it
    # a `sub_region` child, and the family sorts by euclidean distance.
    "nested_regions": [
        {
            "bbox": [0, 0, 400, 600],
            "blocks": [
                ["text", [20, 20, 380, 200], 9, 10, 300],
                ["text", [20, 420, 380, 580], 8, 10, 300],
            ],
        },
        {
            "bbox": [60, 220, 340, 400],
            "blocks": [["text", [70, 230, 330, 390], 8, 10, 250]],
        },
    ],
    # A wide region spanning two column regions: the single-band region group
    # runs the cross-layout pass, the spanning region is marked cross-layout,
    # and it re-enters through `euclidean_insert` inside the same pre-cut.
    "region_spans_columns": [
        {
            "bbox": [0, 0, 190, 380],
            "blocks": [["text", [10, 60, 180, 370], 16, 10, 150]],
        },
        {
            "bbox": [210, 0, 400, 380],
            "blocks": [["text", [220, 60, 390, 370], 16, 10, 150]],
        },
        {
            "bbox": [0, 0, 400, 50],
            "blocks": [["text", [10, 10, 390, 40], 2, 10, 300]],
        },
    ],
    # Vertical regions: the reference corner moves to the top-right and the
    # page direction flips.
    "vertical_regions": [
        {
            "bbox": [210, 0, 400, 600],
            "blocks": [
                ["text", [320, 20, 390, 580], 4, 12, 500],
                ["text", [220, 20, 300, 580], 4, 12, 500],
            ],
        },
        {
            "bbox": [0, 0, 190, 600],
            "blocks": [
                ["text", [110, 20, 180, 580], 4, 12, 500],
                ["text", [10, 20, 100, 580], 4, 12, 500],
            ],
        },
    ],
    "single_region": [
        {
            "bbox": [0, 0, 400, 600],
            "blocks": [
                ["doc_title", [60, 40, 340, 90], 1, 40, 280],
                ["text", [20, 140, 380, 400], 15, 10, 300],
            ],
        },
    ],
    # A 2x2 grid of regions: euclidean matching decides among equals.
    "region_grid": [
        {
            "bbox": [0, 0, 190, 280],
            "blocks": [["text", [10, 10, 180, 270], 13, 10, 150]],
        },
        {
            "bbox": [210, 0, 400, 280],
            "blocks": [["text", [220, 10, 390, 270], 13, 10, 150]],
        },
        {
            "bbox": [0, 300, 190, 600],
            "blocks": [["text", [10, 310, 180, 590], 14, 10, 150]],
        },
        {
            "bbox": [210, 300, 400, 600],
            "blocks": [["text", [220, 310, 390, 590], 14, 10, 150]],
        },
    ],
}


def build_block(spec):
    label, bbox, lines, line_height, line_width = spec
    block = LayoutBlock(label, bbox, content="x " * 8)
    block.num_of_lines = lines
    block.text_line_height = line_height
    block.text_line_width = line_width
    block.seg_start_coordinate = bbox[0]
    block.seg_end_coordinate = bbox[2]
    return block


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    cases = []
    for name, region_specs in PAGES.items():
        page_region_box = [65535, 65535, 0, 0]
        regions = []
        for region_spec in region_specs:
            region_bbox = np.array(region_spec["bbox"]).astype("int")
            blocks = [build_block(spec) for spec in region_spec["blocks"]]
            page_region_box = update_region_box(region_bbox, page_region_box)
            regions.append(LayoutRegion(bbox=region_bbox, blocks=blocks))

        region_records = [
            {
                "bbox": [int(v) for v in region.bbox],
                "blocks": spec["blocks"],
                "direction": region.direction,
                "text_line_height": float(region.text_line_height),
                "text_line_width": float(region.text_line_width),
                "euclidean_distance": float(region.euclidean_distance),
            }
            for region, spec in zip(regions, region_specs)
        ]

        page = LayoutRegion(
            bbox=np.array(page_region_box).astype("int"), blocks=regions
        )
        page_direction = page.direction

        # `sort_layout_parsing_blocks`, flattened to (region, block) indices.
        ordered_regions = xycut_enhanced(page)
        region_order = [int(region.index) for region in ordered_regions]
        flat_order = []
        for region in ordered_regions:
            for block in xycut_enhanced(region):
                flat_order.append([int(region.index), int(block.index)])

        cases.append(
            {
                "case": name,
                "page_bbox": [int(v) for v in page_region_box],
                "page_direction": page_direction,
                "regions": region_records,
                "region_order": region_order,
                "flat_order": flat_order,
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 layout_parsing: pipeline_v2.py "
        "sort_layout_parsing_blocks over LayoutRegion pages, "
        "xycut_enhanced/{xycuts,utils}.py region label path",
        "cases": cases,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(cases)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
