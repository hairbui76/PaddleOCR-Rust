// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Reading order: the XY-cut primitives `PP-StructureV3` orders blocks with.
//!
//! Roadmap item `STRUCT-001`, first slice.
//!
//! `xycut_enhanced` is `1,830` lines, and almost all of it is per-label
//! heuristics — titles, figures, regions. Underneath sit **four pure
//! functions** that are the algorithm itself:
//!
//! | Function | What it does |
//! |---|---|
//! | [`projection`] | boxes → a 1D occupancy histogram on one axis |
//! | [`split_profile`] | histogram → segments, split at gaps |
//! | [`recursive_yx_cut`] | project on `Y`, then `X`, then recurse |
//! | [`recursive_xy_cut`] | the mirror |
//!
//! They take integer boxes and return an ordering, so they are capturable and
//! matchable exactly — the same property that made
//! [`crate::table_pipeline`]'s composition portable ahead of its plumbing.
//!
//! # The two cut orders are not the same reading order
//!
//! On a two-column page, `yx` produces row-major order and `xy` produces
//! column-major:
//!
//! | Layout | `yx` | `xy` |
//! |---|---|---|
//! | Two columns, two rows | `[0, 1, 2, 3]` | **`[0, 2, 1, 3]`** |
//!
//! That is the whole difference between reading a page across and reading it
//! down, and it is why both are captured rather than one being assumed to be
//! "the" reading order.
//!
//! # Boxes are integers
//!
//! The projection uses coordinates as array indices. Upstream passes integer
//! boxes and this port takes `i64` rather than rounding a float box silently:
//! whoever holds the float coordinates should decide how they become indices.
//!
//! # What this is not
//!
//! Not the whole of `STRUCT-001`. The label-aware heuristics above these
//! functions — how a document title, a caption, or a figure is pulled out of
//! the plain ordering — are not ported, and several of them depend on P8
//! modules that have no published ONNX export.
#![allow(dead_code)]

use crate::error::{Error, InputViolation, Result};

/// An axis-aligned box in page pixels, `[left, top, right, bottom]`.
pub(crate) type OrderBox = [i64; 4];

/// Which axis a projection accumulates along.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Axis {
    /// Columns: `left` and `right`.
    Horizontal,
    /// Rows: `top` and `bottom`.
    Vertical,
}

impl Axis {
    const fn low(self) -> usize {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }

    const fn high(self) -> usize {
        match self {
            Self::Horizontal => 2,
            Self::Vertical => 3,
        }
    }
}

