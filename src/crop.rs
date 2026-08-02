// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Private bounded pixel operations for the selected classic perspective crop.
//!
//! Behavioral reference: `tools/infer/utility.py:get_rotate_crop_image` in the
//! pinned upstream baseline. This module implements the observed crop sequence
//! independently; it does not copy or execute upstream source.

use crate::{
    error::{Error, InputViolation, Result},
    geometry::ClassicPerspectiveCropPlan,
    types::ImageDimensions,
};

const MAX_INTERLEAVED_CHANNELS: u8 = 4;
const CUBIC_COEFFICIENT: f64 = -0.75;

/// A private, checked interleaved byte image for crop operations.
///
/// The representation deliberately carries no color-space claim. Future image
/// decoding and preprocessing own BGR/RGB, alpha, orientation, and format
/// semantics. This crop path preserves one through four interleaved channels
/// exactly at integer sampling locations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InterleavedImage {
    dimensions: ImageDimensions,
    channels: u8,
    pixels: Vec<u8>,
}

impl InterleavedImage {
    /// Constructs a bounded image from exact interleaved pixel bytes.
    pub(crate) fn new(dimensions: ImageDimensions, channels: u8, pixels: Vec<u8>) -> Result<Self> {
        if channels == 0 || channels > MAX_INTERLEAVED_CHANNELS {
            return Err(Error::InvalidInput {
                field: "image.channels",
                violation: InputViolation::OutOfRange,
            });
        }
        let expected_bytes = interleaved_byte_len(dimensions, channels)?;
        if pixels.len() != expected_bytes {
            return Err(Error::InvalidInput {
                field: "image.interleaved_bytes",
                violation: InputViolation::OutOfRange,
            });
        }
        Ok(Self {
            dimensions,
            channels,
            pixels,
        })
    }

    /// Returns the checked image dimensions.
    #[must_use]
    pub(crate) const fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }

    /// Returns the number of interleaved channels per pixel.
    #[must_use]
    pub(crate) const fn channels(&self) -> u8 {
        self.channels
    }

    /// Returns the row-major interleaved bytes.
    #[must_use]
    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Applies the selected classic perspective crop plan to interleaved pixels.
///
/// The operation maps each pre-rotation destination pixel through the plan's
/// inverse homography, samples with a fixed cubic kernel and replicated source
/// borders, then applies the exact discrete counter-clockwise byte rotation
/// used by `numpy.rot90` when the plan requires it. The `a = -0.75` cubic
/// kernel is a bounded implementation candidate for the upstream
/// `cv2.INTER_CUBIC` request. Bit-level OpenCV interpolation/rounding parity
/// remains subject to the legal fixture and oracle gates.
pub(crate) fn classic_perspective_crop(
    source: &InterleavedImage,
    plan: ClassicPerspectiveCropPlan,
) -> Result<InterleavedImage> {
    let output_dimensions = ImageDimensions::new(plan.output_width(), plan.output_height())?;
    let output_bytes = interleaved_byte_len(output_dimensions, source.channels())?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_bytes)
        .map_err(|_| Error::Backend {
            message: "crop output allocation failed",
        })?;
    output.resize(output_bytes, 0);

    for output_y in 0..output_dimensions.height() {
        for output_x in 0..output_dimensions.width() {
            let (warp_x, warp_y) = pre_rotation_coordinate(plan, output_x, output_y);
            let (source_x, source_y) =
                plan.map_warp_coordinates_to_source(f64::from(warp_x), f64::from(warp_y))?;
            copy_cubic_sample(
                source,
                source_x,
                source_y,
                &mut output,
                output_dimensions,
                output_x,
                output_y,
            );
        }
    }

    InterleavedImage::new(output_dimensions, source.channels(), output)
}

fn interleaved_byte_len(dimensions: ImageDimensions, channels: u8) -> Result<usize> {
    let bytes =
        dimensions
            .pixels()
            .checked_mul(u64::from(channels))
            .ok_or(Error::ResourceLimit {
                resource: "image.interleaved_bytes",
                limit: u64::MAX,
                actual: u64::MAX,
            })?;
    usize::try_from(bytes).map_err(|_| Error::ResourceLimit {
        resource: "image.interleaved_bytes",
        limit: usize::MAX as u64,
        actual: bytes,
    })
}

fn pre_rotation_coordinate(
    plan: ClassicPerspectiveCropPlan,
    output_x: u32,
    output_y: u32,
) -> (u32, u32) {
    if plan.rotates_counter_clockwise() {
        (plan.warp_width() - 1 - output_y, output_x)
    } else {
        (output_x, output_y)
    }
}

