// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Layout-parsing block assembly: `get_layout_parsing_objects`,
//! `sort_layout_parsing_blocks`, and the `order_index` tail.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase D. This is the
//! post-model half of `get_layout_parsing_res`: it takes the reconciled
//! output of [`crate::structure_standardize::standardized_data`], builds one
//! ordering block per layout box — table blocks take their content from the
//! table results by running index, `formula`-labelled blocks re-match their
//! OCR spans, everything else goes through the text-line machinery — groups
//! the blocks into regions (silently dropping a region that matched
//! nothing), orders the page of regions and then each region with
//! `xycut_enhanced`, and numbers the flattened result.
//!
//! Scope note: seal and chart recognition are off in this port, so their
//! result lists are always empty and their content branches are dead;
//! `formula` and `table` **labels** still occur (the layout detector emits
//! them regardless) and are handled.
//!
//! Quirks preserved on purpose:
//!
//! - Every block starts with infinite `seg_start`/`seg_end`; only the
//!   text-line machinery assigns them, so a table block keeps them infinite
//!   and a single-line block never sets `seg_end`.
//! - A block's direction is text-derived only when it has spans; a block
//!   with none keeps the direction frozen from its float coordinates.
//! - `order_index` starts at `1` and counts only `visualize_index_labels`
//!   minus the ignore list, read from the **post-ordering** label — a text
//!   block relabelled `vision_footnote` is skipped.
//! - The image path stamps the truncated integer coordinates, and `seal`
//!   appears in both its recognition branch (dead here) and the image list.
#![allow(dead_code)]

use crate::layout_order::{Dir, OrderBlock, OrderPage, xycut_enhanced_order};
use crate::structure_glue::{overlap_boxes_idx, update_region_box};
use crate::structure_standardize::StandardizedData;
use crate::text_lines::{OcrSpan, TextDirection, update_text_content};

/// `BLOCK_LABEL_MAP["visualize_index_labels"]`.
const VISUALIZE_INDEX_LABELS: [&str; 18] = [
    "text",
    "formula",
    "algorithm",
    "reference",
    "content",
    "abstract",
    "paragraph_title",
    "doc_title",
    "abstract_title",
    "refer_title",
    "content_title",
    "number",
    "footnote",
    "header",
    "header_image",
    "footer",
    "footer_image",
    "aside_text",
];

/// The pipeline's default `markdown_ignore_labels`.
pub const DEFAULT_MARKDOWN_IGNORE_LABELS: [&str; 7] = [
    "number",
    "footnote",
    "header",
    "header_image",
    "footer",
    "footer_image",
    "aside_text",
];

fn is_image_stamped_label(label: &str) -> bool {
    // ["seal", "table", "formula", "chart"] + image_labels (image, figure, seal).
    matches!(
        label,
        "seal" | "table" | "formula" | "chart" | "image" | "figure"
    )
}

/// One block of the final ordered parsing result.
#[derive(Clone, Debug, PartialEq)]
pub struct AssembledBlock {
    /// The label after ordering — a matched caption may have become
    /// `vision_footnote`.
    pub label: String,
    /// The truncated integer bounding box.
    pub bbox: [i64; 4],
    /// Table `pred_html` or the assembled text content.
    pub content: String,
    /// Position in the flattened reading order.
    pub index: usize,
    /// One-based reading number over the visualized labels; `None` for
    /// labels outside the visualize list or inside the ignore list.
    pub order_index: Option<u32>,
    /// Text lines in the block (vision blocks reset to `1` by ordering).
    pub num_of_lines: u32,
    /// The block's final direction.
    pub direction: Dir,
    /// First line's first span start; infinite when never set.
    pub seg_start: f64,
    /// Last line's last span end; infinite when never set.
    pub seg_end: f64,
    /// Mean text-line height (`1` untouched, `0` when all lines filtered).
    pub text_line_height: f64,
    /// Mean text-line width.
    pub text_line_width: f64,
    /// `imgs/img_in_{label}_box_{x1}_{y1}_{x2}_{y2}.jpg` for visual labels.
    pub image_path: Option<String>,
}

