#!/usr/bin/env python3
"""Capture the layout-parsing geometry glue from the pinned paddlex.

Roadmap item: `STRUCT-001`, the orchestration slice (phase A).

`pipeline_v2.standardized_data` glues layout detections, region detections,
and OCR spans together with a handful of geometry helpers before anything
model-shaped happens. These helpers are pure, so they are pinned first:

    get_overlap_boxes_idx        >3px-by->3px intersection test
    get_sub_regions_ocr_res      OCR span filtering by layout membership
    remove_overlap_blocks        pairwise dedup with the image-label exception
    get_bbox_intersection        bbox and int16-cast poly formats
    calculate_minimum_enclosing_bbox / update_region_box / calculate_bbox_area
    shrink_supplement_region_bbox  the recursive region shrinker

Executed, not transcribed. Needs `numpy` and `paddlex` 3.7.2.

Usage:
    python3 tools/capture_structure_glue_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.utils import (
    calculate_bbox_area,
    calculate_minimum_enclosing_bbox,
    get_bbox_intersection,
    get_overlap_boxes_idx,
    get_sub_regions_ocr_res,
    remove_overlap_blocks,
    shrink_supplement_region_bbox,
    update_region_box,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/structure-glue-oracle-capture/v1"


def boxes(values):
    return np.array(values, dtype=np.float64)


OVERLAP_IDX_CASES = [
    ("basic", [[0, 0, 10, 10], [20, 0, 40, 10], [0, 20, 10, 40]], [[5, 5, 30, 8]]),
    # A 3px intersection is NOT enough: the test is strictly greater.
    ("exactly_three", [[0, 0, 10, 10]], [[7, 7, 20, 20]]),
    ("just_over_three", [[0, 0, 11, 11]], [[7, 7, 20, 20]]),
    ("several_refs", [[0, 0, 10, 10], [50, 50, 60, 60]], [[0, 0, 9, 9], [49, 49, 61, 61]]),
    ("empty_refs", [[0, 0, 10, 10]], []),
]

SUB_REGION_CASES = [
    (
        "within_and_without",
        # OCR spans
        [[2, 2, 30, 12], [2, 40, 30, 50], [70, 2, 90, 12]],
        # object boxes
        [[0, 0, 40, 20]],
    ),
    ("nothing_matches", [[2, 2, 30, 12]], [[100, 100, 140, 120]]),
]

REMOVE_OVERLAP_CASES = [
    (
        "smaller_dropped",
        [
            {"label": "text", "coordinate": [0, 0, 100, 100], "score": 0.9},
            {"label": "text", "coordinate": [10, 10, 50, 50], "score": 0.8},
        ],
    ),
    (
        "image_dropped_even_when_larger",
        [
            {"label": "image", "coordinate": [0, 0, 100, 100], "score": 0.9},
            {"label": "text", "coordinate": [10, 10, 50, 50], "score": 0.8},
        ],
    ),
    (
        "disjoint_kept",
        [
            {"label": "text", "coordinate": [0, 0, 40, 40], "score": 0.9},
            {"label": "text", "coordinate": [50, 50, 90, 90], "score": 0.8},
        ],
    ),
    (
        "chain_first_pair_wins",
        [
            {"label": "text", "coordinate": [0, 0, 60, 60], "score": 0.9},
            {"label": "text", "coordinate": [5, 5, 55, 55], "score": 0.8},
            {"label": "text", "coordinate": [8, 8, 52, 52], "score": 0.7},
        ],
    ),
]

INTERSECTION_CASES = [
    ("plain", [0, 0, 10, 10], [5, 5, 15, 15]),
    ("disjoint", [0, 0, 10, 10], [20, 20, 30, 30]),
    ("touching_edge", [0, 0, 10, 10], [10, 0, 20, 10]),
    ("poly_from_quad", [[0, 0], [12, 0], [12, 8], [0, 8]], [2, 2, 20, 20]),
]

SHRINK_CASES = [
    (
        "shrinks_toward_reference",
        [0, 150, 200, 400],     # supplement, hanging below the reference
        [0, 0, 200, 200],       # reference
        400, 400,               # image size
        [0, 1],                 # block idx set
        {0: [20, 220, 80, 300], 1: [120, 220, 180, 380]},
    ),
    (
        "degenerate_first_probe",
        [0, 0, 200, 200],
        [0, 0, 200, 100],
        400, 400,
        [0, 1],
        {0: [10, 120, 90, 190], 1: [110, 120, 190, 190]},
    ),
    (
        "no_blocks",
        [0, 0, 200, 200],
        [0, 0, 200, 100],
        400, 400,
        [],
        {},
    ),
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    overlap_idx = []
    for name, src, ref in OVERLAP_IDX_CASES:
        result = get_overlap_boxes_idx(boxes(src), boxes(ref)) if src else []
        overlap_idx.append(
            {
                "case": name,
                "src": src,
                "ref": ref,
                "indices": sorted(int(i) for i in set(result)),
            }
        )

    sub_regions = []
    for name, spans, objects in SUB_REGION_CASES:
        ocr = {
            "rec_polys": [[[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]] for b in spans],
            "rec_texts": [f"t{i}" for i in range(len(spans))],
            "rec_scores": [0.9] * len(spans),
            "rec_boxes": boxes(spans),
        }
        for flag_within in (True, False):
            filtered, match = get_sub_regions_ocr_res(
                dict(ocr), boxes(objects), flag_within=flag_within, return_match_idx=True
            )
            sub_regions.append(
                {
                    "case": f"{name}_within_{flag_within}",
                    "spans": spans,
                    "objects": objects,
                    "flag_within": flag_within,
                    "match_indices": sorted(int(i) for i in set(match)),
                    "kept_texts": list(filtered["rec_texts"]),
                }
            )

    removed = []
    for name, blocks in REMOVE_OVERLAP_CASES:
        result = remove_overlap_blocks({"boxes": [dict(b) for b in blocks]}, threshold=0.5, smaller=True)
        removed.append(
            {
                "case": name,
                "blocks": blocks,
                "kept": [
                    {"label": b["label"], "coordinate": [float(v) for v in b["coordinate"]]}
                    for b in result["boxes"]
                ],
            }
        )

    intersections = []
    for name, a, b in INTERSECTION_CASES:
        as_bbox = get_bbox_intersection(a, b, return_format="bbox")
        as_poly = get_bbox_intersection(a, b, return_format="poly")
        intersections.append(
            {
                "case": name,
                "first": a,
                "second": b,
                "bbox": None if as_bbox is None else [float(v) for v in as_bbox],
                "poly": None if as_poly is None else [[int(x), int(y)] for x, y in as_poly],
            }
        )

    enclosing = []
    for name, group in [
        ("two", [[10, 10, 30, 20], [5, 15, 25, 40]]),
        ("one", [[3, 4, 5, 6]]),
    ]:
        enclosing.append(
            {
                "case": name,
                "boxes": group,
                "enclosing": [float(v) for v in calculate_minimum_enclosing_bbox([list(b) for b in group])],
            }
        )

    region_updates = [
        {
            "bbox": [10, 5, 40, 50],
            "region": [20, 20, 30, 30],
            "updated": update_region_box([10, 5, 40, 50], [20, 20, 30, 30]),
        },
        {
            "bbox": [10.6, 5.4, 40.9, 50.1],
            "region": [65535, 65535, 0, 0],
            "updated": update_region_box([10.6, 5.4, 40.9, 50.1], [65535, 65535, 0, 0]),
        },
    ]

    areas = [
        {"bbox": [1.5, 2.5, 10.0, 4.0], "area": calculate_bbox_area([1.5, 2.5, 10.0, 4.0])},
        {"bbox": [10, 4, 1, 2], "area": calculate_bbox_area([10, 4, 1, 2])},
    ]

    shrinks = []
    for name, supplement, reference, width, height, idx_set, block_map in SHRINK_CASES:
        new_bbox, matched = shrink_supplement_region_bbox(
            list(supplement),
            list(reference),
            width,
            height,
            set(idx_set),
            {k: list(v) for k, v in block_map.items()},
        )
        shrinks.append(
            {
                "case": name,
                "supplement": supplement,
                "reference": reference,
                "image_size": [width, height],
                "block_idxes": idx_set,
                "block_bboxes": block_map,
                "result_bbox": [float(v) for v in new_bbox],
                "matched": sorted(int(i) for i in matched),
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/layout_parsing/utils.py",
        "overlap_idx": overlap_idx,
        "sub_regions": sub_regions,
        "remove_overlap": removed,
        "intersections": intersections,
        "enclosing": enclosing,
        "region_updates": region_updates,
        "areas": areas,
        "shrinks": shrinks,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
