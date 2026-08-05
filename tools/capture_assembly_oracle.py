#!/usr/bin/env python3
"""Capture the table-structure metric and the Markdown document assembly.

Roadmap items: `METRIC-001` (table half) and `RECON-001` (assembly half).

Both were recorded as blocked, and neither is. The table-structure metric
scores token sequences — `TBLSTRUCT-001` is done, so the module it scores
exists. And `MarkdownConverter.convert` is pure iteration over `(label,
content)` blocks: the **formatters** for image/chart/formula/seal are blocked on
artifacts, but the assembly that dispatches to formatters is not.

Executed, not transcribed:

    TableStructureMetric   "".join(tokens) exact match; eps 1e-6 (not 1e-5!)
    MarkdownConverter      per-label dispatch, the "\\n\\n" join, the text-
                           continuity exception, unknown labels skipped
    get_seg_flag           paragraph continuity from block geometry
    merge_formula_and_number

Needs `numpy` and `paddlex` 3.7.2 for the converter, and the pinned checkout
for the metric. Nothing is downloaded and no model is run.

Usage:
    python3 tools/capture_assembly_oracle.py <output.json>
"""

from __future__ import annotations

import importlib.util
import json
import sys
import types
from pathlib import Path

CHECKOUT = Path(__file__).resolve().parent.parent / "PaddleOCR"
CAPTURE_SCHEMA_VERSION = "paddleocr-rust/assembly-oracle-capture/v1"


def load_table_metric():
    """Loads `TableStructureMetric` from the pinned file, executed.

    Only the structure metric is needed; `table_metric.py`'s import of
    `DetMetric` (whose bbox scoring this port already implements from its own
    capture) is dropped rather than chasing the relative-import chain, because
    `TableStructureMetric` never touches it.
    """
    path = CHECKOUT / "ppocr" / "metrics" / "table_metric.py"
    text = path.read_text().replace(
        "from ppocr.metrics.det_metric import DetMetric", "DetMetric = None"
    )
    module = types.ModuleType("_table_metric")
    module.__file__ = str(path)
    exec(compile(text, str(path), "exec"), module.__dict__)
    return module.TableStructureMetric


from paddlex.inference.common.result.converter.markdown_converter import (  # noqa: E402
    MarkdownConverter,
)
from paddlex.inference.common.result.converter.markdown_format_funcs import (  # noqa: E402
    _format_normalize_newlines,
    build_handle_funcs_dict,
    format_first_line,
    merge_formula_and_number,
)
from paddlex.inference.pipelines.layout_parsing.utils import get_seg_flag  # noqa: E402


class Block:
    """The DocumentBlock protocol: label, content, bbox, image, geometry."""

    def __init__(self, label, content, **geometry):
        self.label = label
        self.content = content
        self.bbox = geometry.get("bbox", [0, 0, 10, 10])
        self.image = None
        for key, value in geometry.items():
            setattr(self, key, value)


STRUCTURE_CASES = [
    ("identical", ["<tr>", "<td></td>", "</tr>"], ["<tr>", "<td></td>", "</tr>"], False),
    ("differs", ["<tr>", "<td></td>", "</tr>"], ["<tr>", "<td></td>", "<td></td>", "</tr>"], False),
    # These two "different" token lists concatenate to the SAME string, so the
    # metric counts them correct. Upstream's comparison, pinned as a hazard.
    ("concat_hides_a_difference", ["<tr>", "<td></td>", "</tr>"], ["<tr>", "<td>", "</td>", "</tr>"], False),
    # Concatenation can make different token lists equal: this is upstream's
    # comparison, pinned rather than improved.
    ("concat_equal", ["<tr>", "<td>"], ["<tr><td>"], False),
    ("thead_stripped", ["<thead>", "<tr>", "</tr>", "</thead>"], ["<tr>", "</tr>"], True),
    ("thead_kept", ["<thead>", "<tr>", "</tr>", "</thead>"], ["<tr>", "</tr>"], False),
    ("both_empty", [], [], False),
]