fn copy_cubic_sample(
    source: &InterleavedImage,
    source_x: f64,
    source_y: f64,
    output: &mut [u8],
    output_dimensions: ImageDimensions,
    output_x: u32,
    output_y: u32,
) {
    let horizontal = cubic_axis_samples(source_x, source.dimensions().width());
    let vertical = cubic_axis_samples(source_y, source.dimensions().height());
    let channels = usize::from(source.channels());
    let output_offset = pixel_offset(output_dimensions, channels, output_x, output_y);

    for channel in 0..channels {
        let mut value = 0.0;
        for &(source_y, vertical_weight) in &vertical {
            for &(source_x, horizontal_weight) in &horizontal {
                let source_offset = pixel_offset(source.dimensions(), channels, source_x, source_y);
                value += f64::from(source.pixels()[source_offset + channel])
                    * horizontal_weight
                    * vertical_weight;
            }
        }
        output[output_offset + channel] = saturating_round_to_u8(value);
    }
}

fn cubic_axis_samples(coordinate: f64, length: u32) -> [(u32, f64); 4] {
    debug_assert!(coordinate.is_finite());
    debug_assert!(length > 0);

    let base = coordinate.floor();
    [-1.0, 0.0, 1.0, 2.0].map(|offset| {
        let sample_coordinate = base + offset;
        (
            replicated_index(sample_coordinate, length),
            cubic_weight(coordinate - sample_coordinate),
        )
    })
}

fn replicated_index(coordinate: f64, length: u32) -> u32 {
    if coordinate <= 0.0 {
        0
    } else if coordinate >= f64::from(length - 1) {
        length - 1
    } else {
        coordinate as u32
    }
}

fn cubic_weight(distance: f64) -> f64 {
    let distance = distance.abs();
    if distance <= 1.0 {
        (CUBIC_COEFFICIENT + 2.0) * distance.powi(3) - (CUBIC_COEFFICIENT + 3.0) * distance.powi(2)
            + 1.0
    } else if distance < 2.0 {
        CUBIC_COEFFICIENT * distance.powi(3) - 5.0 * CUBIC_COEFFICIENT * distance.powi(2)
            + 8.0 * CUBIC_COEFFICIENT * distance
            - 4.0 * CUBIC_COEFFICIENT
    } else {
        0.0
    }
}

fn saturating_round_to_u8(value: f64) -> u8 {
    if value <= 0.0 {
        0
    } else if value >= f64::from(u8::MAX) {
        u8::MAX
    } else {
        value.round() as u8
    }
}

