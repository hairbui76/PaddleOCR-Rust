// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The classic recognizer path: text crops to decoded strings.
//!
//! This composes the verified steps in the frozen order from
//! `docs/CLASSIC_OCR_CONTRACT.md`:
//!
//! 1. plan the batch: sort crops by aspect ratio and derive the padded width;
//! 2. resize each crop to height 48 and its planned width;
//! 3. normalize and stack into one `NCHW` `f32` batch with zero right padding;
//! 4. run the backend through the validated adapter;
//! 5. decode each row with greedy CTC and the bound dictionary;
//! 6. restore the caller's original order.
//!
//! Step 6 matters: results come back in aspect-sorted order, and returning them
//! that way would silently reorder the caller's lines.

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, run_validated};
use crate::crop::InterleavedImage;
use crate::ctc::{CtcScoreMatrix, classic_ctc_greedy_indices};
use crate::dictionary::CtcDictionary;
use crate::error::{Error, InputViolation, Result};
use crate::recognizer_batch::{RECOGNITION_HEIGHT, plan_batch, restore_order};
use crate::resize::classic_linear_resize;
use crate::tensor::classic_recognizer_batch;
use crate::types::ImageDimensions;

/// One recognized line: its text and CTC confidence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecognizedLine {
    /// Decoded text, with scalars preserved exactly.
    pub(crate) text: String,
    /// Mean of the selected per-timestep maxima.
    pub(crate) score: f64,
}

