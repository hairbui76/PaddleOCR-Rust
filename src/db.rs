// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Private bounded DB probability-map primitives for the classic M2 contract.

use crate::{
    error::{Error, ModelProblem, Result},
    types::ImageDimensions,
};

const CLASSIC_DB_SEGMENTATION_THRESHOLD: f32 = 0.3;
const CLASSIC_DB_MAX_COMPONENTS: usize = 1_000;
const CLASSIC_DB_MAX_COMPONENT_FRONTIER_SEEDS: usize = 1_000_000;

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

/// One bounded 8-connected foreground component in a private binary bitmap.
///
/// Bounds are inclusive bitmap-coordinate limits. This is deliberately not an
/// OpenCV contour: it retains neither boundary points nor hole/hierarchy,
/// retrieval, simplification, or ordering semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BinaryComponent {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    pixel_count: u64,
}

impl BinaryComponent {
    /// Returns the inclusive minimum horizontal bitmap coordinate.
    #[must_use]
    pub(crate) const fn min_x(self) -> u32 {
        self.min_x
    }

    /// Returns the inclusive minimum vertical bitmap coordinate.
    #[must_use]
    pub(crate) const fn min_y(self) -> u32 {
        self.min_y
    }

    /// Returns the inclusive maximum horizontal bitmap coordinate.
    #[must_use]
    pub(crate) const fn max_x(self) -> u32 {
        self.max_x
    }

    /// Returns the inclusive maximum vertical bitmap coordinate.
    #[must_use]
    pub(crate) const fn max_y(self) -> u32 {
        self.max_y
    }

    /// Returns the number of foreground pixels in this component.
    #[must_use]
    pub(crate) const fn pixel_count(self) -> u64 {
        self.pixel_count
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

/// Scans deterministic 8-connected foreground components in one binary bitmap.
///
/// Components are emitted in the row-major order of their first foreground
/// seed. The scan copies the already-bounded bitmap into mutable scratch space,
/// marks visited pixels there, and uses a scanline frontier to avoid storing a
/// per-pixel coordinate list. It caps the number of components and pending
/// scanline seeds with explicit errors rather than silently dropping regions or
/// attempting unbounded allocations.
///
/// This is only a private precursor for DB postprocessing. It does not emulate
/// OpenCV `findContours`, `RETR_LIST`, `CHAIN_APPROX_SIMPLE`, contour holes or
/// hierarchy, min-area rectangles, scores, offsets, or detector boxes.
pub(crate) fn classic_db_connected_components(
    bitmap: &BinaryBitmap,
) -> Result<Vec<BinaryComponent>> {
    let dimensions = bitmap.dimensions();
    let mut working = Vec::new();
    working
        .try_reserve_exact(bitmap.values().len())
        .map_err(|_| Error::Backend {
            message: "DB component scratch allocation failed",
        })?;
    working.extend_from_slice(bitmap.values());

    let component_capacity = usize::try_from(dimensions.pixels())
        .unwrap_or(usize::MAX)
        .min(CLASSIC_DB_MAX_COMPONENTS);
    let mut components = Vec::new();
    components
        .try_reserve_exact(component_capacity)
        .map_err(|_| Error::Backend {
            message: "DB component result allocation failed",
        })?;

    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            let start = bitmap_offset(dimensions, x, y);
            if working[start] != 1 {
                continue;
            }
            let next_component_count = components.len() + 1;
            if next_component_count > CLASSIC_DB_MAX_COMPONENTS {
                return Err(Error::ResourceLimit {
                    resource: "detector.components",
                    limit: CLASSIC_DB_MAX_COMPONENTS as u64,
                    actual: next_component_count as u64,
                });
            }

            working[start] = 2;
            components.push(scan_component(&mut working, dimensions, x, y)?);
        }
    }

    debug_assert!(working.iter().all(|value| *value != 2));
    Ok(components)
}

