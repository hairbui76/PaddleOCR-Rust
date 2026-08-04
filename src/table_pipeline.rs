// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Composing table models into HTML: geometry, matching, and assembly.
//!
//! Roadmap item `TABLEPIPE-001`, first slice.
//!
//! `TableRecognitionV2` turns three model outputs — structure tokens, cell
//! boxes, and OCR boxes with text — into an HTML table. All three models are
//! ported (`TBLCLS-001`, `TBLCELL-001`, `TBLSTRUCT-001`) alongside the classic
//! OCR path, so what remains is the part that composes them.
//!
//! That part is **pure functions over boxes and token lists**, which is why it
//! is the first slice: it can be captured and matched without a single
//! inference session, and it is where the table actually gets built.
//!
//! # `compute_inter` is not IoU and is not symmetric
//!
//! The matcher's threshold is on `intersection / area(second box)`, not on IoU.
//! A large cell fully containing a small OCR box scores `1.0` one way round and
//! `0.01` the other. Substituting IoU would match almost nothing, and the
//! oracle captures both orders so the asymmetry is asserted rather than assumed.
//!
//! # Two branches this port cannot reach, because upstream cannot either
//!
//! `match_table_and_ocr` takes two flag lists and has branches for them
//! disagreeing. `get_table_recognition_res` — the only caller — passes
//! **`table_cells_flag` twice**, so those branches never run. Handed genuinely
//! different lists, upstream raises `KeyError`; that is recorded in the fixture.
//!
//! This port takes a single flag list, which makes the unreachable branches
//! structurally impossible rather than reproducing a crash nothing can trigger.
//!
//! # The route, and a threshold the config does not carry
//!
//! `predict_single_table_recognition_res` branches on the classifier's label:
//! `wired_table` selects the wired structure and cell models, `wireless_table`
//! the wireless pair. **Any other label leaves both predictions unbound** and
//! upstream raises `UnboundLocalError`; [`table_route`] returns `None` instead,
//! which is the same refusal expressed as a type.
//!
//! The cell detector is then called with **`threshold=0.3`**, written into the
//! pipeline with a comment explaining the choice. The artifact's own
//! `draw_threshold` is `0.5`. Taking the config value would silently drop cells
//! the reference pipeline keeps.
//!
//! # Not wired into a public API
//!
//! This slice is the composition logic. Running the four models in order,
//! cropping tables out of a page, and choosing between the wired and wireless
//! cell detectors are the rest of `TABLEPIPE-001`, and they need the artifact
//! plumbing `P9` is for.
#![allow(dead_code)]

use crate::error::{Error, InputViolation, Result};

/// The matcher's acceptance threshold, from `match_table_and_ocr`.
///
/// Strictly greater, not greater-or-equal.
pub(crate) const TABLE_MATCH_THRESHOLD: f64 = 0.7;

/// The row-grouping tolerance in `sort_table_cells_boxes`, in pixels.
pub(crate) const TABLE_ROW_TOLERANCE: f64 = 10.0;

/// The IoU threshold `cells_det_results_nms` suppresses above.
pub(crate) const TABLE_CELL_NMS_THRESHOLD: f64 = 0.3;

/// The threshold the pipeline passes to the cell detector.
///
/// **Not** the artifact's `draw_threshold`, which is `0.5`.
/// `predict_single_table_recognition_res` overrides it with `0.3` and a comment
/// explaining that it improves cell recall.
pub(crate) const TABLE_CELL_DETECTION_THRESHOLD: f32 = 0.3;

/// Which pair of models the classifier's label selects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TableRoute {
    /// `wired_table`: the wired structure and cell-detection models.
    Wired,
    /// `wireless_table`: the wireless pair.
    Wireless,
}

/// Maps the classifier's label to a route.
///
/// Returns `None` for anything else. Upstream has no `else` branch at all and
/// falls through to an `UnboundLocalError`; refusing by type says the same thing
/// where a caller can act on it.
#[must_use]
pub(crate) fn table_route(label: &str) -> Option<TableRoute> {
    match label {
        "wired_table" => Some(TableRoute::Wired),
        "wireless_table" => Some(TableRoute::Wireless),
        _ => None,
    }
}