/// Recognizes one group of crops, returning results in the caller's order.
pub(crate) fn recognize(
    backend: &dyn InferenceBackend,
    contract: &ModelContract,
    dictionary: &CtcDictionary,
    crops: &[&InterleavedImage],
) -> Result<Vec<RecognizedLine>> {
    if crops.is_empty() {
        return Ok(Vec::new());
    }

    let sizes: Vec<(u32, u32)> = crops
        .iter()
        .map(|crop| {
            let dimensions = crop.dimensions();
            (dimensions.width(), dimensions.height())
        })
        .collect();
    let plan = plan_batch(&sizes)?;

    // Resize in aspect-sorted order so the batch rows match the plan.
    let mut resized = Vec::with_capacity(plan.crops.len());
    for entry in &plan.crops {
        let source = crops[entry.original_index];
        let target = ImageDimensions::new(entry.resized_width, RECOGNITION_HEIGHT)?;
        resized.push(classic_linear_resize(source, target)?);
    }
    let borrowed: Vec<&InterleavedImage> = resized.iter().collect();
    let tensor = classic_recognizer_batch(&borrowed, plan.batch_width)?;

    let input = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
    let output = run_validated(backend, contract, &input)?;

    // The recognizer emits [batch, time, classes].
    let shape = output.shape();
    if shape.len() != 3 || shape[0] != plan.crops.len() {
        return Err(Error::InvalidInput {
            field: "recognizer.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }
    dictionary.require_class_count(shape[2])?;

    let (time_steps, classes) = (shape[1], shape[2]);
    let stride = time_steps * classes;
    let mut decoded = Vec::with_capacity(plan.crops.len());
    for row in 0..plan.crops.len() {
        let values = &output.values()[row * stride..(row + 1) * stride];
        let time = u32::try_from(time_steps).map_err(|_| Error::InvalidInput {
            field: "recognizer.output_shape",
            violation: InputViolation::OutOfRange,
        })?;
        let class_count = u32::try_from(classes).map_err(|_| Error::InvalidInput {
            field: "recognizer.output_shape",
            violation: InputViolation::OutOfRange,
        })?;
        let matrix = CtcScoreMatrix::new(time, class_count, values)?;
        let path = classic_ctc_greedy_indices(matrix)?;
        decoded.push(RecognizedLine {
            text: dictionary.decode(&path)?,
            score: f64::from(path.mean_score()),
        });
    }

    // Results are in aspect-sorted order; restore the caller's order.
    let restored = restore_order(&plan, decoded)?;
    restored
        .into_iter()
        .map(|line| {
            line.ok_or(Error::InvalidInput {
                field: "recognizer.batch",
                violation: InputViolation::OutOfRange,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};

    /// A backend that emits a fixed class per batch row.
    struct FakeRecognizer {
        classes: usize,
        /// One selected class index per row, in batch order.
        selected: Vec<u32>,
    }

    impl InferenceBackend for FakeRecognizer {
        fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
            let shape = input.shape();
            assert_eq!(shape.len(), 4, "recognizer input must be NCHW");
            assert_eq!(shape[1], 3, "three interleaved channels");
            assert_eq!(shape[2], 48, "the frozen recognition height");
            let batch = shape[0];
            assert_eq!(batch, self.selected.len(), "one selection per row");

            let time = 2_usize;
            let mut values = vec![0.0_f32; batch * time * self.classes];
            for (row, class) in self.selected.iter().enumerate() {
                for step in 0..time {
                    let base = (row * time + step) * self.classes;
                    values[base + *class as usize] = 0.8;
                }
            }
            let tensor = BackendTensor::new(vec![batch, time, self.classes], values)?;
            Ok(("fetch_name_0".to_owned(), tensor))
        }
    }

    fn contract(batch: usize, classes: usize) -> ModelContract {
        let artifact = match ModelArtifact::new("/nonexistent/rec.onnx", "0".repeat(64)) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid artifact, got {error}"),
        };
        let free = AxisExtent::Bounded {
            minimum: 1,
            maximum: 4096,
        };
        let input = match TensorContract::new(
            "x",
            vec![
                AxisExtent::Fixed(batch),
                AxisExtent::Fixed(3),
                AxisExtent::Fixed(48),
                free,
            ],
        ) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output = match TensorContract::new(
            "fetch_name_0",
            vec![AxisExtent::Fixed(batch), free, AxisExtent::Fixed(classes)],
        ) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid output contract, got {error}"),
        };
        let budget = match RunBudget::new(40_000_000, 40_000_000, 64) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn crop(width: u32, height: u32) -> InterleavedImage {
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        };
        match InterleavedImage::new(dimensions, 3, vec![200_u8; (width * height * 3) as usize]) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid crop, got {error}"),
        }
    }

    fn dictionary() -> CtcDictionary {
        let entries = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        match CtcDictionary::new(entries, true) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid dictionary, got {error}"),
        }
    }

    /// Results must come back in the caller's order, not the batch order.
    #[test]
    fn recognized_lines_return_in_caller_order() {
        let wide = crop(480, 48);
        let narrow = crop(48, 48);
        let middle = crop(240, 48);
        let crops = vec![&wide, &narrow, &middle];

        let dictionary = dictionary();
        // Batch order is narrow, middle, wide. Give each a distinct class.
        let backend = FakeRecognizer {
            classes: dictionary.class_count(),
            selected: vec![1, 2, 3],
        };
        let lines = match recognize(
            &backend,
            &contract(3, dictionary.class_count()),
            &dictionary,
            &crops,
        ) {
            Ok(lines) => lines,
            Err(error) => panic!("expected recognized lines, got {error}"),
        };

        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        // narrow -> "a", middle -> "b", wide -> "c"; caller order is
        // wide, narrow, middle.
        assert_eq!(texts, ["c", "a", "b"], "order must be restored");
        for line in &lines {
            assert!(line.score > 0.0, "a selected class must yield a score");
        }
    }

    #[test]
    fn an_empty_crop_list_recognizes_nothing() {
        let dictionary = dictionary();
        let backend = FakeRecognizer {
            classes: dictionary.class_count(),
            selected: Vec::new(),
        };
        let lines = match recognize(
            &backend,
            &contract(1, dictionary.class_count()),
            &dictionary,
            &[],
        ) {
            Ok(lines) => lines,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(lines.is_empty());
    }

    /// A model whose class count disagrees with the dictionary is rejected.
    #[test]
    fn a_class_count_mismatch_is_a_contract_error() {
        let single = crop(96, 48);
        let dictionary = dictionary();
        // The backend emits one class too few.
        let backend = FakeRecognizer {
            classes: dictionary.class_count() - 1,
            selected: vec![1],
        };
        let outcome = recognize(
            &backend,
            &contract(1, dictionary.class_count() - 1),
            &dictionary,
            &[&single],
        );
        assert!(
            outcome.is_err(),
            "a class-count mismatch must not decode silently"
        );
    }
}
