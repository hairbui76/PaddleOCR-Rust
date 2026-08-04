// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Text-line orientation classification.
//!
//! Roadmap item `DOCORI-001`. This implements the contract frozen in
//! `docs/ORIENTATION_CONTRACT.md` for `PP-LCNet_x1_0_textline_ori`, the model
//! `deploy/cpp_infer/src/configs/OCR.yaml` names at the pinned baseline.
//!
//! # Why this is not `predict_cls.py`
//!
//! The Python `TextClassifier` hard-codes `3, 48, 192`, aspect-preserving resize
//! with zero padding, `(x / 255 - 0.5) / 0.5`, and labels `["0", "180"]`. Those
//! belong to the legacy `ch_ppocr_mobile_v2.0_cls` model. The pinned artifact
//! declares `3, 80, 160`, an unconditional resize, the ImageNet normalization
//! the detector uses, and labels `["0_degree", "180_degree"]`. Implementing the
//! Python path would preprocess correctly for a model this baseline does not
//! select — which is the failure that produces plausible wrong answers rather
//! than errors.
//!
//! # The two boundary rules
//!
//! Both are upstream's and both are easy to invert:
//!
//! - the rotation test is `score > threshold`, **strict**, so a score exactly
//!   equal to the threshold does *not* rotate — the opposite convention from the
//!   detector's box score, where equality is retained;
//! - the label test is a **substring** match, not equality, which is
//!   load-bearing rather than sloppy: real label lists carry a `_degree` suffix,
//!   so comparing against `"180"` for equality would never fire.

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, run_validated};
use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::tensor::{ORIENTATION_INPUT_HEIGHT, ORIENTATION_INPUT_WIDTH, classic_orientation_batch};

/// The largest batch the artifact's declared dynamic shapes permit.
pub(crate) const ORIENTATION_MAX_BATCH: usize = 8;

/// The frozen rotation threshold from `utility.py:init_args`.
///
/// Without the `onnxruntime` feature nothing constructs pipeline thresholds, so
/// this is unreachable in that build. It stays defined rather than moving behind
/// the feature, because the value is a property of the upstream contract and not
/// of whether a backend happens to be compiled in — the same reasoning
/// `RunControl::begin` records.
#[cfg_attr(not(feature = "onnxruntime"), allow(dead_code))]
pub(crate) const ORIENTATION_THRESHOLD: f64 = 0.9;

/// The artifact's label list, in class order.
pub(crate) const ORIENTATION_LABELS: [&str; 2] = ["0_degree", "180_degree"];

/// One crop's classification.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OrientationVerdict {
    /// The winning class index.
    pub(crate) class: usize,
    /// The winning class's probability, as the model reported it.
    pub(crate) score: f64,
    /// Whether the crop should be rotated by 180 degrees.
    pub(crate) rotate: bool,
}

/// Returns whether a label and score call for a 180-degree rotation.
///
/// Kept separate from the decode so the two boundary rules can be tested
/// directly at their boundaries, which is where they are wrong or right.
pub(crate) fn rotates(label: &str, score: f64, threshold: f64) -> bool {
    label.contains("180") && score > threshold
}

/// Picks the winning class from one row of probabilities.
///
/// Ties resolve to the lowest index, matching NumPy's `argmax`, which is the
/// same rule the CTC decoder follows.
fn argmax(row: &[f32]) -> Result<(usize, f64)> {
    let mut best = 0_usize;
    let mut best_value = f32::NEG_INFINITY;
    for (index, value) in row.iter().enumerate() {
        if !value.is_finite() {
            return Err(Error::InvalidInput {
                field: "orientation.scores",
                violation: InputViolation::NonFinite,
            });
        }
        if *value > best_value {
            best_value = *value;
            best = index;
        }
    }
    Ok((best, f64::from(best_value)))
}

