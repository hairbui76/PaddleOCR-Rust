// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Markdown reconstruction: the per-label formatters.
//!
//! Roadmap item `RECON-001`, first slice.
//!
//! `PP-StructureV3` rebuilds a document by mapping each ordered layout block
//! through a **per-label formatter**. Most of those formatters are pure string
//! functions — no model, no image, no artifact — so they are capturable and
//! matchable exactly, the same property that made
//! [`crate::reading_order`] portable ahead of the heuristics above it.
//!
//! # Heading level comes from dots, not from nesting
//!
//! `format_title` reassembles a numbered title, strips trailing periods, and
//! then counts the **remaining dots**:
//!
//! | Content | Markdown |
//! |---|---|
//! | `1 Introduction` | `## 1 Introduction` |
//! | `1.2 Methods` | `### 1.2 Methods` |
//! | `1.2.3 Results` | `#### 1.2.3 Results` |
//! | `A.B.C lettered` | `#### A.B.C lettered` |
//!
//! The last row is the one to notice: `A.B.C` is not numbering the pattern
//! recognises, so it is left alone — and then its dots are counted anyway,
//! making it a third-level heading. That is upstream's behaviour and it is
//! reproduced rather than corrected.
//!
//! `#{'#' * level}` means a level-`1` title emits **two** hashes. The name and
//! the output disagree by one, which matters to anyone comparing this port's
//! documents with upstream's.
//!
//! # No regular-expression dependency
//!
//! The numbering pattern is a hand-written matcher rather than a regex crate:
//! this project has two dependencies and adding a third to parse four
//! alternatives would be a poor trade. The alternatives are tried in the source
//! order, which is what Python's alternation does, and the sixteen captured
//! cases are what check it.
//!
//! # What is not here
//!
//! The image, chart, formula, and seal formatters. They depend on P8 modules
//! with **no published ONNX export**, so a port of them would have nothing to
//! check against — see `docs/P8_ARTIFACT_AVAILABILITY.md`.
#![allow(dead_code)]

/// The CJK numerals the numbering pattern accepts.
const CJK_NUMERALS: [char; 21] = [
    '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '百', '千', '万', '亿', '零', '壹',
    '贰', '叁', '肆', '伍', '陆',
];

/// The remaining CJK numerals, kept separate only to stay within one line each.
const CJK_NUMERALS_TAIL: [char; 4] = ['柒', '捌', '玖', '拾'];

/// Roman numerals, in the source's alternation order.
///
/// The order is load-bearing: Python's alternation is leftmost-first, so `I` is
/// tried before `II`, and only the trailing `.`-or-space requirement makes the
/// longer forms reachable.
const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

fn is_cjk_numeral(character: char) -> bool {
    CJK_NUMERALS.contains(&character) || CJK_NUMERALS_TAIL.contains(&character)
}

/// Matches the numbering prefix, returning its byte length within `rest`.
///
/// `rest` must already have had leading whitespace removed. Returns `None` when
/// no alternative matches, which is upstream's "leave the title alone" case.
fn match_numbering(rest: &str) -> Option<usize> {
    arabic_numbering(rest)
        .or_else(|| parenthesised_numbering(rest))
        .or_else(|| cjk_numbering(rest))
        .or_else(|| roman_numbering(rest))
}

/// `[1-9][0-9]*(?:\.[1-9][0-9]*)*[\.、]?`
fn arabic_numbering(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut index = 0;
    if !matches!(bytes.first(), Some(b'1'..=b'9')) {
        return None;
    }
    index += 1;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    // Each further group must be a full `.<digits>`; a trailing lone dot is the
    // optional suffix below, not a group.
    loop {
        if bytes.get(index) != Some(&b'.') {
            break;
        }
        let mut lookahead = index + 1;
        if !matches!(bytes.get(lookahead), Some(b'1'..=b'9')) {
            break;
        }
        lookahead += 1;
        while matches!(bytes.get(lookahead), Some(b'0'..=b'9')) {
            lookahead += 1;
        }
        index = lookahead;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
    } else if rest[index..].starts_with('、') {
        index += '、'.len_utf8();
    }
    Some(index)
}

