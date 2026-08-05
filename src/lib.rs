#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, missing_docs)]
// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Native Rust foundations for the PaddleOCR-Rust port.
//!
//! This crate intentionally exposes no OCR inference API until the selected
//! model artifacts and backend have passed the later roadmap gates.

/// The public classic OCR surface.
pub mod api;
/// Dependency-free streaming SHA-256 for artifact identity.
pub mod digest;
/// Structured errors returned by public foundation types.
pub mod error;

/// Developer-only byte-driven fuzz driver for current private pure kernels.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub mod fuzz;
pub mod manifest;

/// The versioned model manifest.
mod markdown;
mod markdown_v2;
pub mod metrics;

pub mod observability;
/// The versioned JSON result document.
pub mod resolve;
pub mod result_json;

/// Unicode script classification for dictionary contents.
pub mod script;
/// Checked domain types shared by later image, geometry, model, and pipeline code.
pub mod types;

// P4/P5 kernels remain private until their owning model and API gates close.
// Keeping them non-public avoids freezing internal detector/image/geometry APIs
// before the corresponding compatibility evidence exists.
#[allow(dead_code)]
mod backend;
/// ONNX Runtime implementation of the internal adapter; off by default.
#[cfg(feature = "onnxruntime")]
#[allow(dead_code)]
mod backend_ort;
#[allow(dead_code)]
mod contour;

/// Cancellation and the wall-clock time policy.
pub mod config_reconciliation;

pub mod control;
#[allow(dead_code)]
mod crop;
#[allow(dead_code)]
mod ctc;
#[allow(dead_code)]
mod db;
#[allow(dead_code)]
mod detector;
#[allow(dead_code)]
mod detector_boxes;
#[allow(dead_code)]
mod dictionary;
mod document_orientation;

/// Composed document preprocessing and its coordinate spaces.
pub mod document_pipeline;
#[allow(dead_code)]
mod geometry;
#[allow(dead_code)]
mod image;
mod layout;
mod layout_order;

/// Bounded input acquisition: bytes, paths, and streams.
pub mod input;
#[allow(dead_code)]
mod min_area;
mod orientation;

mod paddlex_detection;
mod pdf_render_plan;
#[allow(dead_code)]
mod pipeline;
mod reading_order;
#[allow(dead_code)]
mod recognizer;
#[allow(dead_code)]
mod recognizer_batch;
#[allow(dead_code)]
mod resize;
mod resize_cubic;
#[allow(dead_code)]
mod score;
#[allow(dead_code)]
mod score_filter;
mod structure_assembly;
/// PP-StructureV3 over the provisioned models; off by default with the
/// inference backend.
#[cfg(feature = "onnxruntime")]
pub mod structure_engine;
mod structure_glue;
pub mod structure_json;
mod structure_standardize;
mod table_cells;
mod table_classification;
#[cfg(feature = "onnxruntime")]
pub mod table_engine;
pub mod table_pipeline;
mod table_structure;
#[allow(dead_code)]
mod tensor;
mod text_lines;
#[allow(dead_code)]
mod unclip;
mod unwarp;

pub use error::{Error, InputViolation, ModelProblem, Result};
pub use types::{
    EncodedImage, ImageDimensions, ImageTransform, MAX_ENCODED_IMAGE_BYTES, ModelIdentity,
    ModelTask, PageIndex, Point, Polygon, Quadrilateral, RecognizedText, Score,
};

/// The package version compiled into this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
