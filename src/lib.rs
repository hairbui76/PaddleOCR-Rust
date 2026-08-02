#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, missing_docs)]

//! Native Rust foundations for the PaddleOCR-Rust port.
//!
//! This crate intentionally exposes no OCR inference API until the selected
//! model artifacts and backend have passed the later roadmap gates.

/// Structured errors returned by public foundation types.
pub mod error;
/// Checked domain types shared by later image, geometry, model, and pipeline code.
pub mod types;

// P4/P5 kernels remain private until their owning model and API gates close.
// Keeping them non-public avoids freezing internal detector/image/geometry APIs
// before the corresponding compatibility evidence exists.
#[allow(dead_code)]
mod crop;
#[allow(dead_code)]
mod ctc;
#[allow(dead_code)]
mod db;
#[allow(dead_code)]
mod geometry;

pub use error::{Error, InputViolation, ModelProblem, Result};
pub use types::{
    EncodedImage, ImageDimensions, ImageTransform, MAX_ENCODED_IMAGE_BYTES, ModelIdentity,
    ModelTask, PageIndex, Point, Polygon, Quadrilateral, RecognizedText, Score,
};

/// The package version compiled into this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
