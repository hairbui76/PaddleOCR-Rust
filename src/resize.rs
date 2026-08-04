// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded `INTER_LINEAR` resize for interleaved `uint8` images.
//!
//! The classic OCR path reaches `cv2.resize(..., INTER_LINEAR)` from both
//! `ppocr/data/imaug/rec_img_aug.py:resize_norm_img` and
//! `ppocr/data/imaug/operators.py:DetResizeForTest`, so the byte-level result
//! is part of the observable contract rather than an implementation detail.
//!
//! OpenCV does not evaluate the linear kernel in floating point for 8-bit
//! images. It derives `float` coefficients, quantizes them to `i16` at
//! `1 << 11`, accumulates the horizontal pass in `i32`, and folds the vertical
//! pass with a fixed shift. This module reproduces that arithmetic exactly,
//! including its coefficient rounding and edge clamping, because a
//! mathematically equivalent floating-point formulation produces different
//! bytes.
//!
//! This module resizes only. It selects no colour convention, no model, and no
//! resize *policy*: which target size to use is `DET-001` and `REC-001` work.

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::types::ImageDimensions;

/// Fractional bits used by OpenCV's fixed-point resize coefficients.
const COEFFICIENT_BITS: u32 = 11;

/// The fixed-point scale, `INTER_RESIZE_COEF_SCALE` in OpenCV.
const COEFFICIENT_SCALE: f32 = (1 << COEFFICIENT_BITS) as f32;

/// One horizontal tap: the left source column and both quantized weights.
#[derive(Clone, Copy, Debug)]
struct LinearTap {
    lower: usize,
    upper: usize,
    lower_weight: i32,
    upper_weight: i32,
}

/// Resizes one interleaved image with OpenCV's `INTER_LINEAR` semantics.
///
/// The channel count is preserved. Channels are treated as opaque interleaved
/// samples: this function assigns them no colour meaning.
pub(crate) fn classic_linear_resize(
    source: &InterleavedImage,
    target: ImageDimensions,
) -> Result<InterleavedImage> {
    let channels = usize::from(source.channels());
    let source_dimensions = source.dimensions();
    let source_width = source_dimensions.width() as usize;
    let source_height = source_dimensions.height() as usize;
    let target_width = target.width() as usize;
    let target_height = target.height() as usize;

    let output_len = target
        .pixels()
        .checked_mul(source.channels().into())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(Error::InvalidInput {
            field: "resize.target",
            violation: InputViolation::OutOfRange,
        })?;

    let horizontal = horizontal_taps(source_width, target_width);
    let vertical = vertical_taps(source_height, target_height);

    // The horizontal pass is evaluated per needed source row and cached for the
    // two rows the vertical pass folds, so only two intermediate rows exist at
    // a time regardless of image height.
    let row_len = target_width * channels;
    let mut lower_row = vec![0_i32; row_len];
    let mut upper_row = vec![0_i32; row_len];
    let mut cached: Option<(usize, usize)> = None;

    let mut output = Vec::new();
    if output.try_reserve_exact(output_len).is_err() {
        return Err(Error::ResourceLimit {
            resource: "resize.output_bytes",
            limit: output_len as u64,
            actual: output_len as u64,
        });
    }
    output.resize(output_len, 0);

    for (row_index, tap) in vertical.iter().enumerate() {
        let wanted = (tap.lower, tap.upper);
        if cached != Some(wanted) {
            horizontal_pass(
                source,
                channels,
                source_width,
                tap.lower,
                &horizontal,
                &mut lower_row,
            );
            horizontal_pass(
                source,
                channels,
                source_width,
                tap.upper,
                &horizontal,
                &mut upper_row,
            );
            cached = Some(wanted);
        }

        let destination = row_index * row_len;
        for column in 0..row_len {
            output[destination + column] =
                fold_vertical(lower_row[column], upper_row[column], *tap);
        }
    }

    InterleavedImage::new(target, source.channels(), output)
}

/// Builds the per-column taps exactly as OpenCV's coefficient loop does.
fn horizontal_taps(source_width: usize, target_width: usize) -> Vec<LinearTap> {
    (0..target_width)
        .map(|column| {
            let (index, fraction) = sample_position(column, source_width, target_width);
            // OpenCV clamps a destination sample that falls outside the first or
            // last source centre onto that centre and zeroes its fraction, so
            // the second tap contributes nothing there.
            let (index, fraction) = clamp_tap(index, fraction, source_width);
            let upper = (index + 1).min(source_width - 1);
            LinearTap {
                lower: index,
                upper,
                lower_weight: quantize(1.0 - fraction),
                upper_weight: quantize(fraction),
            }
        })
        .collect()
}

/// Builds the per-row taps.
///
/// OpenCV does not zero the vertical fraction at the edges. It clips the row
/// indices instead, which makes both taps read the same row and leaves the
/// weighted sum unchanged.
fn vertical_taps(source_height: usize, target_height: usize) -> Vec<LinearTap> {
    (0..target_height)
        .map(|row| {
            let (index, fraction) = sample_position(row, source_height, target_height);
            let lower = index.clamp(0, source_height as isize - 1) as usize;
            let upper = (index + 1).clamp(0, source_height as isize - 1) as usize;
            LinearTap {
                lower,
                upper,
                lower_weight: quantize(1.0 - fraction),
                upper_weight: quantize(fraction),
            }
        })
        .collect()
}

