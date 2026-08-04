// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Wired-versus-wireless table classification, frozen from the PaddleX baseline.
//!
//! Roadmap item `TBLCLS-001`. The model is `PP-LCNet_x1_0_table_cls`, and its
//! preprocessing chain — resize by short edge, centre crop, ImageNet normalize —
//! is the same *shape* as the document orientation classifier's.
//!
//! It is not the same *behaviour*, and that is the whole reason this module
//! exists as its own file rather than a second call into
//! [`crate::document_orientation`].
//!
//! # Two operators with the same name and different results
//!
//! Document orientation was frozen from `deploy/cpp_infer/`. This model is
//! reached only through PaddleX, so its operators are the Python ones, and two
//! of them disagree with their C++ namesakes in ways that produce a working
//! implementation quietly fed the wrong pixels:
//!
//! | Operator | C++ baseline | PaddleX baseline |
//! |---|---|---|
//! | `ResizeByShort` | `std::round` — half **away from zero** | `round` — half **to even** |
//! | `Normalize` | `(x * scale - mean) / std` | `x * (scale/std) + (-mean/std)` |
//!
//! The rounding is reachable, not theoretical: a `512x1025` page scales by
//! exactly `0.5`, so its height lands on `512.5`. C++ gives `513`, Python gives
//! `512`, and every value in the tensor after that differs. The oracle captures
//! that case and its mirror — a `1024x1030` page, whose `257.5` rounds *up* to
//! `258` under the same rule — because a capture that only ever rounded down
//! would also pass with plain truncation.
//!
//! The normalization is the same arithmetic in a different order, which in `f32`
//! is not the same number. `crate::tensor::classic_normalized_batch` divides
//! last; PaddleX folds the division into a constant and multiplies. Reusing the
//! existing helper would have been off in the last bit rather than visibly
//! broken.
//!
//! # Channel order
//!
//! `ReadImage(format="RGB")` converts before the resize, so this model sees
//! **RGB** and the ImageNet constants apply in that order. Callers pass an
//! already-RGB image; this module does not reorder channels, because a silent
//! swap here would be indistinguishable from a correct result.
//!
//! # Not wired into a pipeline
//!
//! Table classification selects between two downstream structure models that
//! this port does not have yet. Exposing a classifier whose answer nothing can
//! act on would widen the public surface for no capability, so composition waits
//! for `P9` — the position `crate::layout` and `crate::unwarp` already take.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::resize::classic_linear_resize;
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// The shorter side's target length, from `inference.yml`.
pub(crate) const TABLE_CLS_RESIZE_SHORT: u32 = 256;

/// The centre crop's side, from `inference.yml`.
pub(crate) const TABLE_CLS_CROP_SIZE: u32 = 224;

/// The artifact's label list, in class order.
pub(crate) const TABLE_CLS_LABELS: [&str; 2] = ["wired_table", "wireless_table"];

/// The configured `topk`.
///
/// It exceeds the class count, which is not a mistake to correct: upstream slices
/// `[-topk:]` off a two-element axis, and NumPy clamps rather than failing. The
/// effective width is therefore `2`, and [`classify_table`] reproduces the clamp
/// instead of the literal.
pub(crate) const TABLE_CLS_TOPK: usize = 5;

/// `scale`, `mean`, and `std` exactly as `inference.yml` spells them.
const TABLE_CLS_SCALE: f64 = 0.003_921_568_627_450_98;
const TABLE_CLS_MEAN: [f64; 3] = [0.485, 0.456, 0.406];
const TABLE_CLS_STD: [f64; 3] = [0.229, 0.224, 0.225];

/// The decimal place `Topk` rounds scores to, via `np.around(..., decimals=5)`.
const TABLE_CLS_SCORE_DECIMALS: i32 = 5;

/// One class the classifier can return.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TableKind {
    /// Ruling lines are drawn between cells.
    Wired,
    /// Cells are separated by whitespace alone.
    Wireless,
}