/// Suppresses overlapping cell boxes by IoU, highest score first.
///
/// Note this uses **IoU**, unlike the matcher in
/// [`intersection_over_second`]. A box fully containing another has a low IoU,
/// so containment is **not** suppressed — which the captured corpus shows.
#[must_use]
pub(crate) fn suppress_overlapping_cells(
    boxes: &[Box],
    scores: &[f32],
    threshold: f64,
) -> (Vec<Box>, Vec<f32>) {
    if boxes.len() != scores.len() {
        return (Vec::new(), Vec::new());
    }
    // `scores.argsort()[::-1]`: ascending, then reversed, so equal scores come
    // out **highest index first**. Captured rather than assumed.
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|left, right| scores[*left].total_cmp(&scores[*right]));
    order.reverse();

    let mut kept_boxes = Vec::new();
    let mut kept_scores = Vec::new();
    while let Some((&current, rest)) = order.split_first() {
        kept_boxes.push(boxes[current]);
        kept_scores.push(scores[current]);
        let survivors: Vec<usize> = rest
            .iter()
            .copied()
            .filter(|other| intersection_over_union(boxes[current], boxes[*other]) <= threshold)
            .collect();
        order = survivors;
    }
    (kept_boxes, kept_scores)
}

/// Re-expresses OCR boxes in a table region's coordinate space.
///
/// Boxes not **fully** inside the region are discarded, not clipped. A box
/// exactly on the boundary is kept: the comparison is inclusive on all four
/// edges.
#[must_use]
pub(crate) fn crop_ocr_boxes_to_table(ocr_boxes: &[Box], table_box: Box) -> Vec<Box> {
    ocr_boxes
        .iter()
        .filter(|entry| {
            entry[0] >= table_box[0]
                && entry[1] >= table_box[1]
                && entry[2] <= table_box[2]
                && entry[3] <= table_box[3]
        })
        .map(|entry| {
            [
                entry[0] - table_box[0],
                entry[1] - table_box[1],
                entry[2] - table_box[0],
                entry[3] - table_box[1],
            ]
        })
        .collect()
}

/// An axis-aligned box, `[left, top, right, bottom]`.
pub(crate) type Box = [f64; 4];

/// Intersection over the **second** box's area.
///
/// Not IoU and not symmetric. A zero-area second box scores `0`, matching
/// upstream's explicit guard rather than producing an infinity.
#[must_use]
pub(crate) fn intersection_over_second(first: Box, second: Box) -> f64 {
    let left = first[0].max(second[0]);
    let top = first[1].max(second[1]);
    let right = first[2].min(second[2]);
    let bottom = first[3].min(second[3]);
    let width = (right - left).max(0.0);
    let height = (bottom - top).max(0.0);
    let area = (second[2] - second[0]) * (second[3] - second[1]);
    if area == 0.0 {
        return 0.0;
    }
    width * height / area
}

/// Ordinary intersection over union.
///
/// Boxes that only touch score `0`: the guard is `>=`, so a shared edge is not
/// an intersection.
#[must_use]
pub(crate) fn intersection_over_union(first: Box, second: Box) -> f64 {
    let area_first = (first[2] - first[0]) * (first[3] - first[1]);
    let area_second = (second[2] - second[0]) * (second[3] - second[1]);
    let left = first[0].max(second[0]);
    let right = first[2].min(second[2]);
    let top = first[1].max(second[1]);
    let bottom = first[3].min(second[3]);
    if left >= right || top >= bottom {
        return 0.0;
    }
    let intersect = (right - left) * (bottom - top);
    intersect / (area_first + area_second - intersect)
}

