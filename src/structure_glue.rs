// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Geometry glue for the StructureV3 layout-parsing orchestration.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase A. Upstream's
//! `standardized_data` stitches layout detections, region detections, and OCR
//! spans together with these helpers before anything model-shaped happens.
//! They are pure geometry, so they land first, pinned by execution in
//! `tests/fixtures/classic-v1-structure-glue/`.
//!
//! Conventions preserved from upstream rather than tidied:
//!
//! - the overlap test in [`overlap_boxes_idx`] is **strictly greater than
//!   three pixels on both axes** — a three-pixel intersection does not match;
//! - [`remove_overlap_blocks`] drops the `image`-labelled block whenever an
//!   image overlaps a non-image, regardless of which is smaller, and already
//!   dropped blocks are skipped mid-scan, so the first overlapping pair wins;
//! - [`bbox_intersection`]'s polygon form casts to `i16` because upstream's
//!   does (`dtype=np.int16`) — page coordinates fit, and the cast is part of
//!   the recorded behaviour;
//! - [`shrink_supplement_region_bbox`] re-reads the index of the removed
//!   minimum from the **original** distance list, exactly as upstream's
//!   `edge_distance_list.index(min_distance)` does after removing from a
//!   copy; block-index sets iterate in ascending order.
#![allow(dead_code)]

/// A bounding box in `[x1, y1, x2, y2]` page coordinates.
pub type GlueBox = [f64; 4];

/// `REGION_SETTINGS["match_block_overlap_ratio_threshold"]`.
pub const MATCH_BLOCK_OVERLAP_RATIO_THRESHOLD: f64 = 0.8;
/// `REGION_SETTINGS["split_block_overlap_ratio_threshold"]`.
const SPLIT_BLOCK_OVERLAP_RATIO_THRESHOLD: f64 = 0.4;

/// `calculate_bbox_area`: absolute area, tolerant of inverted boxes.
#[must_use]
pub fn bbox_area(bbox: GlueBox) -> f64 {
    ((bbox[2] - bbox[0]) * (bbox[3] - bbox[1])).abs()
}

/// `update_region_box`: the integer-floored running union.
#[must_use]
pub fn update_region_box(bbox: GlueBox, region: [i64; 4]) -> [i64; 4] {
    [
        (bbox[0].min(region[0] as f64)).floor() as i64,
        (bbox[1].min(region[1] as f64)).floor() as i64,
        (bbox[2].max(region[2] as f64)).floor() as i64,
        (bbox[3].max(region[3] as f64)).floor() as i64,
    ]
}

/// `calculate_minimum_enclosing_bbox` over a non-empty group.
#[must_use]
pub fn minimum_enclosing_bbox(boxes: &[GlueBox]) -> GlueBox {
    let mut enclosing = boxes[0];
    for bbox in &boxes[1..] {
        enclosing[0] = enclosing[0].min(bbox[0]);
        enclosing[1] = enclosing[1].min(bbox[1]);
        enclosing[2] = enclosing[2].max(bbox[2]);
        enclosing[3] = enclosing[3].max(bbox[3]);
    }
    enclosing
}

/// `get_overlap_boxes_idx`: source indices whose intersection with any
/// reference box is strictly wider and taller than three pixels.
///
/// Returned ascending and deduplicated — upstream builds a list, and its one
/// caller immediately reduces it to a membership set.
#[must_use]
pub fn overlap_boxes_idx(src: &[GlueBox], reference: &[GlueBox]) -> Vec<usize> {
    let mut matched: Vec<usize> = Vec::new();
    for ref_box in reference {
        for (index, src_box) in src.iter().enumerate() {
            let width = src_box[2].min(ref_box[2]) - src_box[0].max(ref_box[0]);
            let height = src_box[3].min(ref_box[3]) - src_box[1].max(ref_box[1]);
            if width > 3.0 && height > 3.0 {
                matched.push(index);
            }
        }
    }
    matched.sort_unstable();
    matched.dedup();
    matched
}

/// `get_sub_regions_ocr_res`, reduced to what its callers consume: the span
/// indices kept by the membership filter.
///
/// `flag_within` keeps the spans **inside** the object boxes; `false` keeps
/// the spans outside them — upstream's "everything except formulas, tables,
/// and seals" path.
#[must_use]
pub fn sub_region_span_indices(
    span_boxes: &[GlueBox],
    object_boxes: &[GlueBox],
    flag_within: bool,
) -> Vec<usize> {
    let matched = overlap_boxes_idx(span_boxes, object_boxes);
    (0..span_boxes.len())
        .filter(|index| matched.binary_search(index).is_ok() == flag_within)
        .collect()
}

