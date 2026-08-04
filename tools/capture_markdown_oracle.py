#!/usr/bin/env python3
"""Capture the per-label Markdown formatters.

Roadmap item: `RECON-001`, first slice.

`PP-StructureV3` reconstructs a document by mapping each ordered layout block
through a **per-label formatter**. The map is `build_handle_funcs_dict`, and most
of its entries are pure string functions: no model, no image, no artifact.

Captured here, all executed from the pinned `paddlex` 3.7.2:

    format_title              numbering -> heading level
    format_para_title         an explicit level when the block carries one
    format_first_line         "abstract"/"references" -> a `##` heading
    simplify_table            strip the <html>/<body> wrapper
    _collapse_soft_newlines   hyphen-newline joins, newline -> space
    _format_normalize_newlines  newline -> blank line

The image, chart, formula, and seal handlers are **not** captured: they depend
on P8 modules that have no published ONNX export, so a port of them would have
nothing to check against. See `docs/P8_ARTIFACT_AVAILABILITY.md`.

Needs `paddlex` 3.7.2. Nothing is downloaded and no model is run.

Usage:
    python3 tools/capture_markdown_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from functools import partial
from pathlib import Path

from paddlex.inference.common.result.converter.markdown_format_funcs import (
    _collapse_soft_newlines,
    _format_normalize_newlines,
    format_first_line,
    format_para_title,
    format_title,
    simplify_table,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/markdown-oracle-capture/v1"


class Block:
    """The minimal shape the formatters read: `content`, and sometimes a level."""

    def __init__(self, content: str, title_level=None):
        self.content = content
        if title_level is not None:
            self.title_level = title_level


# Numbering forms the pattern accepts, plus forms it does not, because the
# level is derived from the dots that survive and getting that wrong silently
# changes a document's heading structure.
TITLE_CASES = [
    "1 Introduction",
    "1. Introduction",
    "1.2 Methods",
    "1.2.3 Results",
    "1.2.3. Results",
    "2、方法",
    "(1) First",
    "（一）第一",
    "一、绪论",
    "IV. Discussion",
    "Introduction",
    "  1.1   Spaced  ",
    "Trailing dots...",
    "A.B.C lettered",
    "10.20.30 deep",
    "Soft-\nwrapped title",
]

PARA_TITLE_CASES = [
    ("1.2 Methods", None),
    ("1.2 Methods", 1),
    ("Methods", 3),
    ("Methods", 6),
]

FIRST_LINE_CASES = [
    ("abstract", ["摘要", "abstract"], "## {}\n", " "),
    ("Abstract This paper", ["摘要", "abstract"], "## {}\n", " "),
    ("摘要 本文", ["摘要", "abstract"], "## {}\n", " "),
    ("  abstract", ["摘要", "abstract"], "## {}\n", " "),
    ("Introduction only", ["摘要", "abstract"], "## {}\n", " "),
    ("references", ["参考文献", "references"], "## {}", " "),
    ("参考文献 [1] a", ["参考文献", "references"], "## {}", " "),
]

NEWLINE_CASES = [
    "plain",
    "soft-\nwrapped",
    "two\nlines",
    "para\n\nbreak",
    "trailing\n",
    "",
    "multi\n\n\nblank",
]

TABLE_CASES = [
    "<html><body><table><tr><td>a</td></tr></table></body></html>",
    "<table><tr><td>bare</td></tr></table>",
    "<html><body></body></html>",
    "",
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    titles = [
        {"content": content, "markdown": format_title(Block(content))}
        for content in TITLE_CASES
    ]

    para_titles = []
    for content, level in PARA_TITLE_CASES:
        block = Block(content, title_level=level)
        para_titles.append(
            {
                "content": content,
                "title_level": level,
                "markdown": format_para_title(block),
            }
        )

    first_lines = []
    for content, templates, template, splitter in FIRST_LINE_CASES:
        handler = partial(
            format_first_line,
            templates=templates,
            format_func=lambda line, t=template: t.format(line),
            splitter=splitter,
        )
        first_lines.append(
            {
                "content": content,
                "templates": templates,
                "template": template,
                "splitter": splitter,
                "markdown": handler(Block(content)),
            }
        )

    collapses = [
        {"content": content, "markdown": _collapse_soft_newlines(content)}
        for content in NEWLINE_CASES
    ]
    normalizes = [
        {"content": content, "markdown": _format_normalize_newlines(Block(content))}
        for content in NEWLINE_CASES
    ]
    tables = [
        {"content": content, "markdown": simplify_table("\n" + content)}
        for content in TABLE_CASES
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "paddlex 3.7.2 inference/common/result/converter/"
        "markdown_format_funcs.py",
        "note": "The image, chart, formula, and seal handlers are deliberately absent: "
        "they depend on P8 modules with no published ONNX export, so a port of them "
        "would have nothing to check against.",
        "titles": titles,
        "para_titles": para_titles,
        "first_lines": first_lines,
        "collapse_soft_newlines": collapses,
        "normalize_newlines": normalizes,
        "tables": tables,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(
        f"wrote {output} ({len(titles)} titles, {len(para_titles)} para titles, "
        f"{len(first_lines)} first lines, {len(tables)} tables)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