/// `[\(\（](?:[1-9][0-9]*|[CJK]+)[\)\）]`
fn parenthesised_numbering(rest: &str) -> Option<usize> {
    let mut characters = rest.char_indices();
    let (_, open) = characters.next()?;
    if open != '(' && open != '（' {
        return None;
    }
    let mut index = open.len_utf8();
    let inner = &rest[index..];
    let mut consumed = 0;
    let mut inner_characters = inner.chars();
    match inner_characters.next()? {
        digit @ '1'..='9' => {
            consumed += digit.len_utf8();
            for character in inner_characters {
                if character.is_ascii_digit() {
                    consumed += character.len_utf8();
                } else {
                    break;
                }
            }
        }
        numeral if is_cjk_numeral(numeral) => {
            consumed += numeral.len_utf8();
            for character in inner_characters {
                if is_cjk_numeral(character) {
                    consumed += character.len_utf8();
                } else {
                    break;
                }
            }
        }
        _ => return None,
    }
    index += consumed;
    let close = rest[index..].chars().next()?;
    if close != ')' && close != '）' {
        return None;
    }
    Some(index + close.len_utf8())
}

/// `[CJK]+[、\.]?`
fn cjk_numbering(rest: &str) -> Option<usize> {
    let mut index = 0;
    for character in rest.chars() {
        if is_cjk_numeral(character) {
            index += character.len_utf8();
        } else {
            break;
        }
    }
    if index == 0 {
        return None;
    }
    if rest[index..].starts_with('.') {
        index += 1;
    } else if rest[index..].starts_with('、') {
        index += '、'.len_utf8();
    }
    Some(index)
}

/// `(?:I|II|III|IV|V|VI|VII|VIII|IX|X)(?:\.|\s)`
fn roman_numbering(rest: &str) -> Option<usize> {
    for numeral in ROMAN {
        let Some(after) = rest.strip_prefix(numeral) else {
            continue;
        };
        let Some(next) = after.chars().next() else {
            continue;
        };
        if next == '.' || next.is_whitespace() {
            return Some(numeral.len() + next.len_utf8());
        }
    }
    None
}

/// Joins soft line wraps: `-\n` disappears, any other newline becomes a space.
#[must_use]
pub(crate) fn collapse_soft_newlines(content: &str) -> String {
    content.replace("-\n", "").replace('\n', " ")
}

/// Turns single newlines into blank lines, leaving existing blanks alone.
///
/// `"\n\n" -> "\n"` runs **first**, so a paragraph break survives as one blank
/// line rather than becoming two. The order is upstream's and reversing it
/// doubles every blank.
#[must_use]
pub(crate) fn normalize_newlines(content: &str) -> String {
    content.replace("\n\n", "\n").replace('\n', "\n\n")
}

/// Formats a numbered or unnumbered title, deriving its level from dots.
#[must_use]
pub(crate) fn format_title(content: &str) -> String {
    let trimmed = content.trim_start_matches(|character: char| character.is_whitespace());
    let mut title = content.to_owned();
    if let Some(length) = match_numbering(trimmed) {
        let numbering = trimmed[..length].trim();
        // `(\s*)(.*)` then `.lstrip()`: the separator collapses to one space and
        // trailing whitespace on the content survives.
        let remainder = trimmed[length..].trim_start();
        title = format!("{numbering} {remainder}");
    }
    let title = title.trim_end_matches('.');
    let level = if title.contains('.') {
        title.matches('.').count() + 1
    } else {
        1
    };
    collapse_soft_newlines(&format!("#{} {title}", "#".repeat(level)))
}

/// Formats a paragraph title, using an explicit level when the block has one.
#[must_use]
pub(crate) fn format_paragraph_title(content: &str, title_level: Option<usize>) -> String {
    match title_level {
        None => format_title(content),
        Some(level) => collapse_soft_newlines(&format!("#{} {content}", "#".repeat(level))),
    }
}

/// Reformats the first non-empty field when it matches one of `templates`.
///
/// Splits on `splitter`, examines fields until it finds a non-empty one,
/// rewrites it if it matches, and **stops either way** — the loop breaks on the
/// first non-empty field whether or not it matched.
#[must_use]
pub(crate) fn format_first_line(
    content: &str,
    templates: &[&str],
    prefix: &str,
    suffix: &str,
    splitter: &str,
) -> String {
    let mut fields: Vec<String> = content.split(splitter).map(str::to_owned).collect();
    for field in &mut fields {
        if field.trim().is_empty() {
            continue;
        }
        if templates.contains(&field.to_lowercase().as_str()) {
            *field = format!("{prefix}{field}{suffix}");
        }
        break;
    }
    fields.join(splitter)
}