/// Classifies crops that are already resized to the fixed classifier input.
///
/// Crops are processed in the artifact's declared maximum batch. Unlike
/// recognition there is no aspect sort: every crop is resized to the same fixed
/// shape, so sorting would change nothing about the tensor and only reorder the
/// work.
pub(crate) fn classify(
    backend: &dyn InferenceBackend,
    contract: &ModelContract,
    crops: &[&InterleavedImage],
    threshold: f64,
) -> Result<Vec<OrientationVerdict>> {
    if crops.is_empty() {
        return Ok(Vec::new());
    }
    if !threshold.is_finite() {
        return Err(Error::InvalidInput {
            field: "orientation.threshold",
            violation: InputViolation::NonFinite,
        });
    }

    let mut verdicts = Vec::with_capacity(crops.len());
    for chunk in crops.chunks(ORIENTATION_MAX_BATCH) {
        let tensor = classic_orientation_batch(chunk)?;
        let input = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
        let output = run_validated(backend, contract, &input)?;

        let shape = output.shape();
        if shape.len() != 2 || shape[0] != chunk.len() || shape[1] != ORIENTATION_LABELS.len() {
            return Err(Error::InvalidInput {
                field: "orientation.output_shape",
                violation: InputViolation::OutOfRange,
            });
        }
        let classes = shape[1];
        for row in 0..chunk.len() {
            let scores = &output.values()[row * classes..(row + 1) * classes];
            let (class, score) = argmax(scores)?;
            let label = ORIENTATION_LABELS[class];
            verdicts.push(OrientationVerdict {
                class,
                score,
                rotate: rotates(label, score, threshold),
            });
        }
    }
    Ok(verdicts)
}

/// Rotates a crop by 180 degrees.
///
/// `cv2.rotate(img, cv2.ROTATE_180)` reverses both axes, which for an
/// interleaved buffer is exactly reversing the pixel sequence while keeping each
/// pixel's channels in order. No resampling and no interpolation: a 180-degree
/// rotation maps every pixel onto an existing pixel centre, which is why
/// upstream uses `rotate` here rather than the `warpAffine` its document-level
/// rotation needs.
pub(crate) fn rotate_180(crop: &InterleavedImage) -> Result<InterleavedImage> {
    let channels = crop.channels() as usize;
    let pixels = crop.pixels();
    let mut rotated = Vec::with_capacity(pixels.len());
    for pixel in (0..pixels.len() / channels).rev() {
        rotated.extend_from_slice(&pixels[pixel * channels..(pixel + 1) * channels]);
    }
    InterleavedImage::new(crop.dimensions(), crop.channels(), rotated)
}

