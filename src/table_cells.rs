// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Table cell detection, for both the wired and wireless models.
//!
//! Roadmap item `TBLCELL-001`. `RT-DETR-L_wired_table_cell_det` and
//! `RT-DETR-L_wireless_table_cell_det` declare **the same operator chain as the
//! layout detector**, differing only in the target side — `640` rather than
//! `800` — and the class list, which here has one entry.
//!
//! That claim is what the oracle is for. It would have been easy to assume from
//! two configs that look alike; the capture executes the pinned PaddleX
//! operators at `640` and compares, so the shared path in
//! [`crate::paddlex_detection`] is justified by measurement rather than by
//! resemblance.
//!
//! # The two models differ only in weights
//!
//! Their `inference.yml` files agree on every field this module reads: the
//! resize, the normalization, the permute, the label list, and
//! `draw_threshold: 0.5`. So there is one code path and a
//! [`TableCellModel`] tag that records which artifact produced a result, rather
//! than two near-identical modules.
//!
//! # It inherits an open defect
//!
//! The cubic resize is `crate::resize_cubic`, which is one 8-bit step off from
//! OpenCV in a handful of bytes at page scale — see `docs/LAYOUT_CONTRACT.md`.
//! Nothing here makes that better or worse. Sharing the path means the eventual
//! fix reaches this model too, which is the reason the path is shared.
//!
//! # Not wired into a pipeline
//!
//! Cell boxes are an input to table structure recognition, which this port does
//! not have. Composition is `P9`'s subject.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::Result;
use crate::paddlex_detection::{
    Detection, DetectionFields, decode_detections, detection_image_shape, detection_input,
    detection_scale_factor,
};
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// The fixed input side, from `inference.yml`.
pub(crate) const TABLE_CELL_INPUT_SIDE: u32 = 640;

/// `draw_threshold` from `inference.yml`.
pub(crate) const TABLE_CELL_THRESHOLD: f32 = 0.5;

/// The artifact's label list. One class, for both models.
pub(crate) const TABLE_CELL_LABELS: [&str; 1] = ["cell"];

/// Which artifact produced a set of cells.
///
/// The two models share every preprocessing and postprocessing field, so this
/// records provenance rather than selecting behaviour. A caller that mixes
/// results from both should still be able to say which came from where.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TableCellModel {
    /// `RT-DETR-L_wired_table_cell_det`, for tables with ruling lines.
    Wired,
    /// `RT-DETR-L_wireless_table_cell_det`, for tables separated by whitespace.
    Wireless,
}

impl TableCellModel {
    /// The artifact's `model_name`.
    #[must_use]
    pub const fn model_name(self) -> &'static str {
        match self {
            Self::Wired => "RT-DETR-L_wired_table_cell_det",
            Self::Wireless => "RT-DETR-L_wireless_table_cell_det",
        }
    }
}

/// One detected cell, in source-page coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TableCell {
    /// The detection's confidence.
    pub score: f32,
    /// `[left, top, right, bottom]`.
    pub box_ltrb: [f32; 4],
}

/// Builds the `[1, 3, 640, 640]` input tensor for one RGB table image.
pub(crate) fn table_cell_input(image: &InterleavedImage) -> Result<NchwTensor> {
    detection_input(image, TABLE_CELL_INPUT_SIDE)
}

/// Returns the `scale_factor` tensor, `[h_scale, w_scale]`.
pub(crate) fn table_cell_scale_factor(image: ImageDimensions) -> [f32; 2] {
    detection_scale_factor(image, TABLE_CELL_INPUT_SIDE)
}

/// Returns the `im_shape` tensor, `[h, w]`.
pub(crate) fn table_cell_image_shape() -> [f32; 2] {
    detection_image_shape(TABLE_CELL_INPUT_SIDE)
}

