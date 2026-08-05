#!/usr/bin/env python3
"""Capture `LayoutBlock.update_text_content` from the pinned paddlex.

Roadmap item: `STRUCT-001`, the orchestration slice (phase B).

This is the machinery that turns OCR spans into a block's content and the
statistics the ordering layer consumes: `group_boxes_into_lines` (projection
IoU at `0.6`, the vertical tall-line filter), `format_line` (span sorting by
half-pixel cells, hyphen and English-letter spacing, the `need_new_line`
geometry), the paragraph-joining rules in `update_text_content` (indent-gap
newlines, the `>0.5`/`>0.6` new-line voting), the per-label delimiter map,
and the derived `num_of_lines`, `seg_start/end_coordinate`, and text-line
means.

Every span here is labelled `text`, so the text-recognition model is never
reached — that path exists only for `formula` spans, which this port's scope
(formula recognition off) never produces. Executed, not transcribed.

Usage:
    python3 tools/capture_text_lines_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.layout_objects import LayoutBlock

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/text-lines-oracle-capture/v1"


# Each case: (name, label, block bbox, spans as (box, text)).
CASES = [
    (
        "english_paragraph",
        "text",
        [10, 10, 310, 70],
        [
            ([12, 12, 300, 26], "The quick brown fox jumps"),
            ([12, 32, 305, 46], "over the lazy dog and keeps"),
            ([12, 52, 150, 66], "going."),
        ],
    ),
    (
        "multi_span_line",
        "text",
        [10, 10, 310, 50],
        [
            ([12, 12, 120, 26], "Left part"),
            ([130, 13, 300, 27], "right part"),
            ([12, 32, 200, 46], "second line"),
        ],
    ),
    (
        "cjk_paragraph",
        "text",
        [10, 10, 310, 70],
        [
            ([12, 12, 300, 26], "第一行的中文内容测试"),
            ([12, 32, 308, 46], "第二行内容继续下去，"),
            ([12, 52, 100, 66], "结束。"),
        ],
    ),
    (
        "hyphenated_line",
        "text",
        [10, 10, 210, 50],
        [
            ([12, 12, 205, 26], "hyphen-"),
            ([12, 32, 100, 46], "ation"),
        ],
    ),
    (
        "short_last_line_breaks",
        "text",
        [10, 10, 400, 70],
        [
            ([12, 12, 395, 26], "a full width first line of text here"),
            ([12, 32, 80, 46], "stub"),
            ([12, 52, 390, 66], "and a full width third line again ok"),
        ],
    ),
    (
        "indented_continuation",
        "text",
        [10, 10, 400, 70],
        [
            ([12, 12, 200, 26], "ends early,"),
            ([150, 32, 395, 46], "deeply indented line"),
        ],
    ),
    (
        "reference_block",
        "reference",
        [10, 10, 400, 70],
        [
            ([40, 12, 395, 26], "[1] someone, somewhere"),
            ([40, 32, 300, 46], "[2] someone else"),
        ],
    ),
    (
        "doc_title_delimiter",
        "doc_title",
        [10, 10, 400, 70],
        [
            ([12, 12, 395, 40], "A Study"),
            ([12, 42, 200, 68], "of Things"),
        ],
    ),
    (
        "content_delimiter",
        "content",
        [10, 10, 400, 70],
        [
            ([12, 12, 395, 26], "Chapter 1 .......... 1"),
            ([12, 32, 395, 46], "Chapter 2 .......... 9"),
        ],
    ),
    (
        "vertical_block",
        "text",
        [10, 10, 70, 310],
        [
            ([40, 12, 56, 300], "縦書きの一行目"),
            ([14, 12, 30, 280], "二行目の内容"),
        ],
    ),
    (
        "vertical_tall_line_filtered",
        "text",
        [10, 10, 100, 310],
        [
            ([70, 12, 86, 300], "一"),
            ([44, 12, 60, 290], "二"),
            ([14, 12, 30, 60], "三"),
        ],
    ),
    (
        "empty_spans",
        "text",
        [10, 10, 100, 40],
        [],
    ),
    # A single line only ever sets seg_start: upstream's last-line branch is
    # an `elif`, so seg_end keeps its -inf initial value. Pinned, not fixed.
    (
        "single_line",
        "text",
        [10, 10, 400, 40],
        [([12, 12, 200, 26], "only line here")],
    ),
    (
        "numeric_tail_keeps_line",
        "text",
        [10, 10, 400, 50],
        [
            ([12, 12, 200, 26], "total 42"),
            ([12, 32, 390, 46], "continues on the next line."),
        ],
    ),
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    records = []
    for name, label, bbox, spans in CASES:
        block = LayoutBlock(label, bbox)
        rec_res = {
            "boxes": [np.array(box, dtype=np.float64) for box, _ in spans],
            "rec_texts": [text for _, text in spans],
            "rec_labels": ["text"] * len(spans),
        }
        block.update_text_content(
            image=None, ocr_rec_res=rec_res, text_rec_model=None
        )
        # `seg_*` stay at their infinite initial values when no span arrives;
        # JSON cannot carry infinities, so they are recorded as null.
        finite = lambda v: float(v) if np.isfinite(v) else None
        records.append(
            {
                "case": name,
                "label": label,
                "bbox": bbox,
                "spans": [[list(map(float, box)), text] for box, text in spans],
                "content": block.content,
                "num_of_lines": int(block.num_of_lines),
                "direction": block.direction,
                "seg_start_coordinate": finite(block.seg_start_coordinate),
                "seg_end_coordinate": finite(block.seg_end_coordinate),
                "text_line_height": float(block.text_line_height),
                "text_line_width": float(block.text_line_width),
            }
        )

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/layout_parsing/layout_objects.py "
        "LayoutBlock.update_text_content and the TextLine machinery beneath it",
        "cases": records,
    }
    output.write_text(json.dumps(document, ensure_ascii=False, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
