// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Document unwarping.
//!
//! Roadmap item `UNWARP-001`, implementing the contract frozen in
//! `docs/UNWARPING_CONTRACT.md` for `UVDoc`, the model
//! `deploy/cpp_infer/src/configs/OCR.yaml` names under `DocUnwarping`.
//!
//! # The bound comes first
//!
//! Every other model in this project has a fixed or derived input shape, and
//! its resource bound follows from that. This one has neither: upstream applies
//! **no resize**, so a caller's page size flows straight into the tensor, and
//! the model returns an output of the same size. Two `f32` tensors of `H x W x 3`
//! are `24` bytes per source pixel.
//!
//! The bound is therefore stated before anything else, and it reuses the decode
//! envelope's precedent rather than inventing a number: at most `256 MiB` across
//! the input and output tensors together, which caps the page at roughly
//! `11.1` megapixels. That is checked from the declared dimensions, before a
//! tensor is allocated.
//!
//! # A third normalization
//!
//! `x / 255` with mean `0` and standard deviation `1`. The detector and both
//! orientation classifiers use ImageNet constants; the recognizer uses
//! `(x / 255 - 0.5) / 0.5`. Three conventions across five models, and choosing
//! the wrong one here yields a plausible image rather than an error.
//!
//! # Unwarping has no inverse
//!
//! The model returns pixels, not a transform. Text detected on an unwarped page
//! has coordinates **in the unwarped page**, and nothing here can map them back
//! to the caller's photograph. `unwarp` therefore returns the new image and
//! nothing that pretends otherwise; deciding what a caller is told about those
//! coordinates belongs to `DOCPIPE-001`.
//!
//! # Why nothing calls this yet
//!
//! For the same reason as `document_orientation`, and more sharply. Exposing
//! unwarping means returning polygons in an image the caller never supplied,
//! with no inverse available to fix that. Wiring it in before deciding what a
//! caller is told would ship coordinates that look like they belong to the
//! input. So the module is complete and verified against a capture, and
//! deliberately unreachable until `DOCPIPE-001` answers that question.
#![allow(dead_code)]

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, run_validated};
use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// Bytes permitted across the input and output tensors together.
///
/// The same `256 MiB` the image decoder budgets for its two buffers, chosen for
/// the same reason: it is the point past which a single page stops being a page.
pub(crate) const MAX_UNWARP_TENSOR_BYTES: u64 = 256 * 1024 * 1024;

/// Bytes one source pixel costs across both tensors: `3` channels, `f32`, twice.
const BYTES_PER_PIXEL: u64 = 3 * 4 * 2;

/// The scale upstream's `Normalize(1/255, 0, 1)` applies.
const UNWARP_SCALE: f32 = 1.0_f32 / 255.0_f32;

/// Returns the largest page this bound admits, in pixels.
pub(crate) const fn max_unwarp_pixels() -> u64 {
    MAX_UNWARP_TENSOR_BYTES / BYTES_PER_PIXEL
}

/// Builds the `[1, 3, H, W]` unwarping input for one page.
///
/// The bound is checked from the dimensions before any allocation, which is the
/// whole point of having it: rejecting after building a `500 MB` tensor would
/// honour the limit's letter and defeat it.
pub(crate) fn unwarp_input(page: &InterleavedImage) -> Result<NchwTensor> {
    if page.channels() != 3 {
        return Err(Error::InvalidInput {
            field: "unwarp.channels",
            violation: InputViolation::OutOfRange,
        });
    }
    let dimensions = page.dimensions();
    let pixels = u64::from(dimensions.width()) * u64::from(dimensions.height());
    if pixels > max_unwarp_pixels() {
        return Err(Error::ResourceLimit {
            resource: "unwarp.page_pixels",
            limit: max_unwarp_pixels(),
            actual: pixels,
        });
    }

    let (width, height) = (dimensions.width() as usize, dimensions.height() as usize);
    let mut values: Vec<f32> = Vec::new();
    values
        .try_reserve_exact(width * height * 3)
        .map_err(|_| Error::Backend {
            message: "unwarp input allocation failed",
        })?;
    let source = page.pixels();
    for channel in 0..3_usize {
        for row in 0..height {
            for column in 0..width {
                let index = (row * width + column) * 3 + channel;
                values.push(f32::from(source[index]) * UNWARP_SCALE);
            }
        }
    }
    NchwTensor::new(1, 3, height, width, values)
}