fn pixel_offset(dimensions: ImageDimensions, channels: usize, x: u32, y: u32) -> usize {
    debug_assert!(x < dimensions.width());
    debug_assert!(y < dimensions.height());
    (y as usize * dimensions.width() as usize + x as usize) * channels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::{Error, InputViolation},
        geometry::classic_perspective_crop_plan,
        types::{Point, Quadrilateral},
    };

    const CAPTURED_OPENCV_CROP_ORACLE: &str =
        include_str!("../tests/fixtures/classic-v1-crop-oracle/capture.json");

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        }
    }

    fn point(x: f32, y: f32) -> Point {
        match Point::new(x, y) {
            Ok(value) => value,
            Err(error) => panic!("expected valid point, got {error}"),
        }
    }

    fn quadrilateral(left: f32, top: f32, width: f32, height: f32) -> Quadrilateral {
        match Quadrilateral::new([
            point(left, top),
            point(left + width, top),
            point(left + width, top + height),
            point(left, top + height),
        ]) {
            Ok(value) => value,
            Err(error) => panic!("expected valid quadrilateral, got {error}"),
        }
    }

    fn must_ok<T>(value: Result<T>) -> T {
        match value {
            Ok(result) => result,
            Err(error) => panic!("expected success, got {error}"),
        }
    }

    fn assert_captured_bgr_crop(
        fixture_id: &str,
        source_dimensions: ImageDimensions,
        source_pixels: &[u8],
        source_points: [(f32, f32); 4],
        expected_dimensions: ImageDimensions,
        expected_pixels: &[u8],
    ) {
        let source = must_ok(InterleavedImage::new(
            source_dimensions,
            3,
            source_pixels.to_vec(),
        ));
        let quadrilateral = must_ok(Quadrilateral::new(source_points.map(|(x, y)| point(x, y))));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.dimensions(), expected_dimensions, "{fixture_id}");
        assert_eq!(crop.channels(), 3, "{fixture_id}");
        assert_eq!(crop.pixels(), expected_pixels, "{fixture_id}");
    }

    fn patterned_bgr_pixels(width: u32, height: u32, seed: u8) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
        let seed = u32::from(seed);
        for y in 0..height {
            for x in 0..width {
                pixels.extend([
                    ((seed + 31 * x + 17 * y + 7 * x * y) % 256) as u8,
                    ((seed + 11 * x + 47 * y + 13 * x * y + 53) % 256) as u8,
                    ((seed + 59 * x + 19 * y + 5 * x * y + 101) % 256) as u8,
                ]);
            }
        }
        pixels
    }

    #[test]
    fn interleaved_image_rejects_invalid_channel_and_byte_counts() {
        let dimensions = dimensions(2, 1);
        assert!(matches!(
            InterleavedImage::new(dimensions, 0, Vec::new()),
            Err(Error::InvalidInput {
                field: "image.channels",
                violation: InputViolation::OutOfRange,
            })
        ));
        assert!(matches!(
            InterleavedImage::new(dimensions, 3, vec![0; 5]),
            Err(Error::InvalidInput {
                field: "image.interleaved_bytes",
                violation: InputViolation::OutOfRange,
            })
        ));
    }

    #[test]
    fn classic_crop_preserves_identity_pixels_and_channels() {
        let source = must_ok(InterleavedImage::new(
            dimensions(3, 2),
            3,
            vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            ],
        ));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral(
            0.0, 0.0, 3.0, 2.0,
        )));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.dimensions(), dimensions(3, 2));
        assert_eq!(crop.channels(), 3);
        assert_eq!(crop.pixels(), source.pixels());
    }

    #[test]
    fn classic_crop_rotates_tall_pixels_counter_clockwise_at_ratio_boundary() {
        let source = must_ok(InterleavedImage::new(
            dimensions(2, 3),
            1,
            vec![0, 1, 2, 3, 4, 5],
        ));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral(
            0.0, 0.0, 2.0, 3.0,
        )));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.dimensions(), dimensions(3, 2));
        assert_eq!(crop.pixels(), [1, 3, 5, 0, 2, 4]);
    }

    #[test]
    fn classic_crop_replicates_borders() {
        let source = must_ok(InterleavedImage::new(dimensions(2, 1), 1, vec![11, 22]));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral(
            -1.0, 0.0, 3.0, 1.0,
        )));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.dimensions(), dimensions(3, 1));
        assert_eq!(crop.pixels(), [11, 11, 22]);
    }

    #[test]
    fn classic_crop_uses_the_fixed_cubic_kernel_for_subpixel_coordinates() {
        let source = must_ok(InterleavedImage::new(
            dimensions(3, 1),
            1,
            vec![0, 100, 200],
        ));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral(
            -0.5, 0.0, 2.0, 1.0,
        )));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.pixels(), [0, 41]);
    }

    #[test]
    fn classic_crop_preserves_constant_channels_across_projective_sampling() {
        let source = must_ok(InterleavedImage::new(
            dimensions(3, 3),
            3,
            [17, 111, 244].repeat(9),
        ));
        let plan = must_ok(classic_perspective_crop_plan(must_ok(Quadrilateral::new(
            [
                point(-0.75, -0.5),
                point(2.25, 0.0),
                point(2.5, 3.0),
                point(-0.5, 2.5),
            ],
        ))));

        let crop = must_ok(classic_perspective_crop(&source, plan));

        assert_eq!(crop.dimensions(), dimensions(3, 3));
        assert_eq!(crop.pixels(), [17, 111, 244].repeat(9));
    }

    #[test]
    fn classic_crop_matches_the_captured_opencv_bgr_oracle_cases() {
        // The expected bytes below are the reviewed OpenCV outputs in the
        // repository fixture included above. They stay independent of Python,
        // OpenCV, and the upstream checkout during normal Rust test execution.
        for fixture_id in [
            "classic-v1-crop-oracle-identity-bgr-3x2",
            "classic-v1-crop-oracle-border-replicate-bgr-3x2",
            "classic-v1-crop-oracle-projective-bgr-4x3",
            "classic-v1-crop-oracle-tall-rotation-bgr-2x3",
        ] {
            assert!(
                CAPTURED_OPENCV_CROP_ORACLE.contains(fixture_id),
                "fixture record is missing {fixture_id}"
            );
        }

        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-identity-bgr-3x2",
            dimensions(3, 2),
            &[
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            ],
            [(0.0, 0.0), (3.0, 0.0), (3.0, 2.0), (0.0, 2.0)],
            dimensions(3, 2),
            &[
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-border-replicate-bgr-3x2",
            dimensions(3, 2),
            &[
                20, 40, 60, 80, 100, 120, 140, 160, 180, 21, 41, 61, 81, 101, 121, 141, 161, 181,
            ],
            [(-1.0, 0.0), (2.0, 0.0), (2.0, 2.0), (-1.0, 2.0)],
            dimensions(3, 2),
            &[
                20, 40, 60, 20, 40, 60, 80, 100, 120, 21, 41, 61, 21, 41, 61, 81, 101, 121,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-projective-bgr-4x3",
            dimensions(4, 3),
            &[
                0, 7, 19, 31, 43, 59, 71, 83, 97, 101, 109, 127, 13, 29, 47, 61, 73, 89, 107, 131,
                149, 151, 173, 191, 17, 37, 53, 67, 79, 103, 113, 137, 157, 179, 199, 223,
            ],
            [(-0.25, 0.25), (3.4, -0.1), (3.2, 2.5), (-0.5, 2.3)],
            dimensions(3, 2),
            &[
                0, 8, 21, 24, 35, 51, 71, 84, 98, 10, 28, 47, 44, 57, 74, 104, 128, 146,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-tall-rotation-bgr-2x3",
            dimensions(2, 3),
            &[
                0, 1, 2, 10, 11, 12, 20, 21, 22, 30, 31, 32, 40, 41, 42, 50, 51, 52,
            ],
            [(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 3.0)],
            dimensions(3, 2),
            &[
                10, 11, 12, 30, 31, 32, 50, 51, 52, 0, 1, 2, 20, 21, 22, 40, 41, 42,
            ],
        );
    }

    #[test]
    fn classic_crop_matches_extended_opencv_projective_bgr_oracle_cases() {
        // These fixed self-authored source patterns and expected bytes were
        // captured by tools/capture_crop_oracle.py with the fixture's recorded
        // OpenCV environment. They deliberately exercise non-linear values,
        // fractional coordinates, each replicated border, and tall rotation.
        for fixture_id in [
            "classic-v1-crop-oracle-interior-projective-bgr-7x6",
            "classic-v1-crop-oracle-edge-projective-bgr-5x4",
            "classic-v1-crop-oracle-tall-projective-bgr-4x7",
        ] {
            assert!(
                CAPTURED_OPENCV_CROP_ORACLE.contains(fixture_id),
                "fixture record is missing {fixture_id}"
            );
        }

        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-interior-projective-bgr-7x6",
            dimensions(7, 6),
            &patterned_bgr_pixels(7, 6, 23),
            [(0.35, 0.2), (5.7, 0.65), (5.25, 4.6), (0.6, 4.85)],
            dimensions(5, 4),
            &[
                34, 87, 146, 78, 108, 199, 117, 145, 73, 153, 179, 99, 232, 224, 182, 62, 167, 196,
                115, 244, 88, 158, 129, 57, 249, 89, 143, 140, 127, 255, 88, 144, 242, 144, 78,
                142, 243, 77, 91, 136, 118, 225, 41, 200, 152, 113, 40, 116, 201, 78, 67, 107, 190,
                161, 35, 97, 101, 73, 53, 38,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-edge-projective-bgr-5x4",
            dimensions(5, 4),
            &patterned_bgr_pixels(5, 4, 71),
            [(-1.1, -0.6), (4.45, 0.3), (5.1, 4.15), (-0.75, 3.4)],
            dimensions(5, 4),
            &[
                70, 121, 171, 70, 119, 173, 107, 131, 201, 142, 147, 25, 184, 163, 125, 76, 137,
                177, 82, 146, 195, 129, 187, 219, 174, 255, 57, 223, 120, 170, 94, 203, 198, 105,
                231, 188, 158, 191, 70, 249, 57, 118, 95, 102, 211, 112, 152, 222, 129, 135, 144,
                213, 97, 50, 103, 125, 190, 62, 202, 43,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-tall-projective-bgr-4x7",
            dimensions(4, 7),
            &patterned_bgr_pixels(4, 7, 149),
            [(0.25, 0.1), (2.85, 0.45), (2.5, 5.9), (0.0, 6.2)],
            dimensions(6, 2),
            &[
                220, 162, 82, 218, 8, 122, 115, 125, 151, 131, 160, 197, 8, 97, 189, 47, 1, 113,
                156, 204, 189, 177, 183, 9, 199, 35, 46, 206, 107, 63, 207, 149, 81, 233, 189, 98,
            ],
        );
    }

    #[test]
    fn classic_crop_rejects_an_output_that_exceeds_image_limits_before_allocation() {
        let source = must_ok(InterleavedImage::new(dimensions(1, 1), 1, vec![0]));
        let plan = must_ok(classic_perspective_crop_plan(quadrilateral(
            0.0, 0.0, 20_000.0, 1.0,
        )));

        assert!(matches!(
            classic_perspective_crop(&source, plan),
            Err(Error::ResourceLimit {
                resource: "image.width_pixels",
                limit: 16_384,
                actual: 20_000,
            })
        ));
    }
}
