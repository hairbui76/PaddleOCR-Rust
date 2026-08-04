// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0
//
// The rotated-rectangle behaviour reproduced here was derived from OpenCV's
// `minAreaRect`/`boxPoints` observable output, recorded in
// `tests/fixtures/classic-v1-min-area-box-grid/`. See NOTICE.

//! Minimum-area rotated rectangle over a contour, matching OpenCV.
//!
//! `ppocr/postprocess/db_postprocess.py:get_mini_boxes` calls
//! `cv2.minAreaRect` then `cv2.boxPoints`, sorts the four corners by `x`, and
//! reindexes them into a fixed order. `sside` is `min(width, height)` of the
//! rotated rectangle itself, not a value recomputed from the corners.
//!
//! Three details are pinned by the recorded oracle because they decide the
//! result on tied inputs, and getting any of them wrong silently changes boxes:
//!
//! - the convex hull is traversed **counter-clockwise**, edges **forward**;
//! - the minimum-area comparison is **non-strict**, so among equal-area
//!   candidates the **last** one wins;
//! - the corners come from the `boxPoints` formula applied to
//!   `(center, size, angle)`, which is what produces OpenCV's duplicated
//!   corners for one-point, two-point, and collinear inputs.

use crate::error::{Error, InputViolation, Result};

/// Maximum number of contour points accepted for one rectangle.
const MAX_POINTS: usize = 1_000_000;

/// A rotated rectangle in OpenCV's `(center, size, angle)` form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RotatedRect {
    center: (f64, f64),
    size: (f64, f64),
    angle_degrees: f64,
}

impl RotatedRect {
    /// Returns `min(width, height)`, the upstream `sside`.
    pub(crate) fn short_side(&self) -> f64 {
        self.size.0.min(self.size.1)
    }

    /// Returns the four corners in OpenCV's `boxPoints` order.
    pub(crate) fn box_points(&self) -> [(f64, f64); 4] {
        let angle = self.angle_degrees.to_radians();
        let b = angle.cos() * 0.5;
        let s = angle.sin() * 0.5;
        let (cx, cy) = self.center;
        let (w, h) = self.size;
        let first = (cx - s * h - b * w, cy + b * h - s * w);
        let second = (cx + s * h - b * w, cy - b * h - s * w);
        [
            first,
            second,
            (2.0 * cx - first.0, 2.0 * cy - first.1),
            (2.0 * cx - second.0, 2.0 * cy - second.1),
        ]
    }

    /// Returns the corners in the frozen `get_mini_boxes` order.
    ///
    /// The sort is by `x` alone and must be stable, because a tie keeps the
    /// `boxPoints` order and that decides the final orientation.
    pub(crate) fn ordered_box(&self) -> [(f64, f64); 4] {
        let mut points = self.box_points();
        points.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let (first, fourth) = if points[1].1 > points[0].1 {
            (0, 1)
        } else {
            (1, 0)
        };
        let (second, third) = if points[3].1 > points[2].1 {
            (2, 3)
        } else {
            (3, 2)
        };
        [points[first], points[second], points[third], points[fourth]]
    }
}

/// Computes the minimum-area rotated rectangle of a point set.
pub(crate) fn classic_min_area_rect(points: &[(f64, f64)]) -> Result<RotatedRect> {
    if points.is_empty() {
        return Err(Error::InvalidInput {
            field: "min_area.points",
            violation: InputViolation::Empty,
        });
    }
    if points.len() > MAX_POINTS {
        return Err(Error::ResourceLimit {
            resource: "min_area.points",
            limit: MAX_POINTS as u64,
            actual: points.len() as u64,
        });
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(Error::InvalidInput {
            field: "min_area.points",
            violation: InputViolation::NonFinite,
        });
    }

    let hull = convex_hull(points);
    match hull.len() {
        1 => Ok(RotatedRect {
            center: hull[0],
            size: (0.0, 0.0),
            angle_degrees: 0.0,
        }),
        2 => {
            let (x0, y0) = hull[0];
            let (x1, y1) = hull[1];
            Ok(RotatedRect {
                center: ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
                size: ((x1 - x0).hypot(y1 - y0), 0.0),
                angle_degrees: (y1 - y0).atan2(x1 - x0).to_degrees(),
            })
        }
        _ => Ok(smallest_enclosing_rect(&hull)),
    }
}