fn scan_component(
    working: &mut [u8],
    dimensions: ImageDimensions,
    initial_x: u32,
    initial_y: u32,
) -> Result<BinaryComponent> {
    let mut frontier = Vec::new();
    push_component_seed(&mut frontier, initial_x, initial_y)?;

    let mut min_x = initial_x;
    let mut min_y = initial_y;
    let mut max_x = initial_x;
    let mut max_y = initial_y;
    let mut pixel_count = 0_u64;

    while let Some((seed_x, y)) = frontier.pop() {
        let seed = bitmap_offset(dimensions, seed_x, y);
        if working[seed] == 0 {
            continue;
        }

        let mut left = seed_x;
        while left > 0 && working[bitmap_offset(dimensions, left - 1, y)] != 0 {
            left -= 1;
        }
        let mut right = seed_x;
        while right + 1 < dimensions.width()
            && working[bitmap_offset(dimensions, right + 1, y)] != 0
        {
            right += 1;
        }

        for x in left..=right {
            let offset = bitmap_offset(dimensions, x, y);
            debug_assert_ne!(working[offset], 0);
            working[offset] = 0;
        }
        min_x = min_x.min(left);
        max_x = max_x.max(right);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        pixel_count += u64::from(right - left + 1);

        let neighbor_start = left.saturating_sub(1);
        let neighbor_end = right.saturating_add(1).min(dimensions.width() - 1);
        if y > 0 {
            enqueue_neighbor_runs(
                working,
                dimensions,
                y - 1,
                neighbor_start,
                neighbor_end,
                &mut frontier,
            )?;
        }
        if y + 1 < dimensions.height() {
            enqueue_neighbor_runs(
                working,
                dimensions,
                y + 1,
                neighbor_start,
                neighbor_end,
                &mut frontier,
            )?;
        }
    }

    Ok(BinaryComponent {
        min_x,
        min_y,
        max_x,
        max_y,
        pixel_count,
    })
}

fn enqueue_neighbor_runs(
    working: &mut [u8],
    dimensions: ImageDimensions,
    y: u32,
    start_x: u32,
    end_x: u32,
    frontier: &mut Vec<(u32, u32)>,
) -> Result<()> {
    debug_assert!(start_x <= end_x);
    let mut x = start_x;
    while x <= end_x {
        let offset = bitmap_offset(dimensions, x, y);
        if working[offset] == 0 {
            x += 1;
            continue;
        }

        let seed_x = x;
        let was_unqueued = working[offset] == 1;
        while x <= end_x && working[bitmap_offset(dimensions, x, y)] != 0 {
            let run_offset = bitmap_offset(dimensions, x, y);
            if working[run_offset] == 1 {
                working[run_offset] = 2;
            }
            x += 1;
        }
        if was_unqueued {
            push_component_seed(frontier, seed_x, y)?;
        }
    }
    Ok(())
}

fn push_component_seed(frontier: &mut Vec<(u32, u32)>, x: u32, y: u32) -> Result<()> {
    let next_length = frontier.len() + 1;
    if next_length > CLASSIC_DB_MAX_COMPONENT_FRONTIER_SEEDS {
        return Err(Error::ResourceLimit {
            resource: "detector.component_frontier_seeds",
            limit: CLASSIC_DB_MAX_COMPONENT_FRONTIER_SEEDS as u64,
            actual: next_length as u64,
        });
    }
    frontier.try_reserve(1).map_err(|_| Error::Backend {
        message: "DB component frontier allocation failed",
    })?;
    frontier.push((x, y));
    Ok(())
}

