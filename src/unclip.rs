// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0
//
// The polygon offset reproduced here was derived from Clipper's observable
// output through pyclipper, recorded in
// `tests/fixtures/classic-v1-unclip-score-grid/`. See NOTICE.

//! Polygon expansion matching PaddleOCR's `unclip`.
//!
//! Upstream computes `distance = area * unclip_ratio / perimeter` with Shapely
//! and then offsets the box with `PyclipperOffset` using `JT_ROUND` and
//! `ET_CLOSEDPOLYGON`.
//!
//! Two upstream behaviours are reproduced deliberately rather than corrected:
//!
//! - Clipper converts each coordinate to an integer by **truncation toward
//!   zero**, and PaddleOCR applies no scaling first. The polygon that is offset
//!   is therefore not the polygon that was scored.
//! - The offset result is not clipped to the map, so negative coordinates reach
//!   the caller unchanged.
//!
//! One deviation is intentional and recorded. Clipper finishes with a union
//! pass that removes duplicate and collinear vertices and re-emits each polygon
//! from its own starting vertex. This module does **not** implement that union.
//! It is unobservable in the M2 pipeline because `get_mini_boxes` immediately
//! reduces the path to a rotated rectangle, and the minimum-area rectangle is
//! invariant to vertex order, duplicate vertices, and collinear vertices. If a
//! later consumer ever reads the polygon directly, this must be revisited.

use crate::error::{Error, InputViolation, Result};

/// Clipper's default arc tolerance.
const ARC_TOLERANCE: f64 = 0.25;

/// Maximum vertices accepted or produced for one polygon.
const MAX_VERTICES: usize = 100_000;