# Assembly blocks: only labels whose formatters this port implements, plus one
# unknown label to pin the skip.
ASSEMBLY_CASES = {
    "plain_document": [
        ("doc_title", "A Study of Things"),
        ("text", "First paragraph."),
        ("text", "Second paragraph."),
        ("paragraph_title", "1.2 Methods"),
        ("text", "Body."),
    ],
    "unknown_label_is_skipped": [
        ("text", "Before."),
        ("mystery_label", "This has no handler."),
        ("text", "After."),
    ],
    "abstract_and_reference": [
        ("abstract", "abstract This paper does things."),
        ("reference", "references [1] someone"),
    ],
    "empty_document": [],
    "single_block": [("text", "Only.")],
}


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    TableStructureMetric = load_table_metric()
    structure = []
    for name, pred, target, strip in STRUCTURE_CASES:
        metric = TableStructureMetric(del_thead_tbody=strip)
        metric(({"structure_batch_list": [(pred, 1.0)]}, {"structure_batch_list": [target]}))
        result = metric.get_metric()
        structure.append(
            {
                "case": name,
                "prediction": pred,
                "target": target,
                "del_thead_tbody": strip,
                "acc": float(result["acc"]),
            }
        )
    # The accumulation across a corpus, with the eps in the denominator.
    metric = TableStructureMetric()
    for name, pred, target, strip in STRUCTURE_CASES:
        if strip:
            continue
        metric(({"structure_batch_list": [(pred, 1.0)]}, {"structure_batch_list": [target]}))
    corpus_acc = float(metric.get_metric()["acc"])

    # Handlers restricted to what this port implements; image/chart/formula/seal
    # are artifact-blocked and deliberately absent.
    handlers = build_handle_funcs_dict(
        text_func=_format_normalize_newlines,
        image_func=lambda block: "",
        chart_func=lambda block: "",
        table_func=lambda block: "",
        formula_func=lambda block: "",
        seal_func=lambda block: "",
    )
    for blocked in ("image", "chart", "table", "formula", "display_formula",
                    "inline_formula", "seal", "header_image", "footer_image",
                    "aside_text", "seal_text", "header", "footer", "number"):
        handlers.pop(blocked, None)

    assembly = []
    for name, blocks in ASSEMBLY_CASES.items():
        result = MarkdownConverter.convert(
            [Block(label, content) for label, content in blocks],
            handle_funcs_dict=handlers,
        )
        assembly.append(
            {
                "case": name,
                "blocks": [[label, content] for label, content in blocks],
                "markdown": result["markdown_texts"],
            }
        )

    # Continuity: consecutive text blocks joined without a separator when the
    # geometry says the paragraph continues.
    continuity = []
    for name, geometry_a, geometry_b in [
        # Second line starts at the left margin and the first ends mid-line:
        # a continuing paragraph.
        (
            "continues",
            dict(bbox=[0, 0, 100, 10], start_coordinate=0, end_coordinate=100,
                 seg_start_coordinate=0, seg_end_coordinate=95, num_of_lines=2,
                 width=100),
            dict(bbox=[0, 12, 100, 22], start_coordinate=0, end_coordinate=100,
                 seg_start_coordinate=0, seg_end_coordinate=40, num_of_lines=1,
                 width=100),
        ),
        # Second line is indented: a new paragraph.
        (
            "new_paragraph",
            dict(bbox=[0, 0, 100, 10], start_coordinate=0, end_coordinate=100,
                 seg_start_coordinate=0, seg_end_coordinate=40, num_of_lines=1,
                 width=100),
            dict(bbox=[0, 12, 100, 22], start_coordinate=0, end_coordinate=100,
                 seg_start_coordinate=20, seg_end_coordinate=90, num_of_lines=1,
                 width=100),
        ),
    ]:
        first = Block("text", "first para", **geometry_a)
        second = Block("text", "second para", **geometry_b)
        start, end = get_seg_flag(second, first)
        joined = MarkdownConverter.convert(
            [first, second],
            handle_funcs_dict=handlers,
            use_seg_flag=True,
            get_seg_flag_func=get_seg_flag,
        )
        continuity.append(
            {
                "case": name,
                "first": geometry_a,
                "second": geometry_b,
                "seg_start": bool(start),
                "seg_end": bool(end),
                "markdown": joined["markdown_texts"],
            }
        )

    formula_merges = [
        {
            "formula": formula,
            "number": number,
            "merged": merge_formula_and_number(formula, number),
        }
        for formula, number in [
            ("$$E = mc^2$$", "(1)"),
            ("E = mc^2", "(2)"),
            ("$$x$$", ""),
        ]
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "PaddleOCR ppocr/metrics/table_metric.py; paddlex 3.7.2 "
        "inference/common/result/converter/markdown_converter.py and "
        "inference/pipelines/layout_parsing/utils.py get_seg_flag",
        "structure_metric": structure,
        "structure_metric_corpus": {"acc": corpus_acc, "eps": 1e-6},
        "assembly": assembly,
        "continuity": continuity,
        "formula_merges": formula_merges,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(
        f"wrote {output} ({len(structure)} metric, {len(assembly)} assembly, "
        f"{len(continuity)} continuity)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