fn bitmap_offset(dimensions: ImageDimensions, x: u32, y: u32) -> usize {
    debug_assert!(x < dimensions.width());
    debug_assert!(y < dimensions.height());
    y as usize * dimensions.width() as usize + x as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const DB_BOUNDARY_INPUT: &str =
        include_str!("../tests/fixtures/classic-v1-db-map-boundaries/input.csv");
    const DB_BOUNDARY_EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-db-map-boundaries/expected.csv");
    const DB_COMPONENT_INPUT: &str =
        include_str!("../tests/fixtures/classic-v1-db-components/input.csv");
    const DB_COMPONENT_EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-db-components/expected.csv");

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

    fn binary_bitmap(width: u32, height: u32, values: &[u8]) -> BinaryBitmap {
        BinaryBitmap {
            dimensions: dimensions(width, height),
            values: values.to_vec(),
        }
    }

    fn component(
        min_x: u32,
        min_y: u32,
        max_x: u32,
        max_y: u32,
        pixel_count: u64,
    ) -> BinaryComponent {
        BinaryComponent {
            min_x,
            min_y,
            max_x,
            max_y,
            pixel_count,
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

    fn parse_component_fixture(fixture: &str) -> Vec<BinaryComponent> {
        let values = parse_fixture_values::<u64>(fixture, "DB component expected");
        assert_eq!(
            values.len() % 5,
            0,
            "DB component expected must have five numeric fields per row"
        );
        values
            .chunks_exact(5)
            .map(|row| {
                component(
                    fixture_u32(row[0], "component min_x"),
                    fixture_u32(row[1], "component min_y"),
                    fixture_u32(row[2], "component max_x"),
                    fixture_u32(row[3], "component max_y"),
                    row[4],
                )
            })
            .collect()
    }

    fn fixture_u32(value: u64, field: &str) -> u32 {
        match u32::try_from(value) {
            Ok(value) => value,
            Err(_) => panic!("DB component expected {field} does not fit u32: {value}"),
        }
    }

    fn reference_components(width: u32, height: u32, values: &[u8]) -> Vec<BinaryComponent> {
        let mut visited = vec![false; values.len()];
        let mut components = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let start = y as usize * width as usize + x as usize;
                if values[start] == 0 || visited[start] {
                    continue;
                }

                let mut queue = VecDeque::from([(x, y)]);
                visited[start] = true;
                let mut min_x = x;
                let mut min_y = y;
                let mut max_x = x;
                let mut max_y = y;
                let mut pixel_count = 0_u64;
                while let Some((current_x, current_y)) = queue.pop_front() {
                    min_x = min_x.min(current_x);
                    min_y = min_y.min(current_y);
                    max_x = max_x.max(current_x);
                    max_y = max_y.max(current_y);
                    pixel_count += 1;

                    for neighbor_y in
                        current_y.saturating_sub(1)..=current_y.saturating_add(1).min(height - 1)
                    {
                        for neighbor_x in
                            current_x.saturating_sub(1)..=current_x.saturating_add(1).min(width - 1)
                        {
                            let neighbor =
                                neighbor_y as usize * width as usize + neighbor_x as usize;
                            if values[neighbor] != 0 && !visited[neighbor] {
                                visited[neighbor] = true;
                                queue.push_back((neighbor_x, neighbor_y));
                            }
                        }
                    }
                }
                components.push(component(min_x, min_y, max_x, max_y, pixel_count));
            }
        }
        components
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
    fn classic_db_components_match_self_authored_fixture() {
        let values = parse_fixture_values::<u8>(DB_COMPONENT_INPUT, "DB component input");
        let expected = parse_component_fixture(DB_COMPONENT_EXPECTED);
        let bitmap = binary_bitmap(9, 7, &values);

        let components = must_ok(classic_db_connected_components(&bitmap));

        assert_eq!(components, expected);
        assert_eq!(bitmap.values(), values);
    }

    #[test]
    fn classic_db_components_leave_an_empty_bitmap_empty() {
        let bitmap = binary_bitmap(3, 2, &[0, 0, 0, 0, 0, 0]);

        let components = must_ok(classic_db_connected_components(&bitmap));

        assert!(components.is_empty());
    }

    #[test]
    fn classic_db_components_match_an_independent_exhaustive_three_by_three_reference() {
        for mask in 0_u16..(1 << 9) {
            let values = (0..9)
                .map(|bit| u8::from((mask & (1 << bit)) != 0))
                .collect::<Vec<_>>();
            let bitmap = binary_bitmap(3, 3, &values);

            let actual = must_ok(classic_db_connected_components(&bitmap));
            let expected = reference_components(3, 3, &values);

            assert_eq!(actual, expected, "mask {mask:#011b}");
        }
    }

    #[test]
    fn classic_db_components_match_an_independent_exhaustive_four_by_four_reference() {
        for mask in 0_u32..(1_u32 << 16) {
            let values = (0..16)
                .map(|bit| u8::from((mask & (1_u32 << bit)) != 0))
                .collect::<Vec<_>>();
            let bitmap = binary_bitmap(4, 4, &values);

            let actual = must_ok(classic_db_connected_components(&bitmap));
            let expected = reference_components(4, 4, &values);

            assert_eq!(actual, expected, "mask {mask:#018b}");
        }
    }

    #[test]
    fn classic_db_components_reject_an_excess_of_isolated_regions() {
        let mut values = vec![0_u8; 2_001];
        for value in values.iter_mut().step_by(2) {
            *value = 1;
        }
        let bitmap = binary_bitmap(2_001, 1, &values);

        let result = classic_db_connected_components(&bitmap);

        assert!(matches!(
            result,
            Err(Error::ResourceLimit {
                resource: "detector.components",
                limit: 1_000,
                actual: 1_001,
            })
        ));
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