/// Strips the `<html>` and `<body>` wrapper from table HTML.
///
/// Only those four tags, by literal replacement.
///
/// **It prepends a newline, and so does its caller.** Upstream's documented
/// call site is `simplify_table("\n" + block.content)` and the function itself
/// returns `"\n" + ...`, so a table in a reconstructed document is preceded by
/// **two** newlines, not one. That is a blank line before every table, and it
/// is reproduced rather than tidied: the caller supplies its own newline, this
/// function supplies the other, and the captured oracle shows both.
#[must_use]
pub(crate) fn simplify_table(table_html: &str) -> String {
    let stripped = table_html
        .replace("<html>", "")
        .replace("</html>", "")
        .replace("<body>", "")
        .replace("</body>", "");
    format!("\n{stripped}")
}

/// One ordered block of a parsed document: a label and its text content.
#[derive(Clone, Debug)]
pub struct DocumentBlock {
    /// The layout label, deciding which formatter runs.
    pub label: String,
    /// The block's recognized text.
    pub content: String,
}

/// The geometry `paragraph_continues` reads, in page coordinates.
///
/// `seg_*` are the first and last **text segment** edges inside the block,
/// where `start`/`end` are the block's own; the difference between them is
/// what distinguishes a full last line from a short one.
#[derive(Clone, Copy, Debug)]
pub struct BlockGeometry {
    /// The block's left edge.
    pub start: f64,
    /// The block's right edge.
    pub end: f64,
    /// The first text segment's left edge.
    pub seg_start: f64,
    /// The last text segment's right edge.
    pub seg_end: f64,
    /// How many text lines the block holds.
    pub lines: u32,
    /// The block's width.
    pub width: f64,
}

/// Upstream's `get_seg_flag`: does this block continue the previous paragraph?
///
/// Returns `(starts_new_segment, ends_segment)`. A `false` start is what makes
/// [`assemble_markdown`] join two `text` blocks without a separator. The `10`
/// pixel tolerances and the multi-line requirement are upstream's own.
#[must_use]
pub fn paragraph_continues(block: BlockGeometry, previous: Option<BlockGeometry>) -> (bool, bool) {
    let mut start_flag = true;
    let mut context_left = block.start;
    let mut context_right = block.end;

    match previous {
        Some(prev) => {
            let mut prev_end_space_small = (prev.end - prev.seg_end).abs() < 10.0;
            let overlap = context_left < prev.end && context_right > prev.start;
            let edge_distance = if overlap {
                context_left = prev.start.min(context_left);
                context_right = prev.end.max(context_right);
                prev_end_space_small = (context_right - prev.seg_end).abs() < 10.0;
                0.0
            } else {
                (block.start - prev.end).abs()
            };
            let current_start_space_small = block.seg_start - context_left < 10.0;
            if prev_end_space_small
                && current_start_space_small
                && prev.lines > 1
                && edge_distance < prev.width.max(block.width)
            {
                start_flag = false;
            }
        }
        None => {
            if block.seg_start - context_left < 10.0 {
                start_flag = false;
            }
        }
    }
    let end_flag = context_right - block.seg_end >= 10.0;
    (start_flag, end_flag)
}

/// `merge_formula_and_number`: a formula and its equation number, as LaTeX.
///
/// The `$$` wrapper is stripped before re-wrapping, so an already-wrapped and
/// a bare formula come out identically.
#[must_use]
pub fn merge_formula_and_number(formula: &str, number: &str) -> String {
    let inner = formula.replace("$$", "");
    let inner = inner.trim();
    format!("$${inner} \\tag*{{{number}}}$$")
}

