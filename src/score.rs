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
/// Computes the upstream `box_score_slow` for one candidate contour.
///
/// `box_score_slow` is handed the raw `findContours` contour rather than the
/// four-corner minimum-area box, and a contour can be **concave** — an L-shaped
/// region produces one. That is why this is not a call into
/// [`classic_box_score`]: the fast path fills each row between its leftmost and
/// rightmost marked pixel, which is correct for the convex quadrilaterals it
/// sees and would overfill a concavity here.
///
/// The fill reproduces `cv2.fillPoly`'s observable output, which the captured
/// masks in `tests/fixtures/classic-v1-slow-score/` show to be two rules
/// superimposed:
///
/// 1. **every edge drawn as an integer line**, endpoints inclusive — the same
///    walk [`classic_box_score`] already pins — so a degenerate one- or
///    two-point contour still marks its pixels; and
/// 2. **an even-odd interior**, from a scanline over half-open edges, so a
///    concavity stays empty.
///
/// Interior spans round **inward** — `ceil` on the left endpoint, `floor` on
/// the right — computed in exact rational arithmetic rather than `f64`, because
/// an intersection like `1/3` has no exact float and a half-pixel error at a
/// span end is a whole wrong column of probabilities.
pub(crate) fn classic_box_score_slow(
    map_values: &[f32],
    width: u32,
    height: u32,
    contour: &[(i64, i64)],
) -> Result<f64> {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || map_values.len() != width * height {
        return Err(Error::InvalidInput {
            field: "score.map",
            violation: InputViolation::OutOfRange,
        });
    }
    if contour.is_empty() {
        return Err(Error::InvalidInput {
            field: "score.contour",
            violation: InputViolation::Empty,
        });
    }
    // Upstream casts the contour with `astype("int32")`, which **wraps** an
    // out-of-range value. Wrapping a hostile coordinate into a plausible one is
    // worse than refusing it, so this port refuses.
    if contour
        .iter()
        .any(|(x, y)| i32::try_from(*x).is_err() || i32::try_from(*y).is_err())
    {
        return Err(Error::InvalidInput {
            field: "score.contour",
            violation: InputViolation::OutOfRange,
        });
    }

    // `np.clip(min/max, 0, w - 1)`: the window is clamped into the map, while
    // the points themselves are not — the mask writes below clamp instead.
    let clamp = |value: i64, limit: usize| value.clamp(0, limit as i64 - 1) as usize;
    let x0 = clamp(contour.iter().map(|p| p.0).min().unwrap_or(0), width);
    let x1 = clamp(contour.iter().map(|p| p.0).max().unwrap_or(0), width);
    let y0 = clamp(contour.iter().map(|p| p.1).min().unwrap_or(0), height);
    let y1 = clamp(contour.iter().map(|p| p.1).max().unwrap_or(0), height);

    let mask_width = x1 - x0 + 1;
    let mask_height = y1 - y0 + 1;
    let shifted: Vec<(i64, i64)> = contour
        .iter()
        .map(|(x, y)| (x - x0 as i64, y - y0 as i64))
        .collect();

    let mask = slow_fill_mask(&shifted, mask_width, mask_height);

    let mut total = 0.0_f64;
    let mut counted = 0_u64;
    for row in 0..mask_height {
        for column in 0..mask_width {
            if !mask[row * mask_width + column] {
                continue;
            }
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
    if counted == 0 {
        return Ok(0.0);
    }
    Ok(total / counted as f64)
}

/// Builds the `fillPoly` mask for one shifted contour.
///
/// Separated from the score so the captured masks can be compared bit for bit;
/// a wrong fill can still produce a right mean, and the mask is where the
/// claim actually lives.
pub(crate) fn slow_fill_mask(
    shifted: &[(i64, i64)],
    mask_width: usize,
    mask_height: usize,
) -> Vec<bool> {
    let mut mask = vec![false; mask_width * mask_height];

    // Rule 1: the boundary, edge by edge. A single-point contour degenerates to
    // one marked pixel, which is what upstream returns for it.
    for index in 0..shifted.len() {
        let start = shifted[index];
        let end = shifted[(index + 1) % shifted.len()];
        mark_line(&mut mask, mask_width, mask_height, start, end);
    }

    // Rule 2: the even-odd interior. Half-open edges `[min_y, max_y)`, so a
    // vertex row is decided by the edge leaving it rather than counted twice;
    // horizontal edges contribute nothing here and are rule 1's alone.
    for row in 0..mask_height {
        let scanline = row as i64;
        // Each crossing as an exact rational `numerator / denominator`.
        let mut crossings: Vec<(i64, i64)> = Vec::new();
        for index in 0..shifted.len() {
            let (ax, ay) = shifted[index];
            let (bx, by) = shifted[(index + 1) % shifted.len()];
            if ay == by {
                continue;
            }
            let (top, bottom) = if ay < by {
                ((ax, ay), (bx, by))
            } else {
                ((bx, by), (ax, ay))
            };
            if scanline < top.1 || scanline >= bottom.1 {
                continue;
            }
            let denominator = bottom.1 - top.1;
            let numerator = top.0 * denominator + (scanline - top.1) * (bottom.0 - top.0);
            crossings.push((numerator, denominator));
        }
        // In `i128`: with `i32`-bounded coordinates the numerators reach
        // `~2^62`, and the cross-multiplied comparison would overflow `i64`.
        crossings.sort_by(|a, b| {
            (i128::from(a.0) * i128::from(b.1)).cmp(&(i128::from(b.0) * i128::from(a.1)))
        });

        // Both span endpoints round to **nearest, half toward negative
        // infinity**: the triangle's row `4` starts its span at `x = 1.5` and
        // OpenCV fills column `1`, while the thin diagonal's row `4` starts at
        // `x = 4.666` and OpenCV leaves column `4` empty. `ceil`-left fits the
        // second and not the first; nearest-half-down fits both.
        let nearest_down = |numerator: i64, denominator: i64| -> i64 {
            (2 * numerator + denominator - 1).div_euclid(2 * denominator)
        };
        for pair in crossings.chunks_exact(2) {
            let (left_n, left_d) = pair[0];
            let (right_n, right_d) = pair[1];
            let from = nearest_down(left_n, left_d).clamp(0, mask_width as i64 - 1) as usize;
            let to = nearest_down(right_n, right_d).clamp(0, mask_width as i64 - 1) as usize;
            if from <= to {
                for column in from..=to {
                    mask[row * mask_width + column] = true;
                }
            }
        }
    }
    mask
}

fn mark_line(
    mask: &mut [bool],
    mask_width: usize,
    mask_height: usize,
    start: (i64, i64),
    end: (i64, i64),
) {
    // OpenCV's non-antialiased line, pinned by tracing its observable output
    // across the captured edges. Two rules:
    //
    //   * canonical direction — x-dominant lines run left to right, y-dominant
    //     top to bottom — which is what makes `line(a, b)` and `line(b, a)`
    //     mark the same pixels;
    //   * the minor coordinate is the **signed** offset from the canonical
    //     start, rounded to nearest — with an exact half resolved differently
    //     per dominance, which is plausible because OpenCV's line code has a
    //     separate branch for each. **X-dominant halves go toward zero**:
    //     `(0,8)-(12,6)` keeps `x = 3` on row `8` (offset `-0.5` to `0`) and
    //     `(0,0)-(8,5)` puts `x = 4` on row `2` (offset `+2.5` to `+2`).
    //     **Y-dominant halves go toward negative infinity**: `(7,5)-(5,9)`
    //     puts `y = 8` at column `5` (offset `-1.5` to `-2`), where toward
    //     zero would wrongly mark column `6` — the notched-rectangle mask is
    //     the case that separated the two.
    let (mut from, mut to) = (start, end);
    let x_dominant = (to.0 - from.0).abs() >= (to.1 - from.1).abs();
    if (x_dominant && from.0 > to.0) || (!x_dominant && from.1 > to.1) {
        core::mem::swap(&mut from, &mut to);
    }

    let mut plot = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && (x as usize) < mask_width && (y as usize) < mask_height {
            mask[y as usize * mask_width + x as usize] = true;
        }
    };

    // Round-half-toward-zero of `numerator / major`, exactly.
    let toward_zero = |numerator: i64, major: i64| -> i64 {
        let magnitude = (2 * numerator.abs() + major - 1).div_euclid(2 * major);
        magnitude * numerator.signum()
    };
    // Round-half-toward-negative-infinity, exactly.
    let toward_down =
        |numerator: i64, major: i64| -> i64 { (2 * numerator + major - 1).div_euclid(2 * major) };

    if x_dominant {
        let major = (to.0 - from.0).max(1);
        let minor = to.1 - from.1;
        for k in 0..=major {
            plot(from.0 + k, from.1 + toward_zero(k * minor, major));
        }
    } else {
        let major = to.1 - from.1;
        let minor = to.0 - from.0;
        for k in 0..=major {
            plot(from.0 + toward_down(k * minor, major), from.1 + k);
        }
    }
}

#[cfg(test)]
mod tests {
    mod slow {

        use super::super::*;

        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD;
        use serde_json::Value;

        const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-slow-score/expected.json");

        /// The capture's synthetic map, by the same closed form.
        fn synthetic_map(width: usize, height: usize) -> Vec<f32> {
            let mut values = Vec::with_capacity(width * height);
            for y in 0..height {
                for x in 0..width {
                    values.push(((x as f32 * 7.0 + y as f32 * 13.0) % 29.0) / 29.0);
                }
            }
            values
        }

        /// Every captured mask bit for bit, and every captured score.
        ///
        /// The mask is the claim; the score is the consequence. Comparing both
        /// keeps a wrong fill from hiding behind a right mean.
        #[test]
        fn the_captured_masks_and_scores_are_reproduced() {
            let document: Value = match serde_json::from_str(FIXTURE) {
                Ok(value) => value,
                Err(error) => panic!("fixture: {error}"),
            };
            let width = document["map"]["width"].as_u64().unwrap_or(0) as usize;
            let height = document["map"]["height"].as_u64().unwrap_or(0) as usize;
            let map = synthetic_map(width, height);
            let records = match document["records"].as_array() {
                Some(value) => value,
                None => panic!("records"),
            };
            assert_eq!(records.len(), 51);

            for record in records {
                let case = record["case"].as_str().unwrap_or("?");
                let contour: Vec<(i64, i64)> = match record["contour"].as_array() {
                    Some(points) => points
                        .iter()
                        .map(|point| match point.as_array() {
                            Some(point) => (
                                point[0].as_i64().unwrap_or(0),
                                point[1].as_i64().unwrap_or(0),
                            ),
                            None => panic!("point"),
                        })
                        .collect(),
                    None => panic!("{case}: contour"),
                };
                let clip = match record["clip"].as_array() {
                    Some(value) => value,
                    None => panic!("{case}: clip"),
                };
                let (x0, y0) = (clip[0].as_i64().unwrap_or(0), clip[1].as_i64().unwrap_or(0));
                let shape = match record["mask_shape"].as_array() {
                    Some(value) => value,
                    None => panic!("{case}: mask shape"),
                };
                let (mask_height, mask_width) = (
                    shape[0].as_u64().unwrap_or(0) as usize,
                    shape[1].as_u64().unwrap_or(0) as usize,
                );

                let shifted: Vec<(i64, i64)> =
                    contour.iter().map(|(x, y)| (x - x0, y - y0)).collect();
                let mask = slow_fill_mask(&shifted, mask_width, mask_height);

                let expected = match STANDARD.decode(record["mask_base64"].as_str().unwrap_or("")) {
                    Ok(value) => value,
                    Err(error) => panic!("{case}: mask: {error}"),
                };
                assert_eq!(expected.len(), mask.len(), "{case}: mask size");
                for (index, (ours, theirs)) in mask.iter().zip(&expected).enumerate() {
                    assert_eq!(
                        u8::from(*ours),
                        *theirs,
                        "{case}: mask bit ({}, {}) differs",
                        index % mask_width,
                        index / mask_width
                    );
                }

                let score =
                    match classic_box_score_slow(&map, width as u32, height as u32, &contour) {
                        Ok(value) => value,
                        Err(error) => panic!("{case}: {error}"),
                    };
                let expected_score = record["score"].as_f64().unwrap_or(f64::NAN);
                assert!(
                    (score - expected_score).abs() < 1e-9,
                    "{case}: score {score} vs {expected_score}"
                );
            }
        }

        /// The fast path would overfill a concavity, and the slow path must not.
        ///
        /// This is the reason the function exists rather than being a call into
        /// `classic_box_score`, so it is asserted directly: on the U-shape, the two
        /// disagree.
        #[test]
        fn the_slow_fill_leaves_a_concavity_empty() {
            let contour = [
                (2_i64, 2_i64),
                (14, 2),
                (14, 12),
                (10, 12),
                (10, 5),
                (6, 5),
                (6, 12),
                (2, 12),
            ];
            let shifted: Vec<(i64, i64)> = contour.iter().map(|(x, y)| (x - 2, y - 2)).collect();
            let mask = slow_fill_mask(&shifted, 13, 11);
            // The middle of the U's mouth: inside the bounding box, outside the
            // polygon.
            assert!(!mask[6 * 13 + 6], "the concavity must stay empty");
            assert!(mask[6 * 13 + 1], "the left arm is filled");
            assert!(mask[6 * 13 + 11], "the right arm is filled");

            // And the fast score over the same shape counts the concavity, so the
            // two scores differ on a map where the concavity is distinguishable.
            let mut map = vec![0.0_f32; 20 * 14];
            for y in 5..12 {
                for x in 7..10 {
                    map[y * 20 + x] = 1.0;
                }
            }
            let corners: Vec<(f64, f64)> = contour
                .iter()
                .map(|(x, y)| (*x as f64, *y as f64))
                .collect();
            let fast = match classic_box_score(&map, 20, 14, &corners) {
                Ok(value) => value,
                Err(error) => panic!("fast: {error}"),
            };
            let slow = match classic_box_score_slow(&map, 20, 14, &contour) {
                Ok(value) => value,
                Err(error) => panic!("slow: {error}"),
            };
            assert!(
                fast > slow,
                "the hot concavity must raise the fast score ({fast}) above the slow ({slow})"
            );
        }

        /// Hostile contours are refused or bounded, never a panic.
        #[test]
        fn hostile_contours_are_refused_or_bounded() {
            let map = synthetic_map(20, 14);
            assert!(classic_box_score_slow(&map, 20, 14, &[]).is_err());
            // A contour entirely outside the map still scores without panicking.
            let outside = match classic_box_score_slow(&map, 20, 14, &[(500, 500), (600, 600)]) {
                Ok(value) => value,
                Err(error) => panic!("outside: {error}"),
            };
            assert!(outside.is_finite());
            // Coordinates outside `i32` are refused: upstream's `astype("int32")`
            // would wrap them into plausible values, which is worse.
            let absurd = classic_box_score_slow(&map, 20, 14, &[(i64::MIN, 0), (i64::MAX, 5)]);
            assert!(matches!(
                absurd,
                Err(Error::InvalidInput {
                    field: "score.contour",
                    ..
                })
            ));
        }
    }

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
