// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The classic recognizer path: text crops to decoded strings.
//!
//! This composes the verified steps in the frozen order from
//! `docs/CLASSIC_OCR_CONTRACT.md`:
//!
//! 1. plan the batches: sort every crop by aspect ratio, split the sorted order
//!    into groups of six, and derive each group's padded width;
//! 2. for each batch, resize its crops to height 48 and their planned widths;
//! 3. normalize and stack into one `NCHW` `f32` batch with zero right padding;
//! 4. run the backend through the validated adapter, once per batch;
//! 5. decode each row with greedy CTC and the bound dictionary;
//! 6. restore the caller's original order across every batch.
//!
//! Two steps carry weight beyond their size. The split in step 1 is what makes
//! the padded width depend only on the five crops nearest in aspect ratio, as
//! upstream does; planning all crops as one batch would pad every short crop to
//! the widest crop on the page. And step 6 matters because results come back in
//! aspect-sorted order, so returning them that way would silently reorder the
//! caller's lines.

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, run_validated};
use crate::crop::InterleavedImage;
use crate::ctc::{CtcScoreMatrix, classic_ctc_greedy_indices};
use crate::dictionary::CtcDictionary;
use crate::error::{Error, InputViolation, Result};
use crate::recognizer_batch::{RECOGNITION_HEIGHT, plan_batches, restore_order};
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
    let plans = plan_batches(&sizes)?;

    let mut decoded = Vec::with_capacity(crops.len());
    for plan in &plans {
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
    }

    // Results are in aspect-sorted order; restore the caller's order.
    let restored = restore_order(&plans, decoded)?;
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

    /// A backend driven by an explicit per-row class path, in batch order.
    ///
    /// The cursor is what makes it usable across several batches: `recognize`
    /// calls the backend once per batch of six, so the fake must remember how
    /// many rows it has already served.
    struct ScriptedRecognizer {
        classes: usize,
        paths: Vec<Vec<u32>>,
        served: std::cell::Cell<usize>,
    }

    impl ScriptedRecognizer {
        fn new(classes: usize, paths: Vec<Vec<u32>>) -> Self {
            Self {
                classes,
                paths,
                served: std::cell::Cell::new(0),
            }
        }
    }

    impl InferenceBackend for ScriptedRecognizer {
        fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
            let batch = input.shape()[0];
            let first = self.served.get();
            let rows = &self.paths[first..first + batch];
            self.served.set(first + batch);

            let time = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let mut values = vec![0.0_f32; batch * time * self.classes];
            for (row, path) in rows.iter().enumerate() {
                for step in 0..time {
                    // A row shorter than the batch's time axis is padded with
                    // blank, which is class zero.
                    let class = path.get(step).copied().unwrap_or(0) as usize;
                    values[(row * time + step) * self.classes + class] = 0.8;
                }
            }
            let tensor = BackendTensor::new(vec![batch, time, self.classes], values)?;
            Ok(("fetch_name_0".to_owned(), tensor))
        }
    }

    /// A batch contract whose batch axis is a bounded range, which is what the
    /// real path uses now that batches of six mean the last batch is short.
    fn free_batch_contract(classes: usize) -> ModelContract {
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
            vec![free, AxisExtent::Fixed(3), AxisExtent::Fixed(48), free],
        ) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output =
            match TensorContract::new("fetch_name_0", vec![free, free, AxisExtent::Fixed(classes)])
            {
                Ok(value) => value,
                Err(error) => panic!("expected a valid output contract, got {error}"),
            };
        let budget = match RunBudget::new(40_000_000, 40_000_000, 64) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn recognize_paths(
        crops: &[&InterleavedImage],
        dictionary: &CtcDictionary,
        paths: Vec<Vec<u32>>,
    ) -> Vec<RecognizedLine> {
        let backend = ScriptedRecognizer::new(dictionary.class_count(), paths);
        match recognize(
            &backend,
            &free_batch_contract(dictionary.class_count()),
            dictionary,
            crops,
        ) {
            Ok(lines) => lines,
            Err(error) => panic!("expected recognized lines, got {error}"),
        }
    }

    /// An all-blank path is an empty string, not a fabricated character.
    ///
    /// This is the shape of a crop that contains no readable text. The line is
    /// still returned; dropping it is the score filter's job, not the
    /// recognizer's.
    #[test]
    fn an_all_blank_path_decodes_to_empty_text() {
        let single = crop(96, 48);
        let dictionary = dictionary();
        let lines = recognize_paths(&[&single], &dictionary, vec![vec![0, 0, 0, 0]]);
        assert_eq!(lines.len(), 1, "an empty decode is still a result");
        assert_eq!(lines[0].text, "", "no text must mean no text");
    }

    /// Adjacent equal classes collapse; a blank between them does not.
    ///
    /// This is the rule that lets CTC spell a doubled letter at all. Collapsing
    /// after blank removal instead of before would turn "aa" into "a".
    #[test]
    fn repeats_collapse_but_a_blank_separates_them() {
        let single = crop(96, 48);
        let dictionary = dictionary();

        let collapsed = recognize_paths(&[&single], &dictionary, vec![vec![1, 1, 1]]);
        assert_eq!(collapsed[0].text, "a", "raw repeats are one character");

        let separated = recognize_paths(&[&single], &dictionary, vec![vec![1, 1, 0, 1]]);
        assert_eq!(separated[0].text, "aa", "a blank splits the repeat");

        let sequence = recognize_paths(&[&single], &dictionary, vec![vec![1, 2, 2, 3, 0, 3]]);
        assert_eq!(sequence[0].text, "abcc");
    }

    /// Dictionary entries reach the output as the exact scalars they were read
    /// as, including multi-scalar entries and characters that a normalizer
    /// would fold.
    #[test]
    fn non_ascii_and_multi_scalar_entries_are_emitted_exactly() {
        let entries = vec![
            "\u{4f60}".to_owned(),
            "\u{597d}".to_owned(),
            // An ideographic space, which NFKC would fold to U+0020.
            "\u{3000}".to_owned(),
            // A ligature, which NFKC would decompose to "fi".
            "\u{fb01}".to_owned(),
            // A multi-scalar entry: base plus combining acute.
            "e\u{301}".to_owned(),
        ];
        let dictionary = match CtcDictionary::new(entries, true) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid dictionary, got {error}"),
        };
        let single = crop(96, 48);
        let lines = recognize_paths(&[&single], &dictionary, vec![vec![1, 2, 3, 4, 5]]);
        assert_eq!(lines[0].text, "\u{4f60}\u{597d}\u{3000}\u{fb01}e\u{301}");
        assert_eq!(
            lines[0].text.chars().count(),
            6,
            "the combining mark stays a separate scalar"
        );
    }

    /// Crops spanning several batches all come back, in the caller's order.
    #[test]
    fn results_from_several_batches_return_in_caller_order() {
        let dictionary = dictionary();
        // Fourteen crops from widest to narrowest, so batch order is the exact
        // reverse of caller order and any per-batch bookkeeping error shows.
        let widths: Vec<u32> = (0..14).rev().map(|index| 48 + index * 16).collect();
        let owned: Vec<InterleavedImage> = widths.iter().map(|width| crop(*width, 48)).collect();
        let crops: Vec<&InterleavedImage> = owned.iter().collect();

        // The k-th decoded row spells class (k % 3) + 1, so every row is
        // identifiable and the mapping back is checkable by hand.
        let paths: Vec<Vec<u32>> = (0..14).map(|row| vec![(row % 3) + 1]).collect();
        let lines = recognize_paths(&crops, &dictionary, paths);

        assert_eq!(lines.len(), 14);
        // Caller index 13 is the narrowest crop, so it decoded first as row 0.
        let expected: Vec<String> = (0..14)
            .map(|caller: u32| {
                let row = 13 - caller;
                match (row % 3) + 1 {
                    1 => "a",
                    2 => "b",
                    _ => "c",
                }
                .to_owned()
            })
            .collect();
        let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
        assert_eq!(texts, expected, "order must survive the batch split");
    }

    /// Extreme crop shapes are recognized up to the image bound, and rejected
    /// with a typed error past it.
    ///
    /// The batch width is `ceil(48 * ratio)`, so an extreme aspect ratio is the
    /// one input that can push a synthesized tensor past the decoded-dimension
    /// budget even though every crop was itself small. The boundary is worth
    /// pinning because the failure is a resource error rather than a truncated
    /// or silently narrowed batch.
    #[test]
    fn extreme_crop_aspect_ratios_are_recognized_up_to_the_image_bound() {
        let dictionary = dictionary();

        // Ratio 1/240 and ratio 85: the batch pads to 48 * 85 = 4080, which is
        // inside this contract's 4096 width axis.
        let sliver = crop(1, 240);
        let banner = crop(680, 8);
        let crops = vec![&sliver, &banner];
        let lines = recognize_paths(&crops, &dictionary, vec![vec![1], vec![2]]);
        assert_eq!(lines.len(), 2);
        // Batch order is sliver then banner, and caller order is the same.
        assert_eq!(lines[0].text, "a");
        assert_eq!(lines[1].text, "b");

        // Ratio 500 needs a 24,000-wide batch, past the 16,384 pixel bound.
        let extreme = crop(4000, 8);
        let backend = ScriptedRecognizer::new(dictionary.class_count(), vec![vec![1]]);
        let outcome = recognize(
            &backend,
            &free_batch_contract(dictionary.class_count()),
            &dictionary,
            &[&extreme],
        );
        assert!(
            matches!(outcome, Err(Error::ResourceLimit { .. })),
            "a 24,000-wide batch must be a typed resource error, got {outcome:?}"
        );
    }

    /// More crops than the declared work-unit budget is a typed resource error.
    #[test]
    fn a_crop_count_above_the_work_unit_budget_is_rejected() {
        let dictionary = dictionary();
        let single = crop(96, 48);
        let crops = vec![&single; 1_001];
        let backend = ScriptedRecognizer::new(dictionary.class_count(), vec![vec![1]; 1_001]);
        let outcome = recognize(
            &backend,
            &free_batch_contract(dictionary.class_count()),
            &dictionary,
            &crops,
        );
        assert!(
            matches!(outcome, Err(Error::ResourceLimit { .. })),
            "1001 crops must be a resource limit, got {outcome:?}"
        );
    }

    /// A row count that disagrees with the batch is a typed error.
    #[test]
    fn an_output_row_count_that_breaks_the_batch_is_a_typed_error() {
        struct WrongRowCount(usize);

        impl InferenceBackend for WrongRowCount {
            fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
                let rows = input.shape()[0] + self.0;
                let tensor = BackendTensor::new(vec![rows, 1, 4], vec![0.5_f32; rows * 4])?;
                Ok(("fetch_name_0".to_owned(), tensor))
            }
        }

        let dictionary = dictionary();
        let single = crop(96, 48);
        let outcome = recognize(
            &WrongRowCount(1),
            &free_batch_contract(dictionary.class_count()),
            &dictionary,
            &[&single],
        );
        assert!(outcome.is_err(), "an extra output row must not be accepted");
    }
}
