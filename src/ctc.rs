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

    /// Returns the finite arithmetic mean of selected timestep maxima, or `0.0` when empty.
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
///
/// Ordinary finite score inputs retain the reviewed `f32` accumulation path.
/// If that aggregate overflows, a parallel `f64` aggregate supplies the finite
/// mean instead; a finite input matrix must not produce a non-finite path score.
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
    let mut selected_sum_f64 = 0.0_f64;
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
            selected_sum_f64 += f64::from(best_score);
        }
        previous_index = Some(best_index);
    }

    let mean_score = finite_selected_mean(selected_sum, selected_sum_f64, selected_count)?;
    Ok(CtcGreedyPath {
        class_indices,
        mean_score,
    })
}

fn finite_selected_mean(
    selected_sum: f32,
    selected_sum_f64: f64,
    selected_count: u32,
) -> Result<f32> {
    if selected_count == 0 {
        return Ok(0.0);
    }

    let mean_score = selected_sum / selected_count as f32;
    if mean_score.is_finite() {
        return Ok(mean_score);
    }

    // Every selected term originated as a finite `f32`, and the bounded count
    // makes its `f64` sum representable. The fallback preserves a finite mean
    // only on the exceptional f32-overflow path, leaving reviewed normal-range
    // `f32` results unchanged.
    let fallback = selected_sum_f64 / f64::from(selected_count);
    if !fallback.is_finite() || fallback < f64::from(f32::MIN) || fallback > f64::from(f32::MAX) {
        return Err(Error::Backend {
            message: "CTC selected-score mean is not representable",
        });
    }
    Ok(fallback as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTC_GREEDY_INPUT: &str =
        include_str!("../tests/fixtures/classic-v1-ctc-greedy-path/input.csv");
    const CTC_GREEDY_EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-ctc-greedy-path/expected.txt");

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

    fn parse_score_fixture(fixture: &str) -> Vec<f32> {
        fixture
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .flat_map(|line| line.split(','))
            .enumerate()
            .map(|(index, value)| match value.parse::<f32>() {
                Ok(parsed) => parsed,
                Err(error) => {
                    panic!("CTC greedy input value {index} is invalid {value:?}: {error}")
                }
            })
            .collect()
    }

    fn expected_fixture_field<'a>(fixture: &'a str, name: &str) -> &'a str {
        fixture
            .lines()
            .filter_map(|line| {
                line.strip_prefix(name)
                    .and_then(|value| value.strip_prefix(':'))
            })
            .map(str::trim)
            .next()
            .unwrap_or_else(|| panic!("CTC greedy expected fixture is missing {name:?}"))
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
    fn classic_ctc_keeps_the_mean_finite_when_f32_sum_overflows() {
        let matrix = must_ok(CtcScoreMatrix::new(
            2,
            3,
            &[
                0.0,
                f32::MAX,
                0.0, // Index 1 is retained.
                0.0,
                0.0,
                f32::MAX, // A distinct index retains another maximum.
            ],
        ));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert_eq!(path.class_indices(), &[1, 2]);
        assert_eq!(path.mean_score(), f32::MAX);
        assert!(path.mean_score().is_finite());
    }

    #[test]
    fn classic_ctc_keeps_a_negative_extreme_mean_finite_when_f32_sum_overflows() {
        let retained_score = f32::MIN / 2.0;
        let values = [
            f32::MIN,
            retained_score,
            f32::MIN,
            f32::MIN, // Index 1 is retained.
            f32::MIN,
            f32::MIN,
            retained_score,
            f32::MIN, // Index 2 is retained.
            f32::MIN,
            f32::MIN,
            f32::MIN,
            retained_score, // Index 3 is retained.
        ];
        let matrix = must_ok(CtcScoreMatrix::new(3, 4, &values));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert_eq!(path.class_indices(), &[1, 2, 3]);
        assert_eq!(path.mean_score(), retained_score);
        assert!(path.mean_score().is_finite());
    }

    #[test]
    fn classic_ctc_matches_self_authored_greedy_path_fixture() {
        let values = parse_score_fixture(CTC_GREEDY_INPUT);
        let expected_indices = expected_fixture_field(CTC_GREEDY_EXPECTED, "class_indices")
            .split(',')
            .enumerate()
            .map(|(index, value)| match value.parse::<u32>() {
                Ok(parsed) => parsed,
                Err(error) => {
                    panic!("CTC greedy expected index {index} is invalid {value:?}: {error}")
                }
            })
            .collect::<Vec<_>>();
        let expected_mean =
            match expected_fixture_field(CTC_GREEDY_EXPECTED, "mean_score").parse::<f32>() {
                Ok(parsed) => parsed,
                Err(error) => panic!("CTC greedy expected mean score is invalid: {error}"),
            };
        let matrix = must_ok(CtcScoreMatrix::new(9, 4, &values));

        let path = must_ok(classic_ctc_greedy_indices(matrix));

        assert_eq!(path.class_indices(), expected_indices);
        assert_eq!(path.mean_score(), expected_mean);
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
