// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The preprocessing and decoding shared by PaddleX's fixed-size detectors.
//!
//! Roadmap items `LAY-001` and `TBLCELL-001`. Extracted when the second model
//! turned out to declare the **same operator chain** as the first, differing
//! only in the target side and the class list:
//!
//! | Model | Side | Classes |
//! |---|---|---|
//! | `PP-DocLayout_plus-L` | `800` | `20` |
//! | `RT-DETR-L_wired_table_cell_det` | `640` | `1` |
//! | `RT-DETR-L_wireless_table_cell_det` | `640` | `1` |
//!
//! Both configs declare `interp: 2`, `keep_ratio: false`, and
//! `norm_type: none`, and both models' names contain `DETR` or a
//! `models_required_imgsize` entry, so `ToBatch` hands each the same three
//! inputs in the same order.
//!
//! # Why this is shared rather than copied
//!
//! The cubic resize these detectors need has a **known open defect** — see
//! `crate::resize_cubic` and `docs/LAYOUT_CONTRACT.md` — that diverges from
//! OpenCV in a handful of bytes at page scale. A copy would mean fixing it twice
//! and, more likely, fixing it once and leaving the other model quietly on the
//! old path. Sharing makes the eventual fix reach both by construction.
//!
//! # The reversal that has bitten once already
//!
//! `Resize` computes `scale_factors` as `[w_scale, h_scale]` and `ToBatch`
//! reverses it. Getting that backwards does not error — it produces boxes
//! outside the page, which is how it was caught the first time and why the
//! oracles here assert containment rather than only shape.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::resize_cubic::classic_cubic_resize;
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// The error field names a caller sees when a decode fails.
///
/// Carried rather than hardcoded so that sharing the decoder does not blur which
/// model refused. A generic `detection.class_index` would still be a correct
/// error and a worse one: the field is the only part of a typed error that says
/// where to look.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DetectionFields {
    /// Reported when the output shape is not `[N, 6]`.
    pub(crate) output_shape: &'static str,
    /// Reported when a row holds a non-finite value.
    pub(crate) rows: &'static str,
    /// Reported when a class index has no label.
    pub(crate) class_index: &'static str,
}

/// One decoded detection, in source-page coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Detection {
    /// The class index, valid against the caller's label list.
    pub(crate) class: usize,
    /// The detection's confidence.
    pub(crate) score: f32,
    /// `[left, top, right, bottom]`.
    pub(crate) box_ltrb: [f32; 4],
}

/// The folded normalization constants, in the form `Normalize.__init__` folds
/// them: `alpha = scale/std`, `beta = -mean/std`.
///
/// `norm_type: none` is rewritten to `mean_std` upstream with `is_scale`
/// defaulting to true, so the transform is a scale by `1/255` — not the identity
/// the config's own wording suggests.
///
/// **It is a multiply, not a divide.** `x * f32(1/255)` and `x / 255.0` disagree
/// on `126` of the `256` possible byte values, because `1/255` is not
/// representable in binary. This module divided until the table cell oracle
/// caught it; see `docs/TABLE_CELLS_CONTRACT.md`.
const DETECTION_ALPHA: f32 = (1.0_f64 / 255.0) as f32;
const DETECTION_BETA: f32 = 0.0;

/// Builds the `[1, 3, side, side]` input tensor for one RGB page.
pub(crate) fn detection_input(page: &InterleavedImage, side: u32) -> Result<NchwTensor> {
    if page.channels() != 3 {
        return Err(Error::InvalidInput {
            field: "detection.channels",
            violation: InputViolation::OutOfRange,
        });
    }
    let target = ImageDimensions::new(side, side)?;
    let resized = classic_cubic_resize(page, target)?;

    let side = side as usize;
    let mut values: Vec<f32> = Vec::new();
    values
        .try_reserve_exact(side * side * 3)
        .map_err(|_| Error::Backend {
            message: "detection input allocation failed",
        })?;
    let pixels = resized.pixels();
    for channel in 0..3_usize {
        for row in 0..side {
            for column in 0..side {
                let index = (row * side + column) * 3 + channel;
                values.push(f32::from(pixels[index]) * DETECTION_ALPHA + DETECTION_BETA);
            }
        }
    }
    NchwTensor::new(1, 3, side, side, values)
}

/// Returns the `scale_factor` tensor the model expects, `[h_scale, w_scale]`.
///
/// Reversed relative to how `Resize` computes it, because `ToBatch` reverses it
/// on the way in.
pub(crate) fn detection_scale_factor(page: ImageDimensions, side: u32) -> [f32; 2] {
    let side = side as f32;
    [side / page.height() as f32, side / page.width() as f32]
}