/// Returns OpenCV's source index and fraction for one destination coordinate.
///
/// The `f32` narrowing is deliberate: OpenCV computes the scale in `double`
/// and then stores the offset in a `float` before flooring it.
fn sample_position(destination: usize, source_len: usize, target_len: usize) -> (isize, f32) {
    let scale = source_len as f64 / target_len as f64;
    let position = ((destination as f64 + 0.5) * scale - 0.5) as f32;
    let index = position.floor();
    (index as isize, position - index)
}

/// Applies OpenCV's out-of-range clamp for the horizontal coefficient loop.
fn clamp_tap(index: isize, fraction: f32, source_len: usize) -> (usize, f32) {
    if index < 0 {
        return (0, 0.0);
    }
    if index >= source_len as isize - 1 {
        return (source_len - 1, 0.0);
    }
    (index as usize, fraction)
}

/// Quantizes one coefficient to OpenCV's `i16` fixed-point representation.
fn quantize(weight: f32) -> i32 {
    // `saturate_cast<short>` rounds to nearest with ties to even.
    let scaled = weight * COEFFICIENT_SCALE;
    let rounded = round_half_to_even(scaled);
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i32
}

/// Rounds one finite `f32` half-way value to the nearest even integer.
fn round_half_to_even(value: f32) -> f32 {
    let nearest = value.round();
    if (value - value.trunc()).abs() == 0.5 && nearest % 2.0 != 0.0 {
        nearest - value.signum()
    } else {
        nearest
    }
}

/// Evaluates the horizontal pass for one source row into `destination`.
fn horizontal_pass(
    source: &InterleavedImage,
    channels: usize,
    source_width: usize,
    row: usize,
    taps: &[LinearTap],
    destination: &mut [i32],
) {
    let pixels = source.pixels();
    let row_start = row * source_width * channels;
    for (column, tap) in taps.iter().enumerate() {
        let lower = row_start + tap.lower * channels;
        let upper = row_start + tap.upper * channels;
        let output = column * channels;
        for channel in 0..channels {
            let left = i32::from(pixels[lower + channel]);
            let right = i32::from(pixels[upper + channel]);
            destination[output + channel] = left * tap.lower_weight + right * tap.upper_weight;
        }
    }
}