/// Returns the fixed dimensions every crop must be resized to first.
#[must_use]
pub(crate) const fn orientation_input_size() -> (u32, u32) {
    (ORIENTATION_INPUT_WIDTH, ORIENTATION_INPUT_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};
    use crate::types::ImageDimensions;

    /// A backend that returns a fixed probability row per batch item.
    struct FakeClassifier {
        rows: Vec<[f32; 2]>,
    }

    impl InferenceBackend for FakeClassifier {
        fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
            let shape = input.shape();
            assert_eq!(shape.len(), 4, "classifier input must be NCHW");
            assert_eq!(shape[1], 3);
            assert_eq!(shape[2], 80, "the artifact's fixed height");
            assert_eq!(shape[3], 160, "the artifact's fixed width");
            let batch = shape[0];
            let mut values = Vec::with_capacity(batch * 2);
            for row in 0..batch {
                values.extend_from_slice(&self.rows[row]);
            }
            Ok((
                "fetch_name_0".to_owned(),
                BackendTensor::new(vec![batch, 2], values)?,
            ))
        }
    }

    fn contract() -> ModelContract {
        let artifact = match ModelArtifact::new("/nonexistent/cls.onnx", "0".repeat(64)) {
            Ok(value) => value,
            Err(error) => panic!("artifact: {error}"),
        };
        let batch = AxisExtent::Bounded {
            minimum: 1,
            maximum: ORIENTATION_MAX_BATCH,
        };
        let input = match TensorContract::new(
            "x",
            vec![
                batch,
                AxisExtent::Fixed(3),
                AxisExtent::Fixed(80),
                AxisExtent::Fixed(160),
            ],
        ) {
            Ok(value) => value,
            Err(error) => panic!("input contract: {error}"),
        };
        let output = match TensorContract::new("fetch_name_0", vec![batch, AxisExtent::Fixed(2)]) {
            Ok(value) => value,
            Err(error) => panic!("output contract: {error}"),
        };
        let budget = match RunBudget::new(40_000_000, 40_000_000, ORIENTATION_MAX_BATCH) {
            Ok(value) => value,
            Err(error) => panic!("budget: {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn crop() -> InterleavedImage {
        let dimensions = match ImageDimensions::new(160, 80) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, 3, vec![128_u8; 160 * 80 * 3]) {
            Ok(value) => value,
            Err(error) => panic!("crop: {error}"),
        }
    }

    /// The strict boundary: exactly the threshold does **not** rotate.
    ///
    /// This is the opposite convention from the detector's box score, and it is
    /// the single most likely detail to be inverted by someone porting both.
    #[test]
    fn the_rotation_threshold_is_strict() {
        assert!(!rotates("180_degree", 0.9, 0.9), "equality must not rotate");
        assert!(rotates("180_degree", 0.9000001, 0.9));
        assert!(!rotates("180_degree", 0.8999999, 0.9));
    }

    /// The substring test is load-bearing, not sloppy.
    #[test]
    fn the_label_test_matches_a_substring_not_an_equality() {
        assert!(
            rotates("180_degree", 1.0, 0.9),
            "the real label carries a suffix, so equality against \"180\" would never fire"
        );
        assert!(!rotates("0_degree", 1.0, 0.9));
        // The documented consequence: a label list containing "1800" would also
        // rotate. Recorded as behaviour rather than defended as desirable.
        assert!(rotates("1800_degree", 1.0, 0.9));
    }

    #[test]
    fn an_upside_down_crop_is_marked_for_rotation() {
        let single = crop();
        let backend = FakeClassifier {
            rows: vec![[0.05, 0.95]],
        };
        let verdicts = match classify(&backend, &contract(), &[&single], ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].class, 1);
        assert!(verdicts[0].rotate);
        assert!((verdicts[0].score - 0.95).abs() < 1e-6);
    }

    #[test]
    fn an_upright_crop_is_left_alone() {
        let single = crop();
        let backend = FakeClassifier {
            rows: vec![[0.99, 0.01]],
        };
        let verdicts = match classify(&backend, &contract(), &[&single], ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert_eq!(verdicts[0].class, 0);
        assert!(!verdicts[0].rotate);
    }

    /// A confident-looking upside-down class below the threshold does not rotate.
    ///
    /// This is the case that separates "the model thinks it is upside down" from
    /// "we act on it", and upstream deliberately keeps them apart.
    #[test]
    fn a_low_confidence_upside_down_class_does_not_rotate() {
        let single = crop();
        let backend = FakeClassifier {
            rows: vec![[0.45, 0.55]],
        };
        let verdicts = match classify(&backend, &contract(), &[&single], ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert_eq!(verdicts[0].class, 1, "the class is still 180");
        assert!(!verdicts[0].rotate, "but 0.55 does not clear 0.9");
    }

    #[test]
    fn an_exact_tie_resolves_to_the_lowest_class() {
        let single = crop();
        let backend = FakeClassifier {
            rows: vec![[0.5, 0.5]],
        };
        let verdicts = match classify(&backend, &contract(), &[&single], ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert_eq!(verdicts[0].class, 0, "argmax keeps the first maximum");
    }

    /// More crops than the artifact's declared batch are chunked, not rejected.
    #[test]
    fn crops_beyond_the_declared_batch_are_chunked() {
        let owned: Vec<InterleavedImage> = (0..20).map(|_| crop()).collect();
        let crops: Vec<&InterleavedImage> = owned.iter().collect();
        let backend = FakeClassifier {
            rows: vec![[0.02, 0.98]; ORIENTATION_MAX_BATCH],
        };
        let verdicts = match classify(&backend, &contract(), &crops, ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert_eq!(verdicts.len(), 20, "every crop gets a verdict");
        assert!(verdicts.iter().all(|verdict| verdict.rotate));
    }

    #[test]
    fn an_empty_crop_list_classifies_nothing() {
        let backend = FakeClassifier { rows: Vec::new() };
        let verdicts = match classify(&backend, &contract(), &[], ORIENTATION_THRESHOLD) {
            Ok(verdicts) => verdicts,
            Err(error) => panic!("classify: {error}"),
        };
        assert!(verdicts.is_empty());
    }

    #[test]
    fn a_wrongly_sized_crop_is_rejected_before_the_backend() {
        let dimensions = match ImageDimensions::new(192, 48) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        // The legacy shape, which is exactly the mistake this module exists to
        // avoid making.
        let legacy = match InterleavedImage::new(dimensions, 3, vec![128_u8; 192 * 48 * 3]) {
            Ok(value) => value,
            Err(error) => panic!("crop: {error}"),
        };
        let backend = FakeClassifier {
            rows: vec![[1.0, 0.0]],
        };
        assert!(matches!(
            classify(&backend, &contract(), &[&legacy], ORIENTATION_THRESHOLD),
            Err(Error::InvalidInput {
                field: "orientation.crop_dimensions",
                ..
            })
        ));
    }

    /// Rotating twice returns the original, and once does not.
    ///
    /// The round trip is the property worth testing: it catches a transposed or
    /// half-reversed implementation that a single-direction check would pass.
    #[test]
    fn rotating_twice_restores_the_original() {
        let dimensions = match ImageDimensions::new(4, 3) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let pixels: Vec<u8> = (0..(4 * 3 * 3) as u8).collect();
        let source = match InterleavedImage::new(dimensions, 3, pixels.clone()) {
            Ok(value) => value,
            Err(error) => panic!("image: {error}"),
        };
        let once = match rotate_180(&source) {
            Ok(value) => value,
            Err(error) => panic!("rotate: {error}"),
        };
        assert_ne!(
            once.pixels(),
            pixels.as_slice(),
            "one rotation must change it"
        );
        // The first pixel of the rotated image is the last pixel of the source,
        // with its channel order preserved.
        assert_eq!(&once.pixels()[..3], &pixels[pixels.len() - 3..]);
        let twice = match rotate_180(&once) {
            Ok(value) => value,
            Err(error) => panic!("rotate: {error}"),
        };
        assert_eq!(twice.pixels(), pixels.as_slice());
        assert_eq!(twice.dimensions().width(), 4);
        assert_eq!(twice.dimensions().height(), 3);
    }

    #[test]
    fn the_fixed_input_size_is_the_artifact_declared_one() {
        assert_eq!(orientation_input_size(), (160, 80));
    }
}

/// Comparison against the captured orientation oracle.
///
/// The capture is produced by `tools/capture_orientation_oracle.py`, which runs
/// the artifact's declared preprocessing and the real model. Two divergences in
/// the classic path were found by exactly this kind of comparison, which is why
/// the classifier gets one before it is exposed through the public API.
#[cfg(test)]
mod oracle {
    use super::*;

    use crate::types::ImageDimensions;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-orientation/expected.json");
    const READING_ORDER: &[u8] =
        include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png");

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

    /// The same formula the capture tool uses, so both sides agree byte for byte.
    fn synthetic_crop(index: usize, width: u32, height: u32) -> InterleavedImage {
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                for channel in 0..3_usize {
                    pixels.push(((x * 7 + y * 13 + channel * 29 + index * 31) % 256) as u8);
                }
            }
        }
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("crop: {error}"),
        }
    }

    /// Rebuilds the source image for one recorded case.
    fn source_for(case: &str) -> InterleavedImage {
        match case {
            "reading-order-upright" | "reading-order-rotated" => {
                let encoded = match crate::types::EncodedImage::new(READING_ORDER) {
                    Ok(value) => value,
                    Err(error) => panic!("encoded: {error}"),
                };
                let decoded = match crate::image::decode_classic_bgr(encoded) {
                    Ok(value) => value,
                    Err(error) => panic!("decode: {error}"),
                };
                if case.ends_with("rotated") {
                    match rotate_180(&decoded) {
                        Ok(value) => value,
                        Err(error) => panic!("rotate: {error}"),
                    }
                } else {
                    decoded
                }
            }
            other => {
                let index: usize = match other.trim_start_matches("synthetic-").parse() {
                    Ok(value) => value,
                    Err(error) => panic!("case {other}: {error}"),
                };
                let sizes = [(160_u32, 80_u32), (320, 48), (48, 160), (97, 53)];
                let (width, height) = sizes[index];
                synthetic_crop(index, width, height)
            }
        }
    }

    fn fixture() -> serde_json::Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        }
    }

    /// This port's preprocessing must reproduce the captured tensor exactly.
    ///
    /// It also proves `rotate_180` agrees with `cv2.ROTATE_180`: the rotated
    /// case's source digest is checked against the one the capture recorded
    /// after calling OpenCV.
    #[test]
    fn the_captured_input_tensors_are_reproduced() {
        let document = fixture();
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        assert_eq!(records.len(), 6);

        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let source = source_for(case);

            // The source bytes themselves, which for the rotated case is the
            // cross-check of rotate_180 against OpenCV.
            assert_eq!(
                sha256_hex(source.pixels()),
                record["source_bgr_sha256"].as_str().unwrap_or_default(),
                "{case}: source BGR bytes differ from the capture"
            );

            let (width, height) = orientation_input_size();
            let target = match ImageDimensions::new(width, height) {
                Ok(value) => value,
                Err(error) => panic!("target: {error}"),
            };
            let resized = match crate::resize::classic_linear_resize(&source, target) {
                Ok(value) => value,
                Err(error) => panic!("{case} resize: {error}"),
            };
            let tensor = match classic_orientation_batch(&[&resized]) {
                Ok(value) => value,
                Err(error) => panic!("{case} tensor: {error}"),
            };

            let expected_shape: Vec<usize> = match record["input_shape"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or_default() as usize)
                    .collect(),
                None => panic!("{case}: no input shape"),
            };
            assert_eq!(tensor.shape(), expected_shape.as_slice(), "{case} shape");

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
        }
    }

    /// Given the recorded model output, this port must reach the same verdict.
    ///
    /// This separates the two halves: the test above checks what we feed the
    /// model, this one checks what we conclude from what it answers. A failure
    /// in one does not implicate the other.
    #[test]
    fn the_captured_verdicts_are_reproduced() {
        let document = fixture();
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };

        let mut rotated_cases = 0;
        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let scores = decode_f32(record["output_values_base64"].as_str().unwrap_or_default());
            let (class, score) = match argmax(&scores) {
                Ok(value) => value,
                Err(error) => panic!("{case} argmax: {error}"),
            };

            assert_eq!(
                ORIENTATION_LABELS[class],
                record["label"].as_str().unwrap_or_default(),
                "{case}: label"
            );
            let expected_score = record["score"].as_f64().unwrap_or_default();
            assert!(
                (score - expected_score).abs() < 1e-6,
                "{case}: score {score} against {expected_score}"
            );

            if rotates(ORIENTATION_LABELS[class], score, ORIENTATION_THRESHOLD) {
                rotated_cases += 1;
            }
        }

        // The upright page is left alone and the rotated one is corrected, which
        // is the whole point of the classifier and the only outcome that would
        // not also be produced by a broken model returning a constant.
        assert_eq!(
            rotated_cases, 1,
            "exactly the 180-degree page should be marked for rotation"
        );
    }
}
