// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0
//
// The polygon fill reproduced here was derived from OpenCV's `fillPoly`
// observable output, recorded in `tests/fixtures/classic-v1-unclip-score-grid/`.
// See NOTICE.

//! Mean probability inside a candidate box, matching `box_score_fast`.
//!
//! `ppocr/postprocess/db_postprocess.py:box_score_fast` clips the box's
//! bounding rectangle with `floor`/`ceil` into the map, builds a mask with
//! `cv2.fillPoly`, and returns `cv2.mean` of the map over that mask.
//!
//! The fill is **not** an analytic scanline. OpenCV walks each polygon edge as
//! an integer line, marks every pixel the edge passes through, and then fills
//! each row between its leftmost and rightmost marked pixel. That distinction
//! is load-bearing: nine different endpoint-rounding rules over an analytic
//! scanline all fail the same slanted case, while the edge walk reproduces
//! every recorded case.

use crate::error::{Error, InputViolation, Result};

/// Computes the upstream `box_score_fast` for one candidate box.
///
/// `map_values` is a row-major probability map of `width * height` finite
/// values. The box corners are in map coordinates and may lie outside it.
pub(crate) fn classic_box_score(
    map_values: &[f32],
    width: u32,
    height: u32,
    box_corners: &[(f64, f64)],
) -> Result<f64> {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || map_values.len() != width * height {
        return Err(Error::InvalidInput {
            field: "score.map",
            violation: InputViolation::OutOfRange,
        });
    }
    if box_corners.len() < 3 {
        return Err(Error::InvalidInput {
            field: "score.box",
            violation: InputViolation::DegenerateGeometry,
        });
    }
    if box_corners
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(Error::InvalidInput {
            field: "score.box",
            violation: InputViolation::NonFinite,
        });
    }

    let clamp = |value: f64, limit: usize| -> usize {
        if value < 0.0 {
            0
        } else if value >= limit as f64 {
            limit - 1
        } else {
            value as usize
        }
    };
    let min_x = box_corners.iter().fold(f64::INFINITY, |a, p| a.min(p.0));
    let max_x = box_corners
        .iter()
        .fold(f64::NEG_INFINITY, |a, p| a.max(p.0));
    let min_y = box_corners.iter().fold(f64::INFINITY, |a, p| a.min(p.1));
    let max_y = box_corners
        .iter()
        .fold(f64::NEG_INFINITY, |a, p| a.max(p.1));
    let x0 = clamp(min_x.floor(), width);
    let x1 = clamp(max_x.ceil(), width);
    let y0 = clamp(min_y.floor(), height);
    let y1 = clamp(max_y.ceil(), height);
    if x1 < x0 || y1 < y0 {
        return Ok(0.0);
    }

    let mask_width = x1 - x0 + 1;
    let mask_height = y1 - y0 + 1;
    // The polygon is shifted into mask space and truncated toward zero, which
    // is what `astype("int32")` does upstream.
    let polygon: Vec<(i64, i64)> = box_corners
        .iter()
        .map(|(x, y)| ((x - x0 as f64) as i64, (y - y0 as f64) as i64))
        .collect();

    let mut mask = vec![false; mask_width * mask_height];
    for index in 0..polygon.len() {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];
        mark_line(&mut mask, mask_width, mask_height, start, end);
    }

    let mut total = 0.0_f64;
    let mut counted = 0_u64;
    for row in 0..mask_height {
        let offset = row * mask_width;
        let first = (0..mask_width).find(|column| mask[offset + column]);
        let last = (0..mask_width).rev().find(|column| mask[offset + column]);
        if let (Some(first), Some(last)) = (first, last) {
            for column in first..=last {
                let value = map_values[(row + y0) * width + column + x0];
                if !value.is_finite() {
                    return Err(Error::InvalidInput {
                        field: "score.map",
                        violation: InputViolation::NonFinite,
                    });
                }
                total += f64::from(value);
                counted += 1;
            }
        }
    }

    if counted == 0 {
        return Ok(0.0);
    }
    Ok(total / counted as f64)
}

