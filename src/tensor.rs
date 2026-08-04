// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded classic normalization, NCHW layout, padding, and batching.
//!
//! This module performs arithmetic only. It never resizes pixels, selects an
//! interpolation, loads a model, or validates a runtime tensor ABI. Its inputs
//! are interleaved images that a caller has already decoded and already sized.
//!
//! The two classic normalizations are deliberately *not* unified, because the
//! pinned upstream source does not express them the same way and `f32`
//! arithmetic is not associative:
//!
//! - The detector multiplies by a `float32` reciprocal and divides by a
//!   per-channel standard deviation, matching
//!   `ppocr/data/imaug/operators.py:NormalizeImage.__call__`:
//!   `(img.astype("float32") * self.scale - self.mean) / self.std`.
//! - The recognizer divides by `255`, subtracts `0.5`, and divides by `0.5`,
//!   matching `ppocr/data/imaug/rec_img_aug.py:resize_norm_img`:
//!   `resized_image.transpose((2, 0, 1)) / 255`, then `-= 0.5`, then `/= 0.5`.
//!
//! Both normalizations act on the interleaved channel axis positionally. For a
//! decoded BGR image that means the detector's first mean, `0.485`, applies to
//! the blue channel. That is the pinned upstream behaviour, which passes the
//! `cv2` BGR array straight into `NormalizeImage`; it is preserved rather than
//! silently corrected.

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};

/// Interleaved channel count required by both classic normalizations.
const CLASSIC_CHANNELS: u8 = 3;

/// Maximum number of `f32` elements in one built tensor.
const MAX_TENSOR_ELEMENTS: u64 = 64_000_000;

/// Maximum number of images stacked into one batch.
const MAX_BATCH_IMAGES: usize = 256;

/// The detector's `float32` reciprocal of `255`.
///
/// The pinned source stores `np.float32(1.0 / 255.0)` and multiplies by it. An
/// `f32` division by `255` is a different operation in general, so the constant
/// is written explicitly. This value is bit-identical to `np.float32(1./255.)`
/// (`0x3b808081`).
const DETECTOR_SCALE: f32 = 1.0_f32 / 255.0_f32;

/// The orientation classifier's fixed input width, from the artifact config.
pub(crate) const ORIENTATION_INPUT_WIDTH: u32 = 160;

/// The orientation classifier's fixed input height, from the artifact config.
pub(crate) const ORIENTATION_INPUT_HEIGHT: u32 = 80;

/// Per-channel means applied positionally to the interleaved channel axis.
const DETECTOR_MEAN: [f32; 3] = [0.485, 0.456, 0.406];

/// Per-channel standard deviations applied positionally.
const DETECTOR_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A bounded, contiguous `NCHW` `float32` tensor.
///
/// Values are stored batch-major, then channel, then row, then column, with no
/// padding between elements: the stride of `width` is `1`, of `height` is
/// `width`, of `channels` is `height * width`, and of `batch` is
/// `channels * height * width`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NchwTensor {
    batch: usize,
    channels: usize,
    height: usize,
    width: usize,
    values: Vec<f32>,
}

impl NchwTensor {
    /// Returns the `[batch, channels, height, width]` shape.
    pub(crate) const fn shape(&self) -> [usize; 4] {
        [self.batch, self.channels, self.height, self.width]
    }

    /// Returns the contiguous row-major values.
    pub(crate) fn values(&self) -> &[f32] {
        &self.values
    }

    /// Returns the element strides for each axis, in shape order.
    pub(crate) const fn strides(&self) -> [usize; 4] {
        let width = self.width;
        let plane = self.height * width;
        [self.channels * plane, plane, width, 1]
    }
}

