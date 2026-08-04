// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Final DB box filtering and rescaling to source dimensions.
//!
//! This is the tail of `ppocr/postprocess/db_postprocess.py:boxes_from_bitmap`,
//! after the contour, minimum-area box, score, and unclip steps. Its arithmetic
//! is small but every detail below is observable, and each was read from the
//! pinned source rather than assumed:
//!
//! - the score comparison is `box_thresh > score`, so a score **exactly equal**
//!   to the threshold is **kept**;
//! - the first short-side check is `sside < min_size`, and the check after
//!   unclipping is `sside < min_size + 2`;
//! - the rescale runs in `f32`, left to right, as `value / source * dest`;
//! - rounding is half-to-even, matching `np.round`;
//! - the clip bounds are **inclusive** of `dest_width` and `dest_height`, which
//!   is one past the last valid pixel index.

use std::cmp::Ordering;

use crate::error::{Error, InputViolation, Result};

/// Returns whether `value < bound` is **false**, NaN included.
///
/// The upstream checks are all written as `if value < bound: continue`, so the
/// surviving condition is the negation. That distinction matters for NaN: a NaN
/// short side or score is *not* less than the bound, so upstream keeps it.
/// Writing `value >= bound` instead would silently drop it.
fn not_less_than(value: f64, bound: f64) -> bool {
    !matches!(value.partial_cmp(&bound), Some(Ordering::Less))
}

/// The frozen M2 minimum short side for a candidate box.
pub(crate) const MIN_SIZE: f64 = 3.0;

/// Returns whether a candidate survives the pre-unclip short-side check.
pub(crate) fn passes_initial_short_side(short_side: f64) -> bool {
    not_less_than(short_side, MIN_SIZE)
}

/// Returns whether a candidate survives the post-unclip short-side check.
///
/// Upstream compares against `min_size + 2`, not `min_size`.
pub(crate) fn passes_unclipped_short_side(short_side: f64) -> bool {
    not_less_than(short_side, MIN_SIZE + 2.0)
}

/// Returns whether a score survives the detector threshold.
///
/// The upstream test is `if box_thresh > score: continue`, so equality is
/// retained. `box_thresh > score` is false exactly when `score` is not less
/// than the threshold, which is what [`not_less_than`] expresses, NaN included.
pub(crate) fn passes_score(threshold: f64, score: f64) -> bool {
    not_less_than(score, threshold)
}

/// Rescales one box from map coordinates to source dimensions.
///
/// `source_width` and `source_height` are the probability map's dimensions;
/// `dest_width` and `dest_height` are the original image's.
pub(crate) fn rescale_box(
    box_corners: &[(f64, f64)],
    source_width: u32,
    source_height: u32,
    dest_width: u32,
    dest_height: u32,
) -> Result<Vec<(i32, i32)>> {
    if source_width == 0 || source_height == 0 {
        return Err(Error::InvalidInput {
            field: "detector.map_dimensions",
            violation: InputViolation::Empty,
        });
    }
    if box_corners.is_empty() {
        return Err(Error::InvalidInput {
            field: "detector.box",
            violation: InputViolation::Empty,
        });
    }

    box_corners
        .iter()
        .map(|(x, y)| {
            if !x.is_finite() || !y.is_finite() {
                return Err(Error::InvalidInput {
                    field: "detector.box",
                    violation: InputViolation::NonFinite,
                });
            }
            Ok((
                rescale_axis(*x as f32, source_width, dest_width),
                rescale_axis(*y as f32, source_height, dest_height),
            ))
        })
        .collect()
}

/// Rescales one coordinate exactly as the upstream `f32` expression does.
fn rescale_axis(value: f32, source: u32, dest: u32) -> i32 {
    // `value / source * dest`, left to right, in f32 — not a single fused
    // multiply and not f64.
    let scaled = value / source as f32 * dest as f32;
    let rounded = round_half_to_even(scaled);
    // The clip bound is inclusive of `dest`, one past the last pixel index.
    rounded.clamp(0.0, dest as f32) as i32
}

/// Rounds half-way cases to the nearest even integer, matching `np.round`.
fn round_half_to_even(value: f32) -> f32 {
    let nearest = value.round();
    if (value - value.trunc()).abs() == 0.5 && nearest % 2.0 != 0.0 {
        nearest - value.signum()
    } else {
        nearest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_equal_to_the_threshold_is_kept() {
        assert!(passes_score(0.5, 0.5), "equality must be retained");
        assert!(passes_score(0.5, 0.6));
        assert!(!passes_score(0.5, 0.4999999));
    }

    #[test]
    fn short_side_checks_use_their_upstream_bounds() {
        assert!(!passes_initial_short_side(2.999));
        assert!(passes_initial_short_side(3.0));
        // The post-unclip check is min_size + 2, that is 5.
        assert!(!passes_unclipped_short_side(4.999));
        assert!(passes_unclipped_short_side(5.0));
    }

    #[test]
    fn rescaling_maps_map_coordinates_onto_source_dimensions() {
        // A 2x downscale of the map onto the source doubles every coordinate.
        let rescaled = match rescale_box(&[(0.0, 0.0), (10.0, 5.0)], 20, 10, 40, 20) {
            Ok(values) => values,
            Err(error) => panic!("expected a rescaled box, got {error}"),
        };
        assert_eq!(rescaled, [(0, 0), (20, 10)]);
    }

    #[test]
    fn rescaling_clips_inclusively_to_the_destination_extent() {
        // The clip bound is dest itself, one past the last pixel index, so a
        // coordinate at or beyond the edge lands exactly on it.
        let rescaled = match rescale_box(&[(-5.0, -5.0), (100.0, 100.0)], 10, 10, 8, 6) {
            Ok(values) => values,
            Err(error) => panic!("expected a rescaled box, got {error}"),
        };
        assert_eq!(rescaled, [(0, 0), (8, 6)]);
    }

    #[test]
    fn rounding_is_half_to_even() {
        assert_eq!(round_half_to_even(0.5), 0.0);
        assert_eq!(round_half_to_even(1.5), 2.0);
        assert_eq!(round_half_to_even(2.5), 2.0);
        assert_eq!(round_half_to_even(3.5), 4.0);
        assert_eq!(round_half_to_even(-0.5), -0.0);
        assert_eq!(round_half_to_even(-1.5), -2.0);
        // A half-way case reachable through the rescale itself: 1/2 * 1 = 0.5.
        let rescaled = match rescale_box(&[(1.0, 3.0)], 2, 2, 1, 1) {
            Ok(values) => values,
            Err(error) => panic!("expected a rescaled box, got {error}"),
        };
        // 1/2*1 = 0.5 rounds to 0; 3/2*1 = 1.5 rounds to 2, then clips to 1.
        assert_eq!(rescaled, [(0, 1)]);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(matches!(
            rescale_box(&[(0.0, 0.0)], 0, 10, 10, 10),
            Err(Error::InvalidInput {
                field: "detector.map_dimensions",
                ..
            })
        ));
        assert!(matches!(
            rescale_box(&[], 10, 10, 10, 10),
            Err(Error::InvalidInput {
                field: "detector.box",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            rescale_box(&[(f64::NAN, 0.0)], 10, 10, 10, 10),
            Err(Error::InvalidInput {
                violation: InputViolation::NonFinite,
                ..
            })
        ));
    }
}
