// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Document-level orientation classification.
//!
//! Roadmap item `DOCORI-001`, the half that decides a whole page's rotation
//! rather than a single line's. `docs/ORIENTATION_CONTRACT.md` records why this
//! is a different capability from text-line orientation: a different model, four
//! classes instead of two, a preprocessing shape this project had never
//! implemented, and — the part that matters downstream — geometry consequences,
//! because rotating a page moves every coordinate on it.
//!
//! # The preprocessing is new
//!
//! `PP-LCNet_x1_0_doc_ori`'s `inference.yml` declares `ResizeImage.resize_short
//! 256` followed by `CropImage.size 224`. Neither existed here: the classic path
//! only ever resizes to a computed size, and never takes a sub-window.
//!
//! Both are transcribed from the C++ deployment source, because the config names
//! the operators and the source defines them:
//!
//! - `ResizeByShort` scales by `target / min(h, w)` and rounds each dimension
//!   with `round`, **half away from zero**, not truncation;
//! - `Crop` in `Center` mode takes `x1 = max(0, (w - crop) / 2)` with **integer
//!   division**, and errors rather than padding when the image is too small.
//!
//! Both roundings are load-bearing. A half-pixel difference in the resize, or a
//! rounding rather than a truncation in the crop origin, moves the window by a
//! whole pixel and changes every value in the tensor.
//!
//! # Why nothing calls this yet
//!
//! Deciding a page's angle and *acting* on it are separate problems, and only
//! the first is solved here. Acting on it means rotating the page before
//! detection and then mapping every returned coordinate back through the inverse
//! transform, which is the "inverse geometry semantics" `DOCORI-001` asks for
//! and which is not implemented.
//!
//! The alternative — wiring this in and returning coordinates in the *rotated*
//! page's space — would be worse than not wiring it in at all, because the
//! polygons would be internally consistent and silently wrong against the image
//! the caller supplied. So this module is complete, verified against a captured
//! oracle, and deliberately unreachable until the geometry exists.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::resize::classic_linear_resize;
use crate::tensor::{NchwTensor, classic_normalized_batch};
use crate::types::ImageDimensions;

/// The shorter side's target length, from the artifact config.
pub(crate) const DOCUMENT_RESIZE_SHORT: u32 = 256;

/// The centre crop's side, from the artifact config.
pub(crate) const DOCUMENT_CROP_SIZE: u32 = 224;

/// The artifact's label list, in class order.
pub(crate) const DOCUMENT_LABELS: [&str; 4] = ["0", "90", "180", "270"];

/// Returns the dimensions `ResizeByShort` produces for a source image.
///
/// Separated from the resize so the rounding can be tested on its own, which is
/// where it is wrong or right.
pub(crate) fn resize_by_short_dimensions(
    source: ImageDimensions,
    target_short_edge: u32,
) -> Result<ImageDimensions> {
    let (width, height) = (f64::from(source.width()), f64::from(source.height()));
    let shorter = width.min(height);
    if shorter <= 0.0 || target_short_edge == 0 {
        return Err(Error::InvalidInput {
            field: "document_orientation.resize_short",
            violation: InputViolation::Empty,
        });
    }
    let scale = f64::from(target_short_edge) / shorter;
    // `static_cast<int>(std::round(x))` rounds half away from zero. Rust's
    // `f64::round` does the same, unlike Python's banker's rounding, so this is
    // the one place the obvious call is also the correct one.
    let scaled_width = (width * scale).round();
    let scaled_height = (height * scale).round();
    if !scaled_width.is_finite()
        || !scaled_height.is_finite()
        || scaled_width < 1.0
        || scaled_height < 1.0
        || scaled_width > f64::from(u32::MAX)
        || scaled_height > f64::from(u32::MAX)
    {
        return Err(Error::InvalidInput {
            field: "document_orientation.resize_short",
            violation: InputViolation::OutOfRange,
        });
    }
    ImageDimensions::new(scaled_width as u32, scaled_height as u32)
}

