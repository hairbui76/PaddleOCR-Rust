#!/usr/bin/env python3
"""Capture the multipage Markdown concatenation and the cross-page text merge.

Roadmap item: `STRUCT-001`, the multipage unit.

`PP-StructureV3` joins per-page results into one document with two functions,
and neither of them renders, decodes, or infers anything:

    concatenate_markdown_pages(markdown_list)   pure: two keys per page
    merge_text_across_page(blocks_by_page)      pure: labels, contents, geometry

`concatenate_markdown_pages` reads only `page_continuation_flags` and
`markdown_texts` from each page, so its whole input space is a pair of booleans
and two strings. `merge_text_across_page` walks blocks and reuses `get_seg_flag`,
which this port already has as `paragraph_continues`.

That is why this capture exists even though `PDF-001` has no approved renderer.
A renderer is what makes multiple pages *reachable*; it is not what makes these
two functions *checkable*. The same separation let the XY-cut primitives be
frozen before any layout model ran.

Captured here, all executed from the pinned `paddlex` 3.7.2:

  * `concatenate_markdown_pages` over eight synthetic page lists that cover
    every branch: the paragraph break, the ASCII join, both CJK sides, an empty
    page, the leading separator on the first page, flag propagation across three
    pages, and the empty list.
  * `merge_text_across_page` over four real multipage block lists, built by
    running the executed post-model chain per page exactly as
    `capture_markdown_v2_oracle.py` does.

Needs `numpy`, `opencv`, and `paddlex` 3.7.2. Nothing is downloaded and no model
is run.

Usage:
    python3 tools/capture_multipage_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))

from capture_layout_assembly_oracle import (
    IMAGE_HEIGHT,
    IMAGE_WIDTH,
    MARKDOWN_IGNORE_LABELS,
    DummySelf,
    StubRecognizer,
)
from paddlex.inference.pipelines.layout_parsing.pipeline_v2 import (
    _LayoutParsingPipelineV2,
)
from paddlex.inference.pipelines.layout_parsing.setting import BLOCK_LABEL_MAP

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/multipage-oracle-capture/v1"

# ---------------------------------------------------------------------------
# Part A: concatenate_markdown_pages
# ---------------------------------------------------------------------------
#
# Each page is `[start_flag, end_flag, markdown_texts]`. The function reads
# nothing else, so these are complete inputs rather than stand-ins.
#
# The first page is the case worth naming: the loop seeds
# `previous_page_last_element_paragraph_end_flag = True`, so page one always
# takes the `else` branch and the document **starts with a blank line**. A
# reimplementation that joins with `"\n\n"` between pages and not before the
# first one would differ on every document ever produced.
CONCATENATION_CASES = {
    # Both pages end and start paragraphs: the plain blank-line join.
    "paragraph_break": [
        [True, True, "First page text."],
        [True, True, "Second page text."],
    ],
    # Page one ends mid-paragraph and page two starts mid-paragraph, both sides
    # ASCII: joined with a single space and no newline.
    "continuation_ascii": [
        [True, False, "a sentence that runs"],
        [False, True, "across the page break"],
    ],
    # Same continuation, but the accumulated text ends in a CJK character: no
    # space, because CJK does not word-separate.
    "continuation_cjk_left": [
        [True, False, "这是中文"],
        [False, True, "across the page break"],
    ],
    # The other side: the next page opens with a CJK character.
    "continuation_cjk_right": [
        [True, False, "a sentence that runs"],
        [False, True, "继续中文内容"],
    ],
    # A continuing page with no text at all. Both characters test as non-CJK,
    # so a bare space is appended — an edge a reasonable implementation would
    # guard against and upstream does not.
    "continuation_empty_page": [
        [True, False, "a sentence that runs"],
        [False, True, ""],
    ],
    # One page, to pin the leading separator on its own.
    "single_page": [[True, True, "Only page."]],
    # Three pages: the end flag of page two decides how page three joins, so
    # this checks the flag is carried rather than recomputed.
    "three_pages_mixed": [
        [True, False, "page one tail"],
        [False, False, "page two whole"],
        [False, True, "page three head"],
    ],
    # No pages at all.
    "empty_document": [],
}


# ---------------------------------------------------------------------------
# Part B: merge_text_across_page
# ---------------------------------------------------------------------------
#
# Real blocks, built per page by the executed chain. A page's first block is
# handed `prev_block=None`, and `get_seg_flag`'s `else` branch clears the start
# flag whenever the block's first segment begins within ten pixels of its own
# left edge — which is what makes a cross-page merge possible at all.
#
# Each page: threshold, layout boxes, ocr spans. Regions and tables stay empty;
# they belong to the single-page captures.
MERGE_CASES = {
    # Page one ends with a multi-line text block, page two opens with a text
    # block flush to its own left edge: the merge fires and page two loses the
    # block entirely.
    "text_runs_across_the_break": [
        {
            "threshold": 0.5,
            "layout": [
                ["doc_title", [60.2, 40.6, 340.8, 90.3], 0.98],
                ["text", [20.6, 160.2, 380.4, 400.8], 0.97],
            ],
            "spans": [
                [[70, 50, 330, 80], "The Title"],
                [[22, 170, 378, 200], "a paragraph that keeps"],
                [[22, 210, 300, 240], "going past the end of"],
            ],
        },
        {
            "threshold": 0.5,
            "layout": [["text", [20.4, 60.2, 380.6, 300.4], 0.96]],
            "spans": [
                [[22, 70, 378, 100], "this first page and onto"],
                [[22, 110, 250, 140], "the second one."],
            ],
        },
    ],
    # The same geometry, but page two's text opens with a CJK character, so the
    # merge joins with no separator.
    "cjk_across_the_break": [
        {
            "threshold": 0.5,
            "layout": [["text", [20.6, 160.2, 380.4, 400.8], 0.97]],
            "spans": [
                [[22, 170, 378, 200], "an English tail"],
                [[22, 210, 300, 240], "before the break"],
            ],
        },
        {
            "threshold": 0.5,
            "layout": [["text", [20.4, 60.2, 380.6, 300.4], 0.96]],
            "spans": [
                [[22, 70, 378, 100], "继续的中文内容"],
                [[22, 110, 250, 140], "第二行"],
            ],
        },
    ],
    # Page two opens with a title, not text: no merge, and the block survives.
    "title_blocks_the_merge": [
        {
            "threshold": 0.5,
            "layout": [["text", [20.6, 160.2, 380.4, 400.8], 0.97]],
            "spans": [
                [[22, 170, 378, 200], "a paragraph that keeps"],
                [[22, 210, 300, 240], "going past the end"],
            ],
        },
        {
            "threshold": 0.5,
            "layout": [
                ["paragraph_title", [20.4, 60.2, 380.6, 90.4], 0.95],
                ["text", [20.4, 120.2, 380.6, 300.4], 0.96],
            ],
            "spans": [
                [[22, 65, 378, 85], "A New Section"],
                [[22, 130, 378, 160], "with its own paragraph"],
            ],
        },
    ],
    # Three pages, so that `global_prev_block` is seen to survive a page whose
    # blocks were all merged away.
    "three_pages_running_text": [
        {
            "threshold": 0.5,
            "layout": [["text", [20.6, 160.2, 380.4, 400.8], 0.97]],
            "spans": [
                [[22, 170, 378, 200], "page one line one"],
                [[22, 210, 300, 240], "page one line two"],
            ],
        },
        {
            "threshold": 0.5,
            "layout": [["text", [20.4, 60.2, 380.6, 300.4], 0.96]],
            "spans": [
                [[22, 70, 378, 100], "page two line one"],
                [[22, 110, 250, 140], "page two line two"],
            ],
        },
        {
            "threshold": 0.5,
            "layout": [["text", [20.4, 60.2, 380.6, 300.4], 0.96]],
            "spans": [
                [[22, 70, 378, 100], "page three line one"],
                [[22, 110, 250, 140], "page three line two"],
            ],
        },
    ],
}


def build_page(spec):
    """Runs one page through the executed post-model chain."""
    layout_det_res = {
        "boxes": [
            {"label": label, "coordinate": [float(v) for v in box], "score": score}
            for label, box, score in spec["layout"]
        ]
    }
    region_det_res = {"boxes": []}
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
        table_res_list=[],
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
    return parsing_res_list


def record_blocks(blocks):
    return [
        {
            "label": block.label,
            "content": getattr(block, "content", ""),
            "group_id": int(block.group_id),
        }
        for block in blocks
    ]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    concatenations = []
    for name, pages in CONCATENATION_CASES.items():
        markdown_list = [
            {
                "page_continuation_flags": [start, end],
                "markdown_texts": text,
            }
            for start, end, text in pages
        ]
        result = _LayoutParsingPipelineV2.concatenate_markdown_pages(
            DummySelf(), markdown_list
        )
        concatenations.append(
            {
                "case": name,
                "pages": [
                    {"continuation_flags": [start, end], "markdown_texts": text}
                    for start, end, text in pages
                ],
                "markdown_texts": result["markdown_texts"],
            }
        )

    merges = []
    for name, page_specs in MERGE_CASES.items():
        blocks_by_page = [build_page(spec) for spec in page_specs]
        before = [len(page) for page in blocks_by_page]
        merged = _LayoutParsingPipelineV2.merge_text_across_page(
            DummySelf(), blocks_by_page
        )
        merges.append(
            {
                "case": name,
                "image_size": [IMAGE_WIDTH, IMAGE_HEIGHT],
                "pages": [
                    {
                        "threshold": spec["threshold"],
                        "layout": spec["layout"],
                        "spans": spec["spans"],
                    }
                    for spec in page_specs
                ],
                "block_counts_before_merge": before,
                "merged_pages": [record_blocks(page) for page in merged],
            }
        )

    # A capture that merged nothing would pass a reimplementation that never
    # merges, so refuse to write one.
    fired = [
        case["case"]
        for case in merges
        if sum(case["block_counts_before_merge"])
        > sum(len(page) for page in case["merged_pages"])
    ]
    if not fired:
        print("no case triggered a cross-page merge", file=sys.stderr)
        return 1

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": (
            "paddlex 3.7.2 layout_parsing/pipeline_v2.py "
            "concatenate_markdown_pages and merge_text_across_page"
        ),
        "cases_that_merged": fired,
        "concatenations": concatenations,
        "merges": merges,
    }
    output.write_text(
        json.dumps(document, ensure_ascii=False, indent=1, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        f"wrote {len(concatenations)} concatenation and {len(merges)} merge cases "
        f"({len(fired)} merged) to {output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
