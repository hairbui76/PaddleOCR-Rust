// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Private bounded DB probability-map primitives for the classic M2 contract.

use crate::{
    error::{Error, ModelProblem, Result},
    types::ImageDimensions,
};

const CLASSIC_DB_SEGMENTATION_THRESHOLD: f32 = 0.3;

/// One checked borrowed detector probability map in row-major order.
///
/// This is a narrow private postprocessing input, not a runtime tensor ABI. A
/// caller must already have selected one image/map plane and supplied checked
/// dimensions; batching, rank, dtype conversion, and backend interaction are
/// later work.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DetectorProbabilityMap<'a> {
    dimensions: ImageDimensions,
    values: &'a [f32],
}

impl<'a> DetectorProbabilityMap<'a> {
    /// Constructs one finite map with exactly one row-major value per pixel.
    pub(crate) fn new(dimensions: ImageDimensions, values: &'a [f32]) -> Result<Self> {
        let expected_length =
            usize::try_from(dimensions.pixels()).map_err(|_| Error::ResourceLimit {
                resource: "detector.map.values",
                limit: usize::MAX as u64,
                actual: dimensions.pixels(),
            })?;
        if values.len() != expected_length || values.iter().any(|value| !value.is_finite()) {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }

        Ok(Self { dimensions, values })
    }

    /// Returns the map dimensions in pixels.
    #[must_use]
    pub(crate) const fn dimensions(self) -> ImageDimensions {
        self.dimensions
    }

    /// Returns the checked row-major probability values.
    #[must_use]
    pub(crate) const fn values(self) -> &'a [f32] {
        self.values
    }
}

/// A private row-major DB segmentation mask with zero/one byte values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BinaryBitmap {
    dimensions: ImageDimensions,
    values: Vec<u8>,
}

impl BinaryBitmap {
    /// Returns the dimensions shared with the source probability map.
    #[must_use]
    pub(crate) const fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }

    /// Returns row-major zero/one segmentation bytes.
    #[must_use]
    pub(crate) fn values(&self) -> &[u8] {
        &self.values
    }
}

/// Applies the fixed M2 DB segmentation rule to one checked probability map.
///
/// Every output byte is `1` exactly when the corresponding finite source value
/// is strictly greater than `0.3`; equality is excluded. This function does not
/// dilate, find contours, score, offset, scale, or emit detector boxes.
pub(crate) fn classic_db_binary_segmentation(
    map: DetectorProbabilityMap<'_>,
) -> Result<BinaryBitmap> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(map.values().len())
        .map_err(|_| Error::Backend {
            message: "DB segmentation mask allocation failed",
        })?;
    values.extend(
        map.values()
            .iter()
            .map(|value| u8::from(*value > CLASSIC_DB_SEGMENTATION_THRESHOLD)),
    );

    Ok(BinaryBitmap {
        dimensions: map.dimensions(),
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB_BOUNDARY_INPUT: &str =
        include_str!("../tests/fixtures/classic-v1-db-map-boundaries/input.csv");
    const DB_BOUNDARY_EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-db-map-boundaries/expected.csv");

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        }
    }

    fn must_ok<T>(result: Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected success, got {error}"),
        }
    }

    fn parse_fixture_values<T>(fixture: &str, fixture_name: &str) -> Vec<T>
    where
        T: core::str::FromStr,
        T::Err: core::fmt::Display,
    {
        fixture
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .flat_map(|line| line.split(','))
            .enumerate()
            .map(|(index, value)| match value.parse::<T>() {
                Ok(parsed) => parsed,
                Err(error) => panic!("{fixture_name} value {index} is invalid {value:?}: {error}"),
            })
            .collect()
    }

    #[test]
    fn classic_db_segmentation_excludes_the_threshold_and_preserves_row_order() {
        let map = must_ok(DetectorProbabilityMap::new(
            dimensions(2, 2),
            &[0.2999, 0.3, 0.3001, 1.0],
        ));

        let bitmap = must_ok(classic_db_binary_segmentation(map));

        assert_eq!(bitmap.dimensions(), dimensions(2, 2));
        assert_eq!(bitmap.values(), &[0, 0, 1, 1]);
    }

    #[test]
    fn classic_db_segmentation_matches_self_authored_boundary_fixture() {
        let values = parse_fixture_values::<f32>(DB_BOUNDARY_INPUT, "DB boundary input");
        let expected = parse_fixture_values::<u8>(DB_BOUNDARY_EXPECTED, "DB boundary expected");
        let map = must_ok(DetectorProbabilityMap::new(dimensions(3, 2), &values));

        let bitmap = must_ok(classic_db_binary_segmentation(map));

        assert_eq!(bitmap.dimensions(), dimensions(3, 2));
        assert_eq!(bitmap.values(), expected);
    }

    #[test]
    fn detector_probability_map_rejects_a_wrong_value_count() {
        let result = DetectorProbabilityMap::new(dimensions(2, 2), &[0.0, 0.0, 0.0]);

        assert!(matches!(
            result,
            Err(Error::Model {
                problem: ModelProblem::TensorContract,
            })
        ));
    }

    #[test]
    fn detector_probability_map_rejects_non_finite_values() {
        for values in [
            [0.0, f32::NAN, 0.0, 0.0],
            [0.0, f32::INFINITY, 0.0, 0.0],
            [0.0, f32::NEG_INFINITY, 0.0, 0.0],
        ] {
            let result = DetectorProbabilityMap::new(dimensions(2, 2), &values);

            assert!(matches!(
                result,
                Err(Error::Model {
                    problem: ModelProblem::TensorContract,
                })
            ));
        }
    }
}