/// One layout detection, as `standardized_data` sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct GlueBlock {
    /// The layout label.
    pub label: String,
    /// The bounding box.
    pub coordinate: GlueBox,
    /// The detection score.
    pub score: f64,
}

/// `calculate_overlap_ratio` in the mode this module needs.
fn overlap_ratio_small(a: GlueBox, b: GlueBox) -> f64 {
    let width = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let height = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let intersection = width * height;
    let reference = bbox_area(a).min(bbox_area(b));
    if reference <= 0.0 {
        0.0
    } else {
        intersection / reference
    }
}

/// `calculate_overlap_ratio` union mode, for the supplement-overlap test.
fn overlap_ratio_union(a: GlueBox, b: GlueBox) -> f64 {
    let width = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let height = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let intersection = width * height;
    let union = bbox_area(a) + bbox_area(b) - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// `remove_overlap_blocks` with upstream's defaults (`threshold=0.5`,
/// `smaller=true`, as `standardized_data` calls it).
///
/// When two blocks overlap past the threshold, the smaller is dropped —
/// except that an `image` block overlapping a non-image block is always the
/// one dropped. A block already dropped no longer participates, so the
/// earliest overlapping pair decides.
#[must_use]
pub fn remove_overlap_blocks(blocks: &[GlueBlock], threshold: f64) -> Vec<GlueBlock> {
    let mut dropped = vec![false; blocks.len()];
    for i in 0..blocks.len() {
        for j in (i + 1)..blocks.len() {
            if dropped[i] || dropped[j] {
                continue;
            }
            let ratio = overlap_ratio_small(blocks[i].coordinate, blocks[j].coordinate);
            if ratio <= threshold {
                continue;
            }
            let area_i = bbox_area(blocks[i].coordinate);
            let area_j = bbox_area(blocks[j].coordinate);
            let smaller_index = if area_i <= area_j { i } else { j };
            let i_is_image = blocks[i].label == "image";
            let j_is_image = blocks[j].label == "image";
            let drop = if i_is_image != j_is_image {
                if i_is_image { i } else { j }
            } else {
                smaller_index
            };
            dropped[drop] = true;
        }
    }
    blocks
        .iter()
        .zip(&dropped)
        .filter(|(_, dropped)| !**dropped)
        .map(|(block, _)| block.clone())
        .collect()
}

/// `get_bbox_intersection(..., return_format="bbox")`.
///
/// `None` when the boxes are disjoint **or merely touching**: upstream's test
/// is `>=`, so a shared edge is not an intersection.
#[must_use]
pub fn bbox_intersection(a: GlueBox, b: GlueBox) -> Option<GlueBox> {
    let x_min = a[0].max(b[0]);
    let y_min = a[1].max(b[1]);
    let x_max = a[2].min(b[2]);
    let y_max = a[3].min(b[3]);
    if x_min >= x_max || y_min >= y_max {
        return None;
    }
    Some([x_min, y_min, x_max, y_max])
}

/// `get_bbox_intersection(..., return_format="poly")`: the same rectangle as
/// four corners, cast to `i16` exactly as upstream's `astype(np.int16)`.
#[must_use]
pub fn bbox_intersection_poly(a: GlueBox, b: GlueBox) -> Option<[[i16; 2]; 4]> {
    let [x_min, y_min, x_max, y_max] = bbox_intersection(a, b)?;
    let cast = |value: f64| value as i16;
    Some([
        [cast(x_min), cast(y_min)],
        [cast(x_max), cast(y_min)],
        [cast(x_max), cast(y_max)],
        [cast(x_min), cast(y_max)],
    ])
}

/// `shrink_supplement_region_bbox`, recursion and stale index included.
///
/// Returns the shrunk bbox and the matched block indices (ascending). The
/// block map's iteration order is ascending by index, which is what upstream's
/// small-integer sets iterate as in every recorded case.
#[must_use]
pub fn shrink_supplement_region_bbox(
    supplement: GlueBox,
    reference: GlueBox,
    image_width: f64,
    image_height: f64,
    block_idxes: &[usize],
    block_bboxes: &std::collections::BTreeMap<usize, GlueBox>,
) -> (GlueBox, Vec<usize>) {
    let index_conversion = |index: usize| -> usize { [2, 3, 0, 1][index] };
    let edge_distances = |outer: GlueBox, inner: GlueBox| -> [f64; 4] {
        [
            (inner[0] - outer[0]) / image_width,
            (inner[1] - outer[1]) / image_height,
            (outer[2] - inner[2]) / image_width,
            (outer[3] - inner[3]) / image_height,
        ]
    };
    let position_of =
        |list: &[f64], value: f64| -> usize { list.iter().position(|v| *v == value).unwrap_or(0) };

    let edge_distance_list = edge_distances(supplement, reference);
    let mut remaining: Vec<f64> = edge_distance_list.to_vec();
    let mut min_distance = remaining.iter().copied().fold(f64::INFINITY, f64::min);
    let mut src_index = index_conversion(position_of(&edge_distance_list, min_distance));
    if block_idxes.is_empty() {
        return (supplement, Vec::new());
    }

    let mut supplement = supplement;
    let mut iner: Vec<usize> = Vec::new();
    for _ in 0..3 {
        let dst_index = index_conversion(src_index);
        let mut tmp_region = supplement;
        tmp_region[dst_index] = reference[src_index];
        iner = Vec::new();
        let mut split: Vec<usize> = Vec::new();
        for &block_idx in block_idxes {
            let Some(&block_bbox) = block_bboxes.get(&block_idx) else {
                continue;
            };
            let ratio = overlap_ratio_small(tmp_region, block_bbox);
            if ratio > MATCH_BLOCK_OVERLAP_RATIO_THRESHOLD {
                iner.push(block_idx);
            } else if ratio > SPLIT_BLOCK_OVERLAP_RATIO_THRESHOLD {
                split.push(block_idx);
            }
        }
        if !iner.is_empty() {
            for &split_idx in &split {
                let Some(&split_bbox) = block_bboxes.get(&split_idx) else {
                    continue;
                };
                let distances = edge_distances(tmp_region, split_bbox);
                let max_distance = distances.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let split_src = position_of(&distances, max_distance);
                let split_dst = index_conversion(split_src);
                tmp_region[split_dst] = split_bbox[split_src];
                let (shrunk, recursed_iner) = shrink_supplement_region_bbox(
                    tmp_region,
                    reference,
                    image_width,
                    image_height,
                    &iner,
                    block_bboxes,
                );
                tmp_region = shrunk;
                if recursed_iner.is_empty() {
                    continue;
                }
            }
            let matched: Vec<GlueBox> = iner
                .iter()
                .filter_map(|idx| block_bboxes.get(idx).copied())
                .collect();
            supplement = minimum_enclosing_bbox(&matched);
            break;
        }
        // The removed minimum's position is re-read from the ORIGINAL list,
        // exactly as upstream does after removing from a copy.
        if let Some(position) = remaining.iter().position(|v| *v == min_distance) {
            remaining.remove(position);
        }
        if remaining.is_empty() {
            break;
        }
        min_distance = remaining.iter().copied().fold(f64::INFINITY, f64::min);
        src_index = index_conversion(position_of(&edge_distance_list, min_distance));
    }
    (supplement, iner)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-structure-glue/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
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
            b[0].as_f64().unwrap_or(f64::NAN),
            b[1].as_f64().unwrap_or(f64::NAN),
            b[2].as_f64().unwrap_or(f64::NAN),
            b[3].as_f64().unwrap_or(f64::NAN),
        ]
    }

    fn read_boxes(value: &Value) -> Vec<GlueBox> {
        items(value, "boxes").iter().map(read_box).collect()
    }

    fn read_usizes(value: &Value) -> Vec<usize> {
        items(value, "indices")
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect()
    }

    #[test]
    fn the_captured_overlap_indices_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["overlap_idx"], "overlap_idx") {
            let name = case["case"].as_str().unwrap_or("?");
            let actual = overlap_boxes_idx(&read_boxes(&case["src"]), &read_boxes(&case["ref"]));
            assert_eq!(actual, read_usizes(&case["indices"]), "{name}");
        }
    }

    #[test]
    fn the_captured_sub_region_filters_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["sub_regions"], "sub_regions") {
            let name = case["case"].as_str().unwrap_or("?");
            let spans = read_boxes(&case["spans"]);
            let kept = sub_region_span_indices(
                &spans,
                &read_boxes(&case["objects"]),
                case["flag_within"].as_bool().unwrap_or(true),
            );
            let expected_texts: Vec<String> = items(&case["kept_texts"], "kept")
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect();
            let actual_texts: Vec<String> = kept.iter().map(|i| format!("t{i}")).collect();
            assert_eq!(actual_texts, expected_texts, "{name}");
        }
    }

    #[test]
    fn the_captured_overlap_removals_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["remove_overlap"], "remove_overlap") {
            let name = case["case"].as_str().unwrap_or("?");
            let blocks: Vec<GlueBlock> = items(&case["blocks"], "blocks")
                .iter()
                .map(|b| GlueBlock {
                    label: b["label"].as_str().unwrap_or("").to_owned(),
                    coordinate: read_box(&b["coordinate"]),
                    score: b["score"].as_f64().unwrap_or(0.0),
                })
                .collect();
            let kept = remove_overlap_blocks(&blocks, 0.5);
            let expected: Vec<(String, GlueBox)> = items(&case["kept"], "kept")
                .iter()
                .map(|b| {
                    (
                        b["label"].as_str().unwrap_or("").to_owned(),
                        read_box(&b["coordinate"]),
                    )
                })
                .collect();
            let actual: Vec<(String, GlueBox)> = kept
                .iter()
                .map(|b| (b.label.clone(), b.coordinate))
                .collect();
            assert_eq!(actual, expected, "{name}");
        }
    }

    #[test]
    fn the_captured_intersections_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["intersections"], "intersections") {
            let name = case["case"].as_str().unwrap_or("?");
            // The poly-input case records the quad's bbox in `first`.
            let first = if case["first"][0].is_array() {
                let quad: Vec<[f64; 2]> = items(&case["first"], "quad")
                    .iter()
                    .map(|p| {
                        let p = items(p, "point");
                        [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)]
                    })
                    .collect();
                let xs: Vec<f64> = quad.iter().map(|p| p[0]).collect();
                let ys: Vec<f64> = quad.iter().map(|p| p[1]).collect();
                [
                    xs.iter().copied().fold(f64::INFINITY, f64::min),
                    ys.iter().copied().fold(f64::INFINITY, f64::min),
                    xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ]
            } else {
                read_box(&case["first"])
            };
            let second = read_box(&case["second"]);
            match (bbox_intersection(first, second), case["bbox"].is_null()) {
                (None, true) => {}
                (Some(actual), false) => {
                    assert_eq!(actual, read_box(&case["bbox"]), "{name}: bbox");
                    let poly = match bbox_intersection_poly(first, second) {
                        Some(poly) => poly,
                        None => panic!("{name}: poly must exist when bbox does"),
                    };
                    let expected: Vec<[i16; 2]> = items(&case["poly"], "poly")
                        .iter()
                        .map(|p| {
                            let p = items(p, "point");
                            [
                                p[0].as_i64().unwrap_or(0) as i16,
                                p[1].as_i64().unwrap_or(0) as i16,
                            ]
                        })
                        .collect();
                    assert_eq!(poly.to_vec(), expected, "{name}: poly");
                }
                (actual, _) => panic!("{name}: presence mismatch, got {actual:?}"),
            }
        }
    }

    #[test]
    fn the_captured_enclosings_updates_areas_and_shrinks_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["enclosing"], "enclosing") {
            let name = case["case"].as_str().unwrap_or("?");
            let actual = minimum_enclosing_bbox(&read_boxes(&case["boxes"]));
            assert_eq!(actual, read_box(&case["enclosing"]), "{name}");
        }
        for case in items(&fixture["region_updates"], "region_updates") {
            let region = read_box(&case["region"]);
            let actual = update_region_box(
                read_box(&case["bbox"]),
                [
                    region[0] as i64,
                    region[1] as i64,
                    region[2] as i64,
                    region[3] as i64,
                ],
            );
            let expected = read_box(&case["updated"]);
            assert_eq!(
                actual,
                [
                    expected[0] as i64,
                    expected[1] as i64,
                    expected[2] as i64,
                    expected[3] as i64
                ],
                "region update"
            );
        }
        for case in items(&fixture["areas"], "areas") {
            let actual = bbox_area(read_box(&case["bbox"]));
            assert!(
                (actual - case["area"].as_f64().unwrap_or(f64::NAN)).abs() < 1e-12,
                "area"
            );
        }
        for case in items(&fixture["shrinks"], "shrinks") {
            let name = case["case"].as_str().unwrap_or("?");
            let size = items(&case["image_size"], "size");
            let block_bboxes: std::collections::BTreeMap<usize, GlueBox> =
                match case["block_bboxes"].as_object() {
                    Some(map) => map
                        .iter()
                        .map(|(key, value)| {
                            (key.parse::<usize>().unwrap_or(usize::MAX), read_box(value))
                        })
                        .collect(),
                    None => panic!("{name}: block_bboxes"),
                };
            let idxes: Vec<usize> = read_usizes(&case["block_idxes"]);
            let (bbox, mut matched) = shrink_supplement_region_bbox(
                read_box(&case["supplement"]),
                read_box(&case["reference"]),
                size[0].as_f64().unwrap_or(1.0),
                size[1].as_f64().unwrap_or(1.0),
                &idxes,
                &block_bboxes,
            );
            matched.sort_unstable();
            assert_eq!(bbox, read_box(&case["result_bbox"]), "{name}: bbox");
            assert_eq!(matched, read_usizes(&case["matched"]), "{name}: matched");
        }
    }
}
