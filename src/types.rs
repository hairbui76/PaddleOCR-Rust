// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Checked domain types for later image, geometry, model, and pipeline code.

use crate::error::{Error, InputViolation, Result};

/// The maximum supported encoded image size in bytes for the M2 scope.
pub const MAX_ENCODED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// The maximum supported decoded image side length in pixels for the M2 scope.
pub const MAX_IMAGE_SIDE_LENGTH: u32 = 16_384;
/// The maximum supported decoded image pixel count for the M2 scope.
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;
/// The maximum number of vertices retained by one polygon.
pub const MAX_POLYGON_VERTICES: usize = 4_096;
/// The maximum UTF-8 byte length of a model identity component.
pub const MAX_MODEL_IDENTITY_COMPONENT_BYTES: usize = 128;

/// A non-empty, bounded borrowed encoded-image byte slice.
///
/// This type validates only the byte boundary. Image format detection, decoding,
/// dimensions, metadata, and pixel limits are later image-pipeline work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedImage<'a> {
    bytes: &'a [u8],
}

impl<'a> EncodedImage<'a> {
    /// Constructs a non-empty encoded image within the M2 byte limit.
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        validate_encoded_image_length(bytes.len())?;
        Ok(Self { bytes })
    }

    /// Returns the borrowed encoded bytes.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the validated encoded byte length.
    #[must_use]
    pub const fn len(self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the encoded byte slice is empty.
    ///
    /// Values constructed by [`EncodedImage::new`] are never empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
}

/// Non-zero decoded image dimensions bounded by the M2 resource policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageDimensions {
    width: u32,
    height: u32,
}

impl ImageDimensions {
    /// Constructs bounded non-zero image dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 {
            return Err(Error::InvalidInput {
                field: "image.width",
                violation: InputViolation::Empty,
            });
        }
        if height == 0 {
            return Err(Error::InvalidInput {
                field: "image.height",
                violation: InputViolation::Empty,
            });
        }
        if width > MAX_IMAGE_SIDE_LENGTH {
            return Err(Error::ResourceLimit {
                resource: "image.width_pixels",
                limit: u64::from(MAX_IMAGE_SIDE_LENGTH),
                actual: u64::from(width),
            });
        }
        if height > MAX_IMAGE_SIDE_LENGTH {
            return Err(Error::ResourceLimit {
                resource: "image.height_pixels",
                limit: u64::from(MAX_IMAGE_SIDE_LENGTH),
                actual: u64::from(height),
            });
        }

        let pixels = u64::from(width) * u64::from(height);
        if pixels > MAX_IMAGE_PIXELS {
            return Err(Error::ResourceLimit {
                resource: "image.total_pixels",
                limit: MAX_IMAGE_PIXELS,
                actual: pixels,
            });
        }

        Ok(Self { width, height })
    }

    /// Returns the image width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the image height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the validated total pixel count.
    #[must_use]
    pub const fn pixels(self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// A finite source-image coordinate in pixels, with origin at the top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    x: f32,
    y: f32,
}

impl Point {
    /// Constructs a finite pixel coordinate.
    pub fn new(x: f32, y: f32) -> Result<Self> {
        if !x.is_finite() {
            return Err(Error::InvalidInput {
                field: "point.x",
                violation: InputViolation::NonFinite,
            });
        }
        if !y.is_finite() {
            return Err(Error::InvalidInput {
                field: "point.y",
                violation: InputViolation::NonFinite,
            });
        }
        Ok(Self { x, y })
    }

    /// Returns the horizontal coordinate in pixels.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the vertical coordinate in pixels.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
}

/// A confidence score in the closed interval `[0.0, 1.0]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score(f32);

impl Score {
    /// Constructs a finite confidence score in `[0.0, 1.0]`.
    pub fn new(value: f32) -> Result<Self> {
        if !value.is_finite() {
            return Err(Error::InvalidInput {
                field: "score",
                violation: InputViolation::NonFinite,
            });
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(Error::InvalidInput {
                field: "score",
                violation: InputViolation::OutOfRange,
            });
        }
        Ok(Self(value))
    }

