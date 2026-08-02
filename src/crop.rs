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
const CUBIC_COEFFICIENT: f32 = -0.75;

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
/// The operation maps each pre-rotation destination pixel through a private
/// OpenCV-style inverse sampling transform, samples with a fixed cubic kernel
/// and replicated source borders, then applies the exact discrete
/// counter-clockwise byte rotation used by `numpy.rot90` when the plan requires
/// it. The transform's `f32` coefficient/row-evaluation boundary and the
/// `a = -0.75` cubic interpolation arithmetic are required by reviewed OpenCV
/// 5.0.0 rounding cases, but remain a bounded implementation candidate rather
/// than a general `cv2.INTER_CUBIC` equivalence claim.
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
                plan.map_warp_pixel_to_source_for_sampling(warp_x, warp_y)?;
            copy_cubic_sample(
                source,
                source_x,
                source_y,
                &mut output,
                output_dimensions,
                output_x,
                output_y,
            )?;
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
) -> Result<()> {
    // The recorded OpenCV 5.0.0 uint8 component case requires single-precision
    // interpolation arithmetic after projective coordinates are established.
    // Keep geometry in f64, then perform sampling in f32 so the oracle can
    // expose its distinct rounding boundary.
    let source_x = sampling_coordinate(source_x, "crop.source_x")?;
    let source_y = sampling_coordinate(source_y, "crop.source_y")?;
    let horizontal = cubic_axis_samples(source_x, source.dimensions().width());
    let vertical = cubic_axis_samples(source_y, source.dimensions().height());
    let channels = usize::from(source.channels());
    let output_offset = pixel_offset(output_dimensions, channels, output_x, output_y);

    for channel in 0..channels {
        let mut value = 0.0_f32;
        for &(source_y, vertical_weight) in &vertical {
            // OpenCV 5.0.0's reference cubic path first accumulates the four
            // horizontal taps, then applies the vertical weight. Preserve that
            // f32 operation grouping: flattening all sixteen products can
            // cross a uint8 rounding boundary for the same source map.
            let mut horizontal_value = 0.0_f32;
            for &(source_x, horizontal_weight) in &horizontal {
                let source_offset = pixel_offset(source.dimensions(), channels, source_x, source_y);
                horizontal_value +=
                    f32::from(source.pixels()[source_offset + channel]) * horizontal_weight;
            }
            value += horizontal_value * vertical_weight;
        }
        output[output_offset + channel] = saturating_round_to_u8(value);
    }
    Ok(())
}

fn sampling_coordinate(coordinate: f64, field: &'static str) -> Result<f32> {
    if !coordinate.is_finite() {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::NonFinite,
        });
    }
    if coordinate < f64::from(f32::MIN) || coordinate > f64::from(f32::MAX) {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(coordinate as f32)
}

fn cubic_axis_samples(coordinate: f32, length: u32) -> [(u32, f32); 4] {
    debug_assert!(coordinate.is_finite());
    debug_assert!(length > 0);

    let base = coordinate.floor();
    let alpha = coordinate - base;
    // A finite `f32` coordinate immediately below a negative integer can lose
    // its sub-unit difference during this subtraction, yielding the exact
    // upper phase endpoint. At `alpha == 1.0`, this cubic kernel selects the
    // next integer sample, which is the correct representable endpoint.
    debug_assert!(
        (0.0..=1.0).contains(&alpha),
        "cubic phase {alpha:?} for coordinate {coordinate:?} and floor {base:?}"
    );
    let weights = cubic_weights(alpha);
    core::array::from_fn(|index| {
        let offset = index as f32 - 1.0;
        let sample_coordinate = base + offset;
        (replicated_index(sample_coordinate, length), weights[index])
    })
}

fn replicated_index(coordinate: f32, length: u32) -> u32 {
    if coordinate <= 0.0 {
        0
    } else if coordinate >= (length - 1) as f32 {
        length - 1
    } else {
        coordinate as u32
    }
}

