// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `standardized_data`: the control flow that reconciles layout detections,
//! region detections, and OCR spans before block assembly.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase C. This is the
//! first stage of `get_layout_parsing_res`: it dedups layout boxes, fixes the
//! footnote and lone-title labels, re-recognizes hurdle spans and text-less
//! blocks through a caller-supplied [`TextRecognizer`], falls back to OCR
//! boxes when layout detection found nothing, and matches every block into a
//! region — growing detected regions to their matched blocks' enclosing box
//! and inventing `SupplementaryRegion`s for whatever remains.
//!
//! Scope note: `convert_formula_res_to_ocr_format` is a loop over the formula
//! result list, and this port runs with formula recognition off, so the list
//! is empty and the call degenerates to a no-op. It is deliberately absent
//! rather than stubbed.
//!
//! Quirks preserved on purpose:
//!
//! - The per-box label tests in the first loop use the **lowercased** label,
//!   but every later mask test reads the raw label — upstream lowercases into
//!   a local variable and never writes it back.
//! - A hurdle span blanks its overlapped neighbours **before** the model is
//!   consulted, so a below-threshold recognition still erases them.
//! - The no-text re-recognition appends to `rec_polys` but **not**
//!   `dt_polys`; the hurdle branch appends to both.
//! - Upstream's fixpoint loop compares list **lengths**, not contents; the
//!   matched set only grows, so length equality is set equality, and the
//!   recorded region box is the enclosing box of the *previous* round.
//! - Python sets of block indices iterate in insertion (ascending) order for
//!   the contiguous 0-based indices this function builds; the port iterates
//!   ascending everywhere, which matches every recorded case.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use crate::structure_glue::{
    GlueBlock, GlueBox, MATCH_BLOCK_OVERLAP_RATIO_THRESHOLD, bbox_area, bbox_intersection,
    bbox_intersection_poly, minimum_enclosing_bbox, overlap_boxes_idx, overlap_ratio_small,
    overlap_ratio_union, remove_overlap_blocks, update_region_box,
};

/// `BLOCK_SETTINGS["title_conversion_area_ratio_threshold"]`.
const TITLE_CONVERSION_AREA_RATIO_THRESHOLD: f64 = 0.3;

/// A four-corner span polygon in page coordinates.
pub type SpanPoly = [[f64; 2]; 4];

/// The OCR arrays `standardized_data` reads and mutates, one entry per span.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrData {
    /// Detection polygons. Replaced for the first hurdle match, appended for
    /// the later ones, and — upstream's asymmetry — never appended by the
    /// no-text re-recognition.
    pub dt_polys: Vec<SpanPoly>,
    /// Recognition polygons, kept in step with `rec_boxes`.
    pub rec_polys: Vec<SpanPoly>,
    /// Axis-aligned span boxes.
    pub rec_boxes: Vec<GlueBox>,
    /// Recognized texts; blanking a hurdle neighbour writes an empty string.
    pub rec_texts: Vec<String>,
    /// Recognition scores.
    pub rec_scores: Vec<f64>,
    /// Span labels; every appended span is labelled `text`.
    pub rec_labels: Vec<String>,
}

/// The text-recognition model, seen from the orchestration as a callback.
///
/// `crop` is the integer pixel rectangle `[x1, y1, x2, y2]` upstream slices
/// out of the page image (`float` coordinates truncated toward zero); the
/// implementor owns the image and returns the recognized text and its score.
pub trait TextRecognizer {
    /// Recognize one crop of the page image.
    fn recognize(&mut self, crop: [i64; 4]) -> (String, f64);
}