    /// Returns the validated score value.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }
}

/// An ordered non-degenerate four-corner source-image region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quadrilateral {
    points: [Point; 4],
}

impl Quadrilateral {
    /// Constructs a strictly convex quadrilateral with four distinct ordered vertices.
    pub fn new(points: [Point; 4]) -> Result<Self> {
        if !has_distinct_points(&points) || !is_strictly_convex(&points) {
            return Err(Error::InvalidInput {
                field: "quadrilateral",
                violation: InputViolation::DegenerateGeometry,
            });
        }
        Ok(Self { points })
    }

    /// Returns the vertex order supplied by the caller.
    #[must_use]
    pub const fn points(self) -> [Point; 4] {
        self.points
    }
}

/// An ordered non-degenerate source-image polygon with bounded vertex count.
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    points: Vec<Point>,
}

impl Polygon {
    /// Constructs a polygon with at least three ordered, non-collinear vertices.
    pub fn new(points: Vec<Point>) -> Result<Self> {
        if points.len() < 3 {
            return Err(Error::InvalidInput {
                field: "polygon.points",
                violation: InputViolation::DegenerateGeometry,
            });
        }
        if points.len() > MAX_POLYGON_VERTICES {
            return Err(Error::ResourceLimit {
                resource: "polygon.vertices",
                limit: MAX_POLYGON_VERTICES as u64,
                actual: points.len() as u64,
            });
        }
        if twice_signed_area(&points).abs() <= f64::EPSILON {
            return Err(Error::InvalidInput {
                field: "polygon",
                violation: InputViolation::DegenerateGeometry,
            });
        }
        Ok(Self { points })
    }

    /// Returns the ordered polygon vertices.
    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }
}

/// A checked scale-and-translation map from source pixels to destination pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageTransform {
    source: ImageDimensions,
    destination: ImageDimensions,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
}

impl ImageTransform {
    /// Constructs a finite transform with strictly positive scale factors.
    pub fn new(
        source: ImageDimensions,
        destination: ImageDimensions,
        scale_x: f32,
        scale_y: f32,
        translate_x: f32,
        translate_y: f32,
    ) -> Result<Self> {
        for (field, value) in [
            ("transform.scale_x", scale_x),
            ("transform.scale_y", scale_y),
            ("transform.translate_x", translate_x),
            ("transform.translate_y", translate_y),
        ] {
            if !value.is_finite() {
                return Err(Error::InvalidInput {
                    field,
                    violation: InputViolation::NonFinite,
                });
            }
        }
        if scale_x <= 0.0 {
            return Err(Error::InvalidInput {
                field: "transform.scale_x",
                violation: InputViolation::OutOfRange,
            });
        }
        if scale_y <= 0.0 {
            return Err(Error::InvalidInput {
                field: "transform.scale_y",
                violation: InputViolation::OutOfRange,
            });
        }
        Ok(Self {
            source,
            destination,
            scale_x,
            scale_y,
            translate_x,
            translate_y,
        })
    }

    /// Returns the source image dimensions associated with this transform.
    #[must_use]
    pub const fn source(self) -> ImageDimensions {
        self.source
    }

    /// Returns the destination image dimensions associated with this transform.
    #[must_use]
    pub const fn destination(self) -> ImageDimensions {
        self.destination
    }

    /// Maps a source-image point into destination coordinates.
    pub fn forward(self, point: Point) -> Result<Point> {
        transformed_point(
            point,
            f64::from(self.scale_x),
            f64::from(self.scale_y),
            f64::from(self.translate_x),
            f64::from(self.translate_y),
        )
    }

    /// Maps a destination-image point back into source coordinates.
    pub fn inverse(self, point: Point) -> Result<Point> {
        let x = (f64::from(point.x()) - f64::from(self.translate_x)) / f64::from(self.scale_x);
        let y = (f64::from(point.y()) - f64::from(self.translate_y)) / f64::from(self.scale_y);
        point_from_f64(x, y, "transform.inverse")
    }
}

/// A zero-based page number in a multi-page input.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PageIndex(u32);

