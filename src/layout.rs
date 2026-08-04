// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Document layout detection.
//!
//! Roadmap item `LAY-001`, implementing the contract frozen in
//! `docs/LAYOUT_CONTRACT.md` for `PP-DocLayout_plus-L` — the first capability
//! ported from the PaddleX baseline `D-013` pinned.
//!
//! # Four reversals, none of them in the artifact config
//!
//! This model's preprocessing is config-driven, and the config is misleading in
//! four separate places. Each was read from `paddlex 3.7.2`:
//!
//! - `interp: 2` is **BICUBIC**, not the linear interpolation every other
//!   resize in this project uses;
//! - `norm_type: none` does **not** disable normalization — `build_normalize`
//!   rewrites `"none"` to `"mean_std"`, making the branch that would zero the
//!   mean and deviation unreachable, so the transform is `x / 255`;
//! - `target_size` is **reversed** before use, unobservable at `800x800`;
//! - `ToBatch` **reverses** `img_size` and `scale_factors` again, so the model
//!   receives `[h, w]` and `[h_scale, w_scale]` where `Resize` computed
//!   `[w, h]` and `[w_scale, h_scale]`.
//!
//! The last one is the one a capture catches rather than a reading: passing the
//! unreversed scale factor produced boxes reaching `y = 1021` on a `720`-tall
//! page. Coordinates outside the source are the cheapest possible signal that a
//! transform is wrong, and they are worth checking for that reason alone.

//! # Why nothing calls this yet
//!
//! `LAY-001` is not wired into any pipeline: composing layout with the classic
//! path is `P9`'s subject, and the cubic resize this depends on has a recorded
//! residual divergence from OpenCV. Exposing a layout API built on an operator
//! that is knowingly one step off at page scale would be selling a precision
//! this port does not have.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::resize_cubic::classic_cubic_resize;
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// The artifact's fixed input side.
pub(crate) const LAYOUT_INPUT_SIDE: u32 = 800;

/// The frozen score threshold from PaddleX's detection postprocess.
pub(crate) const LAYOUT_THRESHOLD: f32 = 0.5;

/// The artifact's twenty classes, in class-index order.
pub(crate) const LAYOUT_LABELS: [&str; 20] = [
    "paragraph_title",
    "image",
    "text",
    "number",
    "abstract",
    "content",
    "figure_title",
    "formula",
    "table",
    "reference",
    "doc_title",
    "footnote",
    "header",
    "algorithm",
    "footer",
    "seal",
    "chart",
    "formula_number",
    "aside_text",
    "reference_content",
];

/// One detected layout region, in the source page's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutRegion {
    /// The class index into [`LAYOUT_LABELS`].
    pub(crate) class: usize,
    /// The detection score.
    pub(crate) score: f32,
    /// `[left, top, right, bottom]` in source-page pixels.
    pub(crate) box_ltrb: [f32; 4],
}

impl LayoutRegion {
    /// The class's label.
    pub(crate) fn label(&self) -> &'static str {
        LAYOUT_LABELS[self.class]
    }
}

/// Builds the `[1, 3, 800, 800]` layout input for one page.
pub(crate) fn layout_input(page: &InterleavedImage) -> Result<NchwTensor> {
    if page.channels() != 3 {
        return Err(Error::InvalidInput {
            field: "layout.channels",
            violation: InputViolation::OutOfRange,
        });
    }
    let target = ImageDimensions::new(LAYOUT_INPUT_SIDE, LAYOUT_INPUT_SIDE)?;
    let resized = classic_cubic_resize(page, target)?;

    let side = LAYOUT_INPUT_SIDE as usize;
    let mut values: Vec<f32> = Vec::new();
    values
        .try_reserve_exact(side * side * 3)
        .map_err(|_| Error::Backend {
            message: "layout input allocation failed",
        })?;
    let pixels = resized.pixels();
    for channel in 0..3_usize {
        for row in 0..side {
            for column in 0..side {
                let index = (row * side + column) * 3 + channel;
                values.push(f32::from(pixels[index]) / 255.0);
            }
        }
    }
    NchwTensor::new(1, 3, side, side, values)
}

/// Returns the `scale_factor` tensor the model expects, `[h_scale, w_scale]`.
///
/// Reversed relative to how `Resize` computes it, because `ToBatch` reverses it
/// on the way in. Getting this backwards does not error: it produces boxes
/// outside the page, which is why the oracle checks containment.
pub(crate) fn layout_scale_factor(page: ImageDimensions) -> [f32; 2] {
    let side = LAYOUT_INPUT_SIDE as f32;
    [side / page.height() as f32, side / page.width() as f32]
}