impl TableKind {
    /// The upstream label string for this class.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wired => TABLE_CLS_LABELS[0],
            Self::Wireless => TABLE_CLS_LABELS[1],
        }
    }

    const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Wired),
            1 => Some(Self::Wireless),
            _ => None,
        }
    }
}

/// A ranked classification result.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TableClassification {
    /// The highest-scoring class.
    pub kind: TableKind,
    /// Its score, rounded the way upstream rounds it.
    pub score: f32,
}

/// Returns the dimensions PaddleX's `ResizeByShort` produces.
///
/// Split out from the resize so the rounding can be tested on its own, which is
/// the only place it is observable.
pub(crate) fn table_resize_by_short_dimensions(
    source: ImageDimensions,
    target_short_edge: u32,
) -> Result<ImageDimensions> {
    let (width, height) = (f64::from(source.width()), f64::from(source.height()));
    let shorter = width.min(height);
    if shorter <= 0.0 || target_short_edge == 0 {
        return Err(Error::InvalidInput {
            field: "table_classification.resize_short",
            violation: InputViolation::Empty,
        });
    }
    let scale = f64::from(target_short_edge) / shorter;
    // Python's `round`, which is half to even. `f64::round` would be half away
    // from zero and would differ on exactly the cases the oracle captures.
    let scaled_width = (width * scale).round_ties_even();
    let scaled_height = (height * scale).round_ties_even();
    if !scaled_width.is_finite()
        || !scaled_height.is_finite()
        || scaled_width < 1.0
        || scaled_height < 1.0
        || scaled_width > f64::from(u32::MAX)
        || scaled_height > f64::from(u32::MAX)
    {
        return Err(Error::InvalidInput {
            field: "table_classification.resize_short",
            violation: InputViolation::OutOfRange,
        });
    }
    ImageDimensions::new(scaled_width as u32, scaled_height as u32)
}

/// Resizes by short edge, skipping the call when the target already matches.
///
/// The skip is upstream's, not an optimization: `F.resize` returns the array
/// untouched when the size is unchanged, so a source whose short side is already
/// `256` never reaches `cv2.resize`.
fn table_resize_by_short(
    source: &InterleavedImage,
    target_short_edge: u32,
) -> Result<Option<InterleavedImage>> {
    let target = table_resize_by_short_dimensions(source.dimensions(), target_short_edge)?;
    if target == source.dimensions() {
        return Ok(None);
    }
    classic_linear_resize(source, target).map(Some)
}