impl PageIndex {
    /// Constructs a zero-based page index.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the zero-based index value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// A task category used in a model identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelTask {
    /// A text detector that identifies source-image text regions.
    TextDetection,
    /// A text recognizer that decodes a cropped text region.
    TextRecognition,
}

/// A bounded, non-empty identity for a model family and version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelIdentity {
    task: ModelTask,
    family: String,
    version: String,
}

impl ModelIdentity {
    /// Constructs a model identity from bounded non-empty components.
    pub fn new(
        task: ModelTask,
        family: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self> {
        let family = family.into();
        validate_model_identity_component("model.family", &family)?;
        let version = version.into();
        validate_model_identity_component("model.version", &version)?;
        Ok(Self {
            task,
            family,
            version,
        })
    }

    /// Returns the model task.
    #[must_use]
    pub const fn task(&self) -> ModelTask {
        self.task
    }

    /// Returns the model family identifier.
    #[must_use]
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Returns the model version identifier.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// UTF-8 text returned by a recognizer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecognizedText(String);

impl RecognizedText {
    /// Constructs recognized UTF-8 text, including a valid empty recognition.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the recognized text as UTF-8.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes this wrapper and returns the owned UTF-8 text.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

fn twice_signed_area(points: &[Point]) -> f64 {
    let mut area = 0.0_f64;
    for (current, next) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        area += f64::from(current.x()) * f64::from(next.y());
        area -= f64::from(next.x()) * f64::from(current.y());
    }
    area
}

fn has_distinct_points(points: &[Point; 4]) -> bool {
    for first_index in 0..points.len() {
        for second_index in (first_index + 1)..points.len() {
            if points[first_index] == points[second_index] {
                return false;
            }
        }
    }
    true
}

fn is_strictly_convex(points: &[Point; 4]) -> bool {
    let mut orientation = 0_i8;
    for point_index in 0..points.len() {
        let first = points[point_index];
        let second = points[(point_index + 1) % points.len()];
        let third = points[(point_index + 2) % points.len()];
        let cross_product = cross_product(first, second, third);
        if cross_product.abs() <= f64::EPSILON {
            return false;
        }
        let current_orientation = if cross_product.is_sign_positive() {
            1
        } else {
            -1
        };
        if orientation == 0 {
            orientation = current_orientation;
        } else if orientation != current_orientation {
            return false;
        }
    }
    true
}

fn cross_product(first: Point, second: Point, third: Point) -> f64 {
    let first_to_second_x = f64::from(second.x()) - f64::from(first.x());
    let first_to_second_y = f64::from(second.y()) - f64::from(first.y());
    let second_to_third_x = f64::from(third.x()) - f64::from(second.x());
    let second_to_third_y = f64::from(third.y()) - f64::from(second.y());
    first_to_second_x * second_to_third_y - first_to_second_y * second_to_third_x
}

fn transformed_point(
    point: Point,
    scale_x: f64,
    scale_y: f64,
    translate_x: f64,
    translate_y: f64,
) -> Result<Point> {
    let x = f64::from(point.x()) * scale_x + translate_x;
    let y = f64::from(point.y()) * scale_y + translate_y;
    point_from_f64(x, y, "transform.forward")
}

fn point_from_f64(x: f64, y: f64, field: &'static str) -> Result<Point> {
    if !x.is_finite()
        || !y.is_finite()
        || x.abs() > f64::from(f32::MAX)
        || y.abs() > f64::from(f32::MAX)
    {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::OutOfRange,
        });
    }
    Point::new(x as f32, y as f32)
}

fn validate_model_identity_component(field: &'static str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::Empty,
        });
    }
    if value.len() > MAX_MODEL_IDENTITY_COMPONENT_BYTES {
        return Err(Error::ResourceLimit {
            resource: "model.identity_component_bytes",
            limit: MAX_MODEL_IDENTITY_COMPONENT_BYTES as u64,
            actual: value.len() as u64,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::InvalidIdentifier,
        });
    }
    Ok(())
}

