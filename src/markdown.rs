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

#[cfg(test)]
mod tests {
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
