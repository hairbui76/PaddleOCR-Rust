// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Composed document preprocessing, and the coordinate space it produces.
//!
//! Roadmap item `DOCPIPE-001`: compose configurable document preprocessing and
//! **preserve every transform needed by downstream coordinates**.
//!
//! The composition itself is small — orientation then unwarping, the order
//! `deploy/cpp_infer/src/pipelines/doc_preprocessor/pipeline.cc` uses. What this
//! module is really for is the second half of that sentence, because the two
//! stages differ in a way that no amount of care in the caller can paper over:
//!
//! | Stage | Invertible |
//! |---|---|
//! | Document rotation | **yes** — `DocumentRotation::inverse` |
//! | Unwarping | **no** — a learned per-pixel deformation, never seen here |
//!
//! # Why the answer is a type rather than a note in the documentation
//!
//! After preprocessing, a detected polygon is in the coordinate space of the
//! processed image. Whether that space can be mapped back to the caller's page
//! depends entirely on which stages ran, and getting it wrong is silent: the
//! polygons are internally consistent and simply describe a different picture.
//!
//! So [`CoordinateSpace`] states it, [`DocumentPreprocessing::to_source`]
//! returns `None` rather than a plausible number when the chain is broken, and a
//! caller cannot accidentally treat unwarped coordinates as source coordinates
//! without ignoring an `Option`.
//!
//! # What this module does not do
//!
//! It does not run the models. Composition and coordinate bookkeeping are
//! separable from inference, and keeping them apart is what lets every rule here
//! be tested without an artifact.
//!
//! Nothing constructs a [`DocumentPreprocessing`] outside tests yet, for the
//! reason `src/unwarp.rs` records: the stages exist and are verified, and wiring
//! them into a public run would mean answering how a caller supplies three more
//! artifacts. The types here are the part that had to exist first, because they
//! are what any such answer has to be built on.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::document_orientation::DocumentRotation;
use crate::error::Result;

/// Which image a set of coordinates describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CoordinateSpace {
    /// The image the caller supplied. Coordinates can be used directly.
    Source,
    /// The preprocessed image. Coordinates **cannot** be mapped back, because an
    /// unwarping step in the chain has no inverse.
    Processed,
}

/// How a page should be preprocessed.
///
/// Both stages default to off, matching `use_doc_orientation_classify` and
/// `use_doc_unwarping` in the upstream pipeline configuration.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct DocumentPreprocessOptions {
    /// Classify the page's orientation and rotate it upright.
    pub orientation: bool,
    /// Flatten a curved or skewed page.
    ///
    /// Enabling this makes detected coordinates unmappable to the source image;
    /// see [`CoordinateSpace`].
    pub unwarping: bool,
}

impl DocumentPreprocessOptions {
    /// Enables orientation classification and rotation.
    #[must_use]
    pub const fn with_orientation(mut self, enabled: bool) -> Self {
        self.orientation = enabled;
        self
    }

    /// Enables unwarping, at the cost of mappable coordinates.
    #[must_use]
    pub const fn with_unwarping(mut self, enabled: bool) -> Self {
        self.unwarping = enabled;
        self
    }
}

/// A preprocessed page and the transforms needed to interpret its coordinates.
#[derive(Debug)]
pub struct DocumentPreprocessing {
    image: InterleavedImage,
    rotation: Option<DocumentRotation>,
    unwarped: bool,
}

impl DocumentPreprocessing {
    /// Records a page that was not preprocessed at all.
    pub(crate) const fn unchanged(image: InterleavedImage) -> Self {
        Self {
            image,
            rotation: None,
            unwarped: false,
        }
    }

    /// Records a rotation applied to the page.
    ///
    /// Composing two rotations is not supported and does not arise: the
    /// classifier runs once and emits one angle.
    pub(crate) fn rotated(image: InterleavedImage, rotation: DocumentRotation) -> Self {
        Self {
            image,
            rotation: Some(rotation),
            unwarped: false,
        }
    }

    /// Marks the page as unwarped, which severs the coordinate chain.
    pub(crate) fn unwarp(mut self, image: InterleavedImage) -> Self {
        self.image = image;
        self.unwarped = true;
        self
    }

    /// The processed image's dimensions.
    ///
    /// The pixels themselves stay internal: `InterleavedImage` is deliberately
    /// not part of the public surface, and exposing it here would widen that
    /// surface for a capability that is not yet reachable.
    #[must_use]
    pub fn image_dimensions(&self) -> crate::types::ImageDimensions {
        self.image.dimensions()
    }

