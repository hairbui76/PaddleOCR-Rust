#!/usr/bin/env python3
"""Capture the layout-parsing block assembly from the pinned paddlex.

Roadmap item: `STRUCT-001`, the orchestration slice (phase D).

This drives the whole post-model chain of `get_layout_parsing_res`:
`standardized_data` (already pinned on its own) feeding
`get_layout_parsing_objects` — block construction with table `pred_html`
injection by running table index, the `formula`-label OCR re-matching,
`update_text_content` for everything else, image-path stamping, and region
grouping that skips empty regions — then `sort_layout_parsing_blocks` (the
double `xycut_enhanced`) and the `order_index` tail over
`visualize_index_labels` minus the default `markdown_ignore_labels`.

The recognizer is the same shape-keyed deterministic stub as the
standardized-data capture; recognition models are never actually reached by
text-labelled spans. Seal and chart result lists are empty — that is the
supported mode this port targets. Executed, not transcribed.

Usage:
    python3 tools/capture_layout_assembly_oracle.py <output.json>
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.pipeline_v2 import (
    _LayoutParsingPipelineV2,
)
from paddlex.inference.pipelines.layout_parsing.setting import BLOCK_LABEL_MAP

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/layout-assembly-oracle-capture/v1"

IMAGE_WIDTH = 400
IMAGE_HEIGHT = 600

MARKDOWN_IGNORE_LABELS = [
    "number",
    "footnote",
    "header",
    "header_image",
    "footer",
    "footer_image",
    "aside_text",
]


class StubRecognizer:
    """Deterministic text_rec_model stand-in keyed on the crop shape."""

    def __init__(self):
        self.calls = []

    def __call__(self, images):
        image = images[0]
        h, w = int(image.shape[0]), int(image.shape[1])
        self.calls.append([h, w])
        return iter(
            [{"rec_text": f"rec-{h}x{w}", "rec_score": ((h * 31 + w * 7) % 97) / 96}]
        )


class DummySelf:
    """Neither stage touches self when the threshold is explicit."""


# Each case: layout boxes, region boxes, ocr spans (box, text), table html.
CASES = {
    # A plain page: doc title, paragraph title, and text through the whole
    # chain; order_index counts every visualize label.
    "single_column_text": {
        "threshold": 0.5,
        "layout": [
            ["doc_title", [60.2, 40.6, 340.8, 90.3], 0.98],
            ["paragraph_title", [20.4, 120.7, 380.2, 144.9], 0.95],
            ["text", [20.6, 160.2, 380.4, 400.8], 0.97],
        ],
        "regions": [],
        "spans": [
            [[70, 50, 330, 80], "The Title"],
            [[30, 125, 370, 140], "Section one"],
            [[30, 170, 370, 200], "First paragraph line one"],
            [[30, 210, 370, 240], "and line two."],
        ],
        "tables": [],
    },
    # A table block takes its content from table_res_list by running index
    # and is stamped with an image path; a formula block re-matches its OCR
    # spans inside get_layout_parsing_objects.
    "table_and_formula": {
        "threshold": 0.5,
        "layout": [
            ["text", [20.3, 20.5, 380.7, 60.2], 0.97],
            ["table", [40.6, 80.4, 360.9, 260.8], 0.96],
            ["formula", [60.2, 280.6, 340.4, 320.9], 0.94],
            ["text", [20.8, 340.3, 380.1, 420.6], 0.97],
        ],
        "regions": [],
        "spans": [
            [[30, 30, 370, 50], "before the table"],
            [[70, 285, 330, 315], "E = mc^2"],
            [[30, 350, 370, 380], "after the formula"],
        ],
        "tables": ["<table><tr><td>cell</td></tr></table>"],
    },
    # A short centered text under an image becomes vision_footnote during
    # ordering: relabelled, expanded as a child, and skipped by order_index.
    "vision_footnote_relabel": {
        "threshold": 0.5,
        "layout": [
            ["image", [60.4, 110.2, 340.6, 360.8], 0.95],
            ["text", [150.3, 366.4, 250.7, 380.2], 0.9],
            ["text", [20.5, 420.6, 380.3, 560.9], 0.97],
        ],
        "regions": [],
        "spans": [
            [[160, 368, 240, 378], "Fig. 1"],
            [[30, 430, 370, 460], "body text after the figure"],
        ],
        "tables": [],
    },
    # Header and footnote are mask labels: own supplementary regions, ordered
    # with the page, but excluded from order_index by the default ignore list.
    "masked_labels_no_order_index": {
        "threshold": 0.5,
        "layout": [
            ["header", [20.2, 5.4, 380.6, 25.8], 0.9],
            ["text", [20.4, 60.2, 380.8, 500.6], 0.97],
            ["footnote", [20.6, 520.3, 380.2, 560.7], 0.9],
        ],
        "regions": [],
        "spans": [
            [[30, 8, 370, 22], "running header"],
            [[30, 70, 370, 100], "body"],
            [[30, 525, 370, 555], "a footnote"],
        ],
        "tables": [],
    },
    # Two detected regions: the nested ordering decides the region order and
    # the blocks flatten region by region.
    "two_detected_regions": {
        "threshold": 0.5,
        "layout": [
            ["text", [20.3, 20.6, 190.8, 280.4], 0.97],
            ["text", [210.5, 20.2, 380.9, 280.7], 0.97],
            ["text", [20.7, 320.4, 380.5, 560.2], 0.97],
        ],
        "regions": [
            ["Region", [15.2, 15.6, 390.4, 290.8], 0.9],
            ["Region", [15.8, 310.2, 390.6, 570.4], 0.9],
        ],
        "spans": [
            [[30, 30, 180, 60], "left column"],
            [[220, 30, 370, 60], "right column"],
            [[30, 330, 370, 360], "bottom band"],
        ],
        "tables": [],
    },
    # A table-labelled block with an empty table result list falls through to
    # update_text_content with no matched spans: empty content, no crash.
    "table_without_result": {
        "threshold": 0.5,
        "layout": [
            ["table", [40.2, 40.8, 360.6, 220.3], 0.95],
            ["text", [20.4, 240.6, 380.8, 400.2], 0.97],
        ],
        "regions": [],
        "spans": [
            [[30, 250, 370, 280], "text under the bare table"],
        ],
        "tables": [],
    },
}


def run_case(spec):
    layout_det_res = {
        "boxes": [
            {"label": label, "coordinate": [float(v) for v in box], "score": score}
            for label, box, score in spec["layout"]
        ]
    }
    region_det_res = {
        "boxes": [
            {"label": label, "coordinate": [float(v) for v in box], "score": score}
            for label, box, score in spec["regions"]
        ]
    }
    spans = spec["spans"]
    overall_ocr_res = {
        "dt_polys": [
            [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])] for b, _ in spans
        ],
        "rec_polys": [
            [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])] for b, _ in spans
        ],
        "rec_boxes": np.array([b for b, _ in spans], dtype=np.float64).reshape(-1, 4),
        "rec_texts": [text for _, text in spans],
        "rec_scores": [0.9] * len(spans),
        "rec_labels": ["text"] * len(spans),
    }
    table_res_list = [{"pred_html": html} for html in spec["tables"]]
    image = np.zeros((IMAGE_HEIGHT, IMAGE_WIDTH, 3), dtype=np.uint8)
    stub = StubRecognizer()

    region_block_ocr_idx_map, region_det_res, layout_det_res = (
        _LayoutParsingPipelineV2.standardized_data(
            DummySelf(),
            image=image,
            region_det_res=region_det_res,
            layout_det_res=layout_det_res,
            overall_ocr_res=overall_ocr_res,
            formula_res_list=[],
            text_rec_model=stub,
            text_rec_score_thresh=spec["threshold"],
        )
    )

    page = _LayoutParsingPipelineV2.get_layout_parsing_objects(
        DummySelf(),
        image=image,
        region_block_ocr_idx_map=region_block_ocr_idx_map,
        region_det_res=region_det_res,
        overall_ocr_res=overall_ocr_res,
        layout_det_res=layout_det_res,
        table_res_list=table_res_list,
        seal_res_list=[],
        chart_res_list=[],
        text_rec_model=stub,
        text_rec_score_thresh=spec["threshold"],
    )

    parsing_res_list = _LayoutParsingPipelineV2.sort_layout_parsing_blocks(
        DummySelf(), page
    )

    order_index = 1
    visualize_order_labels = [
        label
        for label in BLOCK_LABEL_MAP["visualize_index_labels"]
        if label not in MARKDOWN_IGNORE_LABELS
    ]
    for index, block in enumerate(parsing_res_list):
        block.index = index
        if block.label in visualize_order_labels:
            block.order_index = order_index
            order_index += 1

    finite = lambda v: float(v) if math.isfinite(float(v)) else None
    blocks = []
    for block in parsing_res_list:
        blocks.append(
            {
                "label": block.label,
                "bbox": [int(v) for v in block.bbox],
                "content": block.content,
                "index": int(block.index),
                "order_index": (
                    None if block.order_index is None else int(block.order_index)
                ),
                "num_of_lines": int(block.num_of_lines),
                "direction": block.direction,
                "seg_start_coordinate": finite(block.seg_start_coordinate),
                "seg_end_coordinate": finite(block.seg_end_coordinate),
                "text_line_height": float(block.text_line_height),
                "text_line_width": float(block.text_line_width),
                "image_path": None if block.image is None else block.image["path"],
            }
        )
    return {"blocks": blocks, "model_calls": stub.calls}


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    cases = []
    for name, spec in CASES.items():
        record = {
            "case": name,
            "threshold": spec["threshold"],
            "image_size": [IMAGE_WIDTH, IMAGE_HEIGHT],
            "layout": spec["layout"],
            "regions": spec["regions"],
            "spans": spec["spans"],
            "tables": spec["tables"],
            "markdown_ignore_labels": MARKDOWN_IGNORE_LABELS,
        }
        record.update(run_case(spec))
        cases.append(record)

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/layout_parsing/"
        "pipeline_v2.py standardized_data + get_layout_parsing_objects + "
        "sort_layout_parsing_blocks + the order_index tail",
        "cases": cases,
    }
    output.write_text(
        json.dumps(document, ensure_ascii=False, indent=1, sort_keys=True) + "\n"
    )
    print(f"wrote {output} ({len(cases)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
