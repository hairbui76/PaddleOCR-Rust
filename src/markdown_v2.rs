// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The StructureV3 Markdown page: `LayoutParsingResultV2._to_markdown`.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase D. The
//! [`crate::markdown`] module holds the label formatters `RECON-001` pinned;
//! this module holds what `result_v2` builds *around* them for the mode this
//! port supports — formula, seal, and chart recognition off, table
//! recognition selectable, and both `pretty` variants:
//!
//! - the handler map of `build_handle_funcs_dict` with
//!   `use_plain_header_footer_image=True` and the pipeline's
//!   `markdown_ignore_labels` popped out;
//! - the pretty-mode HTML wrappers — centered `<div>`s, the width-scaled
//!   `<img>` tag (`int()`-truncated percentage of the page width), and the
//!   `<table border="1">` rewrite — and their plain counterparts;
//! - `MarkdownConverter.convert` with `use_seg_flag=True`: the `get_seg_flag`
//!   continuity join for consecutive `text` blocks, the page continuation
//!   flags, and image collection.
//!
//! Quirks preserved on purpose:
//!
//! - Image paths are collected for **every** block that has one, whether or
//!   not its label has a handler.
//! - The continuity flags are computed for every block too, so the page's
//!   trailing `seg_end` flag can come from an unhandled block.
//! - An image-labelled block without an image formats to the empty string but
//!   still participates: the separator is appended and `last_label` moves.
//! - With formula, seal, and chart recognition off, their labels format
//!   through the image handler — an image reference, not content.
#![allow(dead_code)]

use crate::layout_order::Dir;
use crate::markdown::{
    BlockGeometry, collapse_soft_newlines, format_first_line, format_paragraph_title, format_title,
    normalize_newlines, paragraph_continues, simplify_table,
};
use crate::structure_assembly::AssembledBlock;

/// How a page is rendered.
#[derive(Clone, Copy, Debug)]
pub struct MarkdownOptions<'a> {
    /// Upstream's `pretty`: HTML-centered titles, scaled `<img>` tags, and
    /// bordered tables, versus plain Markdown.
    pub pretty: bool,
    /// Whether table recognition ran; off routes tables to the image handler.
    pub use_table_recognition: bool,
    /// The page image's pixel width, scaling pretty-mode images.
    pub original_image_width: i64,
    /// Labels excluded from the handler map.
    pub markdown_ignore_labels: &'a [&'a str],
}

/// One converted page.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownPage {
    /// The assembled Markdown text.
    pub markdown: String,
    /// Every image path the page references, in first-seen order.
    pub image_paths: Vec<String>,
    /// `(page_first_element_seg_start_flag, seg_end_flag)`: whether the first
    /// block starts a fresh paragraph and the last ends one, for cross-page
    /// merging.
    pub continuation_flags: (bool, bool),
}

/// `format_centered_by_html` with its default newline collapse.
fn centered(content: &str) -> String {
    let collapsed = collapse_soft_newlines(content);
    format!("<div style=\"text-align: center;\">{collapsed}</div>\n")
}

/// The `text_func` of `_build_handle_funcs_dict`.
fn text_func(content: &str, pretty: bool) -> String {
    if pretty {
        centered(content)
    } else {
        content.to_owned()
    }
}

/// The `image_func`: scaled HTML when pretty, a Markdown reference when not,
/// and the empty string for a block with no image.
fn image_func(block: &AssembledBlock, pretty: bool, original_image_width: i64) -> String {
    let Some(path) = &block.image_path else {
        return String::new();
    };
    if pretty {
        let image_width = block.bbox[2] - block.bbox[0];
        let scale = (image_width as f64 / original_image_width as f64 * 100.0).trunc() as i64;
        let tag = format!(
            "<img src=\"{}\" alt=\"Image\" width=\"{scale}%\" />",
            collapse_soft_newlines(path)
        );
        centered(&tag)
    } else {
        format!("![]({})", collapse_soft_newlines(path))
    }
}

/// `format_image_plain`, which `header_image`/`footer_image` always use.
fn image_plain(block: &AssembledBlock) -> String {
    match &block.image_path {
        Some(path) => format!("![]({})", collapse_soft_newlines(path)),
        None => String::new(),
    }
}

/// The resolved handler map: `None` is an unhandled or ignored label.
fn handle_block(block: &AssembledBlock, options: &MarkdownOptions<'_>) -> Option<String> {
    let label = block.label.as_str();
    if options.markdown_ignore_labels.contains(&label) {
        return None;
    }
    let content = block.content.as_str();
    let pretty = options.pretty;
    Some(match label {
        "paragraph_title" => format_paragraph_title(content, None),
        "abstract_title" | "reference_title" | "content_title" => format_title(content),
        "doc_title" => collapse_soft_newlines(&format!("# {content}")),
        "table_title" | "figure_title" | "chart_title" => text_func(content, pretty),
        "vision_footnote" | "text" | "ocr" | "vertical_text" | "reference_content" => {
            normalize_newlines(content)
        }
        "abstract" => format_first_line(content, &["摘要", "abstract"], "## ", "\n", " "),
        "content" => content.replace("-\n", "  \n").replace('\n', "  \n"),
        // Formula, seal, and chart recognition are off in this port's mode:
        // their labels render through the image handler like upstream's
        // fallbacks do.
        "image" | "chart" | "seal" | "formula" | "display_formula" | "inline_formula" => {
            image_func(block, pretty, options.original_image_width)
        }
        "table" => {
            if options.use_table_recognition {
                if pretty {
                    let bordered =
                        text_func(content, true).replace("<table>", "<table border=\"1\">");
                    format!("\n{bordered}")
                } else {
                    simplify_table(&format!("\n{content}"))
                }
            } else {
                image_func(block, pretty, options.original_image_width)
            }
        }
        "reference" => format_first_line(content, &["参考文献", "references"], "## ", "", "\n"),
        "algorithm" => content.trim_matches('\n').to_owned(),
        "spotting" | "number" | "footnote" | "header" | "footer" | "aside_text" => {
            content.to_owned()
        }
        "header_image" | "footer_image" => image_plain(block),
        _ => return None,
    })
}