    /// The image downstream stages should run on.
    pub(crate) fn image(&self) -> &InterleavedImage {
        &self.image
    }

    /// Which image this preprocessing's output coordinates describe.
    #[must_use]
    pub const fn coordinate_space(&self) -> CoordinateSpace {
        if self.unwarped {
            CoordinateSpace::Processed
        } else {
            CoordinateSpace::Source
        }
    }

    /// Maps a point in the processed image back to the caller's page.
    ///
    /// Returns `None` when the chain cannot be inverted, which today means an
    /// unwarping step ran. Returning `None` rather than an approximation is the
    /// point: a plausible wrong coordinate is worse than an absent one, because
    /// nothing downstream can tell it apart from a right one.
    pub fn to_source(&self, x: f64, y: f64) -> Result<Option<(f64, f64)>> {
        if self.unwarped {
            return Ok(None);
        }
        match self.rotation {
            Some(rotation) => rotation.inverse(x, y).map(Some),
            None => Ok(Some((x, y))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::ImageDimensions;

    fn page(width: u32, height: u32) -> InterleavedImage {
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        let pixels = vec![7_u8; (width * height * 3) as usize];
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("page: {error}"),
        }
    }

    #[test]
    fn an_unprocessed_page_maps_coordinates_straight_through() {
        let preprocessing = DocumentPreprocessing::unchanged(page(64, 48));
        assert_eq!(preprocessing.coordinate_space(), CoordinateSpace::Source);
        match preprocessing.to_source(12.0, 34.0) {
            Ok(Some((x, y))) => assert_eq!((x, y), (12.0, 34.0)),
            other => panic!("expected the identity, got {other:?}"),
        }
    }

    #[test]
    fn a_rotated_page_maps_coordinates_back_through_the_inverse() {
        let source = page(1280, 720);
        let rotation = match DocumentRotation::new(source.dimensions(), 180) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let preprocessing = DocumentPreprocessing::rotated(page(1280, 720), rotation);
        assert_eq!(preprocessing.coordinate_space(), CoordinateSpace::Source);

        // A point in the rotated page comes back to where it started.
        let (rotated_x, rotated_y) = rotation.forward(47.0, 78.0);
        match preprocessing.to_source(rotated_x, rotated_y) {
            Ok(Some((x, y))) => {
                assert!(
                    (x - 47.0).abs() < 1e-6 && (y - 78.0).abs() < 1e-6,
                    "({x}, {y})"
                );
            }
            other => panic!("expected a mapped point, got {other:?}"),
        }
    }

    /// Unwarping severs the chain, and the type says so.
    #[test]
    fn an_unwarped_page_refuses_to_invent_a_source_coordinate() {
        let preprocessing = DocumentPreprocessing::unchanged(page(64, 48)).unwarp(page(64, 48));
        assert_eq!(preprocessing.coordinate_space(), CoordinateSpace::Processed);
        match preprocessing.to_source(12.0, 34.0) {
            Ok(None) => {}
            other => panic!("expected no mapping, got {other:?}"),
        }
    }

    /// Rotation followed by unwarping is still unmappable.
    ///
    /// The rotation alone would be invertible, so this checks that one broken
    /// link breaks the chain rather than being averaged away by the good one.
    #[test]
    fn one_uninvertible_stage_breaks_the_whole_chain() {
        let source = page(400, 300);
        let rotation = match DocumentRotation::new(source.dimensions(), 90) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let preprocessing =
            DocumentPreprocessing::rotated(page(300, 400), rotation).unwarp(page(300, 400));
        assert_eq!(preprocessing.coordinate_space(), CoordinateSpace::Processed);
        assert!(matches!(preprocessing.to_source(1.0, 2.0), Ok(None)));
    }

    #[test]
    fn both_stages_default_to_off() {
        let options = DocumentPreprocessOptions::default();
        assert!(!options.orientation);
        assert!(!options.unwarping);

        let enabled = DocumentPreprocessOptions::default()
            .with_orientation(true)
            .with_unwarping(true);
        assert!(enabled.orientation);
        assert!(enabled.unwarping);
    }

    /// The processed image is what downstream stages must run on.
    #[test]
    fn the_processed_image_is_the_one_carried_forward() {
        let rotated = page(300, 400);
        let source = page(400, 300);
        let rotation = match DocumentRotation::new(source.dimensions(), 90) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let preprocessing = DocumentPreprocessing::rotated(rotated, rotation);
        assert_eq!(preprocessing.image().dimensions().width(), 300);
        assert_eq!(preprocessing.image().dimensions().height(), 400);
    }
}
