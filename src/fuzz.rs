// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Byte-driven developer-only fuzz entry points for current pure kernels.
//!
//! This module is available only with the internal fuzzing feature. It is
//! intentionally not an OCR API, model loader, decoder, or normal runtime
//! dependency. The bounded driver is exposed so an external stdin-oriented
//! fuzzer can exercise private checked kernels without widening their public
//! production interfaces.

use crate::{
    crop::{InterleavedImage, classic_perspective_crop},
    ctc::{CtcScoreMatrix, classic_ctc_greedy_indices},
    db::{DetectorProbabilityMap, classic_db_binary_segmentation, classic_db_connected_components},
    geometry::{
        classic_detector_resize_plan, classic_order_clip_filter_quad,
        classic_perspective_crop_plan, classic_rescale_detector_quad, classic_sort_quadrilaterals,
        minimum_area_quad_candidate, polygon_area, polygon_perimeter, polygon_signed_area,
    },
    types::{
        EncodedImage, ImageDimensions, ImageTransform, ModelIdentity, ModelTask, Point, Polygon,
        Quadrilateral, Score,
    },
};

/// Maximum byte count consumed by one fuzz-target invocation.
pub const MAX_INPUT_BYTES: usize = 16 * 1024;

const MAX_DRIVER_SIDE: u32 = 32;
const MAX_CROP_SIDE: u32 = 16;
const MAX_FUZZ_POLYGON_VERTICES: usize = 10;

/// Exercises current bounded pure processing surfaces with one arbitrary byte input.
///
/// Malformed values are deliberately passed to checked constructors and are
/// expected to return typed errors. The driver itself performs no I/O,
/// allocation derived from an unbounded input length, model loading, decoding,
/// inference, or result serialization.
pub fn exercise(input: &[u8]) {
    let input = &input[..input.len().min(MAX_INPUT_BYTES)];
    let mut reader = ByteReader::new(input);

    exercise_public_validators(&mut reader, input);
    exercise_db_kernels(&mut reader);
    exercise_ctc_kernel(&mut reader);
    exercise_geometry_and_crop_kernels(&mut reader);
}

fn exercise_public_validators(reader: &mut ByteReader<'_>, input: &[u8]) {
    let _ = EncodedImage::new(input);
    let _ = ImageDimensions::new(reader.next_u32(), reader.next_u32());
    let _ = Point::new(reader.next_f32(), reader.next_f32());
    let _ = Score::new(reader.next_f32());

    let mut identity_component = String::new();
    for _ in 0..16 {
        identity_component.push(char::from(reader.next_byte()));
    }
    let task = if reader.next_byte() & 1 == 0 {
        ModelTask::TextDetection
    } else {
        ModelTask::TextRecognition
    };
    let _ = ModelIdentity::new(task, identity_component, "fuzz");

    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    if let Ok(transform) = ImageTransform::new(
        dimensions,
        dimensions,
        reader.next_f32(),
        reader.next_f32(),
        reader.next_f32(),
        reader.next_f32(),
    ) && let Ok(point) = Point::new(reader.next_f32(), reader.next_f32())
    {
        let _ = transform.forward(point);
        let _ = transform.inverse(point);
    }
}

fn exercise_db_kernels(reader: &mut ByteReader<'_>) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    let value_count = dimensions.pixels() as usize;
    let values = (0..value_count)
        .map(|_| score_value(reader))
        .collect::<Vec<_>>();
    let wrong_length = values.len().saturating_sub(1);
    let _ = DetectorProbabilityMap::new(dimensions, &values[..wrong_length]);

    if let Ok(map) = DetectorProbabilityMap::new(dimensions, &values)
        && let Ok(bitmap) = classic_db_binary_segmentation(map)
    {
        let _ = classic_db_connected_components(&bitmap);
    }
}

fn exercise_ctc_kernel(reader: &mut ByteReader<'_>) {
    let _ = CtcScoreMatrix::new(reader.next_u32(), reader.next_u32(), &[]);

    let time_steps = u32::from(reader.next_byte() % 33);
    let class_count = u32::from(reader.next_byte() % 32) + 1;
    let value_count = time_steps as usize * class_count as usize;
    let values = (0..value_count)
        .map(|_| score_value(reader))
        .collect::<Vec<_>>();
    let wrong_length = values.len().saturating_sub(1);
    let _ = CtcScoreMatrix::new(time_steps, class_count, &values[..wrong_length]);

    if let Ok(matrix) = CtcScoreMatrix::new(time_steps, class_count, &values) {
        let _ = classic_ctc_greedy_indices(matrix);
    }
}