/// Marks every mask pixel one integer line passes through.
fn mark_line(
    mask: &mut [bool],
    mask_width: usize,
    mask_height: usize,
    start: (i64, i64),
    end: (i64, i64),
) {
    let (mut x, mut y) = start;
    let (dx, dy) = ((end.0 - x).abs(), (end.1 - y).abs());
    let step_x = if x < end.0 { 1 } else { -1 };
    let step_y = if y < end.1 { 1 } else { -1 };
    let mut error = dx - dy;
    loop {
        if x >= 0 && y >= 0 && (x as usize) < mask_width && (y as usize) < mask_height {
            mask[y as usize * mask_width + x as usize] = true;
        }
        if x == end.0 && y == end.1 {
            break;
        }
        let doubled = 2 * error;
        if doubled > -dy {
            error -= dy;
            x += step_x;
        }
        if doubled < dx {
            error += dx;
            y += step_y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const CAPTURED_SCORE_GRID: &str =
        include_str!("../tests/fixtures/classic-v1-unclip-score-grid/capture.json");

    /// Rebuilds the recorded `lcg-v1` probability map for one case.
    fn probability_map(width: usize, height: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;
        (0..width * height)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (f64::from((state >> 16) & 0xFFFF) / 65535.0) as f32
            })
            .collect()
    }

    #[test]
    fn classic_box_score_executes_every_captured_case() {
        let capture: Value = match serde_json::from_str(CAPTURED_SCORE_GRID) {
            Ok(value) => value,
            Err(error) => panic!("score capture is not valid JSON: {error}"),
        };
        let cases = match capture.get("cases").and_then(Value::as_array) {
            Some(cases) => cases,
            None => panic!("score capture must contain cases"),
        };
        assert_eq!(cases.len(), 8, "captured score case count");

        for (index, case) in cases.iter().enumerate() {
            let fixture_id = case
                .get("fixture_id")
                .and_then(Value::as_str)
                .unwrap_or("<unnamed>");
            let shape = match case
                .pointer("/probability_map/shape")
                .and_then(Value::as_array)
            {
                Some(shape) => shape,
                None => panic!("{fixture_id} is missing its map shape"),
            };
            let height = shape[0].as_u64().unwrap_or_default() as usize;
            let width = shape[1].as_u64().unwrap_or_default() as usize;
            let seed = 0x2000_u32 + 0x37 * index as u32;
            let map = probability_map(width, height, seed);

            let corners: Vec<(f64, f64)> = match case.get("box").and_then(Value::as_array) {
                Some(points) => points
                    .iter()
                    .map(|point| {
                        let pair = match point.as_array() {
                            Some(pair) => pair,
                            None => panic!("{fixture_id} corner must be an array"),
                        };
                        (
                            pair[0].as_f64().unwrap_or_default(),
                            pair[1].as_f64().unwrap_or_default(),
                        )
                    })
                    .collect(),
                None => panic!("{fixture_id} is missing its box"),
            };

            let expected = case
                .pointer("/score/value")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let actual = match classic_box_score(&map, width as u32, height as u32, &corners) {
                Ok(value) => value,
                Err(error) => panic!("{fixture_id} failed: {error}"),
            };
            assert!(
                (actual - expected).abs() < 1e-9,
                "{fixture_id}: got {actual}, want {expected}"
            );
        }
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(matches!(
            classic_box_score(&[0.5], 1, 1, &[(0.0, 0.0), (1.0, 0.0)]),
            Err(Error::InvalidInput {
                violation: InputViolation::DegenerateGeometry,
                ..
            })
        ));
        assert!(matches!(
            classic_box_score(&[0.5], 2, 2, &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]),
            Err(Error::InvalidInput {
                field: "score.map",
                ..
            })
        ));
    }
}
