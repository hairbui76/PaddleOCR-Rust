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
        let _ = polygon_signed_area(&polygon);
        let _ = polygon_area(&polygon);
        let _ = polygon_perimeter(&polygon);
        let _ = minimum_area_quad_candidate(&polygon);
    }

    if let Ok(plan) = classic_perspective_crop_plan(quadrilateral) {
        let _ = plan.map_source_to_warp(points[0]);
        let _ = plan.map_warp_to_source(points[0]);
        let _ =
            plan.map_warp_coordinates_to_source(f64::from(points[0].x()), f64::from(points[0].y()));
    }

    exercise_crop_kernel(reader);
}

fn exercise_crop_kernel(reader: &mut ByteReader<'_>) {
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
    let width = dimensions.width() as f32;
    let height = dimensions.height() as f32;
    let points = [
        Point::new(0.0, 0.0),
        Point::new(width, 0.0),
        Point::new(width, height),
        Point::new(0.0, height),
    ];
    let [Ok(first), Ok(second), Ok(third), Ok(fourth)] = points else {
        return;
    };
    let Ok(quadrilateral) = Quadrilateral::new([first, second, third, fourth]) else {
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
    let points = [
        Point::new(left, top),
        Point::new(left + width, top),
        Point::new(left + width, top + height),
        Point::new(left, top + height),
    ];
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
}