fn exercise_geometry_and_crop_kernels(reader: &mut ByteReader<'_>) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    let _ = classic_detector_resize_plan(dimensions);

    let arbitrary_points =
        core::array::from_fn(|_| Point::new(reader.next_f32(), reader.next_f32()));
    if let [Ok(first), Ok(second), Ok(third), Ok(fourth)] = arbitrary_points {
        let _ = Quadrilateral::new([first, second, third, fourth]);
    }

    let Some(quadrilateral) = bounded_quadrilateral(reader) else {
        return;
    };
    let points = quadrilateral.points();
    let _ = classic_order_clip_filter_quad(points, dimensions);
    let _ = classic_rescale_detector_quad(points, dimensions, dimensions);

    let mut reading_order = [quadrilateral, quadrilateral];
    classic_sort_quadrilaterals(&mut reading_order);

    if let Ok(polygon) = Polygon::new(points.to_vec()) {
        exercise_polygon_measurements(&polygon);
    }
    exercise_polygon_kernels(reader);

    if let Ok(plan) = classic_perspective_crop_plan(quadrilateral) {
        let _ = plan.map_source_to_warp(points[0]);
        let _ = plan.map_warp_to_source(points[0]);
        let _ =
            plan.map_warp_coordinates_to_source(f64::from(points[0].x()), f64::from(points[0].y()));
    }

    exercise_crop_kernel(reader, quadrilateral);
}

fn exercise_polygon_kernels(reader: &mut ByteReader<'_>) {
    // Keep every candidate bounded independently of the fuzzer input length.
    // These shapes deliberately cover construction rejection as well as the
    // convex-hull/minimum-area path on non-convex and repeated vertices.
    let short_count = usize::from(reader.next_byte() % 3);
    let Some(short_points) = bounded_polygon_points(reader, short_count) else {
        return;
    };
    let _ = Polygon::new(short_points);

    let collinear_count = 3 + usize::from(reader.next_byte() % 8);
    let Some(collinear_points) = horizontal_polygon_points(reader, collinear_count) else {
        return;
    };
    exercise_polygon_candidate(collinear_points);

    let Some(concave_points) = concave_polygon_points(reader) else {
        return;
    };
    exercise_polygon_candidate(concave_points.clone());

    let mut repeated_points = concave_points;
    repeated_points[2] = repeated_points[1];
    exercise_polygon_candidate(repeated_points);

    let arbitrary_count = 3 + usize::from(reader.next_byte() % 8);
    let Some(arbitrary_points) = bounded_polygon_points(reader, arbitrary_count) else {
        return;
    };
    exercise_polygon_candidate(arbitrary_points);
}

fn exercise_polygon_candidate(points: Vec<Point>) {
    if let Ok(polygon) = Polygon::new(points) {
        exercise_polygon_measurements(&polygon);
    }
}

fn exercise_polygon_measurements(polygon: &Polygon) {
    let _ = polygon_signed_area(polygon);
    let _ = polygon_area(polygon);
    let _ = polygon_perimeter(polygon);
    let _ = minimum_area_quad_candidate(polygon);
}

fn bounded_polygon_points(reader: &mut ByteReader<'_>, count: usize) -> Option<Vec<Point>> {
    debug_assert!(count <= MAX_FUZZ_POLYGON_VERTICES);
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        points.push(bounded_polygon_point(reader)?);
    }
    Some(points)
}

fn horizontal_polygon_points(reader: &mut ByteReader<'_>, count: usize) -> Option<Vec<Point>> {
    debug_assert!((3..=MAX_FUZZ_POLYGON_VERTICES).contains(&count));
    let left = bounded_polygon_coordinate(reader);
    let y = bounded_polygon_coordinate(reader);
    let mut points = Vec::with_capacity(count);
    for offset in 0..count {
        points.push(Point::new(left + offset as f32, y).ok()?);
    }
    Some(points)
}

fn concave_polygon_points(reader: &mut ByteReader<'_>) -> Option<Vec<Point>> {
    let left = bounded_polygon_coordinate(reader);
    let top = bounded_polygon_coordinate(reader);
    let width = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let height = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let right = left + width;
    let bottom = top + height;
    let notch_x = left + width * 0.5;
    let notch_y = top + height * 0.45;

    [
        Point::new(left, top).ok(),
        Point::new(right, top).ok(),
        Point::new(right, bottom).ok(),
        Point::new(notch_x, notch_y).ok(),
        Point::new(left, bottom).ok(),
    ]
    .into_iter()
    .collect()
}