/// Builds the classic detector input tensor from one decoded BGR image.
///
/// The caller is responsible for having already applied the frozen detector
/// resize and padding policy; this function does not resize.
pub(crate) fn classic_detector_input(image: &InterleavedImage) -> Result<NchwTensor> {
    require_classic_channels(image)?;
    let dimensions = image.dimensions();
    let (height, width) = (dimensions.height() as usize, dimensions.width() as usize);
    let mut values = bounded_tensor_buffer(1, CLASSIC_CHANNELS as usize, height, width)?;

    let pixels = image.pixels();
    for channel in 0..CLASSIC_CHANNELS as usize {
        let mean = DETECTOR_MEAN[channel];
        let deviation = DETECTOR_STD[channel];
        for row in 0..height {
            for column in 0..width {
                let source = (row * width + column) * CLASSIC_CHANNELS as usize + channel;
                let value = f32::from(pixels[source]);
                values.push((value * DETECTOR_SCALE - mean) / deviation);
            }
        }
    }

    Ok(NchwTensor {
        batch: 1,
        channels: CLASSIC_CHANNELS as usize,
        height,
        width,
        values,
    })
}

/// Builds one classic recognizer batch from already resized BGR crops.
///
/// Every crop must share `height` and must be no wider than `batch_width`.
/// Narrower crops are right-padded with `0.0` in the normalized domain, which
/// is what the pinned source does by writing into a zero-filled buffer rather
/// than by normalizing a zero pixel.
/// Builds the `[N, 3, 80, 160]` orientation-classifier input batch.
///
/// The normalization is the **detector's**, not the recognizer's:
/// `PP-LCNet_x1_0_textline_ori`'s `inference.yml` declares `scale 1/255` with the
/// ImageNet mean and standard deviation, which is exactly what
/// [`classic_detector_input`] applies. `docs/ORIENTATION_CONTRACT.md` records how
/// that differs from the legacy `predict_cls.py` path, which uses
/// `(x / 255 - 0.5) / 0.5` and would be wrong for this artifact.
///
/// Every crop must already be resized to the fixed `160x80`. The classifier's
/// resize is unconditional — no aspect ratio, no padding — so unlike recognition
/// there is nothing to plan and no per-batch width.
pub(crate) fn classic_orientation_batch(crops: &[&InterleavedImage]) -> Result<NchwTensor> {
    if crops.is_empty() {
        return Err(Error::InvalidInput {
            field: "orientation.batch",
            violation: InputViolation::Empty,
        });
    }
    let (width, height) = (
        ORIENTATION_INPUT_WIDTH as usize,
        ORIENTATION_INPUT_HEIGHT as usize,
    );
    for crop in crops {
        require_classic_channels(crop)?;
        let dimensions = crop.dimensions();
        if dimensions.width() != ORIENTATION_INPUT_WIDTH
            || dimensions.height() != ORIENTATION_INPUT_HEIGHT
        {
            return Err(Error::InvalidInput {
                field: "orientation.crop_dimensions",
                violation: InputViolation::OutOfRange,
            });
        }
    }

    let mut values = bounded_tensor_buffer(crops.len(), CLASSIC_CHANNELS as usize, height, width)?;
    for crop in crops {
        let pixels = crop.pixels();
        for channel in 0..CLASSIC_CHANNELS as usize {
            let mean = DETECTOR_MEAN[channel];
            let deviation = DETECTOR_STD[channel];
            for row in 0..height {
                for column in 0..width {
                    let source = (row * width + column) * CLASSIC_CHANNELS as usize + channel;
                    let value = f32::from(pixels[source]);
                    values.push((value * DETECTOR_SCALE - mean) / deviation);
                }
            }
        }
    }

    Ok(NchwTensor {
        batch: crops.len(),
        channels: CLASSIC_CHANNELS as usize,
        height,
        width,
        values,
    })
}

