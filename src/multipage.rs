// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `STRUCT-001`'s multipage unit: joining per-page results into one document.
//!
//! Upstream does this with two functions in `pipeline_v2.py`, and neither of
//! them renders, decodes, or infers anything:
//!
//! - [`concatenate_markdown_pages`] reads exactly two things per page, the
//!   continuation flags and the Markdown text, so its entire input space is a
//!   pair of booleans and a string.
//! - [`merge_text_across_page`] walks blocks and reuses `get_seg_flag`, which
//!   this port already has as [`paragraph_continues`].
//!
//! So they are checkable now, while `PDF-001` has no approved renderer. A
//! renderer is what makes several pages *reachable*; it is not what makes these
//! functions *correct*. The XY-cut primitives were frozen before any layout
//! model ran for the same reason.
//!
//! # What the captures pinned
//!
//! Four behaviours a reasonable reimplementation gets wrong.
//!
//! **Every document starts with a blank line.** The loop seeds the previous
//! page's end flag to `true`, so the first page always takes the separator
//! branch and the result begins with `"\n\n"`. Joining *between* pages and not
//! before the first would differ on every document upstream has ever produced.
//!
//! **One CJK side is enough to drop the space.** The check is `or`, not `and`,
//! so a Chinese tail followed by English prose is joined with no separator.
//!
//! **An empty continuing page still contributes a space.** Neither character
//! tests as CJK when one of them does not exist, so upstream appends `" "` and
//! then nothing. This is the kind of edge an implementation would naturally
//! guard, which is exactly why it is captured.
//!
//! **The merge separator keys on the raw last character.** Block contents
//! already end in a space, so a merged paragraph gets *two* — upstream does not
//! trim, and neither does this.
//!
//! # Reach
//!
//! `concatenate_markdown_pages` is reached through `StructureEngine::parse_pdf`
//! under the `pdf` feature, which is what closed the gap these functions spent
//! their first day in: verified against captures, with no way for a caller to
//! hand them two pages. `merge_text_across_page` is still verified rather than
//! reached — joining paragraphs across a page break changes block content, and
//! doing that behind a caller's back is a decision the document API has not been
//! asked to make. The module allows dead code for that reason, the way
//! `markdown.rs` does; the fuzz driver exercises both under the `fuzzing`
//! feature.
#![allow(dead_code)]

use crate::markdown::paragraph_continues;
use crate::markdown_v2::{MarkdownPage, geometry_of};
use crate::structure_assembly::AssembledBlock;

/// One block that survived [`merge_text_across_page`].
///
/// Upstream sets `group_id` on the block object itself. Here it is a separate
/// field rather than a new member of [`AssembledBlock`], because it is a
/// property of a *document* assembly and means nothing to a single page.
#[derive(Clone, Debug, PartialEq)]
pub struct MergedBlock {
    /// The block, with any following pages' text already merged into it.
    pub block: AssembledBlock,
    /// Upstream's `group_id`: the running index across the whole document at
    /// which this block was first seen.
    pub group_id: u32,
}

/// Whether a character is in upstream's CJK range, `\u{4e00}`-`\u{9fff}`.
///
/// Upstream applies `re.match` to a one-character string, so this is a range
/// test and not a script property: the Japanese kana and the fullwidth forms
/// are outside it, and both would take the space.
fn is_cjk(character: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&character)
}

/// The separator upstream puts between two joined strings.
///
/// A single space unless either facing character is CJK. `previous` losing its
/// last character or `next` its first — an empty string on either side — is not
/// a CJK match either, so the space still appears; only
/// [`merge_text_across_page`] additionally requires both to exist.
fn joining_space(previous: Option<char>, next: Option<char>) -> bool {
    !(previous.is_some_and(is_cjk) || next.is_some_and(is_cjk))
}

/// Upstream's `concatenate_markdown_pages`: per-page Markdown into a document.
///
/// A page whose first block does not start a paragraph, following a page whose
/// last block did not end one, is joined inline — with a space unless either
/// side of the join is CJK. Every other page is separated by a blank line,
/// including the first, which is why the result begins with `"\n\n"`.
#[must_use]
pub fn concatenate_markdown_pages(pages: &[MarkdownPage]) -> String {
    let mut text = String::new();
    let mut previous_page_ended_paragraph = true;

    for page in pages {
        let (starts_paragraph, ends_paragraph) = page.continuation_flags;
        if !starts_paragraph && !previous_page_ended_paragraph {
            if joining_space(text.chars().next_back(), page.markdown.chars().next()) {
                text.push(' ');
            }
        } else {
            text.push_str("\n\n");
        }
        text.push_str(&page.markdown);
        previous_page_ended_paragraph = ends_paragraph;
    }

    text
}

