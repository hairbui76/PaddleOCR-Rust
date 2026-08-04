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