/// Everything `standardized_data` returns: the (mutated) detection lists, the
/// two index maps, and the (mutated) OCR arrays.
#[derive(Clone, Debug)]
pub struct StandardizedData {
    /// Layout boxes after dedup, relabels, and the OCR fallback.
    pub layout_boxes: Vec<GlueBlock>,
    /// Region boxes sorted by area, grown to their matches, plus every
    /// `SupplementaryRegion` invented for unmatched and masked blocks.
    pub region_boxes: Vec<GlueBlock>,
    /// Region index → layout-box indices, in match order.
    pub region_to_block_map: BTreeMap<usize, Vec<usize>>,
    /// Layout-box index → OCR span indices, in match order.
    pub block_to_ocr_map: BTreeMap<usize, Vec<usize>>,
    /// The OCR arrays after hurdle replacement and re-recognition.
    pub ocr: OcrData,
}

fn is_mask_label(label: &str) -> bool {
    // unordered_labels + header_labels + footer_labels.
    matches!(
        label,
        "aside_text"
            | "seal"
            | "number"
            | "formula_number"
            | "header"
            | "header_image"
            | "footer"
            | "footer_image"
            | "footnote"
    )
}

fn is_vision_label(label: &str) -> bool {
    matches!(label, "image" | "table" | "chart" | "flowchart" | "figure")
}

/// The enclosing box of a span polygon, as `convert_points_to_boxes` derives
/// it when `get_bbox_intersection` receives a polygon.
fn poly_enclosing_box(poly: SpanPoly) -> GlueBox {
    let mut bbox = [poly[0][0], poly[0][1], poly[0][0], poly[0][1]];
    for point in &poly[1..] {
        bbox[0] = bbox[0].min(point[0]);
        bbox[1] = bbox[1].min(point[1]);
        bbox[2] = bbox[2].max(point[0]);
        bbox[3] = bbox[3].max(point[1]);
    }
    bbox
}

fn poly_from_i16(poly: [[i16; 2]; 4]) -> SpanPoly {
    poly.map(|point| [f64::from(point[0]), f64::from(point[1])])
}

/// `[int(i) for i in crop_box]`: Python `int()` truncates toward zero.
fn truncated_crop(bbox: GlueBox) -> [i64; 4] {
    [
        bbox[0].trunc() as i64,
        bbox[1].trunc() as i64,
        bbox[2].trunc() as i64,
        bbox[3].trunc() as i64,
    ]
}

