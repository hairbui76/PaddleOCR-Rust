// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Private bounded greedy CTC index decoding for the classic M2 contract.

use crate::{
    error::{Error, ModelProblem, Result},
    types::MAX_IMAGE_PIXELS,
};

const MAX_CTC_TIME_STEPS: u32 = 16_384;
const MAX_CTC_CLASS_COUNT: u32 = 65_536;

/// One checked borrowed unbatched CTC score matrix in row-major order.
///
/// The matrix has `[time, classes]` shape. Its values remain deliberately
/// semantic-neutral: later model evidence decides whether a selected artifact
/// yields probabilities, logits, or another explicitly supported score form.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CtcScoreMatrix<'a> {
    time_steps: u32,
    class_count: u32,
    values: &'a [f32],
}

impl<'a> CtcScoreMatrix<'a> {
    /// Constructs one finite bounded CTC matrix with exact row-major length.
    pub(crate) fn new(time_steps: u32, class_count: u32, values: &'a [f32]) -> Result<Self> {
        if time_steps > MAX_CTC_TIME_STEPS {
            return Err(Error::ResourceLimit {
                resource: "ctc.time_steps",
                limit: u64::from(MAX_CTC_TIME_STEPS),
                actual: u64::from(time_steps),
            });
        }
        if class_count == 0 {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        if class_count > MAX_CTC_CLASS_COUNT {
            return Err(Error::ResourceLimit {
                resource: "ctc.classes",
                limit: u64::from(MAX_CTC_CLASS_COUNT),
                actual: u64::from(class_count),
            });
        }

        let expected_length = u64::from(time_steps) * u64::from(class_count);
        if expected_length > MAX_IMAGE_PIXELS {
            return Err(Error::ResourceLimit {
                resource: "ctc.matrix_values",
                limit: MAX_IMAGE_PIXELS,
                actual: expected_length,
            });
        }
        let expected_length =
            usize::try_from(expected_length).map_err(|_| Error::ResourceLimit {
                resource: "ctc.matrix_values",
                limit: usize::MAX as u64,
                actual: MAX_IMAGE_PIXELS,
            })?;
        if values.len() != expected_length || values.iter().any(|value| !value.is_finite()) {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }

        Ok(Self {
            time_steps,
            class_count,
            values,
        })
    }

    /// Returns the number of score rows.
    #[must_use]
    pub(crate) const fn time_steps(self) -> u32 {
        self.time_steps
    }

    /// Returns the number of classes in each score row, including blank index `0`.
    #[must_use]
    pub(crate) const fn class_count(self) -> u32 {
        self.class_count
    }

    /// Returns checked row-major finite scores.
    #[must_use]
    pub(crate) const fn values(self) -> &'a [f32] {
        self.values
    }
}

/// A private CTC path containing non-blank class indexes and their mean score.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CtcGreedyPath {
    class_indices: Vec<u32>,
    mean_score: f32,
}

impl CtcGreedyPath {
    /// Returns decoded non-blank class indexes in timestep order.
    #[must_use]
    pub(crate) fn class_indices(&self) -> &[u32] {
        &self.class_indices
    }

    /// Returns the arithmetic mean of selected timestep maxima, or `0.0` when empty.
    #[must_use]
    pub(crate) const fn mean_score(&self) -> f32 {
        self.mean_score
    }
}