/// Folds the two horizontal rows exactly as OpenCV's 8-bit vertical pass does.
///
/// The `>> 4` then `>> 16` then `+ 2` then `>> 2` sequence is OpenCV's, not an
/// equivalent rearrangement: it discards low bits before the multiply, so a
/// single combined shift produces different bytes.
fn fold_vertical(lower: i32, upper: i32, tap: LinearTap) -> u8 {
    let folded =
        (((tap.lower_weight * (lower >> 4)) >> 16) + ((tap.upper_weight * (upper >> 4)) >> 16) + 2)
            >> 2;
    folded.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;

    const CAPTURED_OPENCV_RESIZE_GRID: &str =
        include_str!("../tests/fixtures/classic-v1-resize-linear-grid/capture.json");

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        }
    }

    fn image(width: u32, height: u32, channels: u8, pixels: Vec<u8>) -> InterleavedImage {
        match InterleavedImage::new(dimensions(width, height), channels, pixels) {
            Ok(value) => value,
            Err(error) => panic!("expected valid image, got {error}"),
        }
    }

    fn string<'a>(value: &'a Value, field: &str) -> &'a str {
        match value.get(field).and_then(Value::as_str) {
            Some(value) => value,
            None => panic!("resize capture field {field:?} must be a string"),
        }
    }

    fn u32_field(value: &Value, field: &str) -> u32 {
        match value.get(field).and_then(Value::as_u64) {
            Some(value) => match u32::try_from(value) {
                Ok(value) => value,
                Err(_) => panic!("resize capture field {field:?} does not fit u32"),
            },
            None => panic!("resize capture field {field:?} must be an unsigned integer"),
        }
    }

    fn payload(case: &Value, role: &str) -> (Vec<u8>, [u32; 3]) {
        let value = match case.get(role) {
            Some(value) => value,
            None => panic!("resize capture case is missing {role:?}"),
        };
        assert_eq!(
            string(value, "channel_order"),
            "BGR",
            "{role} channel order"
        );
        assert_eq!(string(value, "dtype"), "uint8", "{role} dtype");
        let shape = match value.get("shape").and_then(Value::as_array) {
            Some(shape) => shape,
            None => panic!("{role} shape must be an array"),
        };
        assert_eq!(shape.len(), 3, "{role} shape rank");
        let mut extents = [0_u32; 3];
        for (index, axis) in shape.iter().enumerate() {
            extents[index] = match axis.as_u64().and_then(|axis| u32::try_from(axis).ok()) {
                Some(axis) => axis,
                None => panic!("{role} shape axis {index} must be a u32"),
            };
        }
        let bytes = match STANDARD.decode(string(value, "base64")) {
            Ok(bytes) => bytes,
            Err(error) => panic!("{role} base64 is invalid: {error}"),
        };
        (bytes, extents)
    }

    /// Executes every captured OpenCV case against the Rust implementation.
    #[test]
    fn classic_linear_resize_executes_every_captured_opencv_case() {
        let capture: Value = match serde_json::from_str(CAPTURED_OPENCV_RESIZE_GRID) {
            Ok(value) => value,
            Err(error) => panic!("resize capture is not valid JSON: {error}"),
        };
        assert_eq!(
            string(&capture, "schema_version"),
            "paddleocr-rust/resize-oracle/v1"
        );
        let environment = match capture.get("environment") {
            Some(value) => value,
            None => panic!("resize capture is missing its environment"),
        };
        assert_eq!(
            environment.get("opencv_optimized").and_then(Value::as_bool),
            Some(false),
            "the resize capture must record disabled OpenCV optimized paths"
        );

        let cases = match capture.get("cases").and_then(Value::as_array) {
            Some(cases) => cases,
            None => panic!("resize capture must contain cases"),
        };
        assert_eq!(cases.len(), 34, "captured resize case count");

        for case in cases {
            let fixture_id = string(case, "fixture_id");
            let (source_bytes, source_shape) = payload(case, "input");
            let (expected_bytes, expected_shape) = payload(case, "output");
            let target = match case.get("target_size") {
                Some(value) => value,
                None => panic!("{fixture_id} is missing target_size"),
            };
            let target_width = u32_field(target, "width");
            let target_height = u32_field(target, "height");
            assert_eq!(expected_shape[0], target_height, "{fixture_id} height");
            assert_eq!(expected_shape[1], target_width, "{fixture_id} width");
            assert_eq!(expected_shape[2], 3, "{fixture_id} channels");

            let source = image(source_shape[1], source_shape[0], 3, source_bytes);
            let resized =
                match classic_linear_resize(&source, dimensions(target_width, target_height)) {
                    Ok(resized) => resized,
                    Err(error) => panic!("{fixture_id} failed to resize: {error}"),
                };
            assert_eq!(resized.dimensions().width(), target_width, "{fixture_id}");
            assert_eq!(resized.dimensions().height(), target_height, "{fixture_id}");
            assert_eq!(resized.channels(), 3, "{fixture_id}");
            assert_eq!(resized.pixels(), expected_bytes, "{fixture_id} bytes");
        }
    }

    #[test]
    fn classic_linear_resize_preserves_identity_pixels() {
        let source = image(3, 2, 3, (0..18).collect());
        let resized = match classic_linear_resize(&source, dimensions(3, 2)) {
            Ok(resized) => resized,
            Err(error) => panic!("expected an identity resize, got {error}"),
        };
        assert_eq!(resized.pixels(), source.pixels());
    }

    #[test]
    fn classic_linear_resize_preserves_the_channel_count() {
        for channels in 1..=4_u8 {
            let source = image(
                2,
                2,
                channels,
                (0..4 * u32::from(channels))
                    .map(|value| value as u8)
                    .collect(),
            );
            let resized = match classic_linear_resize(&source, dimensions(3, 3)) {
                Ok(resized) => resized,
                Err(error) => panic!("expected a resize for {channels} channels, got {error}"),
            };
            assert_eq!(resized.channels(), channels);
            assert_eq!(resized.pixels().len(), 9 * usize::from(channels));
        }
    }

    #[test]
    fn coefficients_round_half_to_even_like_saturate_cast() {
        assert_eq!(round_half_to_even(0.5), 0.0);
        assert_eq!(round_half_to_even(1.5), 2.0);
        assert_eq!(round_half_to_even(2.5), 2.0);
        assert_eq!(round_half_to_even(-0.5), -0.0);
        assert_eq!(round_half_to_even(-1.5), -2.0);
        assert_eq!(round_half_to_even(3.25), 3.0);
        assert_eq!(quantize(1.0), 2048);
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(0.5), 1024);
    }

    #[test]
    fn horizontal_taps_clamp_outside_the_first_and_last_source_centre() {
        // Upscaling puts the first destination centre before the first source
        // centre and the last one after the last source centre.
        let taps = horizontal_taps(2, 5);
        assert_eq!(taps[0].lower, 0);
        assert_eq!(taps[0].upper_weight, 0);
        let last = taps[taps.len() - 1];
        assert_eq!(last.lower, 1);
        assert_eq!(last.upper, 1);
        assert_eq!(last.upper_weight, 0);
    }

    #[test]
    fn vertical_taps_clip_row_indices_without_zeroing_their_fraction() {
        let taps = vertical_taps(2, 5);
        assert_eq!(taps[0].lower, 0);
        assert_eq!(taps[0].upper, 0);
        let last = taps[taps.len() - 1];
        assert_eq!(last.lower, 1);
        assert_eq!(last.upper, 1);
    }
}