/// Decodes the model's `[N, 6]` detections into source-page regions.
///
/// Each row is `[class, score, left, top, right, bottom]`, already in source
/// coordinates because the model divides by the supplied scale factor itself.
/// Rows below the threshold are dropped, as PaddleX's postprocess does.
pub(crate) fn layout_regions(
    shape: &[usize],
    values: &[f32],
    threshold: f32,
) -> Result<Vec<LayoutRegion>> {
    if shape.len() != 2 || shape[1] != 6 {
        return Err(Error::InvalidInput {
            field: "layout.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }
    if values.len() != shape[0] * 6 {
        return Err(Error::InvalidInput {
            field: "layout.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut regions = Vec::new();
    for row in 0..shape[0] {
        let entry = &values[row * 6..(row + 1) * 6];
        if !entry.iter().all(|value| value.is_finite()) {
            return Err(Error::InvalidInput {
                field: "layout.detections",
                violation: InputViolation::NonFinite,
            });
        }
        // A negative class is the model's own padding for an unused slot, which
        // it emits to keep the output shape fixed at 300 rows.
        if entry[0] < 0.0 || entry[1] < threshold {
            continue;
        }
        let class = entry[0] as usize;
        if class >= LAYOUT_LABELS.len() {
            return Err(Error::InvalidInput {
                field: "layout.class_index",
                violation: InputViolation::OutOfRange,
            });
        }
        regions.push(LayoutRegion {
            class,
            score: entry[1],
            box_ltrb: [entry[2], entry[3], entry[4], entry[5]],
        });
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-layout/expected.json");
    const BENCHMARK_PAGE: &[u8] =
        include_bytes!("../tests/fixtures/classic-v1-benchmark-page/input.png");

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

    fn decode_f32(encoded: &str) -> Vec<f32> {
        let bytes = match STANDARD.decode(encoded) {
            Ok(bytes) => bytes,
            Err(error) => panic!("base64: {error}"),
        };
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    fn synthetic(index: usize, width: u32, height: u32) -> InterleavedImage {
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
            Err(error) => panic!("image: {error}"),
        }
    }

    fn source_for(case: &str) -> InterleavedImage {
        if case == "benchmark-page" {
            let encoded = match crate::types::EncodedImage::new(BENCHMARK_PAGE) {
                Ok(value) => value,
                Err(error) => panic!("encoded: {error}"),
            };
            return match crate::image::decode_classic_bgr(encoded) {
                Ok(value) => value,
                Err(error) => panic!("decode: {error}"),
            };
        }
        let index: usize = case
            .trim_start_matches("synthetic-")
            .parse()
            .unwrap_or_default();
        let sizes = [(400_u32, 300_u32), (297, 421)];
        let (width, height) = sizes[index];
        synthetic(index, width, height)
    }

    #[test]
    fn the_captured_layout_tensors_are_reproduced() {
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
            let source = source_for(case);
            assert_eq!(
                sha256_hex(source.pixels()),
                record["source_bgr_sha256"].as_str().unwrap_or_default(),
                "{case}: source bytes"
            );

            let tensor = match layout_input(&source) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };
            assert_eq!(tensor.shape(), [1, 3, 800, 800], "{case}: shape");

            let exact = record["reproduced_exactly"].as_bool().unwrap_or(false);
            let mut bytes = Vec::with_capacity(tensor.values().len() * 4);
            for value in tensor.values() {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let digest = sha256_hex(&bytes);
            let expected_digest = record["input_values_sha256"].as_str().unwrap_or_default();

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

            if exact {
                assert_eq!(
                    digest, expected_digest,
                    "{case}: input tensor differs from the capture"
                );
                for (index, expected) in indices.iter().zip(&samples) {
                    assert_eq!(
                        tensor.values()[*index].to_bits(),
                        expected.to_bits(),
                        "{case}: element {index}"
                    );
                }
            } else {
                // A case the cubic resize does not yet reproduce exactly. The
                // divergence is bounded rather than ignored: no sampled value
                // may be off by more than one 8-bit step, and the number of
                // differing samples may not grow.
                assert_ne!(
                    digest, expected_digest,
                    "{case} now matches exactly; set reproduced_exactly and drop this branch \
                     rather than leaving a lenient path in place"
                );
                let mut differing = 0;
                for (index, expected) in indices.iter().zip(&samples) {
                    let actual = tensor.values()[*index];
                    if actual.to_bits() != expected.to_bits() {
                        differing += 1;
                        assert!(
                            (actual - expected).abs() <= 1.0 / 255.0 + 1e-6,
                            "{case}: element {index} differs by more than one step"
                        );
                    }
                }
                assert!(
                    differing <= 2,
                    "{case}: {differing} sampled values differ, recorded bound is 2"
                );
            }
        }
    }

    /// The scale factor is `[h_scale, w_scale]`, reversed by `ToBatch`.
    #[test]
    fn the_scale_factor_is_height_first() {
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
            let source = source_for(case);
            let factor = layout_scale_factor(source.dimensions());
            let expected = match record["scale_factor_hw"].as_array() {
                Some(values) => [
                    values[0].as_f64().unwrap_or_default() as f32,
                    values[1].as_f64().unwrap_or_default() as f32,
                ],
                None => panic!("{case}: no scale factor"),
            };
            assert_eq!(factor, expected, "{case}");
        }
        // A non-square page is the only case where the reversal is observable.
        let landscape = synthetic(0, 400, 300);
        let factor = layout_scale_factor(landscape.dimensions());
        assert!(
            factor[0] > factor[1],
            "a landscape page scales more vertically: {factor:?}"
        );
    }

    /// Decoding the captured detections reproduces the recorded regions, and
    /// every box lies inside the page.
    ///
    /// Containment is the assertion that catches a reversed scale factor: an
    /// unreversed one produced boxes reaching `y = 1021` on a `720`-tall page.
    #[test]
    fn the_captured_detections_decode_inside_the_page() {
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
            let shape: Vec<usize> = match record["boxes_shape"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no boxes shape"),
            };
            let boxes = decode_f32(record["boxes_base64"].as_str().unwrap_or_default());
            let regions = match layout_regions(&shape, &boxes, LAYOUT_THRESHOLD) {
                Ok(regions) => regions,
                Err(error) => panic!("{case}: {error}"),
            };

            let expected = match record["kept_at_0_5"].as_array() {
                Some(values) => values,
                None => panic!("{case}: no kept list"),
            };
            assert_eq!(regions.len(), expected.len(), "{case}: kept count");

            let (width, height) = match record["source_wh"].as_array() {
                Some(values) => (
                    values[0].as_f64().unwrap_or_default() as f32,
                    values[1].as_f64().unwrap_or_default() as f32,
                ),
                None => panic!("{case}: no source size"),
            };
            for (region, want) in regions.iter().zip(expected) {
                assert_eq!(
                    region.label(),
                    want["label"].as_str().unwrap_or_default(),
                    "{case}: label"
                );
                assert!(
                    (f64::from(region.score) - want["score"].as_f64().unwrap_or_default()).abs()
                        < 1e-5,
                    "{case}: score"
                );
                for (index, value) in region.box_ltrb.iter().enumerate() {
                    let limit = if index % 2 == 0 { width } else { height };
                    assert!(
                        *value >= -1.0 && *value <= limit + 1.0,
                        "{case}: coordinate {index} = {value} outside 0..{limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn padding_rows_and_low_scores_are_dropped() {
        // Class -1 is the model's padding for an unused slot; 0.4 is below the
        // threshold; the third row survives.
        let values = vec![
            -1.0, 0.99, 0.0, 0.0, 1.0, 1.0, //
            2.0, 0.4, 0.0, 0.0, 1.0, 1.0, //
            2.0, 0.6, 1.0, 2.0, 3.0, 4.0,
        ];
        let regions = match layout_regions(&[3, 6], &values, LAYOUT_THRESHOLD) {
            Ok(regions) => regions,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].label(), "text");
        assert_eq!(regions[0].box_ltrb, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn an_out_of_range_class_is_a_typed_error() {
        let values = vec![99.0, 0.9, 0.0, 0.0, 1.0, 1.0];
        assert!(matches!(
            layout_regions(&[1, 6], &values, LAYOUT_THRESHOLD),
            Err(Error::InvalidInput {
                field: "layout.class_index",
                ..
            })
        ));
    }

    #[test]
    fn there_are_twenty_labels() {
        assert_eq!(LAYOUT_LABELS.len(), 20);
        assert_eq!(LAYOUT_LABELS[0], "paragraph_title");
        assert_eq!(LAYOUT_LABELS[19], "reference_content");
    }
}