/// Takes a centre crop, refusing rather than padding when the source is small.
///
/// Upstream errors in that case, and padding instead would hand the model a
/// border the training data never had.
pub(crate) fn centre_crop(source: &InterleavedImage, size: u32) -> Result<InterleavedImage> {
    let dimensions = source.dimensions();
    let (width, height) = (dimensions.width(), dimensions.height());
    if width < size || height < size {
        return Err(Error::InvalidInput {
            field: "document_orientation.crop_size",
            violation: InputViolation::OutOfRange,
        });
    }
    // Integer division, matching `(w - crop_width) / 2` in C++. Rounding here
    // would shift the window by a pixel on every odd difference.
    let x1 = (width - size) / 2;
    let y1 = (height - size) / 2;

    let channels = source.channels() as usize;
    let pixels = source.pixels();
    let mut cropped = Vec::with_capacity((size * size) as usize * channels);
    for row in 0..size {
        let start = (((y1 + row) * width + x1) as usize) * channels;
        let end = start + size as usize * channels;
        cropped.extend_from_slice(&pixels[start..end]);
    }
    InterleavedImage::new(
        ImageDimensions::new(size, size)?,
        source.channels(),
        cropped,
    )
}

/// Runs the artifact's declared preprocessing over one page.
pub(crate) fn document_orientation_input(page: &InterleavedImage) -> Result<NchwTensor> {
    let resized_dimensions = resize_by_short_dimensions(page.dimensions(), DOCUMENT_RESIZE_SHORT)?;
    let resized = classic_linear_resize(page, resized_dimensions)?;
    let cropped = centre_crop(&resized, DOCUMENT_CROP_SIZE)?;
    classic_normalized_batch(&[&cropped])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        }
    }

    /// The shorter side lands exactly on the target; the longer one scales.
    #[test]
    fn the_shorter_side_reaches_the_target() {
        let landscape = match resize_by_short_dimensions(dimensions(400, 300), 256) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        // 256/300 * 400 = 341.33 -> 341, and the shorter side is exactly 256.
        assert_eq!((landscape.width(), landscape.height()), (341, 256));

        let portrait = match resize_by_short_dimensions(dimensions(300, 400), 256) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!((portrait.width(), portrait.height()), (256, 341));

        let square = match resize_by_short_dimensions(dimensions(256, 256), 256) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!((square.width(), square.height()), (256, 256));
    }

    /// Rounding is half away from zero, not truncation.
    ///
    /// `513 x 371` scales by `256/371`; the width becomes `354.06...` and the
    /// height exactly `256`. A truncating implementation agrees here, so the
    /// case that separates them is checked directly below.
    #[test]
    fn the_scaled_dimension_rounds_rather_than_truncating() {
        let scaled = match resize_by_short_dimensions(dimensions(513, 371), 256) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!((scaled.width(), scaled.height()), (354, 256));

        // 200/150 * 100 = 133.33 -> 133 either way; 175/150 * 100 = 116.67,
        // which rounds to 117 and truncates to 116.
        let rounds_up = match resize_by_short_dimensions(dimensions(175, 150), 100) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(
            rounds_up.width(),
            117,
            "116.67 must round up, not truncate to 116"
        );
    }

    fn page(width: u32, height: u32) -> InterleavedImage {
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                for channel in 0..3_usize {
                    pixels.push(((x * 7 + y * 13 + channel * 29) % 256) as u8);
                }
            }
        }
        match InterleavedImage::new(dimensions(width, height), 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("page: {error}"),
        }
    }

    /// The crop origin uses integer division, which matters on odd differences.
    #[test]
    fn the_crop_origin_truncates() {
        // 11 - 4 = 7, and 7 / 2 is 3 rather than 3.5 rounded to 4.
        let source = page(11, 11);
        let cropped = match centre_crop(&source, 4) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(cropped.dimensions().width(), 4);
        assert_eq!(cropped.dimensions().height(), 4);

        // The first cropped pixel must be the source pixel at (3, 3).
        let expected = {
            let start = ((3 * 11 + 3) * 3) as usize;
            &source.pixels()[start..start + 3]
        };
        assert_eq!(&cropped.pixels()[..3], expected);
    }

    #[test]
    fn a_source_smaller_than_the_crop_is_refused_rather_than_padded() {
        let source = page(100, 300);
        assert!(matches!(
            centre_crop(&source, 224),
            Err(Error::InvalidInput {
                field: "document_orientation.crop_size",
                ..
            })
        ));
    }

    #[test]
    fn the_pipeline_produces_the_declared_input_shape() {
        let source = page(400, 300);
        let tensor = match document_orientation_input(&source) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(tensor.shape(), [1, 3, 224, 224]);
    }

    #[test]
    fn there_are_four_labels_not_two() {
        assert_eq!(DOCUMENT_LABELS.len(), 4);
        assert_eq!(DOCUMENT_LABELS[1], "90");
        // The text-line model's labels carry a `_degree` suffix and these do
        // not. Two models in one pipeline with different label conventions.
        assert!(!DOCUMENT_LABELS[1].contains('_'));
    }
}