/// The continuity geometry `get_seg_flag` reads from a block.
pub(crate) fn geometry_of(block: &AssembledBlock) -> BlockGeometry {
    let (start, end) = match block.direction {
        Dir::Horizontal => (block.bbox[0], block.bbox[2]),
        Dir::Vertical => (block.bbox[1], block.bbox[3]),
    };
    BlockGeometry {
        start: start as f64,
        end: end as f64,
        seg_start: block.seg_start,
        seg_end: block.seg_end,
        lines: block.num_of_lines,
        width: block.width,
    }
}

/// `MarkdownConverter.convert` with `use_seg_flag=True`, over one page's
/// assembled blocks.
#[must_use]
pub fn convert_markdown_page(
    blocks: &[AssembledBlock],
    options: &MarkdownOptions<'_>,
) -> MarkdownPage {
    let mut markdown = String::new();
    let mut image_paths: Vec<String> = Vec::new();
    let mut last_label: Option<&str> = None;
    let mut previous_geometry: Option<BlockGeometry> = None;
    let mut first_start_flag: Option<bool> = None;
    let mut seg_end_flag = true;

    for block in blocks {
        if let Some(path) = &block.image_path
            && !image_paths.contains(path)
        {
            image_paths.push(path.clone());
        }

        let geometry = geometry_of(block);
        let (start_flag, end_flag) = paragraph_continues(geometry, previous_geometry);
        seg_end_flag = end_flag;
        if first_start_flag.is_none() {
            first_start_flag = Some(start_flag);
        }

        let Some(formatted) = handle_block(block, options) else {
            continue;
        };
        previous_geometry = Some(geometry);
        let joined_text = block.label == "text" && last_label == Some("text") && !start_flag;
        if !joined_text && !markdown.is_empty() {
            markdown.push_str("\n\n");
        }
        markdown.push_str(&formatted);
        last_label = Some(block.label.as_str());
    }

    MarkdownPage {
        markdown,
        image_paths,
        continuation_flags: (first_start_flag.unwrap_or(true), seg_end_flag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    use crate::structure_assembly::assemble_layout_parsing;
    use crate::structure_glue::{GlueBlock, GlueBox};
    use crate::structure_standardize::{OcrData, TextRecognizer, standardized_data};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-markdown-v2/expected.json");

    /// The shape-keyed stub shared with the standardized-data and assembly
    /// fixtures.
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

    #[test]
    fn the_captured_markdown_pages_are_reproduced() {
        let fixture: Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        };
        let cases = items(&fixture["cases"], "cases");
        assert_eq!(cases.len(), 6);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let size = items(&case["image_size"], "image_size");
            let threshold = case["threshold"].as_f64().unwrap_or(0.0);
            let layout = read_blocks(&case["layout"]);
            let regions = read_blocks(&case["regions"]);
            let tables: Vec<String> = items(&case["tables"], "tables")
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect();
            let ignore: Vec<&str> = items(&case["markdown_ignore_labels"], "ignore")
                .iter()
                .map(|v| v.as_str().unwrap_or(""))
                .collect();

            let mut ocr = OcrData::default();
            for span in items(&case["spans"], "spans") {
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
            let standardized = standardized_data(
                size[0].as_f64().unwrap_or(0.0),
                size[1].as_f64().unwrap_or(0.0),
                &layout,
                &regions,
                ocr,
                &mut recognizer,
                threshold,
            );
            let assembled = assemble_layout_parsing(&standardized, &tables, &ignore);

            for (variant, pretty) in [("pretty", true), ("plain", false)] {
                let expected = &case[variant];
                let options = MarkdownOptions {
                    pretty,
                    use_table_recognition: true,
                    original_image_width: size[0].as_i64().unwrap_or(0),
                    markdown_ignore_labels: &ignore,
                };
                let page = convert_markdown_page(&assembled, &options);
                assert_eq!(
                    page.markdown,
                    expected["markdown_texts"].as_str().unwrap_or("\u{0}"),
                    "{name}/{variant}: markdown"
                );
                let flags = items(&expected["page_continuation_flags"], "flags");
                assert_eq!(
                    page.continuation_flags,
                    (
                        flags[0].as_bool().unwrap_or(false),
                        flags[1].as_bool().unwrap_or(false)
                    ),
                    "{name}/{variant}: continuation flags"
                );
                let expected_paths: Vec<String> = items(&expected["image_paths"], "paths")
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_owned())
                    .collect();
                assert_eq!(page.image_paths, expected_paths, "{name}/{variant}: paths");
            }
        }
    }
}
