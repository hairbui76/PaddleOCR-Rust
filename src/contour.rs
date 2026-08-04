// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0
//
// The border-following behaviour reproduced here was derived by reading
// OpenCV's `modules/imgproc/src/contours.cpp` (`icvFetchContour` and the
// `cvFindNextContour` raster scan), which carries the Intel/BSD 3-clause
// notice recorded in NOTICE. This file is an independent Rust implementation,
// not a translation of that source, but the observable behaviour is
// deliberately identical and the attribution is required.

//! Bounded contour extraction equivalent to OpenCV's `findContours`.
//!
//! `ppocr/postprocess/db_postprocess.py:boxes_from_bitmap` calls
//! `cv2.findContours((bitmap * 255).astype(uint8), RETR_LIST,
//! CHAIN_APPROX_SIMPLE)` and then truncates the result to `max_candidates`
//! **by index**, so both the emitted points and their order are observable
//! contract rather than implementation detail.
//!
//! Three behaviours are easy to get wrong and are therefore stated explicitly,
//! each verified against the committed OpenCV capture:
//!
//! - The border marker is a single constant (`2`, or `-126` once the "right"
//!   flag is set). It is **not** incremented per contour, so the raster scan's
//!   `p == 1` test is what prevents an already traced border from restarting.
//! - The right flag is set from the swept direction range, not from a test on
//!   the east neighbour. Which pixels stay positive is what makes a hole border
//!   discoverable inside a one-pixel-thick ring.
//! - Contours are returned in reverse discovery order.
//!
//! This module extracts borders only. Minimum-area geometry, unclipping,
//! scoring, and rescaling remain later `DET-003` work.

use crate::db::BinaryBitmap;
use crate::error::{Error, ModelProblem, Result};

/// Maximum number of contours returned for one bitmap.
const MAX_CONTOURS: usize = 4_096;

/// Maximum number of points retained across all contours of one bitmap.
const MAX_TOTAL_POINTS: usize = 1_000_000;

/// The constant border marker; `MARKED_RIGHT` is the same value flagged.
const MARKED: i16 = 2;

/// `nbd | -128` in OpenCV's `schar` working image.
const MARKED_RIGHT: i16 = MARKED - 128;

/// The eight chain directions, in OpenCV's `icvCodeDeltas` order.
const DELTAS: [(i32, i32); 8] = [
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// One extracted border, in traversal order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Contour {
    points: Vec<(u32, u32)>,
    is_hole: bool,
}

impl Contour {
    /// Returns the border points in traversal order.
    pub(crate) fn points(&self) -> &[(u32, u32)] {
        &self.points
    }

    /// Returns whether this border encloses a hole.
    pub(crate) const fn is_hole(&self) -> bool {
        self.is_hole
    }
}

/// A working image with a one-pixel zero frame, mirroring OpenCV's padding.
struct Frame {
    width: usize,
    values: Vec<i16>,
}

impl Frame {
    fn new(bitmap: &BinaryBitmap) -> Self {
        let dimensions = bitmap.dimensions();
        let (width, height) = (dimensions.width() as usize, dimensions.height() as usize);
        let padded_width = width + 2;
        let mut values = vec![0_i16; padded_width * (height + 2)];
        for row in 0..height {
            for column in 0..width {
                values[(row + 1) * padded_width + column + 1] =
                    i16::from(bitmap.values()[row * width + column]);
            }
        }
        Self {
            width: padded_width,
            values,
        }
    }

    fn get(&self, x: usize, y: usize) -> i16 {
        self.values[y * self.width + x]
    }

    fn set(&mut self, x: usize, y: usize, value: i16) {
        self.values[y * self.width + x] = value;
    }

    fn offset(&self, x: usize, y: usize, direction: usize) -> (usize, usize) {
        let (dx, dy) = DELTAS[direction & 7];
        (
            x.wrapping_add_signed(dx as isize),
            y.wrapping_add_signed(dy as isize),
        )
    }
}