/// Formats one block through the label map this port implements.
///
/// Returns `None` for a label with no handler — image, chart, formula, and
/// seal are artifact-blocked, and an unknown label is upstream's own skip.
#[must_use]
pub fn format_block(block: &DocumentBlock) -> Option<String> {
    let content = block.content.as_str();
    Some(match block.label.as_str() {
        "paragraph_title" => format_paragraph_title(content, None),
        "abstract_title" | "reference_title" | "content_title" => format_title(content),
        "doc_title" => collapse_soft_newlines(&format!("# {content}")),
        "table_title" | "figure_title" | "chart_title" | "vision_footnote" | "text" | "ocr"
        | "vertical_text" | "reference_content" => normalize_newlines(content),
        "abstract" => format_first_line(content, &["摘要", "abstract"], "## ", "\n", " "),
        "content" => content.replace("-\n", "  \n").replace('\n', "  \n"),
        // The reference handler splits on newlines, so a heading is only
        // rewritten when it sits alone on the first line.
        "reference" => format_first_line(content, &["参考文献", "references"], "## ", "", "\n"),
        _ => return None,
    })
}

/// Assembles ordered blocks into one Markdown document.
///
/// Upstream's `MarkdownConverter.convert`: each block is formatted by its
/// label's handler and appended with a blank line between blocks — except that
/// a `text` block directly after a `text` block whose geometry says the
/// paragraph continues is appended with **no** separator. Blocks with no
/// handler contribute nothing, including no separator.
#[must_use]
pub fn assemble_markdown(blocks: &[(DocumentBlock, Option<BlockGeometry>)]) -> String {
    let mut markdown = String::new();
    let mut last_label: Option<&str> = None;
    let mut previous_geometry: Option<BlockGeometry> = None;

    for (block, geometry) in blocks {
        let Some(formatted) = format_block(block) else {
            continue;
        };
        let continues = match geometry {
            Some(geometry) => {
                let (start, _) = paragraph_continues(*geometry, previous_geometry);
                previous_geometry = Some(*geometry);
                !start
            }
            None => false,
        };
        // A continuing text paragraph and the document's first block both
        // append bare; the cases are distinct upstream and share a body here.
        let joined_text = block.label == "text" && last_label == Some("text") && continues;
        if !joined_text && !markdown.is_empty() {
            markdown.push_str("\n\n");
        }
        markdown.push_str(&formatted);
        last_label = Some(block.label.as_str());
    }
    markdown
}

#[cfg(test)]
mod tests {
    mod assembly {
        use super::super::*;

        use serde_json::Value;

        const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-assembly/expected.json");

        fn fixture() -> Value {
            match serde_json::from_str(FIXTURE) {
                Ok(value) => value,
                Err(error) => panic!("fixture: {error}"),
            }
        }

        /// Every captured document, byte for byte.
        #[test]
        fn the_captured_assemblies_are_reproduced() {
            let fixture = fixture();
            let cases = match fixture["assembly"].as_array() {
                Some(value) => value,
                None => panic!("assembly"),
            };
            assert_eq!(cases.len(), 5);
            for case in cases {
                let name = case["case"].as_str().unwrap_or("?");
                let blocks: Vec<(DocumentBlock, Option<BlockGeometry>)> =
                    match case["blocks"].as_array() {
                        Some(values) => values
                            .iter()
                            .map(|entry| {
                                let entry = match entry.as_array() {
                                    Some(value) => value,
                                    None => panic!("{name}: block"),
                                };
                                (
                                    DocumentBlock {
                                        label: entry[0].as_str().unwrap_or_default().to_owned(),
                                        content: entry[1].as_str().unwrap_or_default().to_owned(),
                                    },
                                    None,
                                )
                            })
                            .collect(),
                        None => panic!("{name}: blocks"),
                    };
                assert_eq!(
                    assemble_markdown(&blocks),
                    case["markdown"].as_str().unwrap_or_default(),
                    "{name}"
                );
            }
        }