/// Builds the counter-clockwise convex hull with a monotone chain.
fn convex_hull(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted: Vec<(f64, f64)> = points.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    sorted.dedup();
    if sorted.len() <= 2 {
        return sorted;
    }

    fn half(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let mut chain: Vec<(f64, f64)> = Vec::new();
        for point in points {
            while chain.len() >= 2 {
                let last = chain[chain.len() - 1];
                let previous = chain[chain.len() - 2];
                let cross = (last.0 - previous.0) * (point.1 - previous.1)
                    - (last.1 - previous.1) * (point.0 - previous.0);
                if cross <= 0.0 {
                    chain.pop();
                } else {
                    break;
                }
            }
            chain.push(*point);
        }
        chain
    }

    let mut reversed = sorted.clone();
    reversed.reverse();
    let mut lower = half(&sorted);
    let mut upper = half(&reversed);
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Picks the minimum-area rectangle aligned to a hull edge.
///
/// The comparison is non-strict on purpose: among equal-area candidates OpenCV
/// keeps the last one visited, and several recorded cases are exact ties.
fn smallest_enclosing_rect(hull: &[(f64, f64)]) -> RotatedRect {
    let mut best_area = f64::INFINITY;
    let mut best = RotatedRect {
        center: hull[0],
        size: (0.0, 0.0),
        angle_degrees: 0.0,
    };
    for index in 0..hull.len() {
        let (x0, y0) = hull[index];
        let (x1, y1) = hull[(index + 1) % hull.len()];
        let (dx, dy) = (x1 - x0, y1 - y0);
        let length = dx.hypot(dy);
        if length == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        let (vx, vy) = (-uy, ux);

        let mut min_u = f64::INFINITY;
        let mut max_u = f64::NEG_INFINITY;
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        for (x, y) in hull {
            let u = x * ux + y * uy;
            let v = x * vx + y * vy;
            min_u = min_u.min(u);
            max_u = max_u.max(u);
            min_v = min_v.min(v);
            max_v = max_v.max(v);
        }

        let area = (max_u - min_u) * (max_v - min_v);
        if area <= best_area + 1e-12 {
            best_area = area.min(best_area);
            let center_u = (min_u + max_u) * 0.5;
            let center_v = (min_v + max_v) * 0.5;
            best = RotatedRect {
                center: (center_u * ux + center_v * vx, center_u * uy + center_v * vy),
                size: (max_u - min_u, max_v - min_v),
                angle_degrees: uy.atan2(ux).to_degrees(),
            };
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const CAPTURED_OPENCV_MIN_AREA_GRID: &str =
        include_str!("../tests/fixtures/classic-v1-min-area-box-grid/capture.json");

    /// Executes every captured OpenCV `minAreaRect` case.
    ///
    /// The tolerance is `2e-3` because OpenCV evaluates the rectangle in
    /// `float32` while this implementation uses `float64`; the recorded corner
    /// values carry only single-precision significance. Order and `sside` are
    /// compared exactly in structure, only the coordinates are toleranced.
    #[test]
    fn classic_min_area_rect_executes_every_captured_opencv_case() {
        let capture: Value = match serde_json::from_str(CAPTURED_OPENCV_MIN_AREA_GRID) {
            Ok(value) => value,
            Err(error) => panic!("min-area capture is not valid JSON: {error}"),
        };
        let cases = match capture.get("cases").and_then(Value::as_array) {
            Some(cases) => cases,
            None => panic!("min-area capture must contain cases"),
        };
        assert_eq!(cases.len(), 16, "captured min-area case count");

        for case in cases {
            let fixture_id = match case.get("fixture_id").and_then(Value::as_str) {
                Some(value) => value,
                None => panic!("min-area case must name a fixture"),
            };
            let points: Vec<(f64, f64)> = match case.get("points").and_then(Value::as_array) {
                Some(points) => points
                    .iter()
                    .map(|point| {
                        let pair = match point.as_array() {
                            Some(pair) => pair,
                            None => panic!("{fixture_id} point must be an array"),
                        };
                        (
                            pair[0].as_f64().unwrap_or_default(),
                            pair[1].as_f64().unwrap_or_default(),
                        )
                    })
                    .collect(),
                None => panic!("{fixture_id} is missing its points"),
            };

            let rect = match classic_min_area_rect(&points) {
                Ok(rect) => rect,
                Err(error) => panic!("{fixture_id} failed: {error}"),
            };

            let expected_sside = case
                .get("sside")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            assert!(
                (rect.short_side() - expected_sside).abs() < 2e-3,
                "{fixture_id} sside: got {}, want {expected_sside}",
                rect.short_side()
            );

            let expected: Vec<(f64, f64)> = match case.get("ordered_box").and_then(Value::as_array)
            {
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
                None => panic!("{fixture_id} is missing its ordered box"),
            };
            let actual = rect.ordered_box();
            for (index, (got, want)) in actual.iter().zip(&expected).enumerate() {
                assert!(
                    (got.0 - want.0).abs() < 2e-3 && (got.1 - want.1).abs() < 2e-3,
                    "{fixture_id} corner {index}: got {got:?}, want {want:?}"
                );
            }
        }
    }

    #[test]
    fn degenerate_inputs_produce_duplicated_corners() {
        let single = match classic_min_area_rect(&[(4.0, 4.0)]) {
            Ok(rect) => rect,
            Err(error) => panic!("expected a degenerate rect, got {error}"),
        };
        assert_eq!(single.short_side(), 0.0);
        assert_eq!(single.ordered_box(), [(4.0, 4.0); 4]);
    }

    #[test]
    fn invalid_point_sets_are_rejected() {
        assert!(matches!(
            classic_min_area_rect(&[]),
            Err(Error::InvalidInput {
                violation: InputViolation::Empty,
                ..
            })
        ));
        assert!(matches!(
            classic_min_area_rect(&[(0.0, 0.0), (f64::NAN, 1.0)]),
            Err(Error::InvalidInput {
                violation: InputViolation::NonFinite,
                ..
            })
        ));
    }
}