/// Returns the `im_shape` tensor, `[h, w]` — also reversed by `ToBatch`.
///
/// Both detectors resize to a fixed square, so this is the target size rather
/// than the page's, and it is the same on both axes. It is computed rather than
/// hardcoded so a future non-square target does not silently keep working.
pub(crate) fn detection_image_shape(side: u32) -> [f32; 2] {
    [side as f32, side as f32]
}

/// Decodes the model's `[N, 6]` detections into source-page boxes.
///
/// Each row is `[class, score, left, top, right, bottom]`, already in source
/// coordinates because the model divides by the supplied scale factor itself.
pub(crate) fn decode_detections(
    shape: &[usize],
    values: &[f32],
    threshold: f32,
    class_count: usize,
    fields: DetectionFields,
) -> Result<Vec<Detection>> {
    if shape.len() != 2 || shape[1] != 6 {
        return Err(Error::InvalidInput {
            field: fields.output_shape,
            violation: InputViolation::OutOfRange,
        });
    }
    if values.len() != shape[0] * 6 {
        return Err(Error::InvalidInput {
            field: fields.output_shape,
            violation: InputViolation::OutOfRange,
        });
    }

    let mut detections = Vec::new();
    for row in 0..shape[0] {
        let entry = &values[row * 6..(row + 1) * 6];
        if !entry.iter().all(|value| value.is_finite()) {
            return Err(Error::InvalidInput {
                field: fields.rows,
                violation: InputViolation::NonFinite,
            });
        }
        // A negative class is the model's own padding for an unused slot, which
        // it emits to keep the output shape fixed.
        if entry[0] < 0.0 || entry[1] < threshold {
            continue;
        }
        let class = entry[0] as usize;
        if class >= class_count {
            return Err(Error::InvalidInput {
                field: fields.class_index,
                violation: InputViolation::OutOfRange,
            });
        }
        detections.push(Detection {
            class,
            score: entry[1],
            box_ltrb: [entry[2], entry[3], entry[4], entry[5]],
        });
    }
    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FIELDS: DetectionFields = DetectionFields {
        output_shape: "probe.output_shape",
        rows: "probe.rows",
        class_index: "probe.class_index",
    };

    /// The reversal, asserted on a deliberately non-square page.
    ///
    /// A square page would let a transposed pair pass, which is exactly how this
    /// class of bug survives review.
    #[test]
    fn the_scale_factor_is_height_first() {
        let page = match ImageDimensions::new(960, 240) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let factor = detection_scale_factor(page, 640);
        assert_eq!(factor, [640.0 / 240.0, 640.0 / 960.0]);
        assert!(factor[0] > factor[1], "height scale must come first");
    }

    #[test]
    fn padding_rows_and_sub_threshold_rows_are_dropped() {
        let values = [
            -1.0, 0.9, 0.0, 0.0, 10.0, 10.0, // padding
            0.0, 0.4, 1.0, 2.0, 3.0, 4.0, // below threshold
            0.0, 0.6, 5.0, 6.0, 7.0, 8.0, // kept
        ];
        let decoded = match decode_detections(&[3, 6], &values, 0.5, 1, TEST_FIELDS) {
            Ok(value) => value,
            Err(error) => panic!("decode: {error}"),
        };
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].class, 0);
        assert_eq!(decoded[0].box_ltrb, [5.0, 6.0, 7.0, 8.0]);
    }

    /// A class the label list cannot name is refused rather than indexed.
    #[test]
    fn an_out_of_range_class_is_refused() {
        let values = [3.0_f32, 0.9, 0.0, 0.0, 1.0, 1.0];
        assert!(decode_detections(&[1, 6], &values, 0.5, 1, TEST_FIELDS).is_err());
    }

    #[test]
    fn a_non_finite_row_is_refused() {
        let values = [0.0_f32, f32::NAN, 0.0, 0.0, 1.0, 1.0];
        assert!(matches!(
            decode_detections(&[1, 6], &values, 0.5, 1, TEST_FIELDS),
            Err(Error::InvalidInput {
                violation: InputViolation::NonFinite,
                ..
            })
        ));
    }

    #[test]
    fn a_wrong_output_shape_is_refused() {
        assert!(decode_detections(&[1, 5], &[0.0; 5], 0.5, 1, TEST_FIELDS).is_err());
        assert!(decode_detections(&[2, 6], &[0.0; 6], 0.5, 1, TEST_FIELDS).is_err());
    }
}
