#!/usr/bin/env python3
"""Capture the table pipeline's geometry, matching, and HTML assembly.

Roadmap item: `TABLEPIPE-001`, first slice.

`TableRecognitionV2` turns three model outputs -- structure tokens, cell boxes,
and OCR boxes with text -- into an HTML table. The models are already ported
(`TBLCLS-001`, `TBLCELL-001`, `TBLSTRUCT-001`, and the classic OCR path); what is
left is the part that composes them, and it is pure functions over boxes and
token lists.

That makes it capturable without a single inference session, which is why it is
the first slice rather than the pipeline wiring.

Captured here, all executed from the pinned `paddlex` 3.7.2:

    compute_inter        intersection over the *second* box's area
    compute_iou          ordinary IoU
    distance             centre distance plus the smaller corner distance
    find_row_start_index which cell index starts each row
    match_table_and_ocr  cell-to-OCR assignment, per row
    get_html_result      token list plus matches plus text -> HTML

Needs `numpy` and `paddlex` 3.7.2. No model is run and nothing is downloaded.

Usage:
    python3 tools/capture_table_pipeline_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
import traceback
from pathlib import Path

from paddlex.inference.pipelines.table_recognition.table_recognition_post_processing_v2 import (
    compute_inter,
    compute_iou,
    distance,
    find_row_start_index,
    get_html_result,
    map_and_get_max,
    match_table_and_ocr,
    sort_table_cells_boxes,
)

from paddlex.inference.pipelines.table_recognition.pipeline_v2 import (
    _TableRecognitionPipelineV2 as TableRecognitionPipelineV2,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/table-pipeline-oracle-capture/v2"

# `cells_det_results_nms` and `get_region_ocr_det_boxes` are methods but use no
# instance state, so they are called unbound rather than by constructing a
# pipeline -- which would demand four artifacts this capture does not need.
NMS = TableRecognitionPipelineV2.cells_det_results_nms
CROP = TableRecognitionPipelineV2.get_region_ocr_det_boxes

# Box pairs chosen to cover: disjoint, touching edge-on, partial overlap,
# containment either way, and identical.
BOX_PAIRS = [
    ("disjoint", [0, 0, 10, 10], [20, 20, 30, 30]),
    ("touching_edge", [0, 0, 10, 10], [10, 0, 20, 10]),
    ("partial_overlap", [0, 0, 10, 10], [5, 5, 15, 15]),
    ("first_contains_second", [0, 0, 100, 100], [10, 10, 20, 20]),
    ("second_contains_first", [10, 10, 20, 20], [0, 0, 100, 100]),
    ("identical", [3, 4, 13, 24], [3, 4, 13, 24]),
    ("degenerate_second", [0, 0, 10, 10], [5, 5, 5, 5]),
    ("negative_coordinates", [-10, -10, 0, 0], [-5, -5, 5, 5]),
]

TOKEN_LISTS = {
    "two_by_two": [
        "<html>", "<body>", "<table>",
        "<tbody>",
        "<tr>", "<td></td>", "<td></td>", "</tr>",
        "<tr>", "<td></td>", "<td></td>", "</tr>",
        "</tbody>",
        "</table>", "</body>", "</html>",
    ],
    "with_head": [
        "<html>", "<body>", "<table>",
        "<thead>", "<tr>", "<td></td>", "</tr>", "</thead>",
        "<tbody>", "<tr>", "<td></td>", "</tr>", "</tbody>",
        "</table>", "</body>", "</html>",
    ],
    "spanning": [
        "<html>", "<body>", "<table>",
        "<tbody>",
        "<tr>", "<td", ' colspan="2"', ">", "</td>", "</tr>",
        "<tr>", "<td></td>", "<td></td>", "</tr>",
        "</tbody>",
        "</table>", "</body>", "</html>",
    ],
    "empty_table": [
        "<html>", "<body>", "<table>", "<tbody>", "</tbody>",
        "</table>", "</body>", "</html>",
    ],
}


def guarded(name: str, call) -> dict:
    """Records what upstream does, including raising.

    Some of these paths are reachable with inputs a real pipeline can produce
    and upstream does not handle them. Recording the exception is more useful
    than choosing inputs that avoid it, because a port has to decide what to do
    there and should decide against what actually happens.
    """
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

    geometry = []
    for name, first, second in BOX_PAIRS:
        geometry.append(
            {
                "case": name,
                "first": first,
                "second": second,
                "compute_inter": float(compute_inter(first, second)),
                "compute_iou": float(compute_iou(first, second)),
                "distance": float(distance(first, second)),
                # Not symmetric: `compute_inter` divides by the second box.
                "compute_inter_swapped": float(compute_inter(second, first)),
            }
        )

    row_starts = [
        {"case": name, "tokens": tokens, "row_start_index": find_row_start_index(tokens)}
        for name, tokens in TOKEN_LISTS.items()
    ]

    # A 2x2 table whose four cells each hold one OCR box, plus one OCR box that
    # overlaps nothing so the "unmatched text is dropped" behaviour is pinned.
    #
    # The call convention below is `get_table_recognition_res`'s, exactly:
    # `match_table_and_ocr` is passed `table_cells_flag` for **both** flag
    # arguments, which makes the two mismatch branches in its body unreachable
    # from the real pipeline. Passing genuinely different lists raises
    # `KeyError`; that is recorded in `unreachable_branch` rather than smoothed
    # over, because a port has to decide what to do there.
    cell_boxes = [
        [0, 0, 50, 20],
        [50, 0, 100, 20],
        [0, 20, 50, 40],
        [50, 20, 100, 40],
    ]
    ocr_boxes = [
        [2, 2, 48, 18],
        [52, 2, 98, 18],
        [2, 22, 48, 38],
        [52, 22, 98, 38],
        [200, 200, 240, 220],
    ]
    ocr_texts = ["a1", "b1", "a2", "b2", "orphan"]
    tokens = TOKEN_LISTS["two_by_two"]

    sorted_boxes, raw_flag = sort_table_cells_boxes(cell_boxes)
    row_start_index = find_row_start_index(tokens)
    table_cells_flag = map_and_get_max(raw_flag, row_start_index)
    table_cells_flag.append(len(sorted_boxes))
    row_start_index_full = list(row_start_index) + [len(sorted_boxes)]

    raw_matches = match_table_and_ocr(
        sorted_boxes, ocr_boxes, table_cells_flag, table_cells_flag
    )
    matches = {
        "case": "two_by_two",
        "ok": True,
        "value": [
            {str(key): value for key, value in matched.items()}
            for matched in raw_matches
        ],
    }

    html_cases = [
        guarded(
            "two_by_two",
            lambda: get_html_result(
                raw_matches, ocr_texts, tokens, row_start_index_full
            ),
        )
    ]

    # Two OCR boxes inside one cell, which triggers the space joining.
    single_cell = [[0, 0, 50, 20]]
    merged_boxes = [[2, 2, 24, 18], [26, 2, 48, 18]]
    single_tokens = [
        "<html>", "<body>", "<table>", "<tbody>", "<tr>", "<td></td>",
        "</tr>", "</tbody>", "</table>", "</body>", "</html>",
    ]
    merged_sorted, merged_raw_flag = sort_table_cells_boxes(single_cell)
    merged_row_start = find_row_start_index(single_tokens)
    merged_flag = map_and_get_max(merged_raw_flag, merged_row_start)
    merged_flag.append(len(merged_sorted))
    merged_matches = match_table_and_ocr(
        merged_sorted, merged_boxes, merged_flag, merged_flag
    )
    html_cases.append(
        guarded(
            "two_boxes_in_one_cell",
            lambda: get_html_result(
                merged_matches,
                ["left", "right"],
                single_tokens,
                list(merged_row_start) + [len(merged_sorted)],
            ),
        )
    )

    # What the real call site can never reach, recorded rather than avoided.
    unreachable = guarded(
        "mismatched_flag_lists",
        lambda: match_table_and_ocr(sorted_boxes, ocr_boxes, [0, 2, 4], [0, 0, 2]),
    )

    sorting = {
        "input": cell_boxes,
        "sorted": sorted_boxes,
        "raw_flag": raw_flag,
        "row_start_index": row_start_index,
        "table_cells_flag": table_cells_flag,
        "row_start_index_full": row_start_index_full,
    }

    # NMS over cell boxes. The pipeline calls this with its default threshold.
    nms_cases = []
    for name, boxes, scores in [
        (
            "no_overlap",
            [[0, 0, 10, 10], [20, 20, 30, 30]],
            [0.9, 0.8],
        ),
        (
            "heavy_overlap_keeps_best",
            [[0, 0, 10, 10], [1, 1, 11, 11], [40, 40, 50, 50]],
            [0.6, 0.95, 0.7],
        ),
        (
            "tied_scores",
            [[0, 0, 10, 10], [100, 100, 110, 110]],
            [0.5, 0.5],
        ),
        (
            "containment",
            [[0, 0, 100, 100], [10, 10, 20, 20]],
            [0.8, 0.9],
        ),
    ]:
        kept_boxes, kept_scores = NMS(None, boxes, scores)
        nms_cases.append(
            {
                "case": name,
                "boxes": boxes,
                "scores": scores,
                "kept_boxes": kept_boxes,
                "kept_scores": kept_scores,
            }
        )

    # Cropping OCR boxes into a table region's coordinate space.
    crop_cases = []
    table_box = [100, 50, 400, 250]
    for name, ocr in [
        ("fully_inside", [[110, 60, 200, 90]]),
        ("crossing_the_left_edge", [[90, 60, 200, 90]]),
        ("crossing_the_bottom_edge", [[110, 60, 200, 300]]),
        ("exactly_on_the_boundary", [[100, 50, 400, 250]]),
        ("entirely_outside", [[500, 500, 520, 520]]),
    ]:
        crop_cases.append(
            {
                "case": name,
                "table_box": table_box,
                "ocr_boxes": ocr,
                "adjusted": CROP(None, ocr, table_box),
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/table_recognition/"
        "table_recognition_post_processing_v2.py",
        "geometry": geometry,
        "row_starts": row_starts,
        "sorting": sorting,
        "matching": {
            "cell_boxes": cell_boxes,
            "ocr_boxes": ocr_boxes,
            "ocr_texts": ocr_texts,
            "table_cells_flag": table_cells_flag,
            "row_start_index": row_start_index_full,
            "result": matches,
            "unreachable_branch": unreachable,
        },
        "html": html_cases,
        "nms": nms_cases,
        "crop": crop_cases,
        "route": {
            "wired_label": "wired_table",
            "wireless_label": "wireless_table",
            "cell_detection_threshold": 0.3,
            "cell_detection_threshold_source": "pipeline_v2.py passes threshold=0.3 explicitly, overriding the artifact's draw_threshold of 0.5",
        },
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(
        f"wrote {output} ({len(geometry)} geometry, {len(row_starts)} row starts, "
        f"{len(html_cases)} html)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