/// Greedily decodes one checked CTC matrix to class indexes without a dictionary.
///
/// Each row selects the first class index holding its maximum score. Adjacent
/// equal raw indexes are removed before index `0` is treated as blank, matching
/// the classic CTC decoder's selection order. This function intentionally does
/// not map indexes to text or validate artifact-specific output semantics.
pub(crate) fn classic_ctc_greedy_indices(matrix: CtcScoreMatrix<'_>) -> Result<CtcGreedyPath> {
    let output_capacity =
        usize::try_from(matrix.time_steps()).map_err(|_| Error::ResourceLimit {
            resource: "ctc.time_steps",
            limit: usize::MAX as u64,
            actual: u64::from(matrix.time_steps()),
        })?;
    let row_length = usize::try_from(matrix.class_count()).map_err(|_| Error::ResourceLimit {
        resource: "ctc.classes",
        limit: usize::MAX as u64,
        actual: u64::from(matrix.class_count()),
    })?;
    let mut class_indices = Vec::new();
    class_indices
        .try_reserve_exact(output_capacity)
        .map_err(|_| Error::Backend {
            message: "CTC greedy-path allocation failed",
        })?;

    let mut previous_index = None;
    let mut selected_count = 0_u32;
    let mut selected_sum = 0.0_f32;
    for row in matrix.values().chunks_exact(row_length) {
        let mut best_index = 0_u32;
        let mut best_score = row[0];
        for (class_index, score) in row.iter().copied().enumerate().skip(1) {
            if score > best_score {
                best_index = class_index as u32;
                best_score = score;
            }
        }

        if best_index != 0 && previous_index != Some(best_index) {
            class_indices.push(best_index);
            selected_count += 1;
            selected_sum += best_score;
        }
        previous_index = Some(best_index);
    }

    let mean_score = if selected_count == 0 {
        0.0
    } else {
        selected_sum / selected_count as f32
    };
    Ok(CtcGreedyPath {
        class_indices,
        mean_score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must_ok<T>(result: Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected success, got {error}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "actual {actual} did not equal expected {expected}"
        );
    }

    #[test]
    fn classic_ctc_collapses_raw_repeats_before_removing_blank() {
        let matrix = must_ok(CtcScoreMatrix::new(
            6,
            3,
            &[
                0.1, 0.9, 0.0, // 1 is retained.
                0.1, 0.8, 0.0, // Adjacent 1 is removed.
                0.9, 0.05, 0.05, // Blank resets the raw-repeat comparison.
                0.1, 0.6, 0.3, // 1 is retained again after blank.
                0.1, 0.2, 0.7, // 2 is retained.
                0.1, 0.1, 0.8, // Adjacent 2 is removed.
            ],
        ));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert_eq!(path.class_indices(), &[1, 1, 2]);
        assert_close(path.mean_score(), (0.9 + 0.6 + 0.7) / 3.0);
    }

    #[test]
    fn classic_ctc_keeps_the_lowest_index_on_exact_argmax_ties() {
        let matrix = must_ok(CtcScoreMatrix::new(
            3,
            4,
            &[
                0.1, 0.8, 0.8, 0.2, // Index 1 wins over equal index 2.
                0.5, 0.5, 0.4, 0.1, // Blank index 0 wins its tie.
                0.3, 0.3, 0.3, 0.2, // Blank index 0 wins its tie.
            ],
        ));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert_eq!(path.class_indices(), &[1]);
        assert_close(path.mean_score(), 0.8);
    }

    #[test]
    fn classic_ctc_returns_an_empty_zero_score_path_for_zero_timesteps() {
        let matrix = must_ok(CtcScoreMatrix::new(0, 3, &[]));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert!(path.class_indices().is_empty());
        assert_eq!(path.mean_score(), 0.0);
    }

    #[test]
    fn ctc_score_matrix_rejects_wrong_shape_and_non_finite_values() {
        for values in [
            vec![0.0, 0.0],
            vec![0.0, f32::NAN, 0.0],
            vec![0.0, f32::INFINITY, 0.0],
            vec![0.0, f32::NEG_INFINITY, 0.0],
        ] {
            let result = CtcScoreMatrix::new(1, 3, &values);

            assert!(matches!(
                result,
                Err(Error::Model {
                    problem: ModelProblem::TensorContract,
                })
            ));
        }
    }

    #[test]
    fn ctc_score_matrix_rejects_invalid_or_excessive_dimensions() {
        assert!(matches!(
            CtcScoreMatrix::new(1, 0, &[]),
            Err(Error::Model {
                problem: ModelProblem::TensorContract,
            })
        ));
        assert!(matches!(
            CtcScoreMatrix::new(MAX_CTC_TIME_STEPS + 1, 1, &[]),
            Err(Error::ResourceLimit {
                resource: "ctc.time_steps",
                ..
            })
        ));
        assert!(matches!(
            CtcScoreMatrix::new(1, MAX_CTC_CLASS_COUNT + 1, &[]),
            Err(Error::ResourceLimit {
                resource: "ctc.classes",
                ..
            })
        ));
        assert!(matches!(
            CtcScoreMatrix::new(MAX_CTC_TIME_STEPS, MAX_CTC_CLASS_COUNT, &[]),
            Err(Error::ResourceLimit {
                resource: "ctc.matrix_values",
                ..
            })
        ));
    }
}