pub(crate) fn classic_recognizer_batch(
    crops: &[&InterleavedImage],
    batch_width: u32,
) -> Result<NchwTensor> {
    if crops.is_empty() {
        return Err(Error::InvalidInput {
            field: "tensor.batch",
            violation: InputViolation::Empty,
        });
    }
    if crops.len() > MAX_BATCH_IMAGES {
        return Err(Error::ResourceLimit {
            resource: "tensor.batch_images",
            limit: MAX_BATCH_IMAGES as u64,
            actual: crops.len() as u64,
        });
    }
    if batch_width == 0 {
        return Err(Error::InvalidInput {
            field: "tensor.batch_width",
            violation: InputViolation::Empty,
        });
    }

    let first = crops[0].dimensions();
    let height = first.height() as usize;
    let width = batch_width as usize;
    for crop in crops {
        require_classic_channels(crop)?;
        let dimensions = crop.dimensions();
        if dimensions.height() != first.height() {
            return Err(Error::InvalidInput {
                field: "tensor.batch_height",
                violation: InputViolation::OutOfRange,
            });
        }
        if dimensions.width() > batch_width {
            return Err(Error::InvalidInput {
                field: "tensor.batch_width",
                violation: InputViolation::OutOfRange,
            });
        }
    }

    let mut values = bounded_tensor_buffer(crops.len(), CLASSIC_CHANNELS as usize, height, width)?;
    for crop in crops {
        let crop_width = crop.dimensions().width() as usize;
        let pixels = crop.pixels();
        for channel in 0..CLASSIC_CHANNELS as usize {
            for row in 0..height {
                for column in 0..crop_width {
                    let source = (row * crop_width + column) * CLASSIC_CHANNELS as usize + channel;
                    let value = f32::from(pixels[source]);
                    values.push((value / 255.0 - 0.5) / 0.5);
                }
                values.extend(std::iter::repeat_n(0.0, width - crop_width));
            }
        }
    }

    Ok(NchwTensor {
        batch: crops.len(),
        channels: CLASSIC_CHANNELS as usize,
        height,
        width,
        values,
    })
}

