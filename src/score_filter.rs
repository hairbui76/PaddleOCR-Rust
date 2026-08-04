// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The classic pipeline's final score filter.
//!
//! `tools/infer/predict_system.py` keeps a recognized line when
//! `score >= drop_score`, so a score **exactly equal** to the threshold is
//! retained. `tests/fixtures/classic-v1-ctc-score-boundary/` records that
//! boundary from an isolated capture of the upstream source, using the values
//! immediately below, at, and immediately above `0.5`.
//!
//! Input order is preserved: this filter removes entries, it never reorders
//! them. Reading order is established earlier by
//! `geometry::classic_sort_quadrilaterals`.

use crate::error::{Error, InputViolation, Result};

/// Retains the entries whose score is at least `drop_score`, in input order.
pub(crate) fn retain_by_score<T>(entries: Vec<(T, f64)>, drop_score: f64) -> Result<Vec<T>> {
    if !drop_score.is_finite() {
        return Err(Error::InvalidInput {
            field: "pipeline.drop_score",
            violation: InputViolation::NonFinite,
        });
    }
    let mut retained = Vec::new();
    for (value, score) in entries {
        if !score.is_finite() {
            return Err(Error::InvalidInput {
                field: "pipeline.score",
                violation: InputViolation::NonFinite,
            });
        }
        // Written as the upstream comparison, not its negation: `>=` keeps an
        // exactly equal score, which the recorded boundary fixture pins.
        if score >= drop_score {
            retained.push(value);
        }
    }
    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const INPUT: &str = include_str!("../tests/fixtures/classic-v1-ctc-score-boundary/input.json");
    const EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-ctc-score-boundary/expected.json");

    /// Reproduces the recorded upstream selection exactly.
    #[test]
    fn the_recorded_boundary_selection_is_reproduced() {
        let input: Value = match serde_json::from_str(INPUT) {
            Ok(value) => value,
            Err(error) => panic!("input fixture is not valid JSON: {error}"),
        };
        let expected: Value = match serde_json::from_str(EXPECTED) {
            Ok(value) => value,
            Err(error) => panic!("expected fixture is not valid JSON: {error}"),
        };

        let drop_score = input
            .get("drop_score")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let pairs = match input.get("pairs").and_then(Value::as_array) {
            Some(pairs) => pairs,
            None => panic!("input fixture must contain pairs"),
        };
        let entries: Vec<(usize, f64)> = pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| {
                (
                    index,
                    pair.get("score")
                        .and_then(Value::as_f64)
                        .unwrap_or_default(),
                )
            })
            .collect();

        let retained = match retain_by_score(entries, drop_score) {
            Ok(retained) => retained,
            Err(error) => panic!("filter failed: {error}"),
        };

        let wanted: Vec<usize> = match expected
            .get("retained_input_indexes")
            .and_then(Value::as_array)
        {
            Some(values) => values
                .iter()
                .map(|value| value.as_u64().unwrap_or_default() as usize)
                .collect(),
            None => panic!("expected fixture must record retained indexes"),
        };
        assert_eq!(
            retained, wanted,
            "the value immediately below 0.5 is dropped, 0.5 itself is kept"
        );
    }

    #[test]
    fn order_is_preserved_and_non_finite_input_is_rejected() {
        let kept = match retain_by_score(vec![("a", 0.9), ("b", 0.1), ("c", 0.7)], 0.5) {
            Ok(kept) => kept,
            Err(error) => panic!("expected a filtered list, got {error}"),
        };
        assert_eq!(kept, ["a", "c"], "surviving entries keep their input order");

        assert!(matches!(
            retain_by_score(vec![("a", f64::NAN)], 0.5),
            Err(Error::InvalidInput {
                field: "pipeline.score",
                ..
            })
        ));
        assert!(matches!(
            retain_by_score(vec![("a", 0.9)], f64::NAN),
            Err(Error::InvalidInput {
                field: "pipeline.drop_score",
                ..
            })
        ));
    }
}