/// Centre-to-centre distance plus the smaller of the two corner distances.
///
/// Upstream's own composite, reproduced rather than replaced by a plain centre
/// distance: the corner term is what separates two boxes with the same centre
/// and different extents.
#[must_use]
pub(crate) fn box_distance(first: Box, second: Box) -> f64 {
    let centre_first = ((first[0] + first[2]) / 2.0, (first[1] + first[3]) / 2.0);
    let centre_second = ((second[0] + second[2]) / 2.0, (second[1] + second[3]) / 2.0);
    let centres = ((centre_second.0 - centre_first.0).powi(2)
        + (centre_second.1 - centre_first.1).powi(2))
    .sqrt();
    let top_left = (second[0] - first[0]).abs() + (second[1] - first[1]).abs();
    let bottom_right = (second[2] - first[2]).abs() + (second[3] - first[3]).abs();
    centres + top_left.min(bottom_right)
}

/// Sorts cell boxes into reading order and returns the row boundaries.
///
/// Boxes are ordered by top edge, grouped into rows within
/// [`TABLE_ROW_TOLERANCE`] of the row's **first** box — not of a running mean —
/// and each row is then ordered by left edge. The returned flags are cumulative
/// counts starting at `0`.
#[must_use]
pub(crate) fn sort_cell_boxes(boxes: &[Box]) -> (Vec<Box>, Vec<usize>) {
    let mut by_top: Vec<Box> = boxes.to_vec();
    // A stable sort, matching Python's `sorted`: boxes with equal tops keep
    // their input order, which the row grouping below depends on.
    by_top.sort_by(|left, right| left[1].total_cmp(&right[1]));

    let mut rows: Vec<Vec<Box>> = Vec::new();
    let mut current: Vec<Box> = Vec::new();
    let mut current_top: Option<f64> = None;
    for entry in by_top {
        match current_top {
            None => {
                current.push(entry);
                current_top = Some(entry[1]);
            }
            Some(top) if (entry[1] - top).abs() <= TABLE_ROW_TOLERANCE => {
                current.push(entry);
            }
            Some(_) => {
                current.sort_by(|left, right| left[0].total_cmp(&right[0]));
                rows.push(std::mem::take(&mut current));
                current.push(entry);
                current_top = Some(entry[1]);
            }
        }
    }
    if !current.is_empty() {
        current.sort_by(|left, right| left[0].total_cmp(&right[0]));
        rows.push(current);
    }

    let mut sorted = Vec::with_capacity(boxes.len());
    let mut flags = vec![0_usize];
    for row in &rows {
        sorted.extend_from_slice(row);
        let last = *flags.last().unwrap_or(&0);
        flags.push(last + row.len());
    }
    (sorted, flags)
}

/// Returns the cell index that starts each row of a token list.
///
/// Counts every `</td>` and `<td></td>`, and records the count at the first
/// such token inside each `<tr>`.
#[must_use]
pub(crate) fn row_start_indices(tokens: &[String]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut index = 0_usize;
    let mut inside_row = false;
    for token in tokens {
        match token.as_str() {
            "<tr>" => inside_row = true,
            "</tr>" => inside_row = false,
            "</td>" | "<td></td>" if inside_row => {
                starts.push(index);
                inside_row = false;
            }
            _ => {}
        }
        if token == "</td>" || token == "<td></td>" {
            index += 1;
        }
    }
    starts
}

/// Aligns detected-cell row boundaries onto the structure's row starts.
///
/// For each structure row start, takes the largest cell flag not past it,
/// carrying the previous value forward when a row start has no flag of its own.
#[must_use]
pub(crate) fn align_row_flags(cell_flags: &[usize], row_starts: &[usize]) -> Vec<usize> {
    let mut aligned = Vec::with_capacity(row_starts.len());
    let mut cursor = 0_usize;
    let mut best: Option<usize> = None;
    for start in row_starts {
        while cursor < cell_flags.len() && cell_flags[cursor] <= *start {
            if best.is_none_or(|value| cell_flags[cursor] > value) {
                best = Some(cell_flags[cursor]);
            }
            cursor += 1;
        }
        aligned.push(best.unwrap_or(*start));
    }
    aligned
}

/// One row's cell-to-OCR assignment: cell index within the row to OCR indices.
pub(crate) type RowMatches = Vec<(usize, Vec<usize>)>;

