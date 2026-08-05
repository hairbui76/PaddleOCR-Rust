#!/usr/bin/env python3
"""Capture `standardized_data` end to end from the pinned paddlex.

Roadmap item: `STRUCT-001`, the orchestration slice (phase C).

`standardized_data` is the stage of `get_layout_parsing_res` that reconciles
layout detections, region detections, and OCR spans: overlap dedup, the
footnote and lone-title relabels, hurdle-span re-recognition, no-text block
re-recognition, the OCR fallback when layout found nothing, and the
region-to-block matching with enclosing-box growth, supplement regions, and
per-mask-label supplementary regions.

The two model-call sites are exercised through a deterministic stub whose
output depends only on the crop shape — `text = "rec-{h}x{w}"`,
`score = ((h*31 + w*7) % 97) / 96` — so the Rust port can mirror it exactly
behind its `TextRecognizer` trait. The image is zeros; only its shape matters.
The stub's call log is recorded to pin the model-call order.

Executed, not transcribed. Needs `numpy` and `paddlex` 3.7.2.

Usage:
    python3 tools/capture_standardized_data_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.pipelines.layout_parsing.pipeline_v2 import (
    _LayoutParsingPipelineV2,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/standardized-data-oracle-capture/v1"

IMAGE_WIDTH = 400
IMAGE_HEIGHT = 600


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
    """standardized_data never touches self when the threshold is explicit."""


# Each case: layout boxes, region boxes, ocr spans (box, text), threshold.
CASES = {
    # Footnote above the lowest text is relabelled; the one below stays; no
    # region boxes, so one SupplementaryRegion holds every block.
    "labels_and_no_regions": {
        "threshold": 0.5,
        "layout": [
            ["footnote", [20, 20, 380, 45], 0.9],
            ["text", [20, 60, 380, 200], 0.95],
            ["text", [20, 220, 380, 560], 0.95],
            ["footnote", [20, 570, 380, 595], 0.9],
        ],
        "regions": [],
        "spans": [
            [[30, 70, 370, 90], "first block text"],
            [[30, 230, 370, 250], "second block text"],
            [[30, 25, 370, 42], "footnote above"],
            [[30, 572, 370, 592], "footnote below"],
        ],
    },
    # A lone large paragraph title with no doc title is promoted; header and
    # seal are mask labels: excluded from region matching, granted their own
    # supplementary regions at the end.
    "title_promotion_and_masks": {
        "threshold": 0.5,
        "layout": [
            ["header", [20, 5, 380, 25], 0.9],
            ["paragraph_title", [50, 40, 350, 180], 0.95],
            ["text", [20, 200, 380, 320], 0.95],
            ["seal", [300, 500, 380, 580], 0.9],
        ],
        "regions": [
            ["Region", [15, 30, 390, 330], 0.9],
        ],
        "spans": [
            [[60, 60, 340, 160], "big lone title"],
            [[30, 210, 370, 300], "body text"],
            [[310, 510, 370, 570], "seal text"],
            [[30, 8, 370, 22], "running header"],
        ],
    },
    # One OCR span crossing two blocks is re-recognized per block: the first
    # crop replaces it, the second appends; a second crossing span whose stub
    # score falls below the threshold is blanked and never restored.
    "hurdle_replace_append": {
        "threshold": 0.5,
        "layout": [
            ["text", [20, 20, 190, 400], 0.95],
            ["text", [210, 20, 380, 400], 0.95],
        ],
        "regions": [
            ["Region", [15, 15, 390, 410], 0.9],
        ],
        "spans": [
            [[30, 40, 180, 60], "inside first block"],
            [[100, 80, 300, 100], "crosses both blocks"],
            [[95, 300, 305, 330], "crosses below threshold"],
        ],
    },
    # A block whose only span is empty text and a block with no spans are
    # re-recognized from the layout box; an image block is not; a block whose
    # stub score is below the threshold gains nothing.
    "no_text_rerecognition": {
        "threshold": 0.3,
        "layout": [
            ["text", [20, 20, 200, 60], 0.95],
            ["image", [220, 20, 380, 200], 0.9],
            ["text", [20, 100, 240, 180], 0.95],
            ["text", [20, 300, 120, 340], 0.95],
        ],
        "regions": [
            ["Region", [15, 15, 390, 350], 0.9],
        ],
        "spans": [
            [[30, 30, 190, 50], ""],
        ],
    },
    # No layout detections at all: every OCR span becomes a text block and
    # the supplementary region covers their union.
    "ocr_fallback": {
        "threshold": 0.5,
        "layout": [],
        "regions": [],
        "spans": [
            [[30, 30, 200, 50], "line one"],
            [[30, 70, 220, 90], "line two"],
            [[30, 110, 180, 130], "line three"],
        ],
    },
    # Region growth pulls in a block only reachable through the enclosing
    # box (two fixpoint rounds); leftover blocks build a supplement region
    # that overlaps the grown region and is shrunk; an empty region matches
    # nothing; the header gets its own supplementary region.
    "region_expansion_and_shrink": {
        "threshold": 0.5,
        "layout": [
            ["text", [20, 20, 180, 100], 0.95],
            ["text", [60, 120, 140, 380], 0.95],
            ["text", [20, 300, 180, 400], 0.95],
            ["text", [250, 20, 380, 200], 0.95],
            ["text", [250, 220, 380, 400], 0.95],
            ["text", [150, 420, 380, 500], 0.95],
            ["header", [20, 540, 380, 560], 0.9],
        ],
        "regions": [
            ["Region", [300, 570, 340, 590], 0.8],
            ["Region", [15, 15, 200, 300], 0.9],
        ],
        "spans": [
            [[30, 30, 170, 90], "col one a"],
            [[70, 130, 130, 370], "col one b"],
            [[30, 310, 170, 390], "col one c"],
            [[260, 30, 370, 190], "col two a"],
            [[260, 230, 370, 390], "col two b"],
            [[160, 430, 370, 490], "bottom strip"],
            [[30, 542, 370, 558], "running header"],
        ],
    },
}


def normalize_poly(poly):
    return [[float(x), float(y)] for x, y in poly]


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

    return {
        "layout_boxes": [
            {
                "label": b["label"],
                "coordinate": [float(v) for v in b["coordinate"]],
                "score": float(b["score"]),
            }
            for b in layout_det_res["boxes"]
        ],
        "region_boxes": [
            {
                "label": b["label"],
                "coordinate": [float(v) for v in b["coordinate"]],
                "score": float(b["score"]),
            }
            for b in region_det_res["boxes"]
        ],
        "region_to_block_map": {
            str(k): [int(i) for i in v]
            for k, v in region_block_ocr_idx_map["region_to_block_map"].items()
        },
        "block_to_ocr_map": {
            str(k): [int(i) for i in v]
            for k, v in region_block_ocr_idx_map["block_to_ocr_map"].items()
        },
        "ocr": {
            "dt_polys": [normalize_poly(p) for p in overall_ocr_res["dt_polys"]],
            "rec_polys": [normalize_poly(p) for p in overall_ocr_res["rec_polys"]],
            "rec_boxes": [
                [float(v) for v in b] for b in overall_ocr_res["rec_boxes"]
            ],
            "rec_texts": list(overall_ocr_res["rec_texts"]),
            "rec_scores": [float(s) for s in overall_ocr_res["rec_scores"]],
            "rec_labels": list(overall_ocr_res["rec_labels"]),
        },
        "model_calls": stub.calls,
    }


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
        }
        record.update(run_case(spec))
        cases.append(record)

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/pipelines/layout_parsing/"
        "pipeline_v2.py standardized_data",
        "cases": cases,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(cases)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
