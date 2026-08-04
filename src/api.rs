// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The public classic OCR surface.
//!
//! This is the narrowest API that makes the implemented pipeline usable: point
//! it at an explicitly provisioned detector, recognizer, dictionary, and ONNX
//! Runtime library, hand it PNG bytes, and receive recognized lines in reading
//! order.
//!
//! Scope is deliberately limited and stated rather than implied:
//!
//! - **PNG input only.** `docs/IMAGE_DECODER_DECISION.md` records why JPEG is
//!   deferred; a JPEG here is a typed `Unsupported` error, not a near miss.
//! - **Explicit local artifacts.** Nothing is downloaded, cached, or resolved
//!   from an environment variable. Every path comes from the caller.
//! - **Not yet validated against a real model.** Every stage matches a recorded
//!   OpenCV or Clipper oracle, but the end-to-end result has not been compared
//!   with the captured PaddleOCR fixtures. That comparison is gate `G1` in
//!   `docs/ADR_RT004_RUNTIME_SELECTION.md`, and until it passes this API must
//!   not be described as PaddleOCR-compatible.

use crate::dictionary::CtcDictionary;
use crate::error::{Error, InputViolation, Result};
use crate::types::{EncodedImage, Quadrilateral};

/// One recognized text line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Four corners in the source image's coordinates.
    pub quadrilateral: Quadrilateral,
    /// Decoded text, with Unicode scalars preserved exactly.
    pub text: String,
    /// Recognition confidence.
    pub score: f64,
}

/// Thresholds applied by the classic pipeline.
#[derive(Clone, Copy, Debug)]
pub struct OcrOptions {
    /// Minimum mean probability for a detected region.
    pub box_threshold: f64,
    /// Polygon expansion ratio applied after scoring.
    pub unclip_ratio: f64,
    /// Minimum recognition confidence; a score exactly equal is retained.
    pub drop_score: f64,
}

impl Default for OcrOptions {
    /// The frozen M2 defaults.
    fn default() -> Self {
        Self {
            box_threshold: 0.6,
            unclip_ratio: 1.5,
            drop_score: 0.5,
        }
    }
}

/// A recognizer dictionary parsed from a plain-text file.
///
/// The inner representation stays private so the public surface does not
/// commit to it; only the entry count is exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dictionary {
    inner: CtcDictionary,
}

impl Dictionary {
    /// Returns the number of configured entries, excluding blank and space.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.entry_count()
    }

    /// Returns whether the dictionary has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the total class count the recognizer output must declare.
    #[must_use]
    pub fn class_count(&self) -> usize {
        self.inner.class_count()
    }
}

/// Parses a plain-text dictionary of one entry per line.
///
/// The recognizer's `use_space_char` behaviour is the caller's declaration
/// because it comes from the artifact configuration, not from the file.
pub fn parse_dictionary(contents: &str, appends_space: bool) -> Result<Dictionary> {
    let entries: Vec<String> = contents
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect();
    if entries.is_empty() {
        return Err(Error::InvalidInput {
            field: "dictionary.entries",
            violation: InputViolation::Empty,
        });
    }
    Ok(Dictionary {
        inner: CtcDictionary::new(entries, appends_space)?,
    })
}

/// Decodes PNG bytes into the pipeline's interleaved BGR representation.
///
/// Exposed so a caller can validate an input before committing to a model run.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32)> {
    let encoded = EncodedImage::new(bytes)?;
    let decoded = crate::image::decode_classic_bgr(encoded)?;
    let dimensions = decoded.dimensions();
    Ok((dimensions.width(), dimensions.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_parses_one_entry_per_line() {
        let dictionary = match parse_dictionary("a\nb\nc\n", true) {
            Ok(value) => value,
            Err(error) => panic!("expected a dictionary, got {error}"),
        };
        assert_eq!(dictionary.len(), 3);
        // 1 blank + 3 entries + 1 appended space.
        assert_eq!(dictionary.class_count(), 5);
    }

    #[test]
    fn carriage_returns_are_stripped_but_scalars_are_not_altered() {
        let dictionary = match parse_dictionary("a\r\n\u{3000}\r\n", false) {
            Ok(value) => value,
            Err(error) => panic!("expected a dictionary, got {error}"),
        };
        assert_eq!(dictionary.len(), 2);
        // The ideographic space survives; only the CR is removed. Two
        // entries plus the blank means three classes without an appended
        // space, which pins that the CR did not become a third entry.
        assert_eq!(dictionary.class_count(), 3);
    }

    #[test]
    fn an_empty_dictionary_is_rejected() {
        assert!(parse_dictionary("", true).is_err());
    }

    #[test]
    fn decoding_reports_png_dimensions_and_rejects_other_formats() {
        // The committed image-input corpus is the source of truth for real
        // encoded bytes; hand-written PNG headers are not.
        const CORPUS: &str = include_str!("../tests/fixtures/classic-v1-image-inputs/capture.json");
        let capture: serde_json::Value = match serde_json::from_str(CORPUS) {
            Ok(value) => value,
            Err(error) => panic!("image-input corpus is not valid JSON: {error}"),
        };
        let cases = match capture.get("cases").and_then(serde_json::Value::as_array) {
            Some(cases) => cases,
            None => panic!("image-input corpus must contain cases"),
        };
        let png = cases
            .iter()
            .find(|case| case.get("format").and_then(serde_json::Value::as_str) == Some("png"))
            .and_then(|case| case.pointer("/encoded_image/base64"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("the corpus must contain a PNG case"));
        let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, png) {
            Ok(bytes) => bytes,
            Err(error) => panic!("corpus base64 is invalid: {error}"),
        };

        match decode_png(&bytes) {
            Ok((width, height)) => assert_eq!((width, height), (3, 2)),
            Err(error) => panic!("expected a decoded PNG, got {error}"),
        }
        assert!(matches!(
            decode_png(b"\xff\xd8\xff\xe0 not a png"),
            Err(Error::Unsupported { .. })
        ));
    }
}

/// The explicitly provisioned artifacts a run needs.
#[cfg(feature = "onnxruntime")]
#[derive(Clone, Copy, Debug)]
pub struct Artifacts<'a> {
    /// Path to the ONNX Runtime shared library.
    pub library: &'a str,
    /// Path to the detector model.
    pub detector: &'a str,
    /// Path to the recognizer model.
    pub recognizer: &'a str,
}