/// Builds the 1D occupancy histogram for one axis.
///
/// The histogram's length is the largest coordinate on that axis — or the
/// magnitude of the smallest when any is negative, which is upstream's own
/// branch and is reproduced rather than tidied. Intervals are accumulated with
/// their coordinates' absolute values, so a negative box still lands somewhere.
pub(crate) fn projection(boxes: &[OrderBox], axis: Axis) -> Result<Vec<u32>> {
    if boxes.is_empty() {
        return Err(Error::InvalidInput {
            field: "reading_order.boxes",
            violation: InputViolation::Empty,
        });
    }
    let (low, high) = (axis.low(), axis.high());
    let mut minimum = i64::MAX;
    let mut maximum = i64::MIN;
    for entry in boxes {
        minimum = minimum.min(entry[low]).min(entry[high]);
        maximum = maximum.max(entry[low]).max(entry[high]);
    }
    let length = if minimum < 0 {
        minimum.unsigned_abs()
    } else {
        u64::try_from(maximum).unwrap_or(0)
    };
    let length = usize::try_from(length).map_err(|_| Error::InvalidInput {
        field: "reading_order.extent",
        violation: InputViolation::OutOfRange,
    })?;
    // A bound, because the histogram is allocated from caller-supplied
    // coordinates: a page is not 100 million pixels wide, and a box that claims
    // to be is hostile input rather than a layout.
    if length > 1_000_000 {
        return Err(Error::InvalidInput {
            field: "reading_order.extent",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut histogram = vec![0_u32; length];
    for entry in boxes {
        let start = entry[low].unsigned_abs() as usize;
        let end = entry[high].unsigned_abs() as usize;
        // Python slicing clamps rather than panicking, and `a[5:2]` is empty
        // rather than an error. Rust's range panics on both, so both are
        // handled: an inverted box -- right edge left of the left edge -- is
        // exactly what a fuzz campaign found here.
        let start = start.min(length);
        let end = end.min(length).max(start);
        for slot in &mut histogram[start..end] {
            *slot += 1;
        }
    }
    Ok(histogram)
}

/// Splits a projection profile into `[start, end)` segments.
///
/// Returns `None` when nothing exceeds `min_value`, matching upstream's bare
/// `return`. A gap splits only when it is **strictly wider** than `min_gap`.
#[must_use]
pub(crate) fn split_profile(
    values: &[u32],
    min_value: u32,
    min_gap: usize,
) -> Option<(Vec<usize>, Vec<usize>)> {
    let significant: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > min_value)
        .map(|(index, _)| index)
        .collect();
    let first = *significant.first()?;
    let last = *significant.last()?;

    let mut starts = vec![first];
    let mut ends = Vec::new();
    for pair in significant.windows(2) {
        if pair[1] - pair[0] > min_gap {
            ends.push(pair[0]);
            starts.push(pair[1]);
        }
    }
    ends.push(last + 1);
    Some((starts, ends))
}

/// Sorts by one axis's low edge, returning the permutation.
///
/// A **stable** sort. NumPy's default `argsort` is not stable, so upstream's
/// tie order is formally unspecified; the captured cases include ties and this
/// port matches them, which is the most that can be claimed.
fn order_by(boxes: &[OrderBox], axis: Axis) -> Vec<usize> {
    let low = axis.low();
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by_key(|slot| boxes[*slot][low]);
    order
}

/// Projects on `Y`, then `X`, then recurses. Appends to `result`.
pub(crate) fn recursive_yx_cut(
    boxes: &[OrderBox],
    indices: &[usize],
    result: &mut Vec<usize>,
) -> Result<()> {
    if boxes.len() != indices.len() {
        return Err(Error::InvalidInput {
            field: "reading_order.indices",
            violation: InputViolation::OutOfRange,
        });
    }
    if boxes.is_empty() {
        return Ok(());
    }

    let order = order_by(boxes, Axis::Vertical);
    let sorted: Vec<OrderBox> = order.iter().map(|slot| boxes[*slot]).collect();
    let sorted_indices: Vec<usize> = order.iter().map(|slot| indices[*slot]).collect();

    let Some((y_starts, y_ends)) = split_profile(&projection(&sorted, Axis::Vertical)?, 0, 1)
    else {
        return Ok(());
    };

    for (y_start, y_end) in y_starts.iter().zip(&y_ends) {
        let band: Vec<usize> = (0..sorted.len())
            .filter(|slot| {
                let top = sorted[*slot][1];
                top >= i64::try_from(*y_start).unwrap_or(i64::MAX)
                    && top < i64::try_from(*y_end).unwrap_or(i64::MAX)
            })
            .collect();
        if band.is_empty() {
            continue;
        }
        let band_boxes: Vec<OrderBox> = band.iter().map(|slot| sorted[*slot]).collect();
        let band_indices: Vec<usize> = band.iter().map(|slot| sorted_indices[*slot]).collect();

        let column_order = order_by(&band_boxes, Axis::Horizontal);
        let column_boxes: Vec<OrderBox> =
            column_order.iter().map(|slot| band_boxes[*slot]).collect();
        let column_indices: Vec<usize> = column_order
            .iter()
            .map(|slot| band_indices[*slot])
            .collect();

        let Some((mut x_starts, mut x_ends)) =
            split_profile(&projection(&column_boxes, Axis::Horizontal)?, 0, 1)
        else {
            continue;
        };
        // One column: this band is already in order, so emit it and stop.
        if x_starts.len() == 1 {
            result.extend_from_slice(&column_indices);
            continue;
        }
        // Negated coordinates — a vertical region flipped for right-to-left
        // reading. The histogram was built from absolute values, so the
        // columns come out left to right; upstream flips the interval list to
        // visit them right to left.
        if column_boxes.iter().any(|entry| entry[0] < 0) {
            x_starts.reverse();
            x_ends.reverse();
        }

        for (x_start, x_end) in x_starts.iter().zip(&x_ends) {
            let column: Vec<usize> = (0..column_boxes.len())
                .filter(|slot| {
                    let left = column_boxes[*slot][0].unsigned_abs();
                    left >= *x_start as u64 && left < *x_end as u64
                })
                .collect();
            if column.is_empty() {
                continue;
            }
            let next_boxes: Vec<OrderBox> = column.iter().map(|slot| column_boxes[*slot]).collect();
            let next_indices: Vec<usize> =
                column.iter().map(|slot| column_indices[*slot]).collect();
            recursive_yx_cut(&next_boxes, &next_indices, result)?;
        }
    }
    Ok(())
}

/// Projects on `X`, then `Y`, then recurses. Appends to `result`.
pub(crate) fn recursive_xy_cut(
    boxes: &[OrderBox],
    indices: &[usize],
    result: &mut Vec<usize>,
) -> Result<()> {
    if boxes.len() != indices.len() {
        return Err(Error::InvalidInput {
            field: "reading_order.indices",
            violation: InputViolation::OutOfRange,
        });
    }
    if boxes.is_empty() {
        return Ok(());
    }

    let order = order_by(boxes, Axis::Horizontal);
    let sorted: Vec<OrderBox> = order.iter().map(|slot| boxes[*slot]).collect();
    let sorted_indices: Vec<usize> = order.iter().map(|slot| indices[*slot]).collect();

    let Some((mut x_starts, mut x_ends)) =
        split_profile(&projection(&sorted, Axis::Horizontal)?, 0, 1)
    else {
        return Ok(());
    };
    // See `recursive_yx_cut`: negated x means right-to-left column order.
    if sorted.iter().any(|entry| entry[0] < 0) {
        x_starts.reverse();
        x_ends.reverse();
    }

    for (x_start, x_end) in x_starts.iter().zip(&x_ends) {
        let column: Vec<usize> = (0..sorted.len())
            .filter(|slot| {
                let left = sorted[*slot][0].unsigned_abs();
                left >= *x_start as u64 && left < *x_end as u64
            })
            .collect();
        if column.is_empty() {
            continue;
        }
        let column_boxes: Vec<OrderBox> = column.iter().map(|slot| sorted[*slot]).collect();
        let column_indices: Vec<usize> = column.iter().map(|slot| sorted_indices[*slot]).collect();

        let row_order = order_by(&column_boxes, Axis::Vertical);
        let row_boxes: Vec<OrderBox> = row_order.iter().map(|slot| column_boxes[*slot]).collect();
        let row_indices: Vec<usize> = row_order.iter().map(|slot| column_indices[*slot]).collect();

        let Some((y_starts, y_ends)) =
            split_profile(&projection(&row_boxes, Axis::Vertical)?, 0, 1)
        else {
            continue;
        };
        if y_starts.len() == 1 {
            result.extend_from_slice(&row_indices);
            continue;
        }

        for (y_start, y_end) in y_starts.iter().zip(&y_ends) {
            let band: Vec<usize> = (0..row_boxes.len())
                .filter(|slot| {
                    let top = row_boxes[*slot][1];
                    top >= i64::try_from(*y_start).unwrap_or(i64::MAX)
                        && top < i64::try_from(*y_end).unwrap_or(i64::MAX)
                })
                .collect();
            if band.is_empty() {
                continue;
            }
            let next_boxes: Vec<OrderBox> = band.iter().map(|slot| row_boxes[*slot]).collect();
            let next_indices: Vec<usize> = band.iter().map(|slot| row_indices[*slot]).collect();
            recursive_xy_cut(&next_boxes, &next_indices, result)?;
        }
    }
    Ok(())
}

/// A float box for the heuristic layer, `[left, top, right, bottom]`.
pub(crate) type HeuristicBox = [f64; 4];

/// Projection overlap of two boxes on one axis, over their union extent.
///
/// The heuristic layer's `calculate_projection_overlap_ratio` with its default
/// `union` mode: intersection length over combined extent, `0` when disjoint.
#[must_use]
pub(crate) fn projection_overlap_ratio(
    first: HeuristicBox,
    second: HeuristicBox,
    axis: Axis,
) -> f64 {
    let (start, end) = match axis {
        Axis::Horizontal => (0, 2),
        Axis::Vertical => (1, 3),
    };
    let overlap = first[end].min(second[end]) - first[start].max(second[start]);
    if overlap <= 0.0 {
        return 0.0;
    }
    let extent = first[end].max(second[end]) - first[start].min(second[start]);
    overlap / extent
}

/// The per-label directional weights, `[left, right, up, down]`.
///
/// `doc_title` prefers matches to its left-and-down (or right-to-left when the
/// region is vertical); titles and vision labels prefer downward matches; every
/// other label prefers upward ones. The table is upstream's, entry for entry.
#[must_use]
pub(crate) fn label_weights(label: &str, vertical: bool) -> [f64; 4] {
    match label {
        "doc_title" => {
            if vertical {
                [0.2, 0.1, 1.0, 1.0]
            } else {
                [1.0, 0.1, 0.1, 1.0]
            }
        }
        "paragraph_title" | "table_title" | "abstract" | "image" | "seal" | "chart" | "figure" => {
            [1.0, 1.0, 0.1, 1.0]
        }
        _ => [1.0, 1.0, 1.0, 0.1],
    }
}

/// Weighted nearest-edge distance between two boxes.
///
/// Zero when the boxes overlap on **both** projections. Otherwise the nearest
/// horizontal gap weighted by `left`/`right` and the nearest vertical gap by
/// `up`/`down` — where the side is chosen by which of the boxes leads, so a
/// `doc_title` reaching downward pays a tenth of what it pays reaching up.
#[must_use]
pub(crate) fn nearest_edge_distance(
    first: HeuristicBox,
    second: HeuristicBox,
    weight: [f64; 4],
) -> f64 {
    let horizontal = projection_overlap_ratio(first, second, Axis::Horizontal);
    let vertical = projection_overlap_ratio(first, second, Axis::Vertical);
    if horizontal > 0.0 && vertical > 0.0 {
        return 0.0;
    }
    let mut distance = 0.0;
    if horizontal == 0.0 {
        let gap = (first[0] - second[2])
            .abs()
            .min((first[2] - second[0]).abs());
        distance += gap
            * if first[2] < second[0] {
                weight[0]
            } else {
                weight[1]
            };
    }
    if vertical == 0.0 {
        let gap = (first[1] - second[3])
            .abs()
            .min((first[3] - second[1]).abs());
        distance += gap
            * if first[3] < second[1] {
                weight[2]
            } else {
                weight[3]
            };
    }
    distance
}

/// Weighted Manhattan distance between two points.
#[must_use]
pub(crate) fn manhattan_distance(
    first: (f64, f64),
    second: (f64, f64),
    weight_x: f64,
    weight_y: f64,
) -> f64 {
    weight_x * (first.0 - second.0).abs() + weight_y * (first.1 - second.1).abs()
}

/// Sorts plain blocks into reading order, returning the permutation.
///
/// Upstream's `sort_normal_blocks`: coordinates are quantized to text-line
/// cells with **floor** division — Python's `//`, so a negative coordinate
/// floors rather than truncates — and ties inside a cell fall back to the
/// squared centroid distance from the origin (horizontal regions) or from the
/// top-right (vertical ones, via the negated `x` term).
#[must_use]
pub(crate) fn sort_plain_blocks(
    boxes: &[HeuristicBox],
    text_line_height: f64,
    text_line_width: f64,
    vertical: bool,
) -> Vec<usize> {
    let centroid = |b: &HeuristicBox| ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|left, right| {
        let (a, b) = (&boxes[*left], &boxes[*right]);
        let (ac, bc) = (centroid(a), centroid(b));
        let key = |b: &HeuristicBox, c: (f64, f64)| -> (f64, f64, f64) {
            if vertical {
                (
                    (-b[2] / text_line_width).floor(),
                    (b[1] / text_line_height).floor(),
                    -c.0 * c.0 + c.1 * c.1,
                )
            } else {
                (
                    (b[1] / text_line_height).floor(),
                    (b[0] / text_line_width).floor(),
                    c.0 * c.0 + c.1 * c.1,
                )
            }
        };
        let (ka, kb) = (key(a, ac), key(b, bc));
        ka.0.total_cmp(&kb.0)
            .then(ka.1.total_cmp(&kb.1))
            .then(ka.2.total_cmp(&kb.2))
    });
    order
}

#[cfg(test)]
mod tests {
    mod heuristics {
        use super::super::*;

        use serde_json::Value;

        const FIXTURE: &str =
            include_str!("../tests/fixtures/classic-v1-heuristic-primitives/expected.json");

        fn fixture() -> Value {
            match serde_json::from_str(FIXTURE) {
                Ok(value) => value,
                Err(error) => panic!("fixture: {error}"),
            }
        }

        fn read_box(value: &Value) -> HeuristicBox {
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

        #[test]
        fn the_captured_overlaps_and_distances_are_reproduced() {
            let fixture = fixture();
            let cases = match fixture["overlaps"].as_array() {
                Some(value) => value,
                None => panic!("overlaps"),
            };
            assert_eq!(cases.len(), 9);
            for case in cases {
                let name = case["case"].as_str().unwrap_or("?");
                let (a, b) = (read_box(&case["first"]), read_box(&case["second"]));
                for (label, actual, key) in [
                    (
                        "horizontal",
                        projection_overlap_ratio(a, b, Axis::Horizontal),
                        "horizontal",
                    ),
                    (
                        "vertical",
                        projection_overlap_ratio(a, b, Axis::Vertical),
                        "vertical",
                    ),
                    (
                        "edge",
                        nearest_edge_distance(a, b, [1.0; 4]),
                        "edge_distance_unit",
                    ),
                ] {
                    let expected = case[key].as_f64().unwrap_or(f64::NAN);
                    assert!(
                        (actual - expected).abs() < 1e-12,
                        "{name}: {label}: {actual} vs {expected}"
                    );
                }
            }
        }

        #[test]
        fn the_captured_weight_table_is_reproduced() {
            let fixture = fixture();
            let cases = match fixture["weights"].as_array() {
                Some(value) => value,
                None => panic!("weights"),
            };
            assert_eq!(cases.len(), 14);
            for case in cases {
                let label = case["label"].as_str().unwrap_or("?");
                let vertical = case["direction"].as_str() == Some("vertical");
                let expected: Vec<f64> = match case["weights"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_f64().unwrap_or(f64::NAN))
                        .collect(),
                    None => panic!("{label}: weights"),
                };
                assert_eq!(
                    label_weights(label, vertical).to_vec(),
                    expected,
                    "{label}/{vertical}"
                );
            }
        }

        #[test]
        fn the_captured_weighted_distances_are_reproduced() {
            let fixture = fixture();
            let cases = match fixture["weighted_distances"].as_array() {
                Some(value) => value,
                None => panic!("weighted_distances"),
            };
            assert_eq!(cases.len(), 4);
            for case in cases {
                let name = case["case"].as_str().unwrap_or("?");
                let (a, b) = (read_box(&case["first"]), read_box(&case["second"]));
                let weights = label_weights(
                    case["label"].as_str().unwrap_or(""),
                    case["direction"].as_str() == Some("vertical"),
                );
                let actual = nearest_edge_distance(a, b, weights);
                let expected = case["edge_distance"].as_f64().unwrap_or(f64::NAN);
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "{name}: {actual} vs {expected}"
                );
            }

            let manhattan = &fixture["manhattan"][0];
            let actual = manhattan_distance(
                (1.0, 2.0),
                (4.0, 6.0),
                manhattan["weight_x"].as_f64().unwrap_or(1.0),
                manhattan["weight_y"].as_f64().unwrap_or(1.0),
            );
            assert!((actual - manhattan["distance"].as_f64().unwrap_or(f64::NAN)).abs() < 1e-12);
        }

        /// The plain-block sort, in both region directions.
        #[test]
        fn the_captured_sorts_are_reproduced() {
            let fixture = fixture();
            let cases = match fixture["sorts"].as_array() {
                Some(value) => value,
                None => panic!("sorts"),
            };
            assert_eq!(cases.len(), 6);
            for case in cases {
                let name = case["case"].as_str().unwrap_or("?");
                let boxes: Vec<HeuristicBox> = match case["boxes"].as_array() {
                    Some(values) => values.iter().map(read_box).collect(),
                    None => panic!("{name}: boxes"),
                };
                let order = sort_plain_blocks(
                    &boxes,
                    case["text_line_height"].as_f64().unwrap_or(10.0),
                    case["text_line_width"].as_f64().unwrap_or(10.0),
                    case["direction"].as_str() == Some("vertical"),
                );
                let expected: Vec<usize> = match case["order"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or(0) as usize)
                        .collect(),
                    None => panic!("{name}: order"),
                };
                assert_eq!(order, expected, "{name}");
            }
        }
    }

    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-reading-order/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn read_boxes(value: &Value) -> Vec<OrderBox> {
        match value.as_array() {
            Some(values) => values
                .iter()
                .map(|entry| {
                    let entry = match entry.as_array() {
                        Some(value) => value,
                        None => panic!("box"),
                    };
                    [
                        entry[0].as_i64().unwrap_or(0),
                        entry[1].as_i64().unwrap_or(0),
                        entry[2].as_i64().unwrap_or(0),
                        entry[3].as_i64().unwrap_or(0),
                    ]
                })
                .collect(),
            None => panic!("boxes"),
        }
    }

    fn read_usizes(value: &Value) -> Vec<usize> {
        match value.as_array() {
            Some(values) => values
                .iter()
                .map(|entry| entry.as_u64().unwrap_or(0) as usize)
                .collect(),
            None => panic!("indices"),
        }
    }

    #[test]
    fn the_captured_projections_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["projections"].as_array() {
            Some(value) => value,
            None => panic!("projections"),
        };
        assert_eq!(cases.len(), 4);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes = read_boxes(&case["boxes"]);
            let axis = match case["axis"].as_u64().unwrap_or(0) {
                0 => Axis::Horizontal,
                _ => Axis::Vertical,
            };
            let expected: Vec<u32> = match case["projection"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or(0) as u32)
                    .collect(),
                None => panic!("{name}: projection"),
            };
            let actual = match projection(&boxes, axis) {
                Ok(value) => value,
                Err(error) => panic!("{name}: {error}"),
            };
            assert_eq!(actual, expected, "{name}");
        }
    }

    #[test]
    fn the_captured_splits_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["splits"].as_array() {
            Some(value) => value,
            None => panic!("splits"),
        };
        assert_eq!(cases.len(), 6);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let values: Vec<u32> = match case["values"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or(0) as u32)
                    .collect(),
                None => panic!("{name}: values"),
            };
            let min_value = case["min_value"].as_u64().unwrap_or(0) as u32;
            let min_gap = case["min_gap"].as_u64().unwrap_or(1) as usize;
            let actual = split_profile(&values, min_value, min_gap);

            if case["starts"].is_null() {
                assert!(actual.is_none(), "{name}: expected no segments");
                continue;
            }
            let (starts, ends) = match actual {
                Some(value) => value,
                None => panic!("{name}: expected segments"),
            };
            assert_eq!(starts, read_usizes(&case["starts"]), "{name}: starts");
            assert_eq!(ends, read_usizes(&case["ends"]), "{name}: ends");
        }
    }

    /// Every captured layout, through both cut orders.
    #[test]
    fn the_captured_reading_orders_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["orders"].as_array() {
            Some(value) => value,
            None => panic!("orders"),
        };
        assert_eq!(cases.len(), 7);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes = read_boxes(&case["boxes"]);
            let indices: Vec<usize> = (0..boxes.len()).collect();

            let mut yx = Vec::new();
            match recursive_yx_cut(&boxes, &indices, &mut yx) {
                Ok(()) => {}
                Err(error) => panic!("{name}: yx: {error}"),
            }
            assert_eq!(yx, read_usizes(&case["yx_order"]), "{name}: yx");

            let mut xy = Vec::new();
            match recursive_xy_cut(&boxes, &indices, &mut xy) {
                Ok(()) => {}
                Err(error) => panic!("{name}: xy: {error}"),
            }
            assert_eq!(xy, read_usizes(&case["xy_order"]), "{name}: xy");
        }
    }

    /// The two cut orders disagree on a two-column page, and that is the point.
    #[test]
    fn the_two_cut_orders_read_a_two_column_page_differently() {
        let fixture = fixture();
        let cases = match fixture["orders"].as_array() {
            Some(value) => value,
            None => panic!("orders"),
        };
        let two_columns = cases
            .iter()
            .find(|case| case["case"] == "two_columns")
            .unwrap_or(&Value::Null);
        let yx = read_usizes(&two_columns["yx_order"]);
        let xy = read_usizes(&two_columns["xy_order"]);
        assert_ne!(yx, xy, "the corpus must contain a case where they differ");
        assert_eq!(yx, vec![0, 1, 2, 3], "row major");
        assert_eq!(xy, vec![0, 2, 1, 3], "column major");
    }

    /// Every input index appears exactly once in every captured ordering.
    ///
    /// A reading order that drops or duplicates a block is worse than a wrong
    /// one: downstream would silently lose content.
    #[test]
    fn every_ordering_is_a_permutation() {
        let fixture = fixture();
        let cases = match fixture["orders"].as_array() {
            Some(value) => value,
            None => panic!("orders"),
        };
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes = read_boxes(&case["boxes"]);
            let indices: Vec<usize> = (0..boxes.len()).collect();
            for (label, mut order) in [("yx", Vec::new()), ("xy", Vec::new())] {
                let outcome = if label == "yx" {
                    recursive_yx_cut(&boxes, &indices, &mut order)
                } else {
                    recursive_xy_cut(&boxes, &indices, &mut order)
                };
                match outcome {
                    Ok(()) => {}
                    Err(error) => panic!("{name}/{label}: {error}"),
                }
                let mut seen = order.clone();
                seen.sort_unstable();
                assert_eq!(seen, indices, "{name}/{label} is not a permutation");
            }
        }
    }

    /// An empty box list is refused rather than allocating a zero histogram.
    #[test]
    fn an_empty_projection_is_refused() {
        assert!(projection(&[], Axis::Horizontal).is_err());
    }

    /// A coordinate large enough to exhaust memory is refused.
    ///
    /// The histogram is allocated from caller-supplied numbers, so this is the
    /// hostile-input boundary the rest of the project treats image dimensions
    /// with.
    #[test]
    fn an_absurd_coordinate_is_refused() {
        let boxes = [[0_i64, 0, 900_000_000, 10]];
        match projection(&boxes, Axis::Horizontal) {
            Err(Error::InvalidInput { field, .. }) => {
                assert_eq!(field, "reading_order.extent");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// An inverted box contributes nothing rather than panicking.
    ///
    /// Python's `a[5:2]` is an empty slice; Rust's `a[5..2]` panics. A fuzz
    /// campaign found this on the first run after the module was committed, so
    /// the regression is pinned here as well as bounded there.
    #[test]
    fn an_inverted_box_contributes_nothing() {
        // Right edge left of the left edge, and bottom above the top.
        let boxes = [[30_i64, 30, 10, 10], [0, 0, 20, 20]];
        let histogram = match projection(&boxes, Axis::Horizontal) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        // Only the well-formed box contributes.
        assert_eq!(histogram.len(), 30);
        assert!(histogram[0..20].iter().all(|value| *value == 1));
        assert!(histogram[20..30].iter().all(|value| *value == 0));

        // And the inverted box is **dropped from the ordering**, not merely
        // absent from the histogram: the band filter selects on a top edge that
        // no band covers. Checked against upstream, which returns `[1]` for
        // exactly this input.
        let mut order = Vec::new();
        match recursive_yx_cut(&boxes, &[0, 1], &mut order) {
            Ok(()) => {}
            Err(error) => panic!("{error}"),
        }
        assert_eq!(order, vec![1], "upstream drops the inverted box too");
    }

    /// The permutation property holds for well-formed boxes, and not otherwise.
    ///
    /// `every_ordering_is_a_permutation` asserts it across the corpus, which
    /// contains only well-formed boxes. That is a property of the input, not an
    /// invariant of the algorithm — an inverted box is dropped, by upstream as
    /// well as here — and stating the difference is the point of this test.
    #[test]
    fn an_ordering_is_always_a_subset_without_duplicates() {
        let boxes = [
            [30_i64, 30, 10, 10],
            [0, 0, 20, 20],
            [5, 5, 5, 5],
            [40, 0, 60, 20],
        ];
        let indices: Vec<usize> = (0..boxes.len()).collect();
        let mut order = Vec::new();
        match recursive_yx_cut(&boxes, &indices, &mut order) {
            Ok(()) => {}
            Err(error) => panic!("{error}"),
        }
        let mut seen = order.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), order.len(), "no index may appear twice");
        assert!(
            order.iter().all(|index| *index < boxes.len()),
            "every emitted index must be one that was supplied"
        );
        assert!(order.len() < boxes.len(), "the inverted box is dropped");
    }

    /// Mismatched box and index counts are refused.
    #[test]
    fn mismatched_indices_are_refused() {
        let boxes = [[0_i64, 0, 10, 10]];
        let mut order = Vec::new();
        assert!(recursive_yx_cut(&boxes, &[0, 1], &mut order).is_err());
        assert!(recursive_xy_cut(&boxes, &[], &mut order).is_err());
    }

    /// An empty page produces an empty order rather than an error.
    #[test]
    fn an_empty_page_orders_to_nothing() {
        let mut order = Vec::new();
        match recursive_yx_cut(&[], &[], &mut order) {
            Ok(()) => assert!(order.is_empty()),
            Err(error) => panic!("{error}"),
        }
    }
}
