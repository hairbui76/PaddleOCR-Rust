// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for the public foundation API and fixture helpers.

mod support;

use paddleocr_rust::{
    EncodedImage, Error, ImageDimensions, ImageTransform, InputViolation, ModelIdentity, ModelTask,
    Point, RecognizedText, Score, VERSION,
};
use support::assert_absolute_difference;

fn must_ok<T>(result: paddleocr_rust::Result<T>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error}"),
    }
}

#[test]
fn public_foundations_preserve_checked_values() {
    let encoded = must_ok(EncodedImage::new(b"not-decoded-yet"));
    assert_eq!(encoded.len(), 15);

    let dimensions = must_ok(ImageDimensions::new(1_280, 720));
    assert_eq!(dimensions.pixels(), 921_600);

    let source = must_ok(ImageDimensions::new(640, 480));
    let transform = must_ok(ImageTransform::new(source, dimensions, 2.0, 1.5, 0.0, 0.0));
    let original = must_ok(Point::new(16.0, 24.0));
    let restored = must_ok(transform.inverse(must_ok(transform.forward(original))));
    assert_absolute_difference(restored.x(), original.x(), f32::EPSILON);
    assert_absolute_difference(restored.y(), original.y(), f32::EPSILON);

    let score = must_ok(Score::new(0.75));
    assert_absolute_difference(score.value(), 0.75, f32::EPSILON);

    let identity = must_ok(ModelIdentity::new(
        ModelTask::TextRecognition,
        "PP-OCRv6_medium_rec",
        "unresolved-artifact",
    ));
    assert_eq!(identity.family(), "PP-OCRv6_medium_rec");
    assert_eq!(RecognizedText::new("Tiếng Việt").as_str(), "Tiếng Việt");
    assert!(!VERSION.is_empty());
}

#[test]
fn public_foundations_return_structured_errors() {
    assert!(matches!(
        Score::new(-0.01),
        Err(Error::InvalidInput {
            field: "score",
            violation: InputViolation::OutOfRange,
        })
    ));
    assert!(matches!(
        EncodedImage::new(&[]),
        Err(Error::InvalidInput {
            field: "image.bytes",
            violation: InputViolation::Empty,
        })
    ));
}

#[test]
fn fixture_tolerance_helper_accepts_values_inside_its_declared_threshold() {
    assert_absolute_difference(0.75, 0.7505, 0.001);
}