/// Extracts every border of one bitmap, in OpenCV's returned order.
pub(crate) fn classic_find_contours(bitmap: &BinaryBitmap) -> Result<Vec<Contour>> {
    let dimensions = bitmap.dimensions();
    let (width, height) = (dimensions.width() as usize, dimensions.height() as usize);
    let mut frame = Frame::new(bitmap);
    let mut found: Vec<Contour> = Vec::new();
    let mut total_points = 0_usize;

    for y in 1..=height {
        let mut x = 1_usize;
        let mut previous = frame.get(0, y);
        while x <= width {
            let value = frame.get(x, y);
            if value == previous {
                x += 1;
                previous = value;
                continue;
            }

            // An outer border starts on a 0 to 1 transition. Anything else is
            // a hole border only when the run that just ended was positive,
            // which is exactly what the right flag suppresses.
            let is_hole = if previous == 0 && value == 1 {
                false
            } else if value == 0 && previous >= 1 {
                true
            } else {
                previous = value;
                x += 1;
                continue;
            };

            let origin_x = x - usize::from(is_hole);
            let points = trace_border(&mut frame, origin_x, y, is_hole)?;
            total_points = total_points.saturating_add(points.len());
            if found.len() >= MAX_CONTOURS {
                return Err(Error::ResourceLimit {
                    resource: "contour.count",
                    limit: MAX_CONTOURS as u64,
                    actual: found.len() as u64 + 1,
                });
            }
            if total_points > MAX_TOTAL_POINTS {
                return Err(Error::ResourceLimit {
                    resource: "contour.points",
                    limit: MAX_TOTAL_POINTS as u64,
                    actual: total_points as u64,
                });
            }
            found.push(Contour { points, is_hole });

            previous = frame.get(x, y);
            x += 1;
        }
    }

    found.reverse();
    Ok(found)
}

/// Follows one border from its origin, emitting direction-change vertices.
fn trace_border(
    frame: &mut Frame,
    origin_x: usize,
    origin_y: usize,
    is_hole: bool,
) -> Result<Vec<(u32, u32)>> {
    let start = if is_hole { 0_usize } else { 4 };
    let mut direction = start;
    let mut first = None;
    loop {
        direction = (direction.wrapping_sub(1)) & 7;
        let (nx, ny) = frame.offset(origin_x, origin_y, direction);
        if frame.get(nx, ny) != 0 {
            first = Some((nx, ny));
            break;
        }
        if direction == start {
            break;
        }
    }

    let (first_x, first_y) = match first {
        Some(point) => point,
        None => {
            // An isolated pixel is its own border.
            frame.set(origin_x, origin_y, MARKED_RIGHT);
            return Ok(vec![to_point(origin_x, origin_y)?]);
        }
    };

    let mut points = Vec::new();
    let mut previous_direction = direction ^ 4;
    let (mut current_x, mut current_y) = (origin_x, origin_y);
    let (mut emitted_x, mut emitted_y) = (origin_x, origin_y);

    loop {
        let swept_from = direction;
        // The search runs forward over a duplicated sixteen-entry delta table,
        // so the swept range can pass the wrap point; that range is what the
        // right-flag test below inspects.
        let mut probe = direction.min(15);
        let (mut next_x, mut next_y) = (current_x, current_y);
        while probe < 15 {
            probe += 1;
            let (nx, ny) = frame.offset(current_x, current_y, probe);
            if frame.get(nx, ny) != 0 {
                next_x = nx;
                next_y = ny;
                break;
            }
        }
        direction = probe & 7;

        if direction.wrapping_sub(1) < swept_from {
            frame.set(current_x, current_y, MARKED_RIGHT);
        } else if frame.get(current_x, current_y) == 1 {
            frame.set(current_x, current_y, MARKED);
        }

        if direction != previous_direction {
            points.push(to_point(emitted_x, emitted_y)?);
            previous_direction = direction;
        }
        let (dx, dy) = DELTAS[direction];
        emitted_x = emitted_x.wrapping_add_signed(dx as isize);
        emitted_y = emitted_y.wrapping_add_signed(dy as isize);

        if (next_x, next_y) == (origin_x, origin_y) && (current_x, current_y) == (first_x, first_y)
        {
            break;
        }
        current_x = next_x;
        current_y = next_y;
        direction = (direction + 4) & 7;
    }

    Ok(points)
}