/// Comparison against the captured document-orientation oracle.
#[cfg(test)]
mod oracle {
    use super::*;

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-document-orientation/expected.json");
    const BENCHMARK_PAGE: &[u8] =
        include_bytes!("../tests/fixtures/classic-v1-benchmark-page/input.png");

    fn decode_f32(encoded: &str) -> Vec<f32> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let bytes = match STANDARD.decode(encoded) {
            Ok(bytes) => bytes,
            Err(error) => panic!("base64: {error}"),
        };
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn synthetic_page(index: usize, width: u32, height: u32) -> InterleavedImage {
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                for channel in 0..3_usize {
                    pixels.push(((x * 7 + y * 13 + channel * 29 + index * 31) % 256) as u8);
                }
            }
        }
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("page: {error}"),
        }
    }

    /// Rotates a page by a right angle, matching the `cv2.rotate` codes the
    /// capture used.
    fn rotate_right_angle(page: &InterleavedImage, degrees: u32) -> InterleavedImage {
        let dimensions = page.dimensions();
        let (width, height) = (dimensions.width() as usize, dimensions.height() as usize);
        let channels = page.channels() as usize;
        let pixels = page.pixels();
        let (new_width, new_height) = match degrees {
            90 | 270 => (height, width),
            _ => (width, height),
        };
        let mut rotated = vec![0_u8; pixels.len()];
        for y in 0..height {
            for x in 0..width {
                let (target_x, target_y) = match degrees {
                    // Clockwise: the top-left goes to the top-right.
                    90 => (height - 1 - y, x),
                    180 => (width - 1 - x, height - 1 - y),
                    270 => (y, width - 1 - x),
                    _ => (x, y),
                };
                let source = (y * width + x) * channels;
                let target = (target_y * new_width + target_x) * channels;
                rotated[target..target + channels]
                    .copy_from_slice(&pixels[source..source + channels]);
            }
        }
        let dimensions = match ImageDimensions::new(new_width as u32, new_height as u32) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, page.channels(), rotated) {
            Ok(value) => value,
            Err(error) => panic!("rotated page: {error}"),
        }
    }

    fn source_for(case: &str) -> InterleavedImage {
        if let Some(rest) = case.strip_prefix("benchmark-page") {
            let encoded = match crate::types::EncodedImage::new(BENCHMARK_PAGE) {
                Ok(value) => value,
                Err(error) => panic!("encoded: {error}"),
            };
            let page = match crate::image::decode_classic_bgr(encoded) {
                Ok(value) => value,
                Err(error) => panic!("decode: {error}"),
            };
            return match rest {
                "" => page,
                "-90" => rotate_right_angle(&page, 90),
                "-180" => rotate_right_angle(&page, 180),
                "-270" => rotate_right_angle(&page, 270),
                other => panic!("unknown rotation {other}"),
            };
        }
        let index: usize = match case.trim_start_matches("synthetic-").parse() {
            Ok(value) => value,
            Err(error) => panic!("case {case}: {error}"),
        };
        let sizes = [(400_u32, 300_u32), (300, 400), (256, 256), (513, 371)];
        let (width, height) = sizes[index];
        synthetic_page(index, width, height)
    }

    /// The `resize_short` plus centre-crop path must reproduce the capture.
    ///
    /// Two roundings decide every value here — the resize's round-half-away and
    /// the crop origin's integer division — so a bit-identical match is the only
    /// evidence that both are right. The right-angle rotations double as a check
    /// that this port's rotation agrees with `cv2.rotate`, since each rotated
    /// source's digest is the one the capture recorded from OpenCV.
    #[test]
    fn the_captured_document_tensors_are_reproduced() {
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        };
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        assert_eq!(records.len(), 8);

        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let source = source_for(case);
            assert_eq!(
                sha256_hex(source.pixels()),
                record["source_bgr_sha256"].as_str().unwrap_or_default(),
                "{case}: source BGR bytes differ from the capture"
            );

            let resized_dimensions =
                match resize_by_short_dimensions(source.dimensions(), DOCUMENT_RESIZE_SHORT) {
                    Ok(value) => value,
                    Err(error) => panic!("{case} resize plan: {error}"),
                };
            let expected_resized: Vec<usize> = match record["resized_hwc_shape"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no resized shape"),
            };
            assert_eq!(
                (
                    resized_dimensions.height() as usize,
                    resized_dimensions.width() as usize
                ),
                (expected_resized[0], expected_resized[1]),
                "{case}: resize-by-short dimensions"
            );

            let tensor = match document_orientation_input(&source) {
                Ok(value) => value,
                Err(error) => panic!("{case} tensor: {error}"),
            };
            let mut bytes = Vec::with_capacity(tensor.values().len() * 4);
            for value in tensor.values() {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            assert_eq!(
                sha256_hex(&bytes),
                record["input_values_sha256"].as_str().unwrap_or_default(),
                "{case}: input tensor differs from the captured upstream bytes"
            );

            let indices: Vec<usize> = match record["input_sample_indices"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no sample indices"),
            };
            let samples = decode_f32(
                record["input_sample_values_base64"]
                    .as_str()
                    .unwrap_or_default(),
            );
            for (index, expected) in indices.iter().zip(&samples) {
                assert_eq!(
                    tensor.values()[*index].to_bits(),
                    expected.to_bits(),
                    "{case}: element {index}"
                );
            }

            // The recorded verdict must be the argmax of the recorded output,
            // which pins the label convention as well as the arithmetic.
            let scores = decode_f32(record["output_values_base64"].as_str().unwrap_or_default());
            let mut best = 0_usize;
            for (index, value) in scores.iter().enumerate() {
                if *value > scores[best] {
                    best = index;
                }
                let _ = index;
            }
            assert_eq!(
                DOCUMENT_LABELS[best],
                record["label"].as_str().unwrap_or_default(),
                "{case}: label"
            );
        }
    }

    /// All four rotations of one page are identified correctly.
    ///
    /// This is the property, not the numbers: a model that answered `0` for
    /// everything would match neither, and a rotation implementation that was
    /// wrong in one direction would show up as a swapped pair.
    #[test]
    fn every_right_angle_is_identified() {
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        };
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        for (case, expected) in [
            ("benchmark-page", "0"),
            ("benchmark-page-90", "90"),
            ("benchmark-page-180", "180"),
            ("benchmark-page-270", "270"),
        ] {
            let record = match records.iter().find(|entry| entry["case"] == case) {
                Some(record) => record,
                None => panic!("no record for {case}"),
            };
            assert_eq!(
                record["label"].as_str().unwrap_or_default(),
                expected,
                "{case}"
            );
            assert!(
                record["score"].as_f64().unwrap_or_default() > 0.9,
                "{case}: the model should be confident"
            );
        }
    }
}
