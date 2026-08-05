#!/usr/bin/env python3
"""Capture `LayoutParsingResultV2._to_markdown` from the pinned paddlex.

Roadmap item: `STRUCT-001`, the orchestration slice (phase D).

Runs the same six scenarios as `capture_layout_assembly_oracle.py` through the
executed post-model chain (`standardized_data`, `get_layout_parsing_objects`,
`sort_layout_parsing_blocks`, the `order_index` tail), wraps each page in a
real `LayoutParsingResultV2`, and records `_to_markdown` for both `pretty`
variants: the Markdown text, the page continuation flags, and the collected
image paths. Model settings are this port's supported mode - table
recognition on, formula/seal/chart recognition off, the default
`markdown_ignore_labels`.

Executed, not transcribed. Needs `numpy`, `opencv`, and `paddlex` 3.7.2.

Usage:
    python3 tools/capture_markdown_v2_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))

from capture_layout_assembly_oracle import (
    CASES,
    IMAGE_HEIGHT,
    IMAGE_WIDTH,
    MARKDOWN_IGNORE_LABELS,
    DummySelf,
    StubRecognizer,
)
from paddlex.inference.pipelines.layout_parsing.pipeline_v2 import (
    _LayoutParsingPipelineV2,
)
from paddlex.inference.pipelines.layout_parsing.result_v2 import (
    LayoutParsingResultV2,
)
from paddlex.inference.pipelines.layout_parsing.setting import BLOCK_LABEL_MAP

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/markdown-v2-oracle-capture/v1"

MODEL_SETTINGS = {
    "use_table_recognition": True,
    "use_formula_recognition": False,
    "use_seal_recognition": False,
    "use_chart_recognition": False,
    "markdown_ignore_labels": MARKDOWN_IGNORE_LABELS,
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

    result = LayoutParsingResultV2(
        {
            "parsing_res_list": parsing_res_list,
            "doc_preprocessor_res": {"output_img": image},
            "model_settings": dict(MODEL_SETTINGS),
            "imgs_in_doc": [],
            "page_index": None,
            "input_path": None,
        }
    )

    variants = {}
    for pretty in (True, False):
        markdown = result._to_markdown(pretty=pretty)
        start_flag, end_flag = markdown["page_continuation_flags"]
        variants["pretty" if pretty else "plain"] = {
            "markdown_texts": markdown["markdown_texts"],
            "page_continuation_flags": [bool(start_flag), bool(end_flag)],
            "image_paths": list(markdown["markdown_images"].keys()),
        }
    return variants


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
        "upstream": "paddlex 3.7.2 layout_parsing/result_v2.py _to_markdown over "
        "common/result/converter/{markdown_converter,markdown_format_funcs}.py",
        "cases": cases,
    }
    output.write_text(
        json.dumps(document, ensure_ascii=False, indent=1, sort_keys=True) + "\n"
    )
    print(f"wrote {output} ({len(cases)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