        /// The continuity rule joins or separates exactly as captured.
        #[test]
        fn the_captured_continuity_is_reproduced() {
            let fixture = fixture();
            let cases = match fixture["continuity"].as_array() {
                Some(value) => value,
                None => panic!("continuity"),
            };
            assert_eq!(cases.len(), 2);
            for case in cases {
                let name = case["case"].as_str().unwrap_or("?");
                let geometry = |key: &str| -> BlockGeometry {
                    let g = &case[key];
                    BlockGeometry {
                        start: g["start_coordinate"].as_f64().unwrap_or(0.0),
                        end: g["end_coordinate"].as_f64().unwrap_or(0.0),
                        seg_start: g["seg_start_coordinate"].as_f64().unwrap_or(0.0),
                        seg_end: g["seg_end_coordinate"].as_f64().unwrap_or(0.0),
                        lines: g["num_of_lines"].as_u64().unwrap_or(1) as u32,
                        width: g["width"].as_f64().unwrap_or(0.0),
                    }
                };
                let (first, second) = (geometry("first"), geometry("second"));
                let (start, end) = paragraph_continues(second, Some(first));
                assert_eq!(
                    start,
                    case["seg_start"].as_bool().unwrap_or(true),
                    "{name}: start flag"
                );
                assert_eq!(
                    end,
                    case["seg_end"].as_bool().unwrap_or(true),
                    "{name}: end flag"
                );

                let blocks = vec![
                    (
                        DocumentBlock {
                            label: "text".to_owned(),
                            content: "first para".to_owned(),
                        },
                        Some(first),
                    ),
                    (
                        DocumentBlock {
                            label: "text".to_owned(),
                            content: "second para".to_owned(),
                        },
                        Some(second),
                    ),
                ];
                assert_eq!(
                    assemble_markdown(&blocks),
                    case["markdown"].as_str().unwrap_or_default(),
                    "{name}: assembled"
                );
            }
        }

        /// The captured formula merges, byte for byte.
        #[test]
        fn the_captured_formula_merges_are_reproduced() {
            let fixture = fixture();
            let cases = match fixture["formula_merges"].as_array() {
                Some(value) => value,
                None => panic!("formula_merges"),
            };
            assert_eq!(cases.len(), 3);
            for case in cases {
                assert_eq!(
                    merge_formula_and_number(
                        case["formula"].as_str().unwrap_or_default(),
                        case["number"].as_str().unwrap_or_default(),
                    ),
                    case["merged"].as_str().unwrap_or_default()
                );
            }
        }