/// Takes the centre crop, refusing rather than padding when the source is small.
///
/// `x1 = max(0, (w - cw) // 2)` with an explicit error below the crop size, so
/// the `max` never actually clamps. Padding instead would hand the model a
/// border its training data never had.
fn table_centre_crop(source: &InterleavedImage, size: u32) -> Result<InterleavedImage> {
    let dimensions = source.dimensions();
    let (width, height) = (dimensions.width(), dimensions.height());
    if width < size || height < size {
        return Err(Error::InvalidInput {
            field: "table_classification.crop_size",
            violation: InputViolation::OutOfRange,
        });
    }
    let x1 = (width - size) / 2;
    let y1 = (height - size) / 2;

    let channels = usize::from(source.channels());
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

/// Builds the classifier's input tensor from one RGB page.
pub(crate) fn table_classification_input(page: &InterleavedImage) -> Result<NchwTensor> {
    if page.channels() != 3 {
        return Err(Error::InvalidInput {
            field: "table_classification.channels",
            violation: InputViolation::OutOfRange,
        });
    }

    let resized = table_resize_by_short(page, TABLE_CLS_RESIZE_SHORT)?;
    let resized = resized.as_ref().unwrap_or(page);
    let cropped = table_centre_crop(resized, TABLE_CLS_CROP_SIZE)?;

    let side = TABLE_CLS_CROP_SIZE as usize;
    let channels = 3_usize;
    let pixels = cropped.pixels();
    let mut values = Vec::with_capacity(channels * side * side);
    for channel in 0..channels {
        // `alpha` and `beta` are folded in `f64` and applied in `f32`, matching
        // `Normalize.__init__` followed by NumPy's weak-scalar promotion.
        let alpha = (TABLE_CLS_SCALE / TABLE_CLS_STD[channel]) as f32;
        let beta = (-TABLE_CLS_MEAN[channel] / TABLE_CLS_STD[channel]) as f32;
        for row in 0..side {
            for column in 0..side {
                let source = (row * side + column) * channels + channel;
                values.push(f32::from(pixels[source]) * alpha + beta);
            }
        }
    }

    NchwTensor::new(1, channels, side, side, values)
}

/// Rounds to five decimals the way `np.around` does.
///
/// Scale, round half to even, unscale — all in `f32`, because NumPy rounds a
/// `float32` array in `float32` and the intermediate is where the tie is decided.
fn around_five(value: f32) -> f32 {
    let factor = 10_f32.powi(TABLE_CLS_SCORE_DECIMALS);
    (value * factor).round_ties_even() / factor
}

/// Ranks the model's scores the way `Topk` ranks them.
///
/// Upstream sorts **ascending**, slices the last `topk`, then reverses. On a tie
/// that leaves the **higher** class index first, which is the opposite of what a
/// descending sort would give, and is captured rather than assumed.
pub(crate) fn rank_table_scores(scores: &[f32]) -> Vec<(usize, f32)> {
    let mut ranked: Vec<(usize, f32)> = scores.iter().copied().enumerate().collect();
    // A stable ascending sort, so equal scores keep their index order and the
    // reversal below puts the higher index first.
    ranked.sort_by(|left, right| left.1.total_cmp(&right.1));
    let keep = ranked.len().min(TABLE_CLS_TOPK);
    let start = ranked.len() - keep;
    let mut top: Vec<(usize, f32)> = ranked[start..]
        .iter()
        .map(|&(index, score)| (index, around_five(score)))
        .collect();
    top.reverse();
    top
}

/// Classifies a page from the model's raw output scores.
pub(crate) fn classify_table(scores: &[f32]) -> Result<TableClassification> {
    if scores.len() != TABLE_CLS_LABELS.len() {
        return Err(Error::InvalidInput {
            field: "table_classification.scores",
            violation: InputViolation::OutOfRange,
        });
    }
    let ranked = rank_table_scores(scores);
    let &(index, score) = ranked.first().ok_or(Error::InvalidInput {
        field: "table_classification.scores",
        violation: InputViolation::Empty,
    })?;
    let kind = TableKind::from_index(index).ok_or(Error::InvalidInput {
        field: "table_classification.class_index",
        violation: InputViolation::OutOfRange,
    })?;
    Ok(TableClassification { kind, score })
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-table-classification/expected.json");

    /// The capture's synthetic image, by the same closed form.
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

    /// The rounding boundary, checked without building a tensor.
    ///
    /// This is the disagreement with the C++ baseline, so it is asserted
    /// directly as well as through the tensors: a failure here names the cause,
    /// where a tensor mismatch only reports a symptom.
    #[test]
    fn the_short_edge_resize_rounds_half_to_even() {
        let cases = [
            // 1025 * (256/512) is exactly 512.5, and 512 is the even neighbour.
            ((512_u32, 1025_u32), (256_u32, 512_u32)),
            // 1030 * 0.25 is exactly 257.5, and 258 is the even neighbour, so
            // the same rule rounds this one up.
            ((1024, 1030), (256, 258)),
            // Nothing on a boundary, to show the ordinary case is unaffected.
            ((297, 421), (256, 363)),
        ];
        for ((width, height), (expected_width, expected_height)) in cases {
            let source = match ImageDimensions::new(width, height) {
                Ok(value) => value,
                Err(error) => panic!("dimensions: {error}"),
            };
            let resized = match table_resize_by_short_dimensions(source, TABLE_CLS_RESIZE_SHORT) {
                Ok(value) => value,
                Err(error) => panic!("resize: {error}"),
            };
            assert_eq!(
                (resized.width(), resized.height()),
                (expected_width, expected_height),
                "{width}x{height}"
            );
        }
    }

    /// Half away from zero — the C++ rule — would give a different answer.
    ///
    /// Recorded as its own assertion so the divergence between the two pinned
    /// baselines cannot quietly stop being true.
    #[test]
    fn the_two_baselines_disagree_on_the_boundary() {
        let scale = 256.0_f64 / 512.0;
        let height = 1025.0_f64 * scale;
        assert!((height - 512.5).abs() < f64::EPSILON);
        assert_eq!(height.round_ties_even(), 512.0, "PaddleX");
        assert_eq!(height.round(), 513.0, "the C++ baseline");
    }

    /// Every captured tensor, hashed whole and sampled.
    #[test]
    fn the_captured_table_tensors_are_reproduced() {
        let fixture = fixture();
        let records = match fixture["records"].as_array() {
            Some(value) => value,
            None => panic!("records"),
        };
        assert_eq!(records.len(), 6);

        for record in records {
            let case = record["case"].as_str().unwrap_or("?");
            let shape = match record["source_hwc_shape"].as_array() {
                Some(value) => value,
                None => panic!("{case}: source shape"),
            };
            let height = shape[0].as_u64().unwrap_or(0) as u32;
            let width = shape[1].as_u64().unwrap_or(0) as u32;

            let page = synthetic_rgb(width, height);

            // The source pixels must match before the tensor can mean anything.
            let mut hasher = Sha256::new();
            hasher.update(page.pixels());
            let digest = format!("{:x}", hasher.finalize());
            assert_eq!(
                digest,
                record["source_rgb_sha256"].as_str().unwrap_or(""),
                "{case}: source pixels"
            );

            // The intermediate shape, so a resize failure is distinguishable
            // from a crop or normalize failure.
            let resized =
                match table_resize_by_short_dimensions(page.dimensions(), TABLE_CLS_RESIZE_SHORT) {
                    Ok(value) => value,
                    Err(error) => panic!("{case}: {error}"),
                };
            let expected_resized = match record["resized_hwc_shape"].as_array() {
                Some(value) => value,
                None => panic!("{case}: resized shape"),
            };
            assert_eq!(
                (resized.height() as u64, resized.width() as u64),
                (
                    expected_resized[0].as_u64().unwrap_or(0),
                    expected_resized[1].as_u64().unwrap_or(0)
                ),
                "{case}: resized shape"
            );

            let tensor = match table_classification_input(&page) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };
            assert_eq!(tensor.shape(), [1, 3, 224, 224], "{case}: tensor shape");

            let values = tensor.values();
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = format!("{:x}", hasher.finalize());

            let expected_digest = record["input_values_sha256"].as_str().unwrap_or("");
            if digest == expected_digest {
                continue;
            }

            // The hash is the claim; the samples are what makes a failure
            // readable rather than a single mismatched hex string.
            let indices = match record["input_sample_indices"].as_array() {
                Some(value) => value,
                None => panic!("{case}: sample indices"),
            };
            let encoded = record["input_sample_values_base64"].as_str().unwrap_or("");
            let sampled = match STANDARD.decode(encoded) {
                Ok(value) => value,
                Err(error) => panic!("{case}: samples: {error}"),
            };
            let mut differing = 0_usize;
            let mut first: Option<(usize, f32, f32)> = None;
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
                if actual.to_bits() != expected.to_bits() {
                    differing += 1;
                    first.get_or_insert((index, expected, actual));
                }
            }
            panic!(
                "{case}: tensor differs; {differing} of {} samples differ, first {first:?}",
                indices.len()
            );
        }
    }

    /// The `Topk` postprocess, including the tie and the fifth-decimal rounding.
    #[test]
    fn the_captured_topk_ranking_is_reproduced() {
        let fixture = fixture();
        let cases = match fixture["postprocess"].as_array() {
            Some(value) => value,
            None => panic!("postprocess"),
        };
        assert_eq!(cases.len(), 4);

        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let logits: Vec<f32> = match case["logits"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                    .collect(),
                None => panic!("{name}: logits"),
            };
            let ranked = rank_table_scores(&logits);

            let expected_indexes = match case["indexes"].as_array() {
                Some(value) => value,
                None => panic!("{name}: indexes"),
            };
            let expected_scores = match case["scores"].as_array() {
                Some(value) => value,
                None => panic!("{name}: scores"),
            };
            assert_eq!(ranked.len(), expected_indexes.len(), "{name}: rank width");

            for (slot, &(index, score)) in ranked.iter().enumerate() {
                assert_eq!(
                    index as u64,
                    expected_indexes[slot].as_u64().unwrap_or(u64::MAX),
                    "{name}: index {slot}"
                );
                let expected = expected_scores[slot].as_f64().unwrap_or(f64::NAN) as f32;
                assert_eq!(
                    score.to_bits(),
                    expected.to_bits(),
                    "{name}: score {slot}: {score} vs {expected}"
                );
            }

            let classified = match classify_table(&logits) {
                Ok(value) => value,
                Err(error) => panic!("{name}: {error}"),
            };
            let expected_label = match case["labels"].as_array() {
                Some(values) => values[0].as_str().unwrap_or(""),
                None => panic!("{name}: labels"),
            };
            assert_eq!(classified.kind.label(), expected_label, "{name}: label");
        }
    }

    /// `topk: 5` against two classes is clamped, not an error.
    #[test]
    fn the_configured_topk_exceeds_the_class_count_and_is_clamped() {
        assert!(TABLE_CLS_TOPK > TABLE_CLS_LABELS.len());
        assert_eq!(rank_table_scores(&[0.1, 0.9]).len(), TABLE_CLS_LABELS.len());
    }

    /// A page smaller than the crop is refused rather than padded.
    #[test]
    fn a_page_smaller_than_the_crop_is_refused() {
        // 100x100 scales up to 256x256, so the crop succeeds; the refusal has to
        // be provoked at the crop itself.
        let small = synthetic_rgb(64, 64);
        match table_centre_crop(&small, TABLE_CLS_CROP_SIZE) {
            Err(Error::InvalidInput { field, .. }) => {
                assert_eq!(field, "table_classification.crop_size");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Anything but three channels is refused before any arithmetic runs.
    #[test]
    fn a_non_rgb_page_is_refused() {
        let dimensions = match ImageDimensions::new(300, 300) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let grey = match InterleavedImage::new(dimensions, 1, vec![9_u8; 300 * 300]) {
            Ok(value) => value,
            Err(error) => panic!("image: {error}"),
        };
        match table_classification_input(&grey) {
            Err(Error::InvalidInput { field, .. }) => {
                assert_eq!(field, "table_classification.channels");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// A score vector of the wrong width is refused rather than truncated.
    #[test]
    fn a_score_vector_of_the_wrong_width_is_refused() {
        assert!(classify_table(&[0.5]).is_err());
        assert!(classify_table(&[0.2, 0.3, 0.5]).is_err());
    }

    /// The labels are the artifact's, in the artifact's order.
    #[test]
    fn the_labels_match_the_artifact() {
        assert_eq!(TableKind::Wired.label(), "wired_table");
        assert_eq!(TableKind::Wireless.label(), "wireless_table");
        assert_eq!(TABLE_CLS_LABELS, ["wired_table", "wireless_table"]);
    }
}