/// Returns the upstream unclip distance for one polygon.
///
/// This is `area * unclip_ratio / perimeter`, evaluated in that order, with the
/// unsigned shoelace area and the closed-ring perimeter.
pub(crate) fn classic_unclip_distance(polygon: &[(f64, f64)], unclip_ratio: f64) -> Result<f64> {
    if polygon.len() < 3 {
        return Err(Error::InvalidInput {
            field: "unclip.polygon",
            violation: InputViolation::DegenerateGeometry,
        });
    }
    if !unclip_ratio.is_finite() || unclip_ratio <= 0.0 {
        return Err(Error::InvalidInput {
            field: "unclip.ratio",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut twice_area = 0.0_f64;
    let mut perimeter = 0.0_f64;
    for index in 0..polygon.len() {
        let (x0, y0) = polygon[index];
        let (x1, y1) = polygon[(index + 1) % polygon.len()];
        if !x0.is_finite() || !y0.is_finite() {
            return Err(Error::InvalidInput {
                field: "unclip.polygon",
                violation: InputViolation::NonFinite,
            });
        }
        twice_area += x0 * y1 - x1 * y0;
        perimeter += (x1 - x0).hypot(y1 - y0);
    }
    let area = twice_area.abs() * 0.5;
    if perimeter == 0.0 {
        return Err(Error::InvalidInput {
            field: "unclip.polygon",
            violation: InputViolation::DegenerateGeometry,
        });
    }
    Ok(area * unclip_ratio / perimeter)
}

/// Expands one closed polygon outwards by `distance` with round joins.
pub(crate) fn classic_unclip(polygon: &[(f64, f64)], distance: f64) -> Result<Vec<(i64, i64)>> {
    if polygon.len() < 3 || polygon.len() > MAX_VERTICES {
        return Err(Error::InvalidInput {
            field: "unclip.polygon",
            violation: InputViolation::DegenerateGeometry,
        });
    }
    if !distance.is_finite() || distance == 0.0 {
        return Err(Error::InvalidInput {
            field: "unclip.distance",
            violation: InputViolation::OutOfRange,
        });
    }

    // Clipper truncates each coordinate toward zero as it ingests the path,
    // then drops consecutive duplicates and a trailing repeat of the first
    // point.
    let mut path: Vec<(i64, i64)> = Vec::new();
    for (x, y) in polygon {
        if !x.is_finite() || !y.is_finite() {
            return Err(Error::InvalidInput {
                field: "unclip.polygon",
                violation: InputViolation::NonFinite,
            });
        }
        let point = (*x as i64, *y as i64);
        if path.last() != Some(&point) {
            path.push(point);
        }
    }
    if path.len() > 1 && path.first() == path.last() {
        path.pop();
    }
    if path.len() < 3 {
        return Err(Error::InvalidInput {
            field: "unclip.polygon",
            violation: InputViolation::DegenerateGeometry,
        });
    }

    let count = path.len();
    let normals: Vec<(f64, f64)> = (0..count)
        .map(|index| unit_normal(path[index], path[(index + 1) % count]))
        .collect();

    let magnitude = distance.abs();
    let arc = if ARC_TOLERANCE <= magnitude * 0.25 {
        ARC_TOLERANCE
    } else {
        magnitude * 0.25
    };
    let mut steps = std::f64::consts::PI / (1.0 - arc / magnitude).acos();
    if steps > magnitude * std::f64::consts::PI {
        steps = magnitude * std::f64::consts::PI;
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut step_sin = (two_pi / steps).sin();
    let step_cos = (two_pi / steps).cos();
    let steps_per_radian = steps / two_pi;
    if distance < 0.0 {
        step_sin = -step_sin;
    }

    let mut result: Vec<(i64, i64)> = Vec::new();
    for index in 0..count {
        let previous = (index + count - 1) % count;
        let (njx, njy) = normals[index];
        let (nkx, nky) = normals[previous];
        let source = path[index];

        let sine = nkx * njy - njx * nky;
        let cosine = njx * nkx + njy * nky;

        if (sine * distance).abs() < 1.0 && cosine > 0.0 {
            result.push(offset_point(source, (nkx, nky), distance));
        } else if sine.clamp(-1.0, 1.0) * distance < 0.0 {
            result.push(offset_point(source, (nkx, nky), distance));
            result.push(source);
            result.push(offset_point(source, (njx, njy), distance));
        } else {
            let angle = sine.clamp(-1.0, 1.0).atan2(cosine);
            let arc_steps = clipper_round(steps_per_radian * angle.abs()).max(1);
            let (mut x, mut y) = (nkx, nky);
            for _ in 0..arc_steps {
                result.push(offset_point(source, (x, y), distance));
                let rotated = x * step_cos - step_sin * y;
                y = x * step_sin + y * step_cos;
                x = rotated;
            }
            result.push(offset_point(source, (njx, njy), distance));
        }

        if result.len() > MAX_VERTICES {
            return Err(Error::ResourceLimit {
                resource: "unclip.vertices",
                limit: MAX_VERTICES as u64,
                actual: result.len() as u64,
            });
        }
    }
    Ok(result)
}

/// Clipper's unit normal: a reciprocal is computed once and then multiplied.
fn unit_normal(from: (i64, i64), to: (i64, i64)) -> (f64, f64) {
    let dx = (to.0 - from.0) as f64;
    let dy = (to.1 - from.1) as f64;
    if dx == 0.0 && dy == 0.0 {
        return (0.0, 0.0);
    }
    let factor = 1.0 / (dx * dx + dy * dy).sqrt();
    (dy * factor, -dx * factor)
}

/// Offsets one source vertex along a normal and rounds like Clipper.
fn offset_point(source: (i64, i64), normal: (f64, f64), distance: f64) -> (i64, i64) {
    (
        clipper_round(source.0 as f64 + normal.0 * distance),
        clipper_round(source.1 as f64 + normal.1 * distance),
    )
}

/// Clipper's rounding: truncate after moving half a unit away from zero.
fn clipper_round(value: f64) -> i64 {
    if value < 0.0 {
        (value - 0.5) as i64
    } else {
        (value + 0.5) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const CAPTURED_UNCLIP_GRID: &str =
        include_str!("../tests/fixtures/classic-v1-unclip-score-grid/capture.json");

    /// Normalises a closed path so it can be compared across the union pass.
    ///
    /// Clipper's union removes duplicate and collinear vertices and re-emits
    /// the polygon from its own starting vertex. This module deliberately does
    /// not implement that union, so both sides are normalised the same way
    /// before comparison; the module documentation records why that is safe.
    fn normalise(path: &[(i64, i64)]) -> Vec<(i64, i64)> {
        let mut deduped: Vec<(i64, i64)> = Vec::new();
        for point in path {
            if deduped.last() != Some(point) {
                deduped.push(*point);
            }
        }
        if deduped.len() > 1 && deduped.first() == deduped.last() {
            deduped.pop();
        }
        let count = deduped.len();
        let mut corners: Vec<(i64, i64)> = Vec::new();
        for index in 0..count {
            let a = deduped[(index + count - 1) % count];
            let b = deduped[index];
            let c = deduped[(index + 1) % count];
            let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if cross != 0 {
                corners.push(b);
            }
        }
        if corners.is_empty() {
            corners = deduped;
        }
        let start = corners
            .iter()
            .enumerate()
            .min_by_key(|(_, point)| **point)
            .map(|(index, _)| index)
            .unwrap_or(0);
        corners.rotate_left(start);
        corners
    }

    #[test]
    fn classic_unclip_executes_every_captured_case() {
        let capture: Value = match serde_json::from_str(CAPTURED_UNCLIP_GRID) {
            Ok(value) => value,
            Err(error) => panic!("unclip capture is not valid JSON: {error}"),
        };
        let cases = match capture.get("cases").and_then(Value::as_array) {
            Some(cases) => cases,
            None => panic!("unclip capture must contain cases"),
        };
        assert_eq!(cases.len(), 8, "captured unclip case count");

        let mut checked = 0_usize;
        for case in cases {
            let fixture_id = case
                .get("fixture_id")
                .and_then(Value::as_str)
                .unwrap_or("<unnamed>");
            let polygon: Vec<(f64, f64)> = match case.get("box").and_then(Value::as_array) {
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

            let entries = match case.get("unclip").and_then(Value::as_array) {
                Some(entries) => entries,
                None => panic!("{fixture_id} is missing its unclip records"),
            };
            for entry in entries {
                let ratio = entry
                    .get("unclip_ratio")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                let expected_distance = entry
                    .get("distance")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                let distance = match classic_unclip_distance(&polygon, ratio) {
                    Ok(value) => value,
                    Err(error) => panic!("{fixture_id} distance failed: {error}"),
                };
                assert!(
                    (distance - expected_distance).abs() < 1e-12,
                    "{fixture_id} r={ratio}: distance {distance} vs {expected_distance}"
                );

                let expected: Vec<(i64, i64)> = match entry.get("paths").and_then(Value::as_array) {
                    Some(paths) if !paths.is_empty() => paths[0]
                        .as_array()
                        .unwrap_or(&Vec::new())
                        .iter()
                        .map(|point| {
                            let pair = match point.as_array() {
                                Some(pair) => pair,
                                None => panic!("{fixture_id} path point must be an array"),
                            };
                            (
                                pair[0].as_i64().unwrap_or_default(),
                                pair[1].as_i64().unwrap_or_default(),
                            )
                        })
                        .collect(),
                    _ => panic!("{fixture_id} recorded no offset path"),
                };
                let actual = match classic_unclip(&polygon, distance) {
                    Ok(path) => path,
                    Err(error) => panic!("{fixture_id} unclip failed: {error}"),
                };
                assert_eq!(
                    normalise(&actual),
                    normalise(&expected),
                    "{fixture_id} r={ratio} offset path"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 16, "every box must be checked at both ratios");
    }

    #[test]
    fn invalid_polygons_and_distances_are_rejected() {
        assert!(matches!(
            classic_unclip_distance(&[(0.0, 0.0), (1.0, 0.0)], 1.5),
            Err(Error::InvalidInput {
                violation: InputViolation::DegenerateGeometry,
                ..
            })
        ));
        let square = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
        assert!(matches!(
            classic_unclip_distance(&square, 0.0),
            Err(Error::InvalidInput {
                field: "unclip.ratio",
                ..
            })
        ));
        assert!(matches!(
            classic_unclip(&square, 0.0),
            Err(Error::InvalidInput {
                field: "unclip.distance",
                ..
            })
        ));
    }
}