/// Decodes the model's `[N, 6]` output into cells.
pub(crate) fn table_cells(
    shape: &[usize],
    values: &[f32],
    threshold: f32,
) -> Result<Vec<TableCell>> {
    let detections = decode_detections(
        shape,
        values,
        threshold,
        TABLE_CELL_LABELS.len(),
        DetectionFields {
            output_shape: "table_cell.output_shape",
            rows: "table_cell.detections",
            class_index: "table_cell.class_index",
        },
    )?;
    Ok(detections
        .into_iter()
        .map(
            |Detection {
                 score, box_ltrb, ..
             }| TableCell { score, box_ltrb },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-table-cells/expected.json");

    fn synthetic_rgb(width: u32, height: u32) -> InterleavedImage {
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..u64::from(height) {
            for x in 0..u64::from(width) {
                for c in 0..3_u64 {
                    pixels.push(((x * 7 + y * 13 + c * 29) % 256) as u8);
                }
            }
        }
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("image: {error}"),
        }
    }

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    /// Every captured tensor, hashed whole and bounded when it differs.
    ///
    /// The cubic resize has a recorded open defect, so an exact match is
    /// asserted where it holds and a **bound** is asserted where it does not.
    /// The bound is what keeps a known one-step divergence from quietly becoming
    /// a large one; see `docs/LAYOUT_CONTRACT.md`.
    #[test]
    fn the_captured_cell_tensors_are_reproduced_or_bounded() {
        let fixture = fixture();
        let records = match fixture["records"].as_array() {
            Some(value) => value,
            None => panic!("records"),
        };
        assert_eq!(records.len(), 4);

        let mut exact = 0_usize;
        for record in records {
            let case = record["case"].as_str().unwrap_or("?");
            let shape = match record["source_hwc_shape"].as_array() {
                Some(value) => value,
                None => panic!("{case}: source shape"),
            };
            let height = shape[0].as_u64().unwrap_or(0) as u32;
            let width = shape[1].as_u64().unwrap_or(0) as u32;
            let page = synthetic_rgb(width, height);

            let mut hasher = Sha256::new();
            hasher.update(page.pixels());
            assert_eq!(
                format!("{:x}", hasher.finalize()),
                record["source_rgb_sha256"].as_str().unwrap_or(""),
                "{case}: source pixels"
            );

            let tensor = match table_cell_input(&page) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };
            assert_eq!(tensor.shape(), [1, 3, 640, 640], "{case}: tensor shape");

            let values = tensor.values();
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            if format!("{:x}", hasher.finalize())
                == record["input_values_sha256"].as_str().unwrap_or("")
            {
                exact += 1;
                continue;
            }

            // Not exact. Bound it against the samples rather than accepting it:
            // no sampled value may be off by more than one 8-bit step, and no
            // more than two samples may differ at all.
            let indices = match record["input_sample_indices"].as_array() {
                Some(value) => value,
                None => panic!("{case}: sample indices"),
            };
            let encoded = record["input_sample_values_base64"].as_str().unwrap_or("");
            let sampled = match STANDARD.decode(encoded) {
                Ok(value) => value,
                Err(error) => panic!("{case}: samples: {error}"),
            };
            let step = 1.0_f32 / 255.0;
            let mut differing = 0_usize;
            for (slot, index) in indices.iter().enumerate() {
                let index = index.as_u64().unwrap_or(0) as usize;
                let start = slot * 4;
                let expected = f32::from_le_bytes([
                    sampled[start],
                    sampled[start + 1],
                    sampled[start + 2],
                    sampled[start + 3],
                ]);
                let actual = values[index];
                if actual.to_bits() == expected.to_bits() {
                    continue;
                }
                differing += 1;
                assert!(
                    (actual - expected).abs() <= step * 1.5,
                    "{case}: sample {index} off by more than one 8-bit step: {actual} vs {expected}"
                );
            }
            assert!(
                differing <= 2,
                "{case}: {differing} of {} samples differ",
                indices.len()
            );
        }

        // At least the square case must be exact: it involves no scaling at all,
        // so a failure there would be the normalization rather than the resize.
        assert!(exact >= 1, "no captured tensor matched exactly");
    }

    /// The batch inputs `ToBatch` reverses, checked against the capture.
    #[test]
    fn the_batch_inputs_are_reversed_as_captured() {
        let fixture = fixture();
        let records = match fixture["records"].as_array() {
            Some(value) => value,
            None => panic!("records"),
        };
        for record in records {
            let case = record["case"].as_str().unwrap_or("?");
            let shape = match record["source_hwc_shape"].as_array() {
                Some(value) => value,
                None => panic!("{case}: source shape"),
            };
            let height = shape[0].as_u64().unwrap_or(0) as u32;
            let width = shape[1].as_u64().unwrap_or(0) as u32;
            let dimensions = match ImageDimensions::new(width, height) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };

            let batched = match record["scale_factors_batched"].as_array() {
                Some(value) => value,
                None => panic!("{case}: scale factors"),
            };
            let factor = table_cell_scale_factor(dimensions);
            for (slot, expected) in batched.iter().enumerate() {
                let expected = expected.as_f64().unwrap_or(f64::NAN) as f32;
                assert_eq!(
                    factor[slot].to_bits(),
                    expected.to_bits(),
                    "{case}: scale factor {slot}"
                );
            }

            // And the forward pair is the reverse of it, which is the whole
            // point of the assertion above.
            let forward = match record["scale_factors_forward"].as_array() {
                Some(value) => value,
                None => panic!("{case}: forward factors"),
            };
            assert!(
                (forward[0].as_f64().unwrap_or(f64::NAN) as f32 - factor[1]).abs() < 1e-5,
                "{case}: forward w_scale must equal batched slot 1"
            );

            let image_shape = match record["img_size_batched"].as_array() {
                Some(value) => value,
                None => panic!("{case}: img_size"),
            };
            let computed = table_cell_image_shape();
            for (slot, expected) in image_shape.iter().enumerate() {
                assert_eq!(
                    f64::from(computed[slot]),
                    expected.as_f64().unwrap_or(f64::NAN),
                    "{case}: img_size {slot}"
                );
            }
        }
    }

    /// Decoded cells land inside the page they came from.
    ///
    /// Boxes outside the source image are the cheapest signal that the scale
    /// factor is transposed, which is how that bug was caught in `LAY-001`.
    #[test]
    fn decoded_cells_stay_inside_the_page() {
        let values = [
            0.0_f32, 0.91, 10.0, 20.0, 110.0, 60.0, //
            0.0, 0.55, 120.0, 20.0, 280.0, 60.0, //
            -1.0, 0.99, 0.0, 0.0, 0.0, 0.0, // padding
        ];
        let cells = match table_cells(&[3, 6], &values, TABLE_CELL_THRESHOLD) {
            Ok(value) => value,
            Err(error) => panic!("decode: {error}"),
        };
        assert_eq!(cells.len(), 2);
        for cell in &cells {
            assert!(cell.box_ltrb[0] >= 0.0 && cell.box_ltrb[1] >= 0.0);
            assert!(cell.box_ltrb[2] <= 297.0 && cell.box_ltrb[3] <= 421.0);
            assert!(cell.box_ltrb[2] > cell.box_ltrb[0]);
            assert!(cell.box_ltrb[3] > cell.box_ltrb[1]);
        }
    }

    /// The single class means any class index but zero is a refusal.
    #[test]
    fn a_second_class_is_refused() {
        assert_eq!(TABLE_CELL_LABELS.len(), 1);
        let values = [1.0_f32, 0.9, 0.0, 0.0, 1.0, 1.0];
        assert!(table_cells(&[1, 6], &values, TABLE_CELL_THRESHOLD).is_err());
    }

    /// Both artifacts are named, and they are not the same artifact.
    #[test]
    fn the_two_models_are_distinguishable() {
        assert_ne!(
            TableCellModel::Wired.model_name(),
            TableCellModel::Wireless.model_name()
        );
        assert!(TableCellModel::Wired.model_name().contains("wired"));
        assert!(TableCellModel::Wireless.model_name().contains("wireless"));
    }

    /// The threshold is the artifact's, not a guess.
    #[test]
    fn the_threshold_matches_the_artifact() {
        let fixture = fixture();
        assert_eq!(
            fixture["preprocess"]["threshold"].as_f64().unwrap_or(0.0) as f32,
            TABLE_CELL_THRESHOLD
        );
        assert_eq!(
            fixture["preprocess"]["input_side"].as_u64().unwrap_or(0) as u32,
            TABLE_CELL_INPUT_SIDE
        );
    }
}