/// Rejects an image that does not carry the three classic interleaved channels.
fn require_classic_channels(image: &InterleavedImage) -> Result<()> {
    if image.channels() != CLASSIC_CHANNELS {
        return Err(Error::InvalidInput {
            field: "tensor.channels",
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(())
}

/// Reserves an exactly sized tensor buffer under the element limit.
fn bounded_tensor_buffer(
    batch: usize,
    channels: usize,
    height: usize,
    width: usize,
) -> Result<Vec<f32>> {
    let elements = (batch as u64)
        .checked_mul(channels as u64)
        .and_then(|value| value.checked_mul(height as u64))
        .and_then(|value| value.checked_mul(width as u64))
        .ok_or(Error::ResourceLimit {
            resource: "tensor.elements",
            limit: MAX_TENSOR_ELEMENTS,
            actual: u64::MAX,
        })?;
    if elements == 0 {
        return Err(Error::InvalidInput {
            field: "tensor.elements",
            violation: InputViolation::Empty,
        });
    }
    if elements > MAX_TENSOR_ELEMENTS {
        return Err(Error::ResourceLimit {
            resource: "tensor.elements",
            limit: MAX_TENSOR_ELEMENTS,
            actual: elements,
        });
    }
    let capacity = usize::try_from(elements).map_err(|_| Error::ResourceLimit {
        resource: "tensor.elements",
        limit: MAX_TENSOR_ELEMENTS,
        actual: elements,
    })?;

    let mut values = Vec::new();
    if values.try_reserve_exact(capacity).is_err() {
        return Err(Error::ResourceLimit {
            resource: "tensor.elements",
            limit: MAX_TENSOR_ELEMENTS,
            actual: elements,
        });
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::ImageDimensions;

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

    /// The detector scale must be the exact `float32` the pinned source stores.
    #[test]
    fn detector_scale_matches_the_pinned_float32_reciprocal() {
        assert_eq!(DETECTOR_SCALE.to_bits(), 0x3b80_8081);
    }

    /// Expected values were produced independently by NumPy 2.5.1 running the
    /// pinned upstream expression, not by this implementation.
    #[test]
    fn classic_detector_input_matches_the_upstream_normalization() {
        let source = image(
            2,
            2,
            3,
            vec![0, 1, 2, 253, 254, 255, 10, 20, 30, 128, 129, 130],
        );
        let tensor = match classic_detector_input(&source) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a detector tensor, got {error}"),
        };

        assert_eq!(tensor.shape(), [1, 3, 2, 2]);
        assert_eq!(tensor.strides(), [12, 4, 2, 1]);
        let expected: [f32; 12] = [
            -2.117904, 2.214659, -1.9466565, 0.07406463, -2.0182073, 2.4110649, -1.6855742,
            0.2226892, -1.7695861, 2.64, -1.2815685, 0.46135095,
        ];
        assert_eq!(tensor.values(), expected);
    }

    /// Expected values were produced independently by NumPy 2.5.1.
    #[test]
    fn classic_recognizer_batch_matches_the_upstream_normalization_and_padding() {
        let crop = image(
            2,
            2,
            3,
            vec![0, 1, 2, 128, 129, 130, 253, 254, 255, 64, 65, 66],
        );
        let tensor = match classic_recognizer_batch(&[&crop], 4) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a recognizer tensor, got {error}"),
        };

        assert_eq!(tensor.shape(), [1, 3, 2, 4]);
        assert_eq!(tensor.strides(), [24, 8, 4, 1]);
        let expected: [f32; 24] = [
            -1.0,
            0.003921628,
            0.0,
            0.0,
            0.9843137,
            -0.4980392,
            0.0,
            0.0,
            -0.99215686,
            0.011764765,
            0.0,
            0.0,
            0.99215686,
            -0.49019605,
            0.0,
            0.0,
            -0.9843137,
            0.019607902,
            0.0,
            0.0,
            1.0,
            -0.4823529,
            0.0,
            0.0,
        ];
        assert_eq!(tensor.values(), expected);
    }

    /// The detector means are applied positionally, not by colour name.
    #[test]
    fn detector_means_follow_the_interleaved_channel_index() {
        // One pixel whose interleaved value is zero in every channel isolates
        // the per-channel `-mean / std` term.
        let source = image(1, 1, 3, vec![0, 0, 0]);
        let tensor = match classic_detector_input(&source) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a detector tensor, got {error}"),
        };
        for channel in 0..3 {
            let expected =
                (0.0_f32 * DETECTOR_SCALE - DETECTOR_MEAN[channel]) / DETECTOR_STD[channel];
            assert_eq!(
                tensor.values()[channel],
                expected,
                "channel {channel} must use its own positional mean"
            );
        }
        // The first channel of a decoded BGR image is blue, so the first mean
        // is the upstream `0.485`.
        assert_eq!(DETECTOR_MEAN[0], 0.485);
    }

    #[test]
    fn recognizer_batch_preserves_image_order_and_pads_each_row() {
        let wide = image(2, 1, 3, vec![255, 255, 255, 0, 0, 0]);
        let narrow = image(1, 1, 3, vec![0, 0, 0]);
        let tensor = match classic_recognizer_batch(&[&wide, &narrow], 3) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a recognizer tensor, got {error}"),
        };

        assert_eq!(tensor.shape(), [2, 3, 1, 3]);
        // First image: value 255 then 0 then one pad column, per channel.
        assert_eq!(&tensor.values()[0..3], &[1.0, -1.0, 0.0]);
        assert_eq!(&tensor.values()[3..6], &[1.0, -1.0, 0.0]);
        assert_eq!(&tensor.values()[6..9], &[1.0, -1.0, 0.0]);
        // Second image: value 0 then two pad columns, per channel.
        assert_eq!(&tensor.values()[9..12], &[-1.0, 0.0, 0.0]);
        assert_eq!(&tensor.values()[12..15], &[-1.0, 0.0, 0.0]);
        assert_eq!(&tensor.values()[15..18], &[-1.0, 0.0, 0.0]);
    }

    #[test]
    fn tensor_builders_reject_invalid_shapes_and_channel_counts() {
        let single_channel = image(1, 1, 1, vec![7]);
        assert!(matches!(
            classic_detector_input(&single_channel),
            Err(Error::InvalidInput {
                field: "tensor.channels",
                ..
            })
        ));

        let crop = image(2, 2, 3, vec![0; 12]);
        assert!(matches!(
            classic_recognizer_batch(&[], 4),
            Err(Error::InvalidInput {
                field: "tensor.batch",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            classic_recognizer_batch(&[&crop], 0),
            Err(Error::InvalidInput {
                field: "tensor.batch_width",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            classic_recognizer_batch(&[&crop], 1),
            Err(Error::InvalidInput {
                field: "tensor.batch_width",
                violation: InputViolation::OutOfRange,
            })
        ));

        let taller = image(2, 3, 3, vec![0; 18]);
        assert!(matches!(
            classic_recognizer_batch(&[&crop, &taller], 4),
            Err(Error::InvalidInput {
                field: "tensor.batch_height",
                violation: InputViolation::OutOfRange,
            })
        ));
    }

    #[test]
    fn bounded_tensor_buffer_enforces_its_element_limit() {
        assert!(matches!(
            bounded_tensor_buffer(1, 3, 0, 4),
            Err(Error::InvalidInput {
                field: "tensor.elements",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            bounded_tensor_buffer(1, 3, 8_000, 8_000),
            Err(Error::ResourceLimit {
                resource: "tensor.elements",
                limit: MAX_TENSOR_ELEMENTS,
                actual: 192_000_000,
            })
        ));
        let accepted = match bounded_tensor_buffer(2, 3, 4, 5) {
            Ok(buffer) => buffer,
            Err(error) => panic!("expected an accepted buffer, got {error}"),
        };
        assert_eq!(accepted.capacity(), 120);
        assert!(accepted.is_empty());
    }

    #[test]
    fn recognizer_batch_rejects_more_images_than_the_batch_limit() {
        let crop = image(1, 1, 3, vec![0, 0, 0]);
        let crops = vec![&crop; MAX_BATCH_IMAGES + 1];
        assert!(matches!(
            classic_recognizer_batch(&crops, 1),
            Err(Error::ResourceLimit {
                resource: "tensor.batch_images",
                ..
            })
        ));
    }
}

/// Optional comparison against a captured upstream preprocessing tensor.
///
/// This is `PRE-001`'s acceptance criterion. It is ignored by default because
/// the capture is produced by `tools/capture_preprocess_oracle.py`, which needs
/// the read-only upstream checkout plus `numpy`, `cv2`, `PIL`, and `paddle` —
/// none of which this repository depends on, and none of which may be required
/// to run its tests.
///
/// The committed offline fixture holds a sample of the same capture, so the
/// normal test run still checks real captured values; this gate checks every
/// element of every captured tensor.
///
/// ```sh
/// PADDLEOCR_RUST_PREPROCESS_CAPTURE=<capture.json> \
///   cargo test --lib -- --ignored --nocapture pre_001
/// ```
#[cfg(test)]
mod pre_001 {
    use super::*;

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use crate::geometry::classic_detector_resize_plan;
    use crate::image::decode_classic_bgr;
    use crate::resize::classic_linear_resize;
    use crate::types::EncodedImage;

    /// The `m2-tensor-v1` comparison rule from `docs/QUALITY_PROFILE.md`.
    fn within_tolerance(candidate: f64, reference: f64) -> bool {
        (candidate - reference).abs() <= 1e-4 + 1e-4 * reference.abs()
    }

    /// Builds this port's detector input tensor for one encoded PNG.
    pub(super) fn detector_input_for(png: &[u8]) -> (Vec<usize>, Vec<f32>) {
        let encoded = match EncodedImage::new(png) {
            Ok(value) => value,
            Err(error) => panic!("encoded image: {error}"),
        };
        let image = match decode_classic_bgr(encoded) {
            Ok(value) => value,
            Err(error) => panic!("decode: {error}"),
        };
        let plan = classic_detector_resize_plan(image.dimensions());
        let resized = match classic_linear_resize(&image, plan.resized()) {
            Ok(value) => value,
            Err(error) => panic!("resize: {error}"),
        };
        let tensor = match classic_detector_input(&resized) {
            Ok(value) => value,
            Err(error) => panic!("tensor: {error}"),
        };
        (tensor.shape().to_vec(), tensor.values().to_vec())
    }

    /// Decodes a base64 little-endian `float32` payload.
    pub(super) fn decode_f32(encoded: &str) -> Vec<f32> {
        let bytes = match STANDARD.decode(encoded) {
            Ok(bytes) => bytes,
            Err(error) => panic!("base64: {error}"),
        };
        assert_eq!(bytes.len() % 4, 0, "float32 payload must be a whole number");
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    #[test]
    #[ignore = "PRE-001: needs a capture from the upstream checkout"]
    fn every_captured_element_matches_within_the_declared_tolerance() {
        let path = match std::env::var("PADDLEOCR_RUST_PREPROCESS_CAPTURE") {
            Ok(value) => value,
            Err(_) => panic!("set PADDLEOCR_RUST_PREPROCESS_CAPTURE"),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => panic!("capture file: {error}"),
        };
        let document: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) => panic!("capture json: {error}"),
        };

        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("capture must hold records"),
        };
        assert!(!records.is_empty(), "capture must not be empty");

        for record in records {
            let fixture = record["fixture_id"].as_str().unwrap_or("<unnamed>");
            let png_path = record["input_path"].as_str().unwrap_or_default();
            let png = match std::fs::read(png_path) {
                Ok(bytes) => bytes,
                Err(error) => panic!("{fixture} input {png_path}: {error}"),
            };

            let (shape, ours) = detector_input_for(&png);
            let captured = &record["detector_input"];
            let expected_shape: Vec<usize> = match captured["shape"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{fixture} capture must record a shape"),
            };
            assert_eq!(shape, expected_shape, "{fixture} tensor shape");

            let reference = decode_f32(captured["values_base64"].as_str().unwrap_or_default());
            assert_eq!(reference.len(), ours.len(), "{fixture} element count");

            let mut worst = 0.0_f64;
            let mut worst_index = 0;
            let mut failures = 0_usize;
            for (index, (candidate, expected)) in ours.iter().zip(&reference).enumerate() {
                let (candidate, expected) = (f64::from(*candidate), f64::from(*expected));
                let deviation = (candidate - expected).abs();
                if deviation > worst {
                    worst = deviation;
                    worst_index = index;
                }
                if !within_tolerance(candidate, expected) {
                    failures += 1;
                }
            }
            let identical = ours
                .iter()
                .zip(&reference)
                .all(|(candidate, expected)| candidate.to_bits() == expected.to_bits());
            println!(
                "[pre_001] {fixture}: {} elements, worst absolute deviation {worst:.3e} at \
                 index {worst_index}, {failures} outside tolerance, bit-identical: {identical}",
                ours.len()
            );
            assert_eq!(
                failures, 0,
                "{fixture}: {failures} elements outside the m2-tensor-v1 tolerance"
            );
        }
    }
}

/// The committed offline half of `PRE-001`.
///
/// The full upstream capture is tens of megabytes and is not in this repository.
/// What is committed is, per input, the tensor shape and the SHA-256 of the
/// exact `float32` little-endian C-order bytes upstream produced, plus a fixed
/// stride sample of exact values.
///
/// The digest is the real check. The samples exist so that a failure says
/// *where* the tensors diverged rather than only *that* they did, which is the
/// difference between a usable regression signal and a dead end.
#[cfg(test)]
mod preprocess_fixture {
    use super::pre_001::{decode_f32, detector_input_for};

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-preprocess-input/expected.json");

    /// The inputs the fixture covers, paired with their committed PNG bytes.
    const INPUTS: [(&str, &[u8]); 4] = [
        (
            "classic-v1-e2e-reading-order",
            include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png"),
        ),
        (
            "classic-v1-e2e-unicode",
            include_bytes!("../tests/fixtures/classic-v1-e2e-unicode/input.png"),
        ),
        (
            "classic-v1-e2e-tall-crop",
            include_bytes!("../tests/fixtures/classic-v1-e2e-tall-crop/input.png"),
        ),
        (
            "classic-v1-benchmark-page",
            include_bytes!("../tests/fixtures/classic-v1-benchmark-page/input.png"),
        ),
    ];

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

    mod tests {
        use super::*;

        use crate::crop::InterleavedImage;
        use crate::tensor::classic_recognizer_batch;

        #[test]
        fn detector_input_tensors_reproduce_the_captured_upstream_bytes() {
            let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
                Ok(value) => value,
                Err(error) => panic!("fixture json: {error}"),
            };
            let records = match document["records"].as_array() {
                Some(records) => records,
                None => panic!("fixture must hold records"),
            };
            assert_eq!(records.len(), INPUTS.len(), "fixture record count");

            for (record, (name, png)) in records.iter().zip(INPUTS) {
                assert_eq!(
                    record["fixture_id"].as_str().unwrap_or_default(),
                    name,
                    "fixture record order must match the committed inputs"
                );

                let (shape, ours) = detector_input_for(png);
                let expected_shape: Vec<usize> = match record["shape"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as usize)
                        .collect(),
                    None => panic!("{name}: fixture must record a shape"),
                };
                assert_eq!(shape, expected_shape, "{name} tensor shape");

                // The bytes upstream produced, reconstructed exactly: little-endian
                // float32 in C order, which is what the capture hashed.
                let mut bytes = Vec::with_capacity(ours.len() * 4);
                for value in &ours {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                assert_eq!(
                    sha256_hex(&bytes),
                    record["values_sha256"].as_str().unwrap_or_default(),
                    "{name}: detector input tensor differs from the captured upstream bytes"
                );

                // The digest already proved equality; the samples are checked so
                // that a future divergence reports a located value.
                let indices: Vec<usize> = match record["sample_indices"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as usize)
                        .collect(),
                    None => panic!("{name}: fixture must record sample indices"),
                };
                let samples =
                    decode_f32(record["sample_values_base64"].as_str().unwrap_or_default());
                assert_eq!(indices.len(), samples.len(), "{name} sample length");
                for (index, expected) in indices.iter().zip(&samples) {
                    assert_eq!(
                        ours[*index].to_bits(),
                        expected.to_bits(),
                        "{name}: element {index}"
                    );
                }
            }
        }

        /// One deterministic synthetic crop, matching the capture tool's
        /// `synthetic_crop` byte for byte.
        ///
        /// The formula is trivial on purpose: anything depending on a random
        /// seed, a platform float, or a library version would defeat the point
        /// of comparing against a capture produced on the other side.
        fn synthetic_crop(index: usize, width: u32, height: u32) -> InterleavedImage {
            let dimensions = match crate::types::ImageDimensions::new(width, height) {
                Ok(value) => value,
                Err(error) => panic!("crop {index} dimensions: {error}"),
            };
            let mut pixels = Vec::with_capacity((width * height * 3) as usize);
            for y in 0..height as usize {
                for x in 0..width as usize {
                    for channel in 0..3_usize {
                        let value = (x * 7 + y * 13 + channel * 29 + index * 31) % 256;
                        pixels.push(value as u8);
                    }
                }
            }
            match InterleavedImage::new(dimensions, 3, pixels) {
                Ok(value) => value,
                Err(error) => panic!("crop {index}: {error}"),
            }
        }

        /// The recognizer input tensor, per batch, against captured upstream
        /// bytes.
        ///
        /// The detector case above shares its inputs with the end-to-end
        /// fixtures. This one cannot: the recognizer's inputs are crops, which
        /// only exist mid-pipeline. So the capture and this test agree on a
        /// deterministic synthetic crop set instead, chosen to span the narrow
        /// end, the base ratio, a fractional ratio, and enough crops to force
        /// two batches whose padded widths differ.
        #[test]
        fn recognizer_input_tensors_reproduce_the_captured_upstream_bytes() {
            use crate::recognizer_batch::{RECOGNITION_HEIGHT, plan_batches};
            use crate::resize::classic_linear_resize;

            let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
                Ok(value) => value,
                Err(error) => panic!("fixture json: {error}"),
            };
            let recognizer = &document["recognizer"];
            let sizes: Vec<(u32, u32)> = match recognizer["crop_sizes"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|pair| {
                        let pair = match pair.as_array() {
                            Some(pair) => pair,
                            None => panic!("crop size must be a pair"),
                        };
                        (
                            pair[0].as_u64().unwrap_or_default() as u32,
                            pair[1].as_u64().unwrap_or_default() as u32,
                        )
                    })
                    .collect(),
                None => panic!("fixture must record crop sizes"),
            };
            let crops: Vec<InterleavedImage> = sizes
                .iter()
                .enumerate()
                .map(|(index, (width, height))| synthetic_crop(index, *width, *height))
                .collect();

            let plans = match plan_batches(&sizes) {
                Ok(plans) => plans,
                Err(error) => panic!("batch plan: {error}"),
            };
            let batches = match recognizer["batches"].as_array() {
                Some(values) => values,
                None => panic!("fixture must record batches"),
            };
            assert_eq!(plans.len(), batches.len(), "batch count");

            for (batch_index, (plan, expected)) in plans.iter().zip(batches).enumerate() {
                let expected_indices: Vec<usize> = match expected["original_indices"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as usize)
                        .collect(),
                    None => panic!("batch {batch_index} must record its rows"),
                };
                let ours_indices: Vec<usize> =
                    plan.crops.iter().map(|crop| crop.original_index).collect();
                assert_eq!(
                    ours_indices, expected_indices,
                    "batch {batch_index} row order"
                );

                let mut resized = Vec::with_capacity(plan.crops.len());
                for entry in &plan.crops {
                    let target = match crate::types::ImageDimensions::new(
                        entry.resized_width,
                        RECOGNITION_HEIGHT,
                    ) {
                        Ok(value) => value,
                        Err(error) => panic!("batch {batch_index} target: {error}"),
                    };
                    match classic_linear_resize(&crops[entry.original_index], target) {
                        Ok(value) => resized.push(value),
                        Err(error) => panic!("batch {batch_index} resize: {error}"),
                    }
                }
                let borrowed: Vec<&InterleavedImage> = resized.iter().collect();
                let tensor = match classic_recognizer_batch(&borrowed, plan.batch_width) {
                    Ok(value) => value,
                    Err(error) => panic!("batch {batch_index} tensor: {error}"),
                };

                let expected_shape: Vec<usize> = match expected["shape"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as usize)
                        .collect(),
                    None => panic!("batch {batch_index} must record a shape"),
                };
                assert_eq!(
                    tensor.shape(),
                    expected_shape.as_slice(),
                    "batch {batch_index} tensor shape"
                );

                let mut bytes = Vec::with_capacity(tensor.values().len() * 4);
                for value in tensor.values() {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                assert_eq!(
                    sha256_hex(&bytes),
                    expected["values_sha256"].as_str().unwrap_or_default(),
                    "batch {batch_index}: recognizer input differs from the captured bytes"
                );

                let indices: Vec<usize> = match expected["sample_indices"].as_array() {
                    Some(values) => values
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as usize)
                        .collect(),
                    None => panic!("batch {batch_index} must record sample indices"),
                };
                let samples = decode_f32(
                    expected["sample_values_base64"]
                        .as_str()
                        .unwrap_or_default(),
                );
                for (index, want) in indices.iter().zip(&samples) {
                    assert_eq!(
                        tensor.values()[*index].to_bits(),
                        want.to_bits(),
                        "batch {batch_index}: element {index}"
                    );
                }
            }
        }
    }
}
