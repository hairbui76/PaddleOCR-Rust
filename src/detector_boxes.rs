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
    // A zero destination extent is unreachable through the public path, where
    // the destination comes from validated `ImageDimensions`. It is rejected
    // anyway because the arithmetic below would not fail on it: every
    // coordinate would scale to zero and clamp to zero, so the caller would
    // receive four coincident corners as a successful result.
    if dest_width == 0 || dest_height == 0 {
        return Err(Error::InvalidInput {
            field: "detector.destination_dimensions",
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

/// One detected text region: a rescaled quadrilateral and its score.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetectedBox {
    /// Four corners in source-image coordinates, in `get_mini_boxes` order.
    pub(crate) corners: Vec<(i32, i32)>,
    /// The mean probability inside the pre-unclip box.
    pub(crate) score: f64,
}

/// Maximum candidate contours considered, matching upstream `max_candidates`.
const MAX_CANDIDATES: usize = 1_000;

/// Runs the complete classic DB postprocessing sequence.
///
/// The order is the upstream one and each step is deliberate:
/// threshold, contours, minimum-area box, short-side check, **score on the
/// pre-unclip box**, threshold check, unclip, minimum-area box again, second
/// short-side check, then rescale. Scoring before unclipping is not an
/// optimisation; scoring the expanded box would change which regions survive.
pub(crate) fn classic_db_boxes(
    probability: &[f32],
    map_width: u32,
    map_height: u32,
    box_threshold: f64,
    unclip_ratio: f64,
    dest_width: u32,
    dest_height: u32,
) -> Result<Vec<DetectedBox>> {
    use crate::contour::classic_find_contours;
    use crate::db::{DetectorProbabilityMap, classic_db_binary_segmentation};
    use crate::min_area::classic_min_area_rect;
    use crate::score::classic_box_score;
    use crate::types::ImageDimensions;
    use crate::unclip::{classic_unclip, classic_unclip_distance};

    let dimensions = ImageDimensions::new(map_width, map_height)?;
    let map = DetectorProbabilityMap::new(dimensions, probability)?;
    let bitmap = classic_db_binary_segmentation(map)?;
    let contours = classic_find_contours(&bitmap)?;

    let mut detected = Vec::new();
    for contour in contours.iter().take(MAX_CANDIDATES) {
        let points: Vec<(f64, f64)> = contour
            .points()
            .iter()
            .map(|(x, y)| (f64::from(*x), f64::from(*y)))
            .collect();
        let rect = classic_min_area_rect(&points)?;
        if !passes_initial_short_side(rect.short_side()) {
            continue;
        }

        let corners = rect.ordered_box();
        let score = classic_box_score(probability, map_width, map_height, &corners)?;
        if !passes_score(box_threshold, score) {
            continue;
        }

        let distance = classic_unclip_distance(&corners, unclip_ratio)?;
        let expanded = classic_unclip(&corners, distance)?;
        let expanded: Vec<(f64, f64)> = expanded
            .iter()
            .map(|(x, y)| (*x as f64, *y as f64))
            .collect();
        let rect = classic_min_area_rect(&expanded)?;
        if !passes_unclipped_short_side(rect.short_side()) {
            continue;
        }

        let rescaled = rescale_box(
            &rect.ordered_box(),
            map_width,
            map_height,
            dest_width,
            dest_height,
        )?;
        detected.push(DetectedBox {
            corners: rescaled,
            score,
        });
    }
    Ok(detected)
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    /// Builds a map of `width * height` zeros with `fill` written wherever the
    /// predicate holds, so each boundary case reads as its geometry.
    fn map_with(width: u32, height: u32, fill: f32, inside: impl Fn(u32, u32) -> bool) -> Vec<f32> {
        let mut map = vec![0.0_f32; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                if inside(x, y) {
                    map[(y * width + x) as usize] = fill;
                }
            }
        }
        map
    }

    fn boxes(map: &[f32], width: u32, height: u32, box_threshold: f64) -> Vec<DetectedBox> {
        match classic_db_boxes(map, width, height, box_threshold, 1.5, width, height) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected detected boxes, got {error}"),
        }
    }

    /// The binary mask is `value > 0.3`, so a map that is exactly `0.3`
    /// everywhere has no foreground at all.
    ///
    /// This is the boundary the segmentation threshold sits on. A `>=` here
    /// would turn every uniform 0.3 map into one page-sized region, which is
    /// the difference between "no text" and "the whole page is text".
    #[test]
    fn a_probability_exactly_at_the_segmentation_threshold_is_background() {
        let width = 16;
        let height = 16;
        let at = map_with(width, height, 0.3, |_, _| true);
        assert!(
            boxes(&at, width, height, 0.0).is_empty(),
            "0.3 is not greater than 0.3"
        );

        // The next representable step above the threshold is foreground.
        let above = map_with(width, height, 0.3_f32.next_up(), |x, y| {
            (4..12).contains(&x) && (4..12).contains(&y)
        });
        assert_eq!(
            boxes(&above, width, height, 0.0).len(),
            1,
            "one step above the threshold must be foreground"
        );
    }

    /// A region flush against the map border is detected, and unclipping it
    /// cannot push a corner outside the destination extent.
    ///
    /// Border regions are the ones most likely to produce out-of-range
    /// coordinates, because unclip expands outward with no knowledge of the
    /// image edge; the clip in `rescale_axis` is what contains it.
    #[test]
    fn a_region_touching_the_map_border_is_detected_and_clipped() {
        let width = 24;
        let height = 24;
        let corner = map_with(width, height, 0.9, |x, y| x < 10 && y < 10);
        let detected = boxes(&corner, width, height, 0.5);
        assert_eq!(detected.len(), 1, "a corner region must still be detected");
        for (x, y) in &detected[0].corners {
            assert!(
                (0..=i32::try_from(width).unwrap_or(i32::MAX)).contains(x),
                "x {x} left the destination after unclipping"
            );
            assert!(
                (0..=i32::try_from(height).unwrap_or(i32::MAX)).contains(y),
                "y {y} left the destination after unclipping"
            );
        }
    }

    /// Two regions separated by background are two boxes; two regions that
    /// touch are one.
    ///
    /// Nothing in this path merges or splits regions on its own — connectivity
    /// in the bitmap is the only thing that decides, so the boundary worth
    /// pinning is a one-pixel gap against no gap.
    #[test]
    fn separation_alone_decides_whether_regions_merge() {
        let width = 40;
        let height = 20;

        let separated = map_with(width, height, 0.9, |x, y| {
            (5..15).contains(&y) && ((4..16).contains(&x) || (20..32).contains(&x))
        });
        assert_eq!(
            boxes(&separated, width, height, 0.5).len(),
            2,
            "a four-pixel gap must keep the regions apart"
        );

        let joined = map_with(width, height, 0.9, |x, y| {
            (5..15).contains(&y) && (4..32).contains(&x)
        });
        assert_eq!(
            boxes(&joined, width, height, 0.5).len(),
            1,
            "one connected region is one box"
        );
    }

    /// A diagonal region produces a rotated quadrilateral, not the axis-aligned
    /// bounding box of its pixels.
    ///
    /// This is the visible difference between `minAreaRect` and a bounding box.
    /// If the geometry silently degraded to an axis-aligned box, every rotated
    /// line would crop with its neighbours' pixels included.
    #[test]
    fn a_diagonal_region_produces_a_rotated_quadrilateral() {
        let width = 48;
        let height = 48;
        let band = map_with(width, height, 0.9, |x, y| {
            (8..40).contains(&x) && x.abs_diff(y) <= 3
        });
        let detected = boxes(&band, width, height, 0.5);
        assert_eq!(detected.len(), 1, "the band is one region");

        let corners = &detected[0].corners;
        let axis_aligned = corners
            .iter()
            .all(|(x, _)| *x == corners[0].0 || *x == corners[2].0);
        assert!(
            !axis_aligned,
            "a diagonal band must not collapse to an axis-aligned box: {corners:?}"
        );
        // The quadrilateral must still be inside the image and non-degenerate.
        let distinct_x: std::collections::BTreeSet<i32> = corners.iter().map(|(x, _)| *x).collect();
        assert!(distinct_x.len() >= 3, "a rotated box has spread corners");
    }

    /// A one-pixel-tall strip of any length never reaches scoring.
    ///
    /// The short-side check runs before scoring, so an extreme aspect ratio is
    /// rejected on geometry regardless of how confident the map is.
    #[test]
    fn an_extreme_aspect_ratio_strip_is_rejected_before_scoring() {
        let width = 64;
        let height = 16;
        let strip = map_with(width, height, 1.0, |x, y| y == 8 && (2..62).contains(&x));
        assert!(
            boxes(&strip, width, height, 0.0).is_empty(),
            "a one-pixel short side is below min_size even at probability 1.0 \
             and threshold 0.0"
        );
    }

    /// A single foreground pixel is degenerate and produces nothing.
    #[test]
    fn a_single_foreground_pixel_produces_no_box() {
        let width = 16;
        let height = 16;
        let dot = map_with(width, height, 1.0, |x, y| x == 8 && y == 8);
        assert!(boxes(&dot, width, height, 0.0).is_empty());
    }

    /// A non-finite probability is a typed error, not a panic and not a box.
    #[test]
    fn a_non_finite_probability_is_a_typed_error() {
        for poison in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut map = vec![0.9_f32; 16 * 16];
            map[42] = poison;
            assert!(
                classic_db_boxes(&map, 16, 16, 0.5, 1.5, 16, 16).is_err(),
                "{poison} must be rejected"
            );
        }
    }

    /// A map whose length disagrees with its declared dimensions is rejected
    /// before any pixel is read.
    #[test]
    fn a_map_length_mismatch_is_rejected() {
        let map = vec![0.0_f32; 10];
        assert!(classic_db_boxes(&map, 8, 8, 0.5, 1.5, 8, 8).is_err());
        assert!(classic_db_boxes(&[], 8, 8, 0.5, 1.5, 8, 8).is_err());
    }

    /// Zero destination dimensions are rejected rather than dividing by zero.
    #[test]
    fn a_zero_destination_extent_is_rejected() {
        let map = map_with(20, 20, 0.9, |x, y| {
            (4..16).contains(&x) && (4..16).contains(&y)
        });
        assert!(classic_db_boxes(&map, 20, 20, 0.5, 1.5, 0, 20).is_err());
        assert!(classic_db_boxes(&map, 20, 20, 0.5, 1.5, 20, 0).is_err());
    }

    /// A probability map with one solid block produces one detected box.
    #[test]
    fn a_single_region_produces_one_rescaled_box() {
        let (width, height) = (20_u32, 16_u32);
        let mut map = vec![0.0_f32; (width * height) as usize];
        for y in 3..12 {
            for x in 4..16 {
                map[(y * width + x) as usize] = 0.9;
            }
        }

        let boxes = match classic_db_boxes(&map, width, height, 0.5, 1.5, 40, 32) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected a detected box, got {error}"),
        };
        assert_eq!(boxes.len(), 1, "one solid region must yield one box");
        assert_eq!(boxes[0].corners.len(), 4);
        assert!(
            boxes[0].score > 0.8,
            "a solid 0.9 region must score high, got {}",
            boxes[0].score
        );
        // The destination is exactly twice the map, so every corner doubles.
        for (x, y) in &boxes[0].corners {
            assert!((0..=40).contains(x), "x {x} outside the destination");
            assert!((0..=32).contains(y), "y {y} outside the destination");
        }
    }

    #[test]
    fn an_empty_map_produces_no_boxes() {
        let map = vec![0.0_f32; 8 * 8];
        let boxes = match classic_db_boxes(&map, 8, 8, 0.5, 1.5, 8, 8) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(boxes.is_empty());
    }

    /// A region below the score threshold is dropped.
    #[test]
    fn a_low_scoring_region_is_filtered_out() {
        let (width, height) = (20_u32, 16_u32);
        let mut map = vec![0.0_f32; (width * height) as usize];
        for y in 3..12 {
            for x in 4..16 {
                // Above the 0.3 segmentation threshold but below box_thresh.
                map[(y * width + x) as usize] = 0.35;
            }
        }
        let boxes = match classic_db_boxes(&map, width, height, 0.6, 1.5, 20, 16) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(boxes.is_empty(), "a 0.35 region must not survive 0.6");
    }

    /// A region too small in its short side never reaches scoring.
    #[test]
    fn a_tiny_region_is_dropped_by_the_short_side_check() {
        let (width, height) = (12_u32, 12_u32);
        let mut map = vec![0.0_f32; (width * height) as usize];
        // A 2-pixel-tall stripe: its short side is below min_size.
        for y in 5..7 {
            for x in 2..10 {
                map[(y * width + x) as usize] = 0.95;
            }
        }
        let boxes = match classic_db_boxes(&map, width, height, 0.5, 1.5, 12, 12) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(boxes.is_empty(), "a 2-pixel short side must be rejected");
    }
}