fn bounded_polygon_point(reader: &mut ByteReader<'_>) -> Option<Point> {
    Point::new(
        bounded_polygon_coordinate(reader),
        bounded_polygon_coordinate(reader),
    )
    .ok()
}

fn bounded_polygon_coordinate(reader: &mut ByteReader<'_>) -> f32 {
    f32::from(reader.next_byte()) / 8.0 - 16.0
}

fn exercise_crop_kernel(reader: &mut ByteReader<'_>, quadrilateral: Quadrilateral) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_CROP_SIDE) else {
        return;
    };
    let channels = reader.next_byte() % 4 + 1;
    let byte_count = dimensions.pixels() as usize * usize::from(channels);
    let pixels = (0..byte_count)
        .map(|_| reader.next_byte())
        .collect::<Vec<_>>();

    let Some(source) = InterleavedImage::new(dimensions, channels, pixels).ok() else {
        return;
    };
    if let Ok(plan) = classic_perspective_crop_plan(quadrilateral) {
        let _ = classic_perspective_crop(&source, plan);
    }
}

fn bounded_dimensions(reader: &mut ByteReader<'_>, maximum_side: u32) -> Option<ImageDimensions> {
    let width = u32::from(reader.next_byte()) % maximum_side + 1;
    let height = u32::from(reader.next_byte()) % maximum_side + 1;
    ImageDimensions::new(width, height).ok()
}

fn bounded_quadrilateral(reader: &mut ByteReader<'_>) -> Option<Quadrilateral> {
    let left = f32::from(reader.next_byte()) / 8.0 - 16.0;
    let top = f32::from(reader.next_byte()) / 8.0 - 16.0;
    let width = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let height = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let points = if reader.next_byte() & 1 == 0 {
        [
            Point::new(left, top),
            Point::new(left + width, top),
            Point::new(left + width, top + height),
            Point::new(left, top + height),
        ]
    } else {
        let top_inset = width * (f32::from(reader.next_byte() % 48) / 128.0);
        let top_shift = width * (f32::from(reader.next_byte()) / 256.0 - 0.5);
        [
            Point::new(left + top_shift + top_inset, top),
            Point::new(left + top_shift + width - top_inset, top),
            Point::new(left + width, top + height),
            Point::new(left, top + height),
        ]
    };
    let [Ok(first), Ok(second), Ok(third), Ok(fourth)] = points else {
        return None;
    };
    Quadrilateral::new([first, second, third, fourth]).ok()
}