/// `standardized_data`, with the two model calls behind `recognizer` and the
/// score threshold already resolved by the caller (upstream falls back to the
/// OCR pipeline's configured threshold when handed `None`).
#[must_use]
pub fn standardized_data(
    image_width: f64,
    image_height: f64,
    layout_boxes: &[GlueBlock],
    region_boxes: &[GlueBlock],
    mut ocr: OcrData,
    recognizer: &mut dyn TextRecognizer,
    text_rec_score_thresh: f64,
) -> StandardizedData {
    let mut layout_boxes = remove_overlap_blocks(layout_boxes, 0.5);

    let mut footnote_list: Vec<usize> = Vec::new();
    let mut paragraph_title_list: Vec<usize> = Vec::new();
    let mut bottom_text_y_max = 0.0_f64;
    let mut max_block_area = 0.0_f64;
    let mut doc_title_num = 0_usize;
    let mut base_region_bbox = [65535_i64, 65535, 0, 0];
    // Insertion-ordered `matched_ocr_dict`: span index → layout-box indices.
    let mut matched_ocr: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut block_to_ocr_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

    for (box_idx, block) in layout_boxes.iter().enumerate() {
        let label = block.label.to_lowercase();
        base_region_bbox = update_region_box(block.coordinate, base_region_bbox);
        max_block_area = max_block_area.max(bbox_area(block.coordinate));

        if label == "footnote" {
            footnote_list.push(box_idx);
        } else if label == "paragraph_title" {
            paragraph_title_list.push(box_idx);
        }
        if label == "text" {
            bottom_text_y_max = block.coordinate[3].max(bottom_text_y_max);
        }
        if label == "doc_title" {
            doc_title_num += 1;
        }

        if !matches!(label.as_str(), "formula" | "table" | "seal") {
            let matched =
                overlap_boxes_idx(&ocr.rec_boxes, std::slice::from_ref(&block.coordinate));
            for &span in &matched {
                match matched_ocr.iter_mut().find(|(index, _)| *index == span) {
                    Some((_, boxes)) => boxes.push(box_idx),
                    None => matched_ocr.push((span, vec![box_idx])),
                }
            }
            block_to_ocr_map.insert(box_idx, matched);
        }
    }

    // A footnote above the lowest text block is text.
    for &footnote_idx in &footnote_list {
        if layout_boxes[footnote_idx].coordinate[3] < bottom_text_y_max {
            layout_boxes[footnote_idx].label = "text".to_owned();
        }
    }

    // A lone paragraph title on a page without a doc title is promoted when
    // it is large enough.
    if paragraph_title_list.len() == 1 && doc_title_num == 0 {
        let title_idx = paragraph_title_list[0];
        let title_area = bbox_area(layout_boxes[title_idx].coordinate);
        if title_area > max_block_area * TITLE_CONVERSION_AREA_RATIO_THRESHOLD {
            layout_boxes[title_idx].label = "doc_title".to_owned();
        }
    }

    // Replace the OCR information of the hurdles: spans matched by more than
    // one layout box are re-recognized per box.
    for (ocr_idx, layout_ids) in &matched_ocr {
        if layout_ids.len() <= 1 {
            continue;
        }
        let mut matched_no = 0_usize;
        let original_box = ocr.rec_boxes[*ocr_idx];
        let original_poly = ocr.dt_polys[*ocr_idx];
        for &box_idx in layout_ids {
            let layout_box = layout_boxes[box_idx].coordinate;
            // Matching guarantees a >3px overlap on both axes, so the
            // intersections below exist; upstream would crash otherwise.
            let Some(crop_box) = bbox_intersection(original_box, layout_box) else {
                continue;
            };
            let span_ids = block_to_ocr_map.get(&box_idx).cloned().unwrap_or_default();
            for span in span_ids {
                let iou = overlap_ratio_small(ocr.rec_boxes[span], crop_box);
                if iou > 0.8 {
                    ocr.rec_texts[span] = String::new();
                }
            }
            let (text, score) = recognizer.recognize(truncated_crop(crop_box));
            let Some(crop_poly) =
                bbox_intersection_poly(poly_enclosing_box(original_poly), layout_box)
            else {
                continue;
            };
            let crop_poly = poly_from_i16(crop_poly);
            if score >= text_rec_score_thresh {
                matched_no += 1;
                if matched_no == 1 {
                    // The first match replaces the original span in place.
                    ocr.dt_polys[*ocr_idx] = crop_poly;
                    ocr.rec_boxes[*ocr_idx] = crop_box;
                    ocr.rec_polys[*ocr_idx] = crop_poly;
                    ocr.rec_scores[*ocr_idx] = score;
                    ocr.rec_texts[*ocr_idx] = text;
                } else {
                    // Later matches append and remap the block's span list.
                    ocr.dt_polys.push(crop_poly);
                    ocr.rec_boxes.push(crop_box);
                    ocr.rec_polys.push(crop_poly);
                    ocr.rec_scores.push(score);
                    ocr.rec_texts.push(text);
                    ocr.rec_labels.push("text".to_owned());
                    let appended = ocr.rec_texts.len() - 1;
                    if let Some(list) = block_to_ocr_map.get_mut(&box_idx) {
                        if let Some(position) = list.iter().position(|&v| v == *ocr_idx) {
                            list.remove(position);
                        }
                        list.push(appended);
                    }
                }
            }
        }
    }

    // Use the layout bbox for recognition when a block matched no text.
    let block_indices: Vec<usize> = block_to_ocr_map.keys().copied().collect();
    for box_idx in block_indices {
        let has_text = block_to_ocr_map
            .get(&box_idx)
            .is_some_and(|spans| spans.iter().any(|&span| !ocr.rec_texts[span].is_empty()));
        if has_text || is_vision_label(&layout_boxes[box_idx].label) {
            continue;
        }
        let crop_box = layout_boxes[box_idx].coordinate;
        let (text, score) = recognizer.recognize(truncated_crop(crop_box));
        // The self-intersection is the box itself; degenerate boxes never
        // reach this point through real detections.
        let Some(crop_poly) = bbox_intersection_poly(crop_box, crop_box) else {
            continue;
        };
        if score >= text_rec_score_thresh {
            ocr.rec_boxes.push(crop_box);
            ocr.rec_polys.push(poly_from_i16(crop_poly));
            ocr.rec_scores.push(score);
            ocr.rec_texts.push(text);
            ocr.rec_labels.push("text".to_owned());
            let appended = ocr.rec_texts.len() - 1;
            if let Some(list) = block_to_ocr_map.get_mut(&box_idx) {
                list.push(appended);
            }
        }
    }

    // No layout at all: every OCR span becomes a text block.
    if layout_boxes.is_empty() && !ocr.rec_boxes.is_empty() {
        for span_idx in 0..ocr.rec_boxes.len() {
            base_region_bbox = update_region_box(ocr.rec_boxes[span_idx], base_region_bbox);
            layout_boxes.push(GlueBlock {
                label: "text".to_owned(),
                coordinate: ocr.rec_boxes[span_idx],
                score: ocr.rec_scores[span_idx],
            });
            block_to_ocr_map.insert(span_idx, vec![span_idx]);
        }
    }

    // Match blocks into regions.
    let block_bboxes: Vec<GlueBox> = layout_boxes.iter().map(|b| b.coordinate).collect();
    let mut region_boxes: Vec<GlueBlock> = region_boxes.to_vec();
    region_boxes.sort_by(|a, b| bbox_area(a.coordinate).total_cmp(&bbox_area(b.coordinate)));
    let mut region_to_block_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

    if region_boxes.is_empty() {
        region_boxes.push(GlueBlock {
            label: "SupplementaryRegion".to_owned(),
            coordinate: base_region_bbox.map(|v| v as f64),
            score: 1.0,
        });
        region_to_block_map.insert(0, (0..block_bboxes.len()).collect());
    } else {
        let mut block_idxes_set: BTreeSet<usize> = (0..block_bboxes.len()).collect();
        let matching =
            |bbox: GlueBox, set: &BTreeSet<usize>, layout_boxes: &[GlueBlock]| -> Vec<usize> {
                set.iter()
                    .copied()
                    .filter(|&idx| !is_mask_label(&layout_boxes[idx].label))
                    .filter(|&idx| {
                        overlap_ratio_small(bbox, block_bboxes[idx])
                            > MATCH_BLOCK_OVERLAP_RATIO_THRESHOLD
                    })
                    .collect()
            };

        for (region_idx, region) in region_boxes.iter_mut().enumerate() {
            let initial = matching(region.coordinate, &block_idxes_set, &layout_boxes);
            region_to_block_map.insert(region_idx, Vec::new());
            if initial.is_empty() {
                continue;
            }
            // Grow the region to its matches' enclosing box until stable.
            let mut previous = initial;
            let (matched, new_region_bbox) = loop {
                let matched_bboxes: Vec<GlueBox> =
                    previous.iter().map(|&idx| block_bboxes[idx]).collect();
                let enclosing = minimum_enclosing_bbox(&matched_bboxes);
                let next = matching(enclosing, &block_idxes_set, &layout_boxes);
                if next.len() == previous.len() {
                    break (next, enclosing);
                }
                previous = next;
            };
            for &idx in &matched {
                block_idxes_set.remove(&idx);
            }
            region_to_block_map.insert(region_idx, matched);
            region.coordinate = new_region_bbox;
        }

        // Supplement regions for whatever no detected region claimed.
        let all_bboxes: BTreeMap<usize, GlueBox> =
            block_bboxes.iter().copied().enumerate().collect();
        while !block_idxes_set.is_empty() {
            let unmatched: Vec<GlueBox> = block_idxes_set
                .iter()
                .map(|&idx| block_bboxes[idx])
                .collect();
            let mut supplement = minimum_enclosing_bbox(&unmatched);
            let mut matched: Vec<usize> = Vec::new();
            for (region_idx, region) in region_boxes.iter().enumerate() {
                if region_to_block_map
                    .get(&region_idx)
                    .is_none_or(Vec::is_empty)
                {
                    continue;
                }
                let region_bbox = region.coordinate;
                if overlap_ratio_union(supplement, region_bbox) > 0.0 {
                    let idxes: Vec<usize> = block_idxes_set.iter().copied().collect();
                    let (shrunk, shrunk_matched) =
                        crate::structure_glue::shrink_supplement_region_bbox(
                            supplement,
                            region_bbox,
                            image_width,
                            image_height,
                            &idxes,
                            &all_bboxes,
                        );
                    supplement = shrunk;
                    matched = shrunk_matched;
                }
            }
            let mut matched: Vec<usize> = matched
                .into_iter()
                .filter(|&idx| !is_mask_label(&layout_boxes[idx].label))
                .collect();
            if matched.is_empty() {
                matched = block_idxes_set
                    .iter()
                    .copied()
                    .filter(|&idx| !is_mask_label(&layout_boxes[idx].label))
                    .collect();
                if matched.is_empty() {
                    break;
                }
            }
            let matched_bboxes: Vec<GlueBox> =
                matched.iter().map(|&idx| block_bboxes[idx]).collect();
            let supplement = minimum_enclosing_bbox(&matched_bboxes);
            let region_idx = region_boxes.len();
            region_to_block_map.insert(region_idx, matched.clone());
            for &idx in &matched {
                block_idxes_set.remove(&idx);
            }
            region_boxes.push(GlueBlock {
                label: "SupplementaryRegion".to_owned(),
                coordinate: supplement,
                score: 1.0,
            });
        }

        // Every mask-labelled block becomes its own supplementary region,
        // whether or not anything still holds it.
        for block_idx in 0..layout_boxes.len() {
            if !is_mask_label(&layout_boxes[block_idx].label) {
                continue;
            }
            let region_idx = region_boxes.len();
            region_to_block_map.insert(region_idx, vec![block_idx]);
            region_boxes.push(GlueBlock {
                label: "SupplementaryRegion".to_owned(),
                coordinate: block_bboxes[block_idx],
                score: 1.0,
            });
        }
    }

    StandardizedData {
        layout_boxes,
        region_boxes,
        region_to_block_map,
        block_to_ocr_map,
        ocr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-standardized-data/expected.json");

    /// The capture tool's recognizer stub, mirrored bit for bit: the text and
    /// score depend only on the crop shape.
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

    fn read_result_blocks(value: &Value) -> Vec<GlueBlock> {
        items(value, "result blocks")
            .iter()
            .map(|entry| GlueBlock {
                label: entry["label"].as_str().unwrap_or("").to_owned(),
                coordinate: read_box(&entry["coordinate"]),
                score: entry["score"].as_f64().unwrap_or(0.0),
            })
            .collect()
    }

    fn read_poly(value: &Value) -> SpanPoly {
        let points = items(value, "poly");
        let mut poly = [[0.0_f64; 2]; 4];
        for (slot, point) in poly.iter_mut().zip(points) {
            let point = items(point, "point");
            slot[0] = point[0].as_f64().unwrap_or(0.0);
            slot[1] = point[1].as_f64().unwrap_or(0.0);
        }
        poly
    }

    fn read_map(value: &Value) -> BTreeMap<usize, Vec<usize>> {
        let Some(map) = value.as_object() else {
            panic!("fixture map is not an object");
        };
        map.iter()
            .map(|(key, list)| {
                let key: usize = match key.parse() {
                    Ok(key) => key,
                    Err(error) => panic!("map key {key}: {error}"),
                };
                let list = items(list, "map entry")
                    .iter()
                    .map(|v| v.as_u64().unwrap_or(0) as usize)
                    .collect();
                (key, list)
            })
            .collect()
    }

    fn box_from_span(bbox: GlueBox) -> SpanPoly {
        [
            [bbox[0], bbox[1]],
            [bbox[2], bbox[1]],
            [bbox[2], bbox[3]],
            [bbox[0], bbox[3]],
        ]
    }

    #[test]
    fn the_captured_standardizations_are_reproduced() {
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

            let mut ocr = OcrData::default();
            for span in items(&case["spans"], "spans") {
                let span = items(span, "span");
                let bbox = read_box(&span[0]);
                ocr.rec_boxes.push(bbox);
                ocr.dt_polys.push(box_from_span(bbox));
                ocr.rec_polys.push(box_from_span(bbox));
                ocr.rec_texts
                    .push(span[1].as_str().unwrap_or("").to_owned());
                ocr.rec_scores.push(0.9);
                ocr.rec_labels.push("text".to_owned());
            }

            let mut recognizer = StubRecognizer { calls: Vec::new() };
            let result = standardized_data(
                size[0].as_f64().unwrap_or(0.0),
                size[1].as_f64().unwrap_or(0.0),
                &layout,
                &regions,
                ocr,
                &mut recognizer,
                threshold,
            );

            assert_eq!(
                result.layout_boxes,
                read_result_blocks(&case["layout_boxes"]),
                "{name}: layout boxes"
            );
            assert_eq!(
                result.region_boxes,
                read_result_blocks(&case["region_boxes"]),
                "{name}: region boxes"
            );
            assert_eq!(
                result.region_to_block_map,
                read_map(&case["region_to_block_map"]),
                "{name}: region_to_block_map"
            );
            assert_eq!(
                result.block_to_ocr_map,
                read_map(&case["block_to_ocr_map"]),
                "{name}: block_to_ocr_map"
            );

            let expected_calls: Vec<[i64; 2]> = items(&case["model_calls"], "model_calls")
                .iter()
                .map(|pair| {
                    let pair = items(pair, "call");
                    [pair[0].as_i64().unwrap_or(0), pair[1].as_i64().unwrap_or(0)]
                })
                .collect();
            assert_eq!(recognizer.calls, expected_calls, "{name}: model calls");

            let expected_ocr = &case["ocr"];
            assert_eq!(
                result.ocr.rec_texts,
                items(&expected_ocr["rec_texts"], "rec_texts")
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>(),
                "{name}: rec_texts"
            );
            assert_eq!(
                result.ocr.rec_labels,
                items(&expected_ocr["rec_labels"], "rec_labels")
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>(),
                "{name}: rec_labels"
            );
            let expected_boxes: Vec<GlueBox> = items(&expected_ocr["rec_boxes"], "rec_boxes")
                .iter()
                .map(read_box)
                .collect();
            assert_eq!(result.ocr.rec_boxes, expected_boxes, "{name}: rec_boxes");
            let expected_scores: Vec<f64> = items(&expected_ocr["rec_scores"], "rec_scores")
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN))
                .collect();
            assert_eq!(
                result.ocr.rec_scores.len(),
                expected_scores.len(),
                "{name}: rec_scores length"
            );
            for (index, (actual, expected)) in result
                .ocr
                .rec_scores
                .iter()
                .zip(&expected_scores)
                .enumerate()
            {
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "{name}: rec_scores[{index}] {actual} vs {expected}"
                );
            }
            let expected_dt: Vec<SpanPoly> = items(&expected_ocr["dt_polys"], "dt_polys")
                .iter()
                .map(read_poly)
                .collect();
            assert_eq!(result.ocr.dt_polys, expected_dt, "{name}: dt_polys");
            let expected_rec: Vec<SpanPoly> = items(&expected_ocr["rec_polys"], "rec_polys")
                .iter()
                .map(read_poly)
                .collect();
            assert_eq!(result.ocr.rec_polys, expected_rec, "{name}: rec_polys");
        }
    }
}