/// Converts the model's `[1, 3, H, W]` output back into an interleaved image.
///
/// `DocTr` multiplies by `255` and converts to `uint8`. OpenCV's `convertTo`
/// goes through `saturate_cast<uchar>`, which uses `cvRound` and therefore
/// rounds **half to even**, not half away from zero.
///
/// That distinction was found by comparing against the capture rather than
/// reasoned about: `f32::round` rounds half away from zero and produced a
/// different image. Half-to-even is the third rounding convention in this
/// project, alongside the recognizer's `ceil`, the batch width's truncation, and
/// the page rotation's own truncated output size — and the detector's rescale
/// already needed exactly this one, which is why the helper is shared.
pub(crate) fn unwarp_output(shape: &[usize], values: &[f32]) -> Result<InterleavedImage> {
    if shape.len() != 4 || shape[0] != 1 || shape[1] != 3 {
        return Err(Error::InvalidInput {
            field: "unwarp.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }
    let (height, width) = (shape[2], shape[3]);
    if values.len() != height * width * 3 {
        return Err(Error::InvalidInput {
            field: "unwarp.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut pixels: Vec<u8> = Vec::new();
    pixels
        .try_reserve_exact(height * width * 3)
        .map_err(|_| Error::Backend {
            message: "unwarp output allocation failed",
        })?;
    for row in 0..height {
        for column in 0..width {
            for channel in 0..3_usize {
                let index = channel * height * width + row * width + column;
                let value = values[index] * 255.0;
                if !value.is_finite() {
                    return Err(Error::InvalidInput {
                        field: "unwarp.output",
                        violation: InputViolation::NonFinite,
                    });
                }
                pixels
                    .push(crate::detector_boxes::round_half_to_even(value).clamp(0.0, 255.0) as u8);
            }
        }
    }
    InterleavedImage::new(
        ImageDimensions::new(width as u32, height as u32)?,
        3,
        pixels,
    )
}

/// Unwarps one page, returning the flattened image.
///
/// The result is a **new image**. Nothing links its coordinates to the caller's
/// page, because the model emits pixels rather than a deformation, and inventing
/// a mapping would be worse than not offering one.
pub(crate) fn unwarp(
    backend: &dyn InferenceBackend,
    contract: &ModelContract,
    page: &InterleavedImage,
) -> Result<InterleavedImage> {
    let tensor = unwarp_input(page)?;
    let input = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
    let output = run_validated(backend, contract, &input)?;
    unwarp_output(output.shape(), output.values())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn page(width: u32, height: u32, index: usize) -> InterleavedImage {
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

    #[test]
    fn the_input_is_scaled_without_a_mean_or_deviation() {
        let source = page(4, 3, 0);
        let tensor = match unwarp_input(&source) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(tensor.shape(), [1, 3, 3, 4]);
        // The first value is the source's first channel divided by 255 and
        // nothing else — not the detector's ImageNet path, not the
        // recognizer's centred one.
        let expected = f32::from(source.pixels()[0]) / 255.0;
        assert_eq!(tensor.values()[0].to_bits(), expected.to_bits());
    }

    /// The bound is checked from the dimensions, before any allocation.
    #[test]
    fn an_oversized_page_is_refused_before_allocating() {
        // 4096 x 4096 is 16.7 megapixels, past the 11.1 the bound admits.
        let dimensions = match ImageDimensions::new(4096, 4096) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        // Build the image cheaply; the point is that `unwarp_input` rejects it
        // without building a tensor from it.
        let pixels = vec![0_u8; (4096_usize * 4096) * 3];
        let source = match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("page: {error}"),
        };
        match unwarp_input(&source) {
            Err(Error::ResourceLimit {
                resource, limit, ..
            }) => {
                assert_eq!(resource, "unwarp.page_pixels");
                assert_eq!(limit, max_unwarp_pixels());
            }
            other => panic!("expected a resource limit, got {other:?}"),
        }
    }

    #[test]
    fn the_bound_admits_about_eleven_megapixels() {
        assert_eq!(max_unwarp_pixels(), 11_184_810);
        // A 3000 x 3000 page fits; 4000 x 3000 does not.
        assert!(3_000_u64 * 3_000 <= max_unwarp_pixels());
        assert!(4_000_u64 * 3_000 > max_unwarp_pixels());
    }

    #[test]
    fn the_output_is_scaled_back_and_saturated() {
        // One pixel per channel, including values that must clamp.
        let values = vec![0.0_f32, 1.0, 2.0, -1.0, 0.5, 0.25];
        let image = match unwarp_output(&[1, 3, 1, 2], &values) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(image.dimensions().width(), 2);
        assert_eq!(image.dimensions().height(), 1);
        // Channel-major input becomes interleaved output.
        assert_eq!(image.pixels(), [0, 255, 128, 255, 0, 64]);
    }

    #[test]
    fn a_non_finite_output_is_a_typed_error() {
        for poison in [f32::NAN, f32::INFINITY] {
            let values = vec![0.5_f32, poison, 0.5, 0.5, 0.5, 0.5];
            assert!(matches!(
                unwarp_output(&[1, 3, 1, 2], &values),
                Err(Error::InvalidInput {
                    field: "unwarp.output",
                    ..
                })
            ));
        }
    }

    #[test]
    fn a_malformed_output_shape_is_refused() {
        let values = vec![0.5_f32; 6];
        assert!(unwarp_output(&[3, 1, 2], &values).is_err());
        assert!(unwarp_output(&[2, 3, 1, 2], &values).is_err());
        assert!(unwarp_output(&[1, 4, 1, 2], &values).is_err());
        assert!(unwarp_output(&[1, 3, 2, 2], &values).is_err());
    }

    #[test]
    fn a_non_three_channel_page_is_refused() {
        let dimensions = match ImageDimensions::new(2, 2) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let grey = match InterleavedImage::new(dimensions, 1, vec![0_u8; 4]) {
            Ok(value) => value,
            Err(error) => panic!("page: {error}"),
        };
        assert!(matches!(
            unwarp_input(&grey),
            Err(Error::InvalidInput {
                field: "unwarp.channels",
                ..
            })
        ));
    }
}

/// Comparison against the captured `UVDoc` oracle.
#[cfg(test)]
mod oracle {
    use super::tests::page;
    use super::*;

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-unwarp/expected.json");

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

    /// Both halves of the contract, checked separately.
    ///
    /// The input tensor is compared against the capture, which pins the third
    /// normalization convention. The output conversion is then driven from the
    /// model's *recorded* raw output, which pins the `×255` saturating cast
    /// without needing the model present. A failure in one does not implicate
    /// the other.
    #[test]
    fn the_captured_tensors_and_images_are_reproduced() {
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        };
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        assert_eq!(records.len(), 3);

        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let index: usize = case
                .trim_start_matches("synthetic-")
                .parse()
                .unwrap_or_default();
            let size = match record["source_wh"].as_array() {
                Some(values) => (
                    values[0].as_u64().unwrap_or_default() as u32,
                    values[1].as_u64().unwrap_or_default() as u32,
                ),
                None => panic!("{case}: no source size"),
            };
            let source = page(size.0, size.1, index);
            assert_eq!(
                sha256_hex(source.pixels()),
                record["source_bgr_sha256"].as_str().unwrap_or_default(),
                "{case}: source bytes"
            );

            let tensor = match unwarp_input(&source) {
                Ok(value) => value,
                Err(error) => panic!("{case} input: {error}"),
            };
            let expected_shape: Vec<usize> = match record["input_shape"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no input shape"),
            };
            assert_eq!(tensor.shape(), expected_shape.as_slice(), "{case}: shape");

            let mut bytes = Vec::with_capacity(tensor.values().len() * 4);
            for value in tensor.values() {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            assert_eq!(
                sha256_hex(&bytes),
                record["input_values_sha256"].as_str().unwrap_or_default(),
                "{case}: input tensor differs from the captured upstream bytes"
            );

            // The recorded raw output, converted by this port's DocTr step.
            let raw =
                match STANDARD.decode(record["output_raw_base64"].as_str().unwrap_or_default()) {
                    Ok(bytes) => bytes,
                    Err(error) => panic!("{case}: base64 {error}"),
                };
            let values: Vec<f32> = raw
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            let output_shape: Vec<usize> = match record["output_shape"].as_array() {
                Some(shape) => shape
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no output shape"),
            };
            let image = match unwarp_output(&output_shape, &values) {
                Ok(value) => value,
                Err(error) => panic!("{case} output: {error}"),
            };
            assert_eq!(
                sha256_hex(image.pixels()),
                record["output_bgr_sha256"].as_str().unwrap_or_default(),
                "{case}: unwarped image differs from the capture"
            );
        }
    }

    /// The model preserves the page's dimensions, which is what makes the
    /// missing resize observable rather than merely undeclared.
    #[test]
    fn the_output_keeps_the_input_size() {
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        };
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let input = record["input_shape"]
                .as_array()
                .unwrap_or(&Vec::new())
                .clone();
            let output = record["output_shape"]
                .as_array()
                .unwrap_or(&Vec::new())
                .clone();
            assert_eq!(input, output, "{case}: the model resized the page");
        }
    }
}