/// Matches cells to OCR boxes, one row at a time.
///
/// `flags` is the cumulative row boundary list; upstream passes the same list
/// for both of its flag parameters, which is why this takes one.
pub(crate) fn match_cells_to_ocr(
    cells: &[Box],
    ocr_boxes: &[Box],
    flags: &[usize],
) -> Result<Vec<RowMatches>> {
    if flags.is_empty() {
        return Ok(Vec::new());
    }
    if flags.windows(2).any(|pair| pair[0] > pair[1])
        || flags.last().is_some_and(|last| *last > cells.len())
    {
        return Err(Error::InvalidInput {
            field: "table_pipeline.row_flags",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut all = Vec::with_capacity(flags.len().saturating_sub(1));
    for window in flags.windows(2) {
        let mut matched: RowMatches = Vec::new();
        for (position, cell) in cells[window[0]..window[1]].iter().enumerate() {
            for (index, ocr) in ocr_boxes.iter().enumerate() {
                if intersection_over_second(*cell, *ocr) > TABLE_MATCH_THRESHOLD {
                    match matched.iter_mut().find(|(key, _)| *key == position) {
                        Some((_, values)) => values.push(index),
                        None => matched.push((position, vec![index])),
                    }
                }
            }
        }
        all.push(matched);
    }
    Ok(all)
}

/// Assembles the final HTML from tokens, matches, and recognized text.
///
/// `row_starts` is what the caller passes as the flag list — upstream's
/// parameter is *named* `table_cells_flag` and its only caller hands it
/// `row_start_index`, so the name is misleading and the behaviour is the
/// caller's.
pub(crate) fn table_html(
    matches: &[RowMatches],
    texts: &[String],
    tokens: &[String],
    row_starts: &[usize],
) -> Result<String> {
    if tokens.len() < 6 {
        return Err(Error::InvalidInput {
            field: "table_pipeline.tokens",
            violation: InputViolation::OutOfRange,
        });
    }
    let mut html = String::new();
    for token in &tokens[0..3] {
        html.push_str(token);
    }

    let mut cell_index = 0_usize;
    let mut cell_count = 0_usize;
    let mut row = 0_usize;
    for token in &tokens[3..tokens.len() - 3] {
        if !token.contains("</td>") {
            html.push_str(token);
            continue;
        }
        let empty_cell = token == "<td></td>";
        if empty_cell {
            html.push_str("<td>");
        }

        let mut skipped = false;
        if let Some((_, indices)) = matches
            .get(row)
            .and_then(|entries| entries.iter().find(|(key, _)| *key == cell_index))
        {
            if indices.is_empty() {
                // Upstream `continue`s here, which emits the opening `<td>` and
                // never closes it. Unreachable from the real caller, since an
                // empty list only comes from a branch that caller cannot take —
                // but reproduced rather than quietly repaired.
                skipped = true;
            } else {
                let multiple = indices.len() > 1;
                let bold = multiple
                    && texts
                        .get(indices[0])
                        .is_some_and(|text| text.contains("<b>"));
                if bold {
                    html.push_str("<b>");
                }
                for (position, index) in indices.iter().enumerate() {
                    let Some(content) = texts.get(*index) else {
                        return Err(Error::InvalidInput {
                            field: "table_pipeline.text_index",
                            violation: InputViolation::OutOfRange,
                        });
                    };
                    if !multiple {
                        html.push_str(content);
                        continue;
                    }
                    let mut content = content.as_str();
                    if content.is_empty() {
                        continue;
                    }
                    if content.starts_with(' ') {
                        content = &content[1..];
                    }
                    if content.contains("<b>") {
                        content = &content[3..];
                    }
                    if content.contains("</b>") {
                        content = &content[..content.len() - 4];
                    }
                    if content.is_empty() {
                        continue;
                    }
                    html.push_str(content);
                    if position != indices.len() - 1 && !content.ends_with(' ') {
                        html.push(' ');
                    }
                }
                if bold {
                    html.push_str("</b>");
                }
            }
        }

        if !skipped {
            if empty_cell {
                html.push_str("</td>");
            } else {
                html.push_str(token);
            }
        }
        cell_index += 1;
        cell_count += 1;
        if row + 1 < row_starts.len()
            && cell_count >= row_starts[row + 1]
            && row + 1 < matches.len()
        {
            row += 1;
            cell_index = 0;
        }
    }

    for token in &tokens[tokens.len() - 3..] {
        html.push_str(token);
    }
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-table-pipeline/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn read_box(value: &Value) -> Box {
        let values = match value.as_array() {
            Some(values) => values,
            None => panic!("box"),
        };
        [
            values[0].as_f64().unwrap_or(f64::NAN),
            values[1].as_f64().unwrap_or(f64::NAN),
            values[2].as_f64().unwrap_or(f64::NAN),
            values[3].as_f64().unwrap_or(f64::NAN),
        ]
    }

    fn read_strings(value: &Value) -> Vec<String> {
        match value.as_array() {
            Some(values) => values
                .iter()
                .map(|entry| entry.as_str().unwrap_or_default().to_owned())
                .collect(),
            None => panic!("strings"),
        }
    }

    /// Every captured geometry case, all three functions and the swap.
    #[test]
    fn the_captured_geometry_is_reproduced() {
        let fixture = fixture();
        let cases = match fixture["geometry"].as_array() {
            Some(value) => value,
            None => panic!("geometry"),
        };
        assert_eq!(cases.len(), 8);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let first = read_box(&case["first"]);
            let second = read_box(&case["second"]);
            for (label, actual, expected) in [
                (
                    "compute_inter",
                    intersection_over_second(first, second),
                    case["compute_inter"].as_f64().unwrap_or(f64::NAN),
                ),
                (
                    "compute_iou",
                    intersection_over_union(first, second),
                    case["compute_iou"].as_f64().unwrap_or(f64::NAN),
                ),
                (
                    "distance",
                    box_distance(first, second),
                    case["distance"].as_f64().unwrap_or(f64::NAN),
                ),
                (
                    "compute_inter_swapped",
                    intersection_over_second(second, first),
                    case["compute_inter_swapped"].as_f64().unwrap_or(f64::NAN),
                ),
            ] {
                assert!(
                    (actual - expected).abs() < 1e-9,
                    "{name}: {label}: {actual} vs {expected}"
                );
            }
        }
    }

    /// The matcher's score is not symmetric, and the fixture proves it.
    #[test]
    fn the_matcher_score_is_not_symmetric() {
        let fixture = fixture();
        let cases = match fixture["geometry"].as_array() {
            Some(value) => value,
            None => panic!("geometry"),
        };
        let asymmetric = cases.iter().filter(|case| {
            let forward = case["compute_inter"].as_f64().unwrap_or(0.0);
            let backward = case["compute_inter_swapped"].as_f64().unwrap_or(0.0);
            (forward - backward).abs() > 1e-9
        });
        assert!(
            asymmetric.count() >= 2,
            "the corpus must contain cases where the order matters"
        );
    }

    #[test]
    fn the_captured_row_starts_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["row_starts"].as_array() {
            Some(value) => value,
            None => panic!("row_starts"),
        };
        assert_eq!(cases.len(), 4);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let tokens = read_strings(&case["tokens"]);
            let expected: Vec<usize> = match case["row_start_index"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or(0) as usize)
                    .collect(),
                None => panic!("{name}: row_start_index"),
            };
            assert_eq!(row_start_indices(&tokens), expected, "{name}");
        }
    }

    /// The sort, its flags, and the alignment onto structure rows.
    #[test]
    fn the_captured_sorting_is_reproduced() {
        let fixture = fixture();
        let sorting = &fixture["sorting"];
        let input: Vec<Box> = match sorting["input"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("input"),
        };
        let expected_sorted: Vec<Box> = match sorting["sorted"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("sorted"),
        };
        let (sorted, flags) = sort_cell_boxes(&input);
        assert_eq!(sorted, expected_sorted, "sorted boxes");

        let expected_flags: Vec<usize> = match sorting["raw_flag"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("raw_flag"),
        };
        assert_eq!(flags, expected_flags, "row flags");

        let row_starts: Vec<usize> = match sorting["row_start_index"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("row_start_index"),
        };
        let mut aligned = align_row_flags(&flags, &row_starts);
        aligned.push(sorted.len());
        let expected_aligned: Vec<usize> = match sorting["table_cells_flag"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("table_cells_flag"),
        };
        assert_eq!(aligned, expected_aligned, "aligned flags");
    }

    /// The captured cell-to-OCR assignment, including the orphan box.
    #[test]
    fn the_captured_matching_is_reproduced() {
        let fixture = fixture();
        let matching = &fixture["matching"];
        let cells: Vec<Box> = match matching["cell_boxes"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("cell_boxes"),
        };
        let ocr: Vec<Box> = match matching["ocr_boxes"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("ocr_boxes"),
        };
        let flags: Vec<usize> = match matching["table_cells_flag"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("table_cells_flag"),
        };
        let (sorted, _) = sort_cell_boxes(&cells);
        let matched = match match_cells_to_ocr(&sorted, &ocr, &flags) {
            Ok(value) => value,
            Err(error) => panic!("match: {error}"),
        };

        let expected = match matching["result"]["value"].as_array() {
            Some(value) => value,
            None => panic!("result"),
        };
        assert_eq!(matched.len(), expected.len(), "row count");
        for (row, entries) in expected.iter().enumerate() {
            let entries = match entries.as_object() {
                Some(value) => value,
                None => panic!("row {row}"),
            };
            assert_eq!(matched[row].len(), entries.len(), "row {row} cell count");
            for (key, value) in entries {
                let cell: usize = key.parse().unwrap_or(usize::MAX);
                let indices: Vec<usize> = match value.as_array() {
                    Some(values) => values
                        .iter()
                        .map(|entry| entry.as_u64().unwrap_or(0) as usize)
                        .collect(),
                    None => panic!("row {row} cell {cell}"),
                };
                let found = matched[row].iter().find(|(index, _)| *index == cell);
                match found {
                    Some((_, actual)) => assert_eq!(*actual, indices, "row {row} cell {cell}"),
                    None => panic!("row {row} cell {cell} missing"),
                }
            }
        }

        // The fifth OCR box overlaps nothing and must appear in no row.
        let orphan = ocr.len() - 1;
        assert!(
            !matched
                .iter()
                .flatten()
                .any(|(_, indices)| indices.contains(&orphan)),
            "the orphan OCR box must not be matched"
        );
    }

    /// The captured HTML, byte for byte.
    #[test]
    fn the_captured_html_is_reproduced() {
        let fixture = fixture();
        let cases = match fixture["html"].as_array() {
            Some(value) => value,
            None => panic!("html"),
        };
        assert_eq!(cases.len(), 2);

        // Case one: the 2x2 table the matching test also uses.
        let matching = &fixture["matching"];
        let cells: Vec<Box> = match matching["cell_boxes"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("cell_boxes"),
        };
        let ocr: Vec<Box> = match matching["ocr_boxes"].as_array() {
            Some(values) => values.iter().map(read_box).collect(),
            None => panic!("ocr_boxes"),
        };
        let texts = read_strings(&matching["ocr_texts"]);
        let flags: Vec<usize> = match matching["table_cells_flag"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("table_cells_flag"),
        };
        let row_starts: Vec<usize> = match matching["row_start_index"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("row_start_index"),
        };
        let tokens = {
            let row_cases = match fixture["row_starts"].as_array() {
                Some(value) => value,
                None => panic!("row_starts"),
            };
            let entry = row_cases
                .iter()
                .find(|entry| entry["case"] == "two_by_two")
                .unwrap_or(&Value::Null);
            read_strings(&entry["tokens"])
        };

        let (sorted, _) = sort_cell_boxes(&cells);
        let matched = match match_cells_to_ocr(&sorted, &ocr, &flags) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let html = match table_html(&matched, &texts, &tokens, &row_starts) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let expected = cases[0]["value"].as_str().unwrap_or("");
        assert_eq!(html, expected, "two_by_two");

        // Case two: two OCR boxes in one cell, which joins them with a space.
        let single = [[0.0, 0.0, 50.0, 20.0]];
        let merged = [[2.0, 2.0, 24.0, 18.0], [26.0, 2.0, 48.0, 18.0]];
        let single_tokens: Vec<String> = [
            "<html>",
            "<body>",
            "<table>",
            "<tbody>",
            "<tr>",
            "<td></td>",
            "</tr>",
            "</tbody>",
            "</table>",
            "</body>",
            "</html>",
        ]
        .iter()
        .map(|token| (*token).to_owned())
        .collect();
        let (sorted, flags) = sort_cell_boxes(&single);
        let starts = row_start_indices(&single_tokens);
        let mut aligned = align_row_flags(&flags, &starts);
        aligned.push(sorted.len());
        let matched = match match_cells_to_ocr(&sorted, &merged, &aligned) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let mut starts_full = starts;
        starts_full.push(sorted.len());
        let texts: Vec<String> = ["left", "right"]
            .iter()
            .map(|text| (*text).to_owned())
            .collect();
        let html = match table_html(&matched, &texts, &single_tokens, &starts_full) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(
            html,
            cases[1]["value"].as_str().unwrap_or(""),
            "two_boxes_in_one_cell"
        );
        assert!(html.contains("left right"), "the join inserts one space");
    }

    /// A cell with no OCR box produces an empty `<td></td>`, not a skipped one.
    #[test]
    fn an_unmatched_cell_still_closes_its_tag() {
        let tokens: Vec<String> = [
            "<html>",
            "<body>",
            "<table>",
            "<tbody>",
            "<tr>",
            "<td></td>",
            "<td></td>",
            "</tr>",
            "</tbody>",
            "</table>",
            "</body>",
            "</html>",
        ]
        .iter()
        .map(|token| (*token).to_owned())
        .collect();
        // Only the first cell is matched.
        let matched = vec![vec![(0_usize, vec![0_usize])]];
        let texts = vec!["only".to_owned()];
        let html = match table_html(&matched, &texts, &tokens, &[0, 2]) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(
            html,
            "<html><body><table><tbody><tr><td>only</td><td></td></tr></tbody></table></body></html>"
        );
    }

    /// Upstream raises on mismatched flag lists; this port cannot be handed them.
    ///
    /// The fixture records the `KeyError`, and this asserts that the recorded
    /// behaviour is the one this port structurally avoids rather than one it
    /// silently diverges from.
    #[test]
    fn the_unreachable_branch_is_recorded_as_a_crash_upstream() {
        let fixture = fixture();
        let branch = &fixture["matching"]["unreachable_branch"];
        assert_eq!(branch["ok"].as_bool(), Some(false));
        assert_eq!(branch["error_type"].as_str(), Some("KeyError"));
    }

    /// Malformed flag lists are refused rather than indexed out of bounds.
    #[test]
    fn malformed_flags_are_refused() {
        let cells = [[0.0, 0.0, 1.0, 1.0]];
        assert!(match_cells_to_ocr(&cells, &[], &[0, 5]).is_err());
        assert!(match_cells_to_ocr(&cells, &[], &[2, 1]).is_err());
        assert!(match_cells_to_ocr(&cells, &[], &[]).is_ok());
    }

    /// A token list too short to hold the wrapper is refused.
    #[test]
    fn a_truncated_token_list_is_refused() {
        let tokens: Vec<String> = ["<html>", "<body>"]
            .iter()
            .map(|token| (*token).to_owned())
            .collect();
        assert!(table_html(&[], &[], &tokens, &[]).is_err());
    }

    /// The captured NMS, including the tie order and the containment case.
    #[test]
    fn the_captured_cell_suppression_is_reproduced() {
        let fixture = fixture();
        let cases = match fixture["nms"].as_array() {
            Some(value) => value,
            None => panic!("nms"),
        };
        assert_eq!(cases.len(), 4);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes: Vec<Box> = match case["boxes"].as_array() {
                Some(values) => values.iter().map(read_box).collect(),
                None => panic!("{name}: boxes"),
            };
            let scores: Vec<f32> = match case["scores"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(f64::NAN) as f32)
                    .collect(),
                None => panic!("{name}: scores"),
            };
            let (kept, kept_scores) =
                suppress_overlapping_cells(&boxes, &scores, TABLE_CELL_NMS_THRESHOLD);
            let expected: Vec<Box> = match case["kept_boxes"].as_array() {
                Some(values) => values.iter().map(read_box).collect(),
                None => panic!("{name}: kept_boxes"),
            };
            assert_eq!(kept, expected, "{name}: kept boxes");
            let expected_scores: Vec<f32> = match case["kept_scores"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(f64::NAN) as f32)
                    .collect(),
                None => panic!("{name}: kept_scores"),
            };
            assert_eq!(kept_scores, expected_scores, "{name}: kept scores");
        }
    }

    /// Containment survives NMS, because NMS uses IoU and the matcher does not.
    #[test]
    fn nms_does_not_suppress_containment() {
        let outer = [0.0, 0.0, 100.0, 100.0];
        let inner = [10.0, 10.0, 20.0, 20.0];
        let (kept, _) =
            suppress_overlapping_cells(&[outer, inner], &[0.8, 0.9], TABLE_CELL_NMS_THRESHOLD);
        assert_eq!(kept.len(), 2, "IoU is only 0.01, so neither is suppressed");
        // The matcher would score the same pair at 1.0 in one direction.
        assert!((intersection_over_second(outer, inner) - 1.0).abs() < 1e-9);
    }

    /// The captured cropping, including the boundary and the crossings.
    #[test]
    fn the_captured_cropping_is_reproduced() {
        let fixture = fixture();
        let cases = match fixture["crop"].as_array() {
            Some(value) => value,
            None => panic!("crop"),
        };
        assert_eq!(cases.len(), 5);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let table_box = read_box(&case["table_box"]);
            let ocr: Vec<Box> = match case["ocr_boxes"].as_array() {
                Some(values) => values.iter().map(read_box).collect(),
                None => panic!("{name}: ocr_boxes"),
            };
            let expected: Vec<Box> = match case["adjusted"].as_array() {
                Some(values) => values.iter().map(read_box).collect(),
                None => panic!("{name}: adjusted"),
            };
            assert_eq!(crop_ocr_boxes_to_table(&ocr, table_box), expected, "{name}");
        }
    }

    /// The route, including the label upstream has no branch for.
    #[test]
    fn the_route_refuses_an_unknown_label() {
        let fixture = fixture();
        let route = &fixture["route"];
        assert_eq!(
            table_route(route["wired_label"].as_str().unwrap_or("")),
            Some(TableRoute::Wired)
        );
        assert_eq!(
            table_route(route["wireless_label"].as_str().unwrap_or("")),
            Some(TableRoute::Wireless)
        );
        // Upstream falls through to an UnboundLocalError here.
        assert_eq!(table_route("borderless_table"), None);
        assert_eq!(table_route(""), None);
    }

    /// The pipeline overrides the artifact's own detection threshold.
    #[test]
    fn the_cell_detection_threshold_is_the_pipelines_not_the_artifacts() {
        let fixture = fixture();
        assert_eq!(
            fixture["route"]["cell_detection_threshold"]
                .as_f64()
                .unwrap_or(0.0) as f32,
            TABLE_CELL_DETECTION_THRESHOLD
        );
        // A compile-time assertion, because both sides are constants: the
        // pipeline keeps cells the artifact default would drop.
        const _: () =
            assert!(TABLE_CELL_DETECTION_THRESHOLD < crate::table_cells::TABLE_CELL_THRESHOLD);
    }

    /// Rows are grouped against the row's first box, not a running mean.
    #[test]
    fn row_grouping_anchors_on_the_first_box_of_the_row() {
        // Tops 0, 9, 18: the third is 18 from the anchor, so it starts a row,
        // even though it is only 9 from its predecessor.
        let boxes = [
            [0.0, 0.0, 10.0, 5.0],
            [20.0, 9.0, 30.0, 14.0],
            [40.0, 18.0, 50.0, 23.0],
        ];
        let (_, flags) = sort_cell_boxes(&boxes);
        assert_eq!(flags, vec![0, 2, 3]);
    }
}