/// `get_layout_parsing_objects` + `sort_layout_parsing_blocks` + the
/// `order_index` tail, over the standardized data.
///
/// `table_html` is `pred_html` per recognized table, in table-block order;
/// empty when table recognition found nothing.
#[must_use]
pub fn assemble_layout_parsing(
    standardized: &StandardizedData,
    table_html: &[String],
    markdown_ignore_labels: &[&str],
) -> Vec<AssembledBlock> {
    let ocr = &standardized.ocr;
    let mut table_index = 0_usize;
    let mut blocks: Vec<OrderBlock> = Vec::new();
    let mut contents: Vec<String> = Vec::new();
    let mut image_paths: Vec<Option<String>> = Vec::new();

    for (box_idx, layout_box) in standardized.layout_boxes.iter().enumerate() {
        let label = layout_box.label.as_str();
        let mut block = OrderBlock::from_detection(label, layout_box.coordinate);
        // Blocks start with infinite segment coordinates; only text lines
        // assign them.
        block.seg_start = f64::INFINITY;
        block.seg_end = f64::NEG_INFINITY;

        let content = if label == "table" && !table_html.is_empty() {
            let html = table_html.get(table_index).cloned().unwrap_or_default();
            table_index += 1;
            html
        } else {
            // Seal and chart recognition are off: their branches are dead and
            // their labels fall through to the text path like upstream's.
            let span_ids: Vec<usize> = if label == "formula" {
                overlap_boxes_idx(&ocr.rec_boxes, std::slice::from_ref(&layout_box.coordinate))
            } else {
                standardized
                    .block_to_ocr_map
                    .get(&box_idx)
                    .cloned()
                    .unwrap_or_default()
            };
            let spans: Vec<OcrSpan> = span_ids
                .iter()
                .map(|&span| OcrSpan {
                    bbox: ocr.rec_boxes[span],
                    text: ocr.rec_texts[span].clone(),
                })
                .collect();
            let text = update_text_content(label, block.bbox.map(|v| v as f64), &spans);
            block.num_of_lines = text.num_of_lines;
            block.text_line_height = text.text_line_height;
            block.text_line_width = text.text_line_width;
            if let Some(seg_start) = text.seg_start {
                block.seg_start = seg_start;
            }
            if let Some(seg_end) = text.seg_end {
                block.seg_end = seg_end;
            }
            if !spans.is_empty() {
                block.direction = match text.direction {
                    TextDirection::Horizontal => Dir::Horizontal,
                    TextDirection::Vertical => Dir::Vertical,
                };
            }
            text.content
        };

        let image_path = if is_image_stamped_label(label) {
            let [x1, y1, x2, y2] = layout_box.coordinate.map(|v| v.trunc() as i64);
            Some(format!("imgs/img_in_{label}_box_{x1}_{y1}_{x2}_{y2}.jpg"))
        } else {
            None
        };

        blocks.push(block);
        contents.push(content);
        image_paths.push(image_path);
    }

    // Group blocks into regions, skipping regions that matched nothing.
    let mut page_region_bbox = [65535_i64, 65535, 0, 0];
    let mut inner_pages: Vec<(OrderPage, Vec<usize>)> = Vec::new();
    for (region_idx, region) in standardized.region_boxes.iter().enumerate() {
        let region_bbox = region.coordinate.map(|v| v.trunc() as i64);
        let member_ids = standardized
            .region_to_block_map
            .get(&region_idx)
            .cloned()
            .unwrap_or_default();
        if member_ids.is_empty() {
            continue;
        }
        page_region_bbox = update_region_box(region_bbox.map(|v| v as f64), page_region_bbox);
        let members: Vec<OrderBlock> = member_ids.iter().map(|&idx| blocks[idx].clone()).collect();
        inner_pages.push((OrderPage::new(region_bbox, members), member_ids));
    }

    let region_blocks: Vec<OrderBlock> = inner_pages
        .iter()
        .map(|(page, _)| OrderBlock::from_region_page(page))
        .collect();
    let mut outer = OrderPage::new(page_region_bbox, region_blocks);
    let region_order = xycut_enhanced_order(&mut outer);

    // Flatten: order each region's blocks in region order.
    let visualize: Vec<&str> = VISUALIZE_INDEX_LABELS
        .iter()
        .copied()
        .filter(|label| !markdown_ignore_labels.contains(label))
        .collect();
    let mut result: Vec<AssembledBlock> = Vec::new();
    let mut order_index = 1_u32;
    for &region_position in &region_order {
        let (inner, member_ids) = &mut inner_pages[region_position];
        let block_order = xycut_enhanced_order(inner);
        for local in block_order {
            let block = &inner.blocks[local];
            let global = member_ids[local];
            let index = result.len();
            let numbered = visualize.contains(&block.label.as_str());
            result.push(AssembledBlock {
                label: block.label.clone(),
                bbox: block.bbox,
                content: contents[global].clone(),
                index,
                order_index: if numbered {
                    let assigned = order_index;
                    order_index += 1;
                    Some(assigned)
                } else {
                    None
                },
                num_of_lines: block.num_of_lines,
                direction: block.direction,
                seg_start: block.seg_start,
                seg_end: block.seg_end,
                text_line_height: block.text_line_height,
                text_line_width: block.text_line_width,
                image_path: image_paths[global].clone(),
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    use crate::structure_glue::{GlueBlock, GlueBox};
    use crate::structure_standardize::{OcrData, TextRecognizer, standardized_data};

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-layout-assembly/expected.json");

    /// The capture tool's recognizer stub, shared with the standardized-data
    /// fixture: text and score depend only on the crop shape.
    struct StubRecognizer {
        calls: Vec<[i64; 2]>,
    }

    impl TextRecognizer for StubRecognizer {
        fn recognize(&mut self, crop: [i64; 4]) -> (String, f64) {
            let height = crop[3] - crop[1];
            let width = crop[2] - crop[0];
            self.calls.push([height, width]);
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
    fn the_captured_assemblies_are_reproduced() {
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

            let mut recognizer = StubRecognizer { calls: Vec::new() };
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

            let expected_calls: Vec<[i64; 2]> = items(&case["model_calls"], "model_calls")
                .iter()
                .map(|pair| {
                    let pair = items(pair, "call");
                    [pair[0].as_i64().unwrap_or(0), pair[1].as_i64().unwrap_or(0)]
                })
                .collect();
            assert_eq!(recognizer.calls, expected_calls, "{name}: model calls");

            let expected_blocks = items(&case["blocks"], "blocks");
            assert_eq!(
                assembled.len(),
                expected_blocks.len(),
                "{name}: block count"
            );
            for (block, expected) in assembled.iter().zip(expected_blocks) {
                let which = format!("{name}: block {}", block.index);
                assert_eq!(
                    block.label,
                    expected["label"].as_str().unwrap_or(""),
                    "{which}: label"
                );
                let expected_bbox = items(&expected["bbox"], "bbox");
                let expected_bbox = [
                    expected_bbox[0].as_i64().unwrap_or(0),
                    expected_bbox[1].as_i64().unwrap_or(0),
                    expected_bbox[2].as_i64().unwrap_or(0),
                    expected_bbox[3].as_i64().unwrap_or(0),
                ];
                assert_eq!(block.bbox, expected_bbox, "{which}: bbox");
                assert_eq!(
                    block.content,
                    expected["content"].as_str().unwrap_or(""),
                    "{which}: content"
                );
                assert_eq!(
                    block.index as u64,
                    expected["index"].as_u64().unwrap_or(u64::MAX),
                    "{which}: index"
                );
                assert_eq!(
                    block.order_index.map(u64::from),
                    expected["order_index"].as_u64(),
                    "{which}: order_index"
                );
                assert_eq!(
                    u64::from(block.num_of_lines),
                    expected["num_of_lines"].as_u64().unwrap_or(u64::MAX),
                    "{which}: num_of_lines"
                );
                let direction = match block.direction {
                    Dir::Horizontal => "horizontal",
                    Dir::Vertical => "vertical",
                };
                assert_eq!(
                    direction,
                    expected["direction"].as_str().unwrap_or(""),
                    "{which}: direction"
                );
                let check_seg = |actual: f64, recorded: &Value, what: &str| match recorded.as_f64()
                {
                    Some(value) => assert!(
                        (actual - value).abs() < 1e-9,
                        "{which}: {what} {actual} vs {value}"
                    ),
                    None => assert!(
                        actual.is_infinite(),
                        "{which}: {what} expected infinite, got {actual}"
                    ),
                };
                check_seg(
                    block.seg_start,
                    &expected["seg_start_coordinate"],
                    "seg_start",
                );
                check_seg(block.seg_end, &expected["seg_end_coordinate"], "seg_end");
                assert!(
                    (block.text_line_height
                        - expected["text_line_height"].as_f64().unwrap_or(-1.0))
                    .abs()
                        < 1e-9,
                    "{which}: text_line_height"
                );
                assert!(
                    (block.text_line_width - expected["text_line_width"].as_f64().unwrap_or(-1.0))
                        .abs()
                        < 1e-9,
                    "{which}: text_line_width"
                );
                assert_eq!(
                    block.image_path.as_deref(),
                    expected["image_path"].as_str(),
                    "{which}: image_path"
                );
            }
        }
    }
}