fn score_value(reader: &mut ByteReader<'_>) -> f32 {
    match reader.next_byte() & 0x0f {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => f32::NEG_INFINITY,
        _ => (f32::from(reader.next_byte()) - 128.0) / 32.0,
    }
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn next_byte(&mut self) -> u8 {
        if self.bytes.is_empty() {
            return 0;
        }
        let value = self.bytes[self.offset % self.bytes.len()];
        self.offset = self.offset.wrapping_add(1);
        value
    }

    fn next_u32(&mut self) -> u32 {
        u32::from_le_bytes(core::array::from_fn(|_| self.next_byte()))
    }

    fn next_f32(&mut self) -> f32 {
        f32::from_bits(self.next_u32())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_INPUT_BYTES, exercise};

    const GENERATED_STRESS_CASES: usize = 4_096;
    const MUTATION_CAMPAIGN_CASES: usize = 2_048;
    const MUTATION_OPERATIONS_PER_CASE: usize = 8;
    const MUTATION_SEEDS: &[&[u8]] = &[
        b"",
        b"\x00",
        b"\xff",
        b"\x00\x01\x7f\x80\xfe\xff",
        b"\x00\x00\x00\x00\x01\x00\x00\x00\xff\xff\x7f\x7f\x00\x00\x80\x7f",
        b"PaddleOCR-Rust bounded primitive mutation seed",
    ];
    const BYTE_BOUNDARIES: [u8; 8] = [0, 1, 2, 0x7f, 0x80, 0xfe, 0xff, 0x55];
    const FLOAT_BOUNDARIES: [u32; 8] = [
        0x0000_0000,
        0x0000_0001,
        0x3f80_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7fc0_0000,
        0xff80_0000,
        0xffff_ffff,
    ];

    #[test]
    fn byte_driven_fuzz_driver_handles_bounded_seed_corpus() {
        exercise(&[]);
        for seed in 0_u8..=127 {
            let mut input = [0_u8; 97];
            for (index, value) in input.iter_mut().enumerate() {
                *value = seed
                    .wrapping_mul(37)
                    .wrapping_add(index as u8)
                    .rotate_left((index % 8) as u32);
            }
            exercise(&input);
        }
        exercise(&vec![0xA5; MAX_INPUT_BYTES + 1]);
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_generated_stress_corpus() {
        for case_index in 0..GENERATED_STRESS_CASES {
            let length = match case_index {
                0 => 0,
                1 => MAX_INPUT_BYTES,
                _ => (case_index * 193) % (MAX_INPUT_BYTES - 1) + 1,
            };
            let mut state = 0x9E37_79B9_u32 ^ case_index as u32;
            let mut input = Vec::with_capacity(length);
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                input.push((state >> 24) as u8);
            }
            exercise(&input);
        }
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_deterministic_mutation_campaign() {
        for case_index in 0..MUTATION_CAMPAIGN_CASES {
            let seed = MUTATION_SEEDS[case_index % MUTATION_SEEDS.len()];
            let mut input = seed.to_vec();
            let mut state = 0xD1B5_4A35_u32 ^ case_index as u32;

            for operation_index in 0..MUTATION_OPERATIONS_PER_CASE {
                mutate_input(&mut input, &mut state, (case_index + operation_index) % 7);
                assert!(
                    input.len() <= MAX_INPUT_BYTES,
                    "mutation case {case_index} exceeded its input bound"
                );
            }

            exercise(&input);
        }
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_bounded_polygon_variants() {
        for selector in 0_u8..=u8::MAX {
            let mut input = [0_u8; 97];
            for (index, value) in input.iter_mut().enumerate() {
                *value = selector
                    .wrapping_add((index as u8).wrapping_mul(29))
                    .rotate_left((index % 8) as u32);
            }
            exercise(&input);
        }
    }

    fn mutate_input(input: &mut Vec<u8>, state: &mut u32, mutation_kind: usize) {
        match mutation_kind {
            0 => flip_one_bit(input, state),
            1 => overwrite_one_byte(input, state),
            2 => insert_one_byte(input, state),
            3 => remove_one_byte(input, state),
            4 => duplicate_one_byte(input, state),
            5 => inject_float_boundary(input, state),
            6 => truncate_input(input, state),
            _ => unreachable!("mutation kind is reduced modulo seven"),
        }
    }

    fn flip_one_bit(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            input.push(1_u8 << (next_word(state) % 8));
            return;
        }
        let index = next_index(state, input.len());
        input[index] ^= 1_u8 << (next_word(state) % 8);
    }

    fn overwrite_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        let value = BYTE_BOUNDARIES[next_index(state, BYTE_BOUNDARIES.len())];
        if input.is_empty() {
            input.push(value);
            return;
        }
        let index = next_index(state, input.len());
        input[index] = value;
    }

    fn insert_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.len() == MAX_INPUT_BYTES {
            return;
        }
        let index = next_index(state, input.len() + 1);
        let value = BYTE_BOUNDARIES[next_index(state, BYTE_BOUNDARIES.len())];
        input.insert(index, value);
    }

    fn remove_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            return;
        }
        let index = next_index(state, input.len());
        input.remove(index);
    }

    fn duplicate_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() || input.len() == MAX_INPUT_BYTES {
            return;
        }
        let source = next_index(state, input.len());
        let destination = next_index(state, input.len() + 1);
        input.insert(destination, input[source]);
    }

    fn inject_float_boundary(input: &mut Vec<u8>, state: &mut u32) {
        let bits = FLOAT_BOUNDARIES[next_index(state, FLOAT_BOUNDARIES.len())];
        write_bounded_bytes(input, state, &bits.to_le_bytes());
    }

    fn truncate_input(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            return;
        }
        input.truncate(next_index(state, input.len() + 1));
    }

    fn write_bounded_bytes(input: &mut Vec<u8>, state: &mut u32, bytes: &[u8]) {
        let start = next_index(state, input.len() + 1);
        for (offset, byte) in bytes.iter().copied().enumerate() {
            let index = start + offset;
            if index < input.len() {
                input[index] = byte;
            } else if input.len() < MAX_INPUT_BYTES {
                input.push(byte);
            } else {
                let replacement = next_index(state, input.len());
                input[replacement] = byte;
            }
        }
    }

    fn next_index(state: &mut u32, length: usize) -> usize {
        debug_assert!(length > 0);
        next_word(state) as usize % length
    }

    fn next_word(state: &mut u32) -> u32 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *state
    }
}