/// Upstream's `merge_text_across_page`: a paragraph that runs past a page break.
///
/// A `text` block whose start flag is clear, following a `text` block anywhere
/// earlier in the document, has its content appended to that block and is
/// **dropped from its own page** — which is why a page can come back empty.
///
/// Two details carry the behaviour. The start flag is computed against the
/// previous block *on the same page*, so a page's first block is measured
/// against nothing and `get_seg_flag`'s no-previous branch decides it: the flag
/// clears whenever the block's first text segment begins within ten pixels of
/// the block's own leading edge. And the block absorbing the text is the last
/// *surviving* one, so several consecutive pages can collapse into a single
/// paragraph.
#[must_use]
pub fn merge_text_across_page(pages: &[Vec<AssembledBlock>]) -> Vec<Vec<MergedBlock>> {
    let mut merged: Vec<Vec<MergedBlock>> = Vec::with_capacity(pages.len());
    // The last surviving block, as (page, position) into `merged`. Upstream
    // holds the object itself; an index is the same thing without aliasing.
    let mut previous_surviving: Option<(usize, usize)> = None;
    let mut group_id = 0_u32;

    for page in pages {
        merged.push(Vec::new());
        let page_index = merged.len() - 1;
        let mut previous_on_page = None;

        for block in page {
            let geometry = geometry_of(block);
            let (starts_paragraph, _) = paragraph_continues(geometry, previous_on_page);
            previous_on_page = Some(geometry);

            let previous_was_text = previous_surviving
                .is_some_and(|(page, position)| merged[page][position].block.label == "text");

            if block.label == "text" && previous_was_text && !starts_paragraph {
                let (previous_page, position) = match previous_surviving {
                    Some(location) => location,
                    // Unreachable: `previous_was_text` is false without it.
                    None => unreachable!("previous_was_text implies a location"),
                };
                let target = &mut merged[previous_page][position].block.content;
                let last = target.chars().next_back();
                let first = block.content.chars().next();
                if last.is_some() && first.is_some() && joining_space(last, first) {
                    target.push(' ');
                }
                target.push_str(&block.content);
            } else {
                merged[page_index].push(MergedBlock {
                    block: block.clone(),
                    group_id,
                });
                previous_surviving = Some((page_index, merged[page_index].len() - 1));
            }

            group_id += 1;
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    use crate::structure_assembly::assemble_layout_parsing;
    use crate::structure_glue::{GlueBlock, GlueBox};
    use crate::structure_standardize::{OcrData, TextRecognizer, standardized_data};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-multipage/expected.json");

    /// The shape-keyed stub shared with every assembly fixture.
    struct StubRecognizer;

    impl TextRecognizer for StubRecognizer {
        fn recognize(&mut self, crop: [i64; 4]) -> (String, f64) {
            let height = crop[3] - crop[1];
            let width = crop[2] - crop[0];
            let score = ((height * 31 + width * 7) % 97) as f64 / 96.0;
            (format!("rec-{height}x{width}"), score)
        }
    }

    fn items<'a>(value: &'a Value, what: &str) -> &'a [Value] {
        match value.as_array() {
            Some(values) => values,
            None => panic!("fixture field {what} is not an array"),
        }
    }

    fn read_box(value: &Value) -> GlueBox {
        let b = items(value, "box");
        [
            b[0].as_f64().unwrap_or(0.0),
            b[1].as_f64().unwrap_or(0.0),
            b[2].as_f64().unwrap_or(0.0),
            b[3].as_f64().unwrap_or(0.0),
        ]
    }

    fn read_blocks(value: &Value) -> Vec<GlueBlock> {
        items(value, "blocks")
            .iter()
            .map(|spec| {
                let spec = items(spec, "block spec");
                GlueBlock {
                    label: spec[0].as_str().unwrap_or("").to_owned(),
                    coordinate: read_box(&spec[1]),
                    score: spec[2].as_f64().unwrap_or(0.0),
                }
            })
            .collect()
    }

    fn box_corners(bbox: GlueBox) -> [[f64; 2]; 4] {
        [
            [bbox[0], bbox[1]],
            [bbox[2], bbox[1]],
            [bbox[2], bbox[3]],
            [bbox[0], bbox[3]],
        ]
    }

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    /// Builds one page's assembled blocks from its captured spec.
    fn build_page(spec: &Value, width: f64, height: f64) -> Vec<AssembledBlock> {
        let threshold = spec["threshold"].as_f64().unwrap_or(0.0);
        let layout = read_blocks(&spec["layout"]);
        let mut ocr = OcrData::default();
        for span in items(&spec["spans"], "spans") {
            let span = items(span, "span");
            let bbox = read_box(&span[0]);
            ocr.rec_boxes.push(bbox);
            ocr.dt_polys.push(box_corners(bbox));
            ocr.rec_polys.push(box_corners(bbox));
            ocr.rec_texts
                .push(span[1].as_str().unwrap_or("").to_owned());
            ocr.rec_scores.push(0.9);
            ocr.rec_labels.push("text".to_owned());
        }
        let mut recognizer = StubRecognizer;
        let standardized =
            standardized_data(width, height, &layout, &[], ocr, &mut recognizer, threshold);
        assemble_layout_parsing(&standardized, &[], &[])
    }

    /// The captured concatenations, byte for byte.
    #[test]
    fn the_captured_concatenations_are_reproduced() {
        let fixture = fixture();
        let cases = items(&fixture["concatenations"], "concatenations");
        assert_eq!(cases.len(), 8);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let pages: Vec<MarkdownPage> = items(&case["pages"], "pages")
                .iter()
                .map(|page| {
                    let flags = items(&page["continuation_flags"], "flags");
                    MarkdownPage {
                        markdown: page["markdown_texts"].as_str().unwrap_or("").to_owned(),
                        image_paths: Vec::new(),
                        continuation_flags: (
                            flags[0].as_bool().unwrap_or(false),
                            flags[1].as_bool().unwrap_or(false),
                        ),
                    }
                })
                .collect();
            assert_eq!(
                concatenate_markdown_pages(&pages),
                case["markdown_texts"].as_str().unwrap_or("\u{0}"),
                "{name}"
            );
        }
    }

    /// The captured cross-page merges: surviving blocks, contents, group ids.
    #[test]
    fn the_captured_cross_page_merges_are_reproduced() {
        let fixture = fixture();
        let cases = items(&fixture["merges"], "merges");
        assert_eq!(cases.len(), 4);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let size = items(&case["image_size"], "image_size");
            let width = size[0].as_f64().unwrap_or(0.0);
            let height = size[1].as_f64().unwrap_or(0.0);

            let pages: Vec<Vec<AssembledBlock>> = items(&case["pages"], "pages")
                .iter()
                .map(|spec| build_page(spec, width, height))
                .collect();

            let before: Vec<usize> = items(&case["block_counts_before_merge"], "counts")
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect();
            let actual_before: Vec<usize> = pages.iter().map(Vec::len).collect();
            assert_eq!(actual_before, before, "{name}: block counts before merge");

            let merged = merge_text_across_page(&pages);
            let expected_pages = items(&case["merged_pages"], "merged pages");
            assert_eq!(merged.len(), expected_pages.len(), "{name}: page count");

            for (index, (page, expected)) in merged.iter().zip(expected_pages).enumerate() {
                let expected = items(expected, "page blocks");
                assert_eq!(
                    page.len(),
                    expected.len(),
                    "{name}/page{index}: block count"
                );
                for (block, expected) in page.iter().zip(expected) {
                    assert_eq!(
                        block.block.label,
                        expected["label"].as_str().unwrap_or("\u{0}"),
                        "{name}/page{index}: label"
                    );
                    assert_eq!(
                        block.block.content,
                        expected["content"].as_str().unwrap_or("\u{0}"),
                        "{name}/page{index}: content"
                    );
                    assert_eq!(
                        u64::from(block.group_id),
                        expected["group_id"].as_u64().unwrap_or(u64::MAX),
                        "{name}/page{index}: group id"
                    );
                }
            }
        }
    }

    /// At least one captured case actually merged.
    ///
    /// Without this, an implementation that never merges anything would pass
    /// every assertion above for three of the four cases and fail none.
    #[test]
    fn the_capture_contains_a_merge_that_fired() {
        let fixture = fixture();
        let fired = items(&fixture["cases_that_merged"], "cases_that_merged");
        assert!(!fired.is_empty(), "no captured case merged");

        let cases = items(&fixture["merges"], "merges");
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            if !fired.iter().any(|value| value.as_str() == Some(name)) {
                continue;
            }
            let before: usize = items(&case["block_counts_before_merge"], "counts")
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .sum();
            let after: usize = items(&case["merged_pages"], "pages")
                .iter()
                .map(|page| items(page, "blocks").len())
                .sum();
            assert!(
                after < before,
                "{name} is listed as merged but kept every block"
            );
        }
    }

    /// An empty document concatenates to the empty string, not to `"\n\n"`.
    #[test]
    fn no_pages_produce_no_text() {
        assert_eq!(concatenate_markdown_pages(&[]), "");
    }

    /// Merging no pages yields no pages, and one empty page stays one page.
    #[test]
    fn merging_degenerate_page_lists_is_bounded() {
        assert!(merge_text_across_page(&[]).is_empty());
        let merged = merge_text_across_page(&[Vec::new(), Vec::new()]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(Vec::is_empty));
    }
}