        /// A label with no handler contributes nothing, including no separator.
        #[test]
        fn an_unhandled_label_leaves_no_trace() {
            let blocks = vec![
                (
                    DocumentBlock {
                        label: "text".to_owned(),
                        content: "Before.".to_owned(),
                    },
                    None,
                ),
                (
                    DocumentBlock {
                        label: "image".to_owned(),
                        content: "artifact-blocked".to_owned(),
                    },
                    None,
                ),
                (
                    DocumentBlock {
                        label: "text".to_owned(),
                        content: "After.".to_owned(),
                    },
                    None,
                ),
            ];
            assert_eq!(assemble_markdown(&blocks), "Before.\n\nAfter.");
        }
    }

    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-markdown/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn cases<'a>(fixture: &'a Value, key: &str) -> &'a Vec<Value> {
        match fixture[key].as_array() {
            Some(value) => value,
            None => panic!("{key}"),
        }
    }

    #[test]
    fn the_captured_titles_are_reproduced() {
        let fixture = fixture();
        let entries = cases(&fixture, "titles");
        assert_eq!(entries.len(), 16);
        for entry in entries {
            let content = entry["content"].as_str().unwrap_or_default();
            let expected = entry["markdown"].as_str().unwrap_or_default();
            assert_eq!(format_title(content), expected, "{content:?}");
        }
    }

    #[test]
    fn the_captured_paragraph_titles_are_reproduced() {
        let fixture = fixture();
        let entries = cases(&fixture, "para_titles");
        assert_eq!(entries.len(), 4);
        for entry in entries {
            let content = entry["content"].as_str().unwrap_or_default();
            let level = entry["title_level"].as_u64().map(|value| value as usize);
            let expected = entry["markdown"].as_str().unwrap_or_default();
            assert_eq!(
                format_paragraph_title(content, level),
                expected,
                "{content:?} at {level:?}"
            );
        }
    }

    #[test]
    fn the_captured_first_lines_are_reproduced() {
        let fixture = fixture();
        let entries = cases(&fixture, "first_lines");
        assert_eq!(entries.len(), 7);
        for entry in entries {
            let content = entry["content"].as_str().unwrap_or_default();
            let templates: Vec<String> = match entry["templates"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_str().unwrap_or_default().to_owned())
                    .collect(),
                None => panic!("templates"),
            };
            let borrowed: Vec<&str> = templates.iter().map(String::as_str).collect();
            let template = entry["template"].as_str().unwrap_or_default();
            let (prefix, suffix) = match template.split_once("{}") {
                Some(parts) => parts,
                None => panic!("template must hold a placeholder"),
            };
            let splitter = entry["splitter"].as_str().unwrap_or(" ");
            let expected = entry["markdown"].as_str().unwrap_or_default();
            assert_eq!(
                format_first_line(content, &borrowed, prefix, suffix, splitter),
                expected,
                "{content:?}"
            );
        }
    }

    #[test]
    fn the_captured_newline_handling_is_reproduced() {
        let fixture = fixture();
        for entry in cases(&fixture, "collapse_soft_newlines") {
            let content = entry["content"].as_str().unwrap_or_default();
            let expected = entry["markdown"].as_str().unwrap_or_default();
            assert_eq!(collapse_soft_newlines(content), expected, "{content:?}");
        }
        for entry in cases(&fixture, "normalize_newlines") {
            let content = entry["content"].as_str().unwrap_or_default();
            let expected = entry["markdown"].as_str().unwrap_or_default();
            assert_eq!(normalize_newlines(content), expected, "{content:?}");
        }
    }

    #[test]
    fn the_captured_table_simplification_is_reproduced() {
        let fixture = fixture();
        let entries = cases(&fixture, "tables");
        assert_eq!(entries.len(), 4);
        for entry in entries {
            let content = entry["content"].as_str().unwrap_or_default();
            let expected = entry["markdown"].as_str().unwrap_or_default();
            // Upstream's call site prepends its own newline before calling,
            // and the function prepends another. Both are reproduced.
            assert_eq!(
                simplify_table(&format!("\n{content}")),
                expected,
                "{content:?}"
            );
        }
    }

    /// A level-one heading emits two hashes, which the name does not suggest.
    #[test]
    fn a_level_one_title_emits_two_hashes() {
        assert_eq!(format_title("Introduction"), "## Introduction");
        assert_eq!(format_paragraph_title("Methods", Some(1)), "## Methods");
    }

    /// Dots are counted even when they are not numbering.
    ///
    /// `A.B.C` is not a numbering form the pattern accepts, so it survives
    /// untouched — and then becomes a third-level heading anyway.
    #[test]
    fn dots_that_are_not_numbering_still_set_the_level() {
        assert_eq!(format_title("A.B.C lettered"), "#### A.B.C lettered");
        assert!(match_numbering("A.B.C lettered").is_none());
    }

    /// `normalize_newlines` collapses before it expands.
    ///
    /// Reversing the two replacements turns one paragraph break into two, which
    /// is silent in a rendered document and obvious in a diff.
    #[test]
    fn a_paragraph_break_survives_as_one_blank_line() {
        assert_eq!(normalize_newlines("para\n\nbreak"), "para\n\nbreak");
        assert_eq!(normalize_newlines("two\nlines"), "two\n\nlines");
    }

    /// The roman alternation's order is load-bearing.
    #[test]
    fn roman_numbering_needs_a_dot_or_space_after_it() {
        assert_eq!(roman_numbering("IV. Discussion"), Some(3));
        assert_eq!(roman_numbering("IV Discussion"), Some(3));
        // `IVORY` is not `IV` followed by a separator, so nothing matches.
        assert_eq!(roman_numbering("IVORY tower"), None);
    }

    /// A field that is non-empty but does not match still stops the scan.
    #[test]
    fn the_scan_stops_at_the_first_non_empty_field() {
        // The second field would match, but the first non-empty one does not,
        // and the loop breaks there.
        assert_eq!(
            format_first_line("intro abstract", &["abstract"], "## ", "", " "),
            "intro abstract"
        );
    }

    /// The newline is doubled at the real call site, and that is upstream's.
    #[test]
    fn a_table_is_preceded_by_two_newlines_in_a_document() {
        let content = "<html><body><table></table></body></html>";
        // What `result_v2` actually does.
        assert!(
            simplify_table(&format!("\n{content}")).starts_with("\n\n"),
            "a blank line precedes every reconstructed table"
        );
    }

    /// Only the four wrapper tags are stripped.
    #[test]
    fn table_simplification_leaves_other_markup_alone() {
        assert_eq!(
            simplify_table("<html><body><table><td>a</td></table></body></html>"),
            "\n<table><td>a</td></table>"
        );
        assert_eq!(simplify_table("<div>kept</div>"), "\n<div>kept</div>");
    }
}