fn cubic_weights(alpha: f32) -> [f32; 4] {
    debug_assert!((0.0..=1.0).contains(&alpha));

    // Preserve the OpenCV 5.0.0 bicubic weight construction order rather than
    // using an algebraically equivalent distance polynomial. The two forms can
    // differ at a uint8 rounding boundary after f32 arithmetic.
    let alpha_squared = alpha * alpha;
    let inverse = 1.0 - alpha;
    let inverse_squared = inverse * inverse;
    let first = CUBIC_COEFFICIENT * alpha * inverse_squared;
    let fourth = CUBIC_COEFFICIENT * alpha_squared * inverse;
    let second =
        alpha_squared * ((CUBIC_COEFFICIENT + 2.0) * alpha - (CUBIC_COEFFICIENT + 3.0)) + 1.0;
    let third = 1.0 - first - second - fourth;
    [first, second, third, fourth]
}

fn saturating_round_to_u8(value: f32) -> u8 {
    if value <= 0.0 {
        0
    } else if value >= f32::from(u8::MAX) {
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

    fn lcg_bgr_pixels(width: u32, height: u32, seed: u32) -> Vec<u8> {
        let byte_count = width as usize * height as usize * 3;
        let mut pixels = Vec::with_capacity(byte_count);
        let mut state = seed;
        for _ in 0..byte_count {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            pixels.push((state >> 24) as u8);
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
    fn cubic_sampling_coordinates_reject_nonfinite_or_unrepresentable_values() {
        assert!(matches!(
            sampling_coordinate(f64::INFINITY, "crop.source_x"),
            Err(Error::InvalidInput {
                field: "crop.source_x",
                violation: InputViolation::NonFinite,
            })
        ));
        assert!(matches!(
            sampling_coordinate(f64::from(f32::MAX) * 2.0, "crop.source_y"),
            Err(Error::InvalidInput {
                field: "crop.source_y",
                violation: InputViolation::OutOfRange,
            })
        ));
    }

    #[test]
    fn cubic_sampling_accepts_the_representable_upper_phase_endpoint() {
        // `-f32::MIN_POSITIVE` is negative but too close to zero for
        // `coordinate - floor(coordinate)` to retain a value below one. The
        // resulting exact endpoint must select the next integral sample rather
        // than panicking in debug builds.
        let samples = cubic_axis_samples(-f32::MIN_POSITIVE, 4);
        assert_eq!(samples.map(|(index, _)| index), [0, 0, 0, 1]);
        assert_eq!(samples.map(|(_, weight)| weight), [0.0, 0.0, 1.0, 0.0]);
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
    fn classic_crop_preserves_all_interleaved_channels_across_projective_cases() {
        // These self-authored source-level cases deliberately do not make an
        // OpenCV or decoded-image claim. They exercise the crop's stated
        // private 1–4-channel invariant across replicated borders, fractional
        // projective sampling, and the tall-crop counter-clockwise rotation.
        let cases = [
            (
                "bordered-square",
                dimensions(3, 3),
                [(-0.75, -0.5), (2.75, 0.25), (3.25, 2.75), (-0.5, 3.25)],
                dimensions(3, 3),
                false,
            ),
            (
                "fractional-wide",
                dimensions(9, 6),
                [(0.25, 0.0), (8.8, 0.6), (8.2, 5.7), (0.1, 5.2)],
                dimensions(8, 5),
                false,
            ),
            (
                "fractional-tall",
                dimensions(5, 11),
                [(0.1, -0.3), (4.6, 0.2), (4.3, 10.9), (-0.25, 10.4)],
                dimensions(10, 4),
                true,
            ),
        ];
        let channel_values = [23_u8, 97, 171, 245];

        for (case_id, source_dimensions, source_points, expected_dimensions, expected_rotation) in
            cases
        {
            let plan = must_ok(classic_perspective_crop_plan(must_ok(Quadrilateral::new(
                source_points.map(|(x, y)| point(x, y)),
            ))));
            assert_eq!(
                plan.output_width(),
                expected_dimensions.width(),
                "{case_id} output width"
            );
            assert_eq!(
                plan.output_height(),
                expected_dimensions.height(),
                "{case_id} output height"
            );
            assert_eq!(
                plan.rotates_counter_clockwise(),
                expected_rotation,
                "{case_id} rotation"
            );

            for channels in 1..=MAX_INTERLEAVED_CHANNELS {
                let channel_values = &channel_values[..usize::from(channels)];
                let source = must_ok(InterleavedImage::new(
                    source_dimensions,
                    channels,
                    channel_values.repeat(source_dimensions.pixels() as usize),
                ));

                let crop = must_ok(classic_perspective_crop(&source, plan));
                assert_eq!(
                    crop.dimensions(),
                    expected_dimensions,
                    "{case_id}, {channels} channels"
                );
                assert_eq!(crop.channels(), channels, "{case_id}, {channels} channels");
                assert_eq!(
                    crop.pixels(),
                    channel_values.repeat(expected_dimensions.pixels() as usize),
                    "{case_id}, {channels} channels"
                );
            }
        }
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
    fn classic_crop_matches_fractional_extent_opencv_oracle_cases() {
        // These self-authored BGR cases extend the recorded OpenCV corpus with
        // eighth-pixel interior phases, a one-pixel result, and a tall thin
        // result. Their expected bytes remain offline and independent of the
        // Python/OpenCV capture environment at Rust test time.
        for fixture_id in [
            "classic-v1-crop-oracle-phase-projective-bgr-8x8",
            "classic-v1-crop-oracle-single-pixel-bgr-3x3",
            "classic-v1-crop-oracle-tall-thin-projective-bgr-3x9",
        ] {
            assert!(
                CAPTURED_OPENCV_CROP_ORACLE.contains(fixture_id),
                "fixture record is missing {fixture_id}"
            );
        }

        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-phase-projective-bgr-8x8",
            dimensions(8, 8),
            &patterned_bgr_pixels(8, 8, 211),
            [
                (0.125, 0.375),
                (6.875, 0.625),
                (6.625, 6.875),
                (0.375, 6.625),
            ],
            dimensions(6, 6),
            &[
                211, 23, 67, 111, 43, 144, 26, 63, 177, 80, 82, 74, 116, 122, 82, 180, 158, 176,
                220, 82, 92, 0, 119, 192, 85, 166, 162, 126, 241, 26, 216, 136, 136, 98, 78, 255,
                95, 140, 120, 54, 229, 161, 117, 191, 58, 207, 97, 93, 133, 104, 222, 26, 150, 132,
                14, 150, 149, 96, 70, 144, 169, 102, 22, 126, 129, 160, 22, 248, 109, 83, 125, 50,
                48, 187, 183, 121, 22, 192, 225, 171, 59, 144, 143, 218, 47, 77, 129, 141, 183, 79,
                69, 95, 136, 163, 154, 57, 147, 168, 132, 52, 62, 119, 115, 185, 53, 237, 122, 168,
            ],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-single-pixel-bgr-3x3",
            dimensions(3, 3),
            &patterned_bgr_pixels(3, 3, 37),
            [(0.49, 0.49), (1.49, 0.49), (1.49, 1.49), (0.49, 1.49)],
            dimensions(1, 1),
            &[57, 113, 193],
        );
        assert_captured_bgr_crop(
            "classic-v1-crop-oracle-tall-thin-projective-bgr-3x9",
            dimensions(3, 9),
            &patterned_bgr_pixels(3, 9, 101),
            [(0.4, 0.1), (1.8, 0.2), (1.6, 7.9), (0.2, 7.6)],
            dimensions(7, 1),
            &[
                112, 162, 123, 133, 209, 156, 156, 143, 144, 186, 48, 8, 206, 130, 48, 186, 150,
                66, 167, 183, 90,
            ],
        );
    }

    #[test]
    fn classic_crop_matches_cubic_rounding_opencv_oracle_case() {
        // This high-variation self-authored case was added after an independent
        // diagnostic found a one-byte disagreement near a cubic half-byte
        // rounding boundary. It stays a narrow OpenCV 5.0.0 component oracle.
        const FIXTURE_ID: &str = "classic-v1-crop-oracle-cubic-rounding-bgr-8x10";
        assert!(
            CAPTURED_OPENCV_CROP_ORACLE.contains(FIXTURE_ID),
            "fixture record is missing {FIXTURE_ID}"
        );

        assert_captured_bgr_crop(
            FIXTURE_ID,
            dimensions(8, 10),
            &lcg_bgr_pixels(8, 10, 162),
            [
                (1.8328327, -0.8944577),
                (8.7014475, -0.5864337),
                (8.67722, 11.502462),
                (2.2030663, 11.573961),
            ],
            dimensions(12, 6),
            &[
                195, 255, 233, 89, 198, 203, 98, 20, 107, 202, 74, 89, 67, 237, 91, 15, 121, 47,
                25, 0, 22, 176, 111, 160, 136, 166, 211, 90, 172, 154, 116, 184, 137, 113, 182,
                140, 217, 148, 157, 140, 166, 191, 39, 184, 172, 92, 96, 76, 38, 149, 86, 82, 189,
                128, 105, 57, 122, 108, 119, 169, 108, 192, 131, 106, 164, 118, 116, 153, 144, 115,
                155, 141, 37, 114, 166, 51, 172, 137, 72, 210, 165, 143, 73, 131, 157, 64, 70, 106,
                210, 217, 160, 188, 171, 77, 72, 175, 106, 186, 124, 171, 238, 38, 164, 218, 32,
                162, 217, 36, 188, 31, 255, 147, 57, 222, 140, 106, 87, 190, 45, 66, 67, 12, 109,
                96, 71, 93, 111, 102, 77, 46, 22, 177, 71, 97, 161, 95, 132, 160, 80, 109, 185, 85,
                117, 176, 229, 76, 175, 211, 78, 142, 182, 106, 0, 192, 177, 14, 27, 124, 133, 129,
                126, 202, 246, 247, 105, 230, 91, 38, 234, 0, 76, 122, 107, 75, 50, 168, 73, 58,
                153, 80, 46, 17, 235, 64, 22, 210, 148, 50, 121, 89, 22, 173, 30, 108, 54, 56, 205,
                107, 120, 40, 136, 38, 5, 46, 157, 134, 74, 196, 224, 176, 121, 230, 209, 127, 228,
                198,
            ],
        );
    }

    #[test]
    fn classic_crop_matches_cubic_weight_construction_opencv_oracle_case() {
        const FIXTURE_ID: &str = "classic-v1-crop-oracle-cubic-weight-order-bgr-5x10";
        assert!(
            CAPTURED_OPENCV_CROP_ORACLE.contains(FIXTURE_ID),
            "fixture record is missing {FIXTURE_ID}"
        );

        assert_captured_bgr_crop(
            FIXTURE_ID,
            dimensions(5, 10),
            &lcg_bgr_pixels(5, 10, 847_333),
            [
                (0.9, -0.666_666_7),
                (5.142_857, -0.846_666_7),
                (5.142_857, 9.526_316),
                (1.29, 9.676_315),
            ],
            dimensions(10, 4),
            &[
                91, 89, 192, 129, 112, 131, 101, 223, 82, 123, 101, 81, 255, 0, 58, 246, 61, 51,
                233, 0, 72, 255, 115, 137, 151, 47, 37, 152, 50, 50, 147, 131, 178, 193, 111, 162,
                195, 157, 122, 179, 207, 167, 130, 205, 216, 92, 57, 250, 147, 66, 111, 101, 157,
                110, 93, 194, 212, 110, 177, 162, 56, 64, 167, 159, 123, 83, 135, 85, 68, 6, 160,
                146, 124, 213, 187, 127, 181, 121, 242, 140, 41, 80, 128, 74, 54, 50, 238, 175, 50,
                143, 228, 67, 189, 190, 61, 138, 176, 26, 76, 146, 63, 92, 45, 176, 175, 184, 127,
                231, 128, 118, 186, 155, 95, 183, 27, 131, 129, 182, 46, 85,
            ],
        );
    }

    #[test]
    fn classic_crop_matches_sampling_matrix_opencv_oracle_case() {
        // This high-variation case preserves the OpenCV 5.0.0 behavior where
        // warpPerspective creates a source-to-warp matrix, inverts it for
        // sampling, and evaluates the sampler's matrix in f32. A direct f64
        // inverse-coordinate path differs by one output byte for this case.
        const FIXTURE_ID: &str = "classic-v1-crop-oracle-sampling-matrix-bgr-12x11";
        assert!(
            CAPTURED_OPENCV_CROP_ORACLE.contains(FIXTURE_ID),
            "fixture record is missing {FIXTURE_ID}"
        );

        assert_captured_bgr_crop(
            FIXTURE_ID,
            dimensions(12, 11),
            &lcg_bgr_pixels(12, 11, 3_130_585_584),
            // The capture stores `float32` points. Keep the exact bits here
            // instead of rounding decimal source text to a different input.
            [
                (f32::from_bits(0x3fd6_221d), f32::from_bits(0x3f81_de3e)),
                (f32::from_bits(0x414e_9f30), f32::from_bits(0x3fd9_9bc8)),
                (f32::from_bits(0x4164_0ae6), f32::from_bits(0x4140_39fb)),
                (f32::from_bits(0x3fc7_15a3), f32::from_bits(0x4146_9e6b)),
            ],
            dimensions(12, 11),
            &[
                172, 43, 82, 245, 41, 209, 142, 99, 98, 211, 148, 159, 200, 59, 131, 194, 166, 73,
                244, 91, 112, 219, 44, 99, 112, 181, 98, 91, 162, 181, 221, 39, 155, 213, 62, 162,
                27, 192, 5, 141, 147, 63, 181, 148, 42, 202, 250, 174, 215, 165, 255, 223, 87, 88,
                215, 53, 191, 218, 90, 148, 102, 156, 53, 100, 181, 160, 255, 180, 225, 255, 183,
                213, 169, 173, 188, 101, 107, 229, 1, 26, 124, 65, 169, 168, 203, 226, 163, 232,
                255, 40, 56, 194, 141, 136, 57, 90, 131, 47, 1, 136, 143, 107, 243, 191, 220, 223,
                177, 201, 121, 127, 159, 82, 201, 113, 149, 155, 221, 32, 57, 91, 121, 18, 83, 200,
                190, 13, 25, 68, 77, 181, 85, 147, 153, 170, 121, 126, 154, 147, 182, 134, 180,
                177, 143, 173, 109, 140, 194, 188, 59, 56, 159, 19, 100, 72, 69, 70, 232, 3, 108,
                151, 119, 247, 32, 28, 64, 201, 46, 0, 121, 82, 162, 152, 199, 160, 201, 253, 114,
                188, 239, 117, 88, 30, 138, 95, 43, 68, 78, 208, 103, 113, 242, 134, 208, 99, 160,
                43, 164, 106, 148, 134, 85, 45, 77, 112, 4, 93, 90, 13, 103, 43, 13, 102, 29, 10,
                103, 32, 106, 227, 159, 191, 60, 152, 207, 117, 109, 211, 232, 132, 6, 190, 110,
                207, 167, 162, 144, 164, 28, 19, 84, 10, 152, 119, 237, 39, 220, 95, 13, 237, 57,
                18, 235, 63, 136, 108, 127, 206, 135, 48, 165, 103, 20, 131, 103, 31, 233, 65, 85,
                103, 172, 229, 34, 108, 37, 61, 182, 30, 216, 241, 250, 162, 190, 27, 158, 185, 10,
                159, 185, 13, 25, 149, 173, 173, 213, 49, 53, 76, 31, 3, 242, 210, 8, 116, 202,
                121, 40, 160, 252, 23, 176, 186, 98, 110, 5, 92, 225, 175, 82, 121, 172, 82, 122,
                172, 82, 122, 215, 165, 163, 255, 232, 98, 92, 167, 15, 172, 128, 36, 185, 86, 88,
                179, 42, 34, 144, 233, 225, 203, 103, 130, 34, 212, 15, 69, 117, 222, 68, 123, 207,
                69, 122, 207, 203, 164, 162, 255, 231, 95, 88, 161, 16, 169, 131, 45, 179, 84, 91,
                175, 51, 45, 152, 225, 226, 191, 110, 114, 32, 207, 29, 68, 115, 230, 66, 124, 209,
                66, 124, 209,
            ],
        );
    }

    #[test]
    fn classic_crop_matches_perspective_lu_opencv_oracle_case() {
        // This high-variation case preserves OpenCV 5.0.0
        // getPerspectiveTransform behavior: float32 coefficient products,
        // its default LU solve, and the 3-by-3 inverse used by
        // warpPerspective. A generic f64 homography solve differs by one
        // output byte for this case.
        const FIXTURE_ID: &str = "classic-v1-crop-oracle-perspective-lu-bgr-12x13";
        assert!(
            CAPTURED_OPENCV_CROP_ORACLE.contains(FIXTURE_ID),
            "fixture record is missing {FIXTURE_ID}"
        );

        assert_captured_bgr_crop(
            FIXTURE_ID,
            dimensions(12, 13),
            &lcg_bgr_pixels(12, 13, 384_875_819),
            // The capture stores `float32` points. Keep the exact bits here
            // instead of rounding decimal source text to a different input.
            [
                (f32::from_bits(0x3ffa_3bf6), f32::from_bits(0xbfda_fd60)),
                (f32::from_bits(0x414f_a120), f32::from_bits(0x3e03_844a)),
                (f32::from_bits(0x412b_05ee), f32::from_bits(0x4141_0286)),
                (f32::from_bits(0xbf13_2c69), f32::from_bits(0x416b_f129)),
            ],
            dimensions(11, 16),
            &[
                44, 33, 35, 158, 133, 143, 155, 204, 187, 218, 139, 166, 79, 177, 228, 6, 50, 63,
                201, 41, 52, 91, 21, 5, 47, 124, 115, 63, 187, 183, 65, 176, 156, 51, 24, 61, 157,
                97, 131, 152, 226, 191, 216, 143, 170, 107, 175, 221, 28, 84, 120, 132, 105, 48,
                92, 121, 37, 88, 153, 69, 109, 205, 41, 110, 202, 28, 161, 102, 105, 149, 107, 65,
                177, 222, 225, 140, 61, 99, 157, 108, 110, 119, 145, 217, 109, 174, 120, 113, 237,
                81, 142, 207, 90, 24, 213, 0, 22, 212, 0, 234, 95, 142, 176, 124, 133, 32, 218,
                222, 47, 150, 66, 53, 186, 176, 88, 108, 77, 161, 47, 166, 218, 136, 134, 189, 214,
                144, 71, 175, 116, 89, 168, 134, 76, 0, 95, 81, 112, 73, 175, 178, 121, 33, 255,
                181, 123, 192, 118, 18, 195, 22, 181, 94, 26, 125, 42, 86, 119, 192, 168, 215, 122,
                237, 232, 103, 246, 127, 124, 40, 104, 69, 103, 97, 252, 70, 189, 156, 116, 236,
                36, 119, 150, 143, 106, 177, 67, 115, 168, 173, 22, 62, 159, 148, 30, 109, 100, 24,
                93, 56, 48, 36, 85, 184, 170, 127, 110, 89, 46, 159, 74, 142, 171, 154, 107, 173,
                241, 53, 76, 55, 127, 226, 151, 76, 202, 87, 155, 82, 140, 85, 19, 218, 0, 210,
                192, 171, 84, 78, 118, 149, 45, 2, 97, 127, 114, 23, 189, 121, 18, 114, 97, 110,
                180, 183, 110, 155, 95, 116, 54, 80, 101, 88, 79, 64, 202, 27, 79, 107, 90, 103,
                81, 46, 227, 143, 105, 205, 176, 118, 114, 248, 108, 95, 51, 163, 151, 130, 235,
                104, 188, 133, 24, 74, 74, 52, 75, 97, 66, 140, 59, 132, 178, 179, 200, 142, 18,
                199, 148, 228, 225, 70, 164, 217, 184, 75, 255, 135, 83, 176, 30, 188, 92, 116,
                162, 136, 127, 161, 91, 102, 180, 56, 188, 72, 176, 74, 46, 240, 202, 142, 67, 46,
                180, 224, 81, 179, 124, 64, 50, 177, 92, 22, 153, 31, 108, 34, 83, 175, 52, 233,
                134, 23, 62, 37, 126, 165, 144, 167, 167, 166, 186, 214, 171, 102, 82, 133, 162,
                142, 99, 104, 211, 83, 91, 117, 82, 118, 120, 230, 203, 32, 136, 66, 78, 132, 153,
                112, 102, 220, 162, 173, 164, 162, 94, 23, 97, 118, 197, 134, 178, 73, 188, 79, 62,
                166, 79, 145, 218, 22, 66, 123, 95, 101, 174, 154, 130, 73, 169, 203, 143, 157,
                232, 162, 154, 73, 103, 207, 51, 69, 74, 106, 113, 194, 213, 100, 195, 178, 154,
                240, 232, 69, 77, 170, 90, 51, 52, 234, 94, 153, 164, 130, 149, 144, 149, 91, 125,
                81, 50, 87, 250, 106, 147, 35, 80, 66, 213, 118, 36, 96, 225, 86, 208, 128, 85,
                191, 109, 107, 73, 159, 162, 55, 159, 123, 13, 108, 57, 98, 41, 103, 202, 53, 89,
                245, 92, 138, 38, 98, 72, 222, 95, 48, 104, 234, 58, 175, 85, 120, 235, 0, 174, 97,
                152, 99, 54, 184, 20, 116, 68, 36, 143, 79, 87, 208,
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