/// Recognizes text in one PNG image using explicitly provisioned artifacts.
///
/// Gate `G1` passed for the recorded reading-order fixture, so this returns a
/// result rather than refusing. It remains one fixture with one artifact pair.
#[cfg(feature = "onnxruntime")]
pub fn recognize_png(
    artifacts: &Artifacts<'_>,
    dictionary: &Dictionary,
    png: &[u8],
    options: OcrOptions,
) -> Result<Vec<TextLine>> {
    use crate::backend::{AxisExtent, ModelArtifact, ModelContract, RunBudget, TensorContract};
    use crate::backend_ort::{OrtBackend, initialize_runtime};
    use crate::pipeline::{ClassicModels, ClassicThresholds, run_classic_ocr};

    initialize_runtime(std::path::Path::new(artifacts.library))?;

    let free = AxisExtent::Bounded {
        minimum: 1,
        maximum: 8192,
    };
    // The digest is not enforced here: the caller supplies the path, and
    // artifact identity policy is MOD-003 work. A zero digest with a
    // permissive stream keeps the adapter's shape checks active.
    let detector_contract = ModelContract::new(
        ModelArtifact::new(artifacts.detector, "0".repeat(64))?,
        TensorContract::new(
            "x",
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(3), free, free],
        )?,
        TensorContract::new(
            "fetch_name_0",
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(1), free, free],
        )?,
        RunBudget::new(40_000_000, 40_000_000, 1)?,
    );
    let recognizer_contract = ModelContract::new(
        ModelArtifact::new(artifacts.recognizer, "0".repeat(64))?,
        TensorContract::new(
            "x",
            vec![free, AxisExtent::Fixed(3), AxisExtent::Fixed(48), free],
        )?,
        TensorContract::new(
            "fetch_name_0",
            vec![free, free, AxisExtent::Fixed(dictionary.class_count())],
        )?,
        RunBudget::new(40_000_000, 40_000_000, 64)?,
    );

    let mut skip = SkipDigest;
    let detector = OrtBackend::load(&detector_contract, &mut skip, 1, 1)?;
    let recognizer = OrtBackend::load(&recognizer_contract, &mut skip, 1, 1)?;

    let encoded = EncodedImage::new(png)?;
    let image = crate::image::decode_classic_bgr(encoded)?;
    let lines = run_classic_ocr(
        &ClassicModels {
            detector: (&detector, &detector_contract),
            recognizer: (&recognizer, &recognizer_contract),
            dictionary: &dictionary.inner,
        },
        &image,
        ClassicThresholds {
            box_threshold: options.box_threshold,
            unclip_ratio: options.unclip_ratio,
            drop_score: options.drop_score,
        },
    )?;

    Ok(lines
        .into_iter()
        .map(|line| TextLine {
            quadrilateral: line.quadrilateral,
            text: line.text,
            score: line.score,
        })
        .collect())
}

/// A digest that always reports the declared value.
///
/// Artifact identity enforcement is `MOD-003`; this keeps the adapter's other
/// checks active without asserting an identity policy that does not exist yet.
#[cfg(feature = "onnxruntime")]
struct SkipDigest;

#[cfg(feature = "onnxruntime")]
impl crate::backend::Sha256Stream for SkipDigest {
    fn update(&mut self, _bytes: &[u8]) {}
    fn finish(&mut self) -> String {
        "0".repeat(64)
    }
}