fn validate_encoded_image_length(length: usize) -> Result<()> {
    if length == 0 {
        return Err(Error::InvalidInput {
            field: "image.bytes",
            violation: InputViolation::Empty,
        });
    }
    if length > MAX_ENCODED_IMAGE_BYTES {
        return Err(Error::ResourceLimit {
            resource: "image.encoded_bytes",
            limit: MAX_ENCODED_IMAGE_BYTES as u64,
            actual: length as u64,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::Value;
    use std::collections::BTreeSet;

    fn must_ok<T, E: std::fmt::Display>(result: std::result::Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected success, got {error}"),
        }
    }

    #[test]
    fn image_dimensions_enforce_zero_side_and_pixel_limits() {
        assert!(matches!(
            ImageDimensions::new(0, 1),
            Err(Error::InvalidInput {
                field: "image.width",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            ImageDimensions::new(MAX_IMAGE_SIDE_LENGTH + 1, 1),
            Err(Error::ResourceLimit {
                resource: "image.width_pixels",
                ..
            })
        ));
        assert!(matches!(
            ImageDimensions::new(10_000, 10_000),
            Err(Error::ResourceLimit {
                resource: "image.total_pixels",
                ..
            })
        ));
    }

    #[test]
    fn encoded_image_enforces_non_empty_and_byte_limits() {
        assert!(matches!(
            EncodedImage::new(&[]),
            Err(Error::InvalidInput {
                field: "image.bytes",
                violation: InputViolation::Empty,
            })
        ));
        let bytes = [0x89, b'P', b'N', b'G'];
        let encoded = must_ok(EncodedImage::new(&bytes));
        assert_eq!(encoded.bytes(), &bytes);
        assert_eq!(encoded.len(), bytes.len());
        assert!(matches!(
            validate_encoded_image_length(MAX_ENCODED_IMAGE_BYTES + 1),
            Err(Error::ResourceLimit {
                resource: "image.encoded_bytes",
                limit,
                actual,
            }) if limit == MAX_ENCODED_IMAGE_BYTES as u64
                && actual == MAX_ENCODED_IMAGE_BYTES as u64 + 1
        ));
    }

    #[test]
    fn self_authored_image_input_fixture_stays_within_encoded_byte_contract() {
        let document: Value = must_ok(serde_json::from_str(include_str!(
            "../tests/fixtures/classic-v1-image-inputs/capture.json"
        )));
        let cases = array_value(&document, "cases");
        let negative_cases = array_value(&document, "negative_cases");
        assert_eq!(cases.len(), 15, "image-input fixture case count changed");
        assert_eq!(
            negative_cases.len(),
            5,
            "image-input fixture negative-case count changed"
        );

        let mut identifiers = BTreeSet::new();
        for case in cases {
            let identifier = string_value(case, "fixture_id");
            assert!(
                identifiers.insert(identifier.to_owned()),
                "duplicate valid image fixture identifier {identifier:?}"
            );
            let bytes = fixture_payload(case, "encoded_image");
            let encoded = must_ok(EncodedImage::new(&bytes));
            assert_eq!(encoded.len(), bytes.len());
        }

        for case in negative_cases {
            let identifier = string_value(case, "fixture_id");
            assert!(
                identifiers.insert(identifier.to_owned()),
                "duplicate negative image fixture identifier {identifier:?}"
            );
            let bytes = fixture_payload(case, "encoded_input");
            if identifier == "classic-v1-image-input-empty" {
                assert!(matches!(
                    EncodedImage::new(&bytes),
                    Err(Error::InvalidInput {
                        field: "image.bytes",
                        violation: InputViolation::Empty,
                    })
                ));
            } else {
                let encoded = must_ok(EncodedImage::new(&bytes));
                assert_eq!(encoded.len(), bytes.len());
            }
        }
    }

    #[test]
    fn finite_point_and_score_validation_reject_invalid_values() {
        assert!(matches!(
            Point::new(f32::NAN, 0.0),
            Err(Error::InvalidInput {
                field: "point.x",
                violation: InputViolation::NonFinite,
            })
        ));
        assert!(matches!(
            Score::new(f32::INFINITY),
            Err(Error::InvalidInput {
                field: "score",
                violation: InputViolation::NonFinite,
            })
        ));
        assert!(matches!(
            Score::new(1.01),
            Err(Error::InvalidInput {
                field: "score",
                violation: InputViolation::OutOfRange,
            })
        ));
    }

    #[test]
    fn quadrilateral_rejects_zero_area() {
        let point = must_ok(Point::new(1.0, 1.0));
        assert!(matches!(
            Quadrilateral::new([point; 4]),
            Err(Error::InvalidInput {
                field: "quadrilateral",
                violation: InputViolation::DegenerateGeometry,
            })
        ));
    }

    #[test]
    fn quadrilateral_rejects_non_convex_or_repeated_vertices() {
        let top_left = must_ok(Point::new(0.0, 0.0));
        let top_right = must_ok(Point::new(4.0, 0.0));
        let bottom_right = must_ok(Point::new(4.0, 4.0));
        let bottom_left = must_ok(Point::new(0.0, 4.0));
        assert!(Quadrilateral::new([top_left, top_right, bottom_right, bottom_left]).is_ok());
        assert!(matches!(
            Quadrilateral::new([top_left, top_right, bottom_left, bottom_right]),
            Err(Error::InvalidInput {
                field: "quadrilateral",
                violation: InputViolation::DegenerateGeometry,
            })
        ));
        assert!(matches!(
            Quadrilateral::new([top_left, top_right, bottom_right, top_right]),
            Err(Error::InvalidInput {
                field: "quadrilateral",
                violation: InputViolation::DegenerateGeometry,
            })
        ));
    }

    #[test]
    fn transform_round_trip_preserves_a_finite_point() {
        let source = must_ok(ImageDimensions::new(640, 480));
        let destination = must_ok(ImageDimensions::new(1_280, 960));
        let transform = must_ok(ImageTransform::new(
            source,
            destination,
            2.0,
            2.0,
            10.0,
            20.0,
        ));
        let original = must_ok(Point::new(42.5, 7.25));
        let restored = must_ok(transform.inverse(must_ok(transform.forward(original))));
        assert!((restored.x() - original.x()).abs() < f32::EPSILON);
        assert!((restored.y() - original.y()).abs() < f32::EPSILON);
    }

    #[test]
    fn model_identity_is_bounded_and_rejects_control_characters() {
        assert!(matches!(
            ModelIdentity::new(ModelTask::TextDetection, "", "v6"),
            Err(Error::InvalidInput {
                field: "model.family",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            ModelIdentity::new(ModelTask::TextDetection, "PP\nOCR", "v6"),
            Err(Error::InvalidInput {
                field: "model.family",
                violation: InputViolation::InvalidIdentifier,
            })
        ));
    }

    fn array_value<'a>(value: &'a Value, field: &str) -> &'a [Value] {
        match value.get(field).and_then(Value::as_array) {
            Some(values) => values,
            None => panic!("fixture field {field:?} must be an array"),
        }
    }

    fn object_value<'a>(value: &'a Value, field: &str) -> &'a Value {
        match value.get(field).filter(|candidate| candidate.is_object()) {
            Some(object) => object,
            None => panic!("fixture field {field:?} must be an object"),
        }
    }

    fn string_value<'a>(value: &'a Value, field: &str) -> &'a str {
        match value.get(field).and_then(Value::as_str) {
            Some(text) => text,
            None => panic!("fixture field {field:?} must be a string"),
        }
    }

    fn fixture_payload(case: &Value, field: &str) -> Vec<u8> {
        let payload = object_value(case, field);
        let bytes = must_ok(STANDARD.decode(string_value(payload, "base64")));
        let recorded_length = match payload.get("byte_length").and_then(Value::as_u64) {
            Some(length) => length,
            None => panic!("fixture payload byte_length must be an unsigned integer"),
        };
        assert_eq!(
            usize::try_from(recorded_length).ok(),
            Some(bytes.len()),
            "fixture payload byte length does not match base64 bytes"
        );
        bytes
    }
}