/// Converts a padded coordinate back to bitmap space.
fn to_point(x: usize, y: usize) -> Result<(u32, u32)> {
    let x = x.checked_sub(1).ok_or(Error::Model {
        problem: ModelProblem::TensorContract,
    })?;
    let y = y.checked_sub(1).ok_or(Error::Model {
        problem: ModelProblem::TensorContract,
    })?;
    let x = u32::try_from(x).map_err(|_| Error::Model {
        problem: ModelProblem::TensorContract,
    })?;
    let y = u32::try_from(y).map_err(|_| Error::Model {
        problem: ModelProblem::TensorContract,
    })?;
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    use crate::db::{DetectorProbabilityMap, classic_db_binary_segmentation};
    use crate::types::ImageDimensions;

    const CAPTURED_OPENCV_CONTOUR_GRID: &str =
        include_str!("../tests/fixtures/classic-v1-contour-grid/capture.json");

    /// Builds a bitmap through the real DB threshold, not a private shortcut.
    fn bitmap(rows: &[&str]) -> BinaryBitmap {
        let height = rows.len() as u32;
        let width = rows[0].len() as u32;
        let values: Vec<f32> = rows
            .iter()
            .flat_map(|row| row.bytes().map(|byte| if byte == b'1' { 1.0 } else { 0.0 }))
            .collect();
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(dimensions) => dimensions,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        };
        let map = match DetectorProbabilityMap::new(dimensions, &values) {
            Ok(map) => map,
            Err(error) => panic!("expected a valid probability map, got {error}"),
        };
        match classic_db_binary_segmentation(map) {
            Ok(bitmap) => bitmap,
            Err(error) => panic!("expected a valid bitmap, got {error}"),
        }
    }

    /// Executes every captured OpenCV case, including contour order.
    #[test]
    fn classic_find_contours_executes_every_captured_opencv_case() {
        let capture: Value = match serde_json::from_str(CAPTURED_OPENCV_CONTOUR_GRID) {
            Ok(value) => value,
            Err(error) => panic!("contour capture is not valid JSON: {error}"),
        };
        let cases = match capture.get("cases").and_then(Value::as_array) {
            Some(cases) => cases,
            None => panic!("contour capture must contain cases"),
        };
        assert_eq!(cases.len(), 18, "captured contour case count");

        let mut total = 0_usize;
        for case in cases {
            let fixture_id = match case.get("fixture_id").and_then(Value::as_str) {
                Some(value) => value,
                None => panic!("contour case must name a fixture"),
            };
            let rows: Vec<String> = match case.pointer("/bitmap/rows").and_then(Value::as_array) {
                Some(rows) => rows
                    .iter()
                    .map(|row| match row.as_str() {
                        Some(row) => row.to_owned(),
                        None => panic!("{fixture_id} bitmap row must be a string"),
                    })
                    .collect(),
                None => panic!("{fixture_id} is missing its bitmap rows"),
            };
            let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();

            let expected = match case.get("contours").and_then(Value::as_array) {
                Some(contours) => contours,
                None => panic!("{fixture_id} is missing its contours"),
            };
            let actual = match classic_find_contours(&bitmap(&borrowed)) {
                Ok(contours) => contours,
                Err(error) => panic!("{fixture_id} failed: {error}"),
            };
            assert_eq!(
                actual.len(),
                expected.len(),
                "{fixture_id} contour count and order are part of the contract"
            );
            for (index, (contour, wanted)) in actual.iter().zip(expected).enumerate() {
                let wanted: Vec<(u32, u32)> = match wanted.as_array() {
                    Some(points) => points
                        .iter()
                        .map(|point| {
                            let pair = match point.as_array() {
                                Some(pair) => pair,
                                None => panic!("{fixture_id} point must be an array"),
                            };
                            (
                                pair[0].as_u64().unwrap_or_default() as u32,
                                pair[1].as_u64().unwrap_or_default() as u32,
                            )
                        })
                        .collect(),
                    None => panic!("{fixture_id} contour must be an array"),
                };
                assert_eq!(
                    contour.points(),
                    wanted,
                    "{fixture_id} contour {index} points"
                );
                total += contour.points().len();
            }
        }
        assert!(total > 0, "the capture must exercise real points");
    }

    #[test]
    fn an_empty_bitmap_yields_no_contour() {
        let contours = match classic_find_contours(&bitmap(&["000", "000"])) {
            Ok(contours) => contours,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(contours.is_empty());
    }

    #[test]
    fn a_ring_reports_its_hole_border() {
        let contours = match classic_find_contours(&bitmap(&[
            "0000000", "0111110", "0100010", "0100010", "0111110", "0000000",
        ])) {
            Ok(contours) => contours,
            Err(error) => panic!("expected two borders, got {error}"),
        };
        assert_eq!(contours.len(), 2);
        // Reverse discovery order: the hole is discovered second and returned
        // first.
        assert!(
            contours[0].is_hole(),
            "the first returned border is the hole"
        );
        assert!(!contours[1].is_hole(), "the second is the outer border");
    }

    #[test]
    fn a_single_pixel_is_its_own_border() {
        let contours = match classic_find_contours(&bitmap(&["000", "010", "000"])) {
            Ok(contours) => contours,
            Err(error) => panic!("expected one border, got {error}"),
        };
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].points(), [(1, 1)]);
    }
}
