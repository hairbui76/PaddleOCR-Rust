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

use crate::control::RunControl;
use crate::dictionary::CtcDictionary;
use crate::error::{Error, InputViolation, Result};
use crate::types::{EncodedImage, Quadrilateral};

/// One recognized text line.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TextLine {
    /// Four corners in the source image's coordinates.
    pub quadrilateral: Quadrilateral,
    /// Decoded text, with Unicode scalars preserved exactly.
    pub text: String,
    /// Recognition confidence.
    pub score: f64,
}

/// Thresholds and run control applied by the classic pipeline.
///
/// Construct with [`OcrOptions::default`] and the `with_*` methods rather than a
/// struct literal. The type is `#[non_exhaustive]` so a future option can be
/// added without breaking callers, which is the whole reason the builders exist:
/// this struct already grew once, when `OCR-003` added `control`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct OcrOptions {
    /// Minimum mean probability for a detected region.
    pub box_threshold: f64,
    /// Polygon expansion ratio applied after scoring.
    pub unclip_ratio: f64,
    /// Minimum recognition confidence; a score exactly equal is retained.
    pub drop_score: f64,
    /// Minimum orientation confidence to act on.
    ///
    /// Equality does **not** rotate: the upstream test is strict, which is the
    /// opposite convention from `box_threshold`. Ignored unless an orientation
    /// artifact was supplied.
    pub orientation_threshold: f64,
    /// Which document preprocessing stages to run before detection.
    ///
    /// Both default to off, matching upstream. Enabling unwarping makes the
    /// returned coordinates unmappable to the caller's page, which is why
    /// [`OcrEngine::recognize_png`] refuses it and
    /// [`OcrEngine::recognize_document`] exists.
    pub document: crate::document_pipeline::DocumentPreprocessOptions,
    /// How the caller may abandon a run in progress.
    ///
    /// The default imposes no budget and no cancellation. See
    /// [`crate::control`] for what "cancellation" guarantees here: a run stops
    /// at a stage boundary, so overshoot is bounded by one backend call rather
    /// than being immediate.
    pub control: RunControl,
}

impl Default for OcrOptions {
    /// The frozen M2 defaults.
    fn default() -> Self {
        Self {
            box_threshold: 0.6,
            unclip_ratio: 1.5,
            drop_score: 0.5,
            orientation_threshold: crate::orientation::ORIENTATION_THRESHOLD,
            document: crate::document_pipeline::DocumentPreprocessOptions::default(),
            control: RunControl::unbounded(),
        }
    }
}

impl OcrOptions {
    /// Sets the minimum mean probability for a detected region.
    #[must_use]
    pub fn with_box_threshold(mut self, threshold: f64) -> Self {
        self.box_threshold = threshold;
        self
    }

    /// Sets the polygon expansion ratio applied after scoring.
    #[must_use]
    pub fn with_unclip_ratio(mut self, ratio: f64) -> Self {
        self.unclip_ratio = ratio;
        self
    }

    /// Sets the minimum recognition confidence; equality is retained.
    #[must_use]
    pub fn with_drop_score(mut self, score: f64) -> Self {
        self.drop_score = score;
        self
    }

    /// Sets the minimum orientation confidence to act on.
    #[must_use]
    pub fn with_orientation_threshold(mut self, threshold: f64) -> Self {
        self.orientation_threshold = threshold;
        self
    }

    /// Sets which document preprocessing stages run before detection.
    #[must_use]
    pub fn with_document_preprocessing(
        mut self,
        document: crate::document_pipeline::DocumentPreprocessOptions,
    ) -> Self {
        self.document = document;
        self
    }

    /// Sets how the caller may abandon a run in progress.
    #[must_use]
    pub fn with_control(mut self, control: RunControl) -> Self {
        self.control = control;
        self
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
    /// Counts the Unicode scripts this dictionary can spell.
    ///
    /// This reports what classes exist in the output layer, and nothing more.
    /// A dictionary that contains emoji does not make this port an emoji
    /// recogniser, and a count of CJK scalars does not decide whether a model
    /// was trained for Chinese, Japanese, or Korean. See
    /// `docs/LANGUAGE_SUPPORT.md` for the difference between what a dictionary
    /// contains and what has actually been verified.
    #[must_use]
    pub fn script_census(&self) -> Vec<crate::script::ScriptCount> {
        crate::script::census(self.inner.entries().iter().map(String::as_str))
    }

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
///
/// The two digest fields are how `MOD-003` identity checking is requested. A
/// `Some` digest is verified by streaming the file **before** the model is
/// loaded, so a wrong or tampered artifact never reaches the runtime. A `None`
/// digest skips that check, which is a deliberate choice the caller makes
/// rather than a silent default.
#[cfg(feature = "onnxruntime")]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Artifacts<'a> {
    /// Path to the ONNX Runtime shared library.
    pub library: &'a str,
    /// Path to the detector model.
    pub detector: &'a str,
    /// Expected detector SHA-256, lowercase hexadecimal.
    pub detector_sha256: Option<&'a str>,
    /// Path to the recognizer model.
    pub recognizer: &'a str,
    /// Expected recognizer SHA-256, lowercase hexadecimal.
    pub recognizer_sha256: Option<&'a str>,
    /// Optional text-line orientation classifier.
    ///
    /// `None` matches upstream's `use_angle_cls = False`: no classifier is
    /// loaded and no crop is rotated.
    pub orientation: Option<&'a str>,
    /// Expected orientation classifier SHA-256, lowercase hexadecimal.
    pub orientation_sha256: Option<&'a str>,
    /// Optional document orientation classifier.
    pub document_orientation: Option<&'a str>,
    /// Expected document orientation classifier SHA-256.
    pub document_orientation_sha256: Option<&'a str>,
    /// Optional unwarping model.
    pub unwarping: Option<&'a str>,
    /// Expected unwarping model SHA-256.
    pub unwarping_sha256: Option<&'a str>,
}

#[cfg(feature = "onnxruntime")]
impl<'a> Artifacts<'a> {
    /// Declares the three paths a run always needs, with no digest checking.
    ///
    /// Omitting the digests is a choice, not a default that happens to you:
    /// without them a substituted or swapped artifact loads without complaint,
    /// because the detector and recognizer are not distinguishable by shape.
    /// Use [`Artifacts::with_detector_sha256`] and
    /// [`Artifacts::with_recognizer_sha256`], or a `MOD-002` manifest.
    #[must_use]
    pub const fn new(library: &'a str, detector: &'a str, recognizer: &'a str) -> Self {
        Self {
            library,
            detector,
            detector_sha256: None,
            recognizer,
            recognizer_sha256: None,
            orientation: None,
            orientation_sha256: None,
            document_orientation: None,
            document_orientation_sha256: None,
            unwarping: None,
            unwarping_sha256: None,
        }
    }

    /// Requires the detector to match this lowercase hexadecimal SHA-256.
    #[must_use]
    pub const fn with_detector_sha256(mut self, digest: &'a str) -> Self {
        self.detector_sha256 = Some(digest);
        self
    }

    /// Requires the recognizer to match this lowercase hexadecimal SHA-256.
    #[must_use]
    pub const fn with_recognizer_sha256(mut self, digest: &'a str) -> Self {
        self.recognizer_sha256 = Some(digest);
        self
    }

    /// Adds a text-line orientation classifier.
    ///
    /// Supplying one turns the stage on. Without it the pipeline behaves exactly
    /// as it did before the classifier existed, which is also upstream's
    /// default.
    #[must_use]
    pub const fn with_orientation(mut self, path: &'a str) -> Self {
        self.orientation = Some(path);
        self
    }

    /// Requires the orientation classifier to match this SHA-256.
    #[must_use]
    pub const fn with_orientation_sha256(mut self, digest: &'a str) -> Self {
        self.orientation_sha256 = Some(digest);
        self
    }

    /// Adds a document orientation classifier, for whole-page rotation.
    ///
    /// Distinct from [`Artifacts::with_orientation`], which corrects individual
    /// text lines. The two are different models with different classes.
    #[must_use]
    pub const fn with_document_orientation(mut self, path: &'a str) -> Self {
        self.document_orientation = Some(path);
        self
    }

    /// Requires the document orientation classifier to match this SHA-256.
    #[must_use]
    pub const fn with_document_orientation_sha256(mut self, digest: &'a str) -> Self {
        self.document_orientation_sha256 = Some(digest);
        self
    }

    /// Adds an unwarping model.
    #[must_use]
    pub const fn with_unwarping(mut self, path: &'a str) -> Self {
        self.unwarping = Some(path);
        self
    }

    /// Requires the unwarping model to match this SHA-256.
    #[must_use]
    pub const fn with_unwarping_sha256(mut self, digest: &'a str) -> Self {
        self.unwarping_sha256 = Some(digest);
        self
    }
}

/// A loaded classic OCR engine: both models, their contracts, and the
/// dictionary, ready to run over many images.
///
/// # Why this exists
///
/// Creating the two sessions costs roughly `1.4 s` on the reference host, which
/// `docs/G3_RESOURCE_EVIDENCE.md` measures as the gap between the `4.2 s` cold
/// run and the `2.8 s` warm one. A caller processing a directory of pages should
/// pay that once. [`recognize_png`] pays it every call by construction, which is
/// correct for the one-image CLI and wrong for anything else.
///
/// # Concurrency
///
/// An engine is usable from one thread at a time, and the type system says so
/// rather than the documentation alone: the backend holds its session in a
/// `RefCell`, so `OcrEngine` is `!Sync` and will not compile behind a shared
/// reference across threads. That is deliberate. The runtime's session is not
/// documented as concurrently callable through this adapter, and a `Mutex` here
/// would convert a compile error into a runtime queue, which hides the
/// serialisation rather than removing it.
///
/// To use several threads, load one engine per thread. Each pays its own session
/// creation; nothing is shared, so nothing needs locking.
#[cfg(feature = "onnxruntime")]
#[derive(Debug)]
pub struct OcrEngine {
    detector: crate::backend_ort::OrtBackend,
    detector_contract: crate::backend::ModelContract,
    recognizer: crate::backend_ort::OrtBackend,
    recognizer_contract: crate::backend::ModelContract,
    dictionary: CtcDictionary,
    orientation: Option<(
        crate::backend_ort::OrtBackend,
        crate::backend::ModelContract,
    )>,
    document_orientation: Option<(
        crate::backend_ort::OrtBackend,
        crate::backend::ModelContract,
    )>,
    unwarping: Option<(
        crate::backend_ort::OrtBackend,
        crate::backend::ModelContract,
    )>,
}

#[cfg(feature = "onnxruntime")]
impl OcrEngine {
    /// Loads both models once, verifying any declared artifact digest first.
    ///
    /// The dictionary is cloned into the engine because the recognizer's output
    /// contract is built from its class count: an engine whose dictionary could
    /// change underneath it would have a contract that no longer describes the
    /// model it validates.
    pub fn load(artifacts: &Artifacts<'_>, dictionary: &Dictionary) -> Result<Self> {
        use crate::backend::{AxisExtent, ModelArtifact, ModelContract, RunBudget, TensorContract};
        use crate::backend_ort::initialize_runtime;

        initialize_runtime(std::path::Path::new(artifacts.library))?;

        let free = AxisExtent::Bounded {
            minimum: 1,
            maximum: 8192,
        };
        // A declared digest is checked; an absent one is recorded as unchecked
        // by the placeholder value, which the matching stream then satisfies.
        let detector_contract = ModelContract::new(
            ModelArtifact::new(
                artifacts.detector,
                artifacts.detector_sha256.unwrap_or(UNCHECKED_DIGEST),
            )?,
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
            ModelArtifact::new(
                artifacts.recognizer,
                artifacts.recognizer_sha256.unwrap_or(UNCHECKED_DIGEST),
            )?,
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

        let detector = load_backend(&detector_contract, artifacts.detector_sha256.is_some())?;
        let recognizer = load_backend(&recognizer_contract, artifacts.recognizer_sha256.is_some())?;

        // The classifier's shape is fixed by its artifact rather than derived,
        // so its contract is entirely concrete: no bounded axis except batch.
        let orientation = match artifacts.orientation {
            Some(path) => {
                let batch = AxisExtent::Bounded {
                    minimum: 1,
                    maximum: crate::orientation::ORIENTATION_MAX_BATCH,
                };
                let contract = ModelContract::new(
                    ModelArtifact::new(
                        path,
                        artifacts.orientation_sha256.unwrap_or(UNCHECKED_DIGEST),
                    )?,
                    TensorContract::new(
                        "x",
                        vec![
                            batch,
                            AxisExtent::Fixed(3),
                            AxisExtent::Fixed(80),
                            AxisExtent::Fixed(160),
                        ],
                    )?,
                    TensorContract::new("fetch_name_0", vec![batch, AxisExtent::Fixed(2)])?,
                    RunBudget::new(
                        40_000_000,
                        40_000_000,
                        crate::orientation::ORIENTATION_MAX_BATCH,
                    )?,
                );
                let backend = load_backend(&contract, artifacts.orientation_sha256.is_some())?;
                Some((backend, contract))
            }
            None => None,
        };

        // Document orientation: a fixed 224 square, four classes.
        let document_orientation = match artifacts.document_orientation {
            Some(path) => {
                let batch = AxisExtent::Bounded {
                    minimum: 1,
                    maximum: 8,
                };
                let contract = ModelContract::new(
                    ModelArtifact::new(
                        path,
                        artifacts
                            .document_orientation_sha256
                            .unwrap_or(UNCHECKED_DIGEST),
                    )?,
                    TensorContract::new(
                        "x",
                        vec![
                            batch,
                            AxisExtent::Fixed(3),
                            AxisExtent::Fixed(224),
                            AxisExtent::Fixed(224),
                        ],
                    )?,
                    TensorContract::new("fetch_name_0", vec![batch, AxisExtent::Fixed(4)])?,
                    RunBudget::new(40_000_000, 40_000_000, 8)?,
                );
                let backend =
                    load_backend(&contract, artifacts.document_orientation_sha256.is_some())?;
                Some((backend, contract))
            }
            None => None,
        };

        // Unwarping: every axis dynamic, bounded by the module's own page cap
        // rather than by a shape, because upstream applies no resize.
        let unwarping = match artifacts.unwarping {
            Some(path) => {
                let free_side = AxisExtent::Bounded {
                    minimum: 1,
                    maximum: 8192,
                };
                let contract = ModelContract::new(
                    ModelArtifact::new(
                        path,
                        artifacts.unwarping_sha256.unwrap_or(UNCHECKED_DIGEST),
                    )?,
                    TensorContract::new(
                        "image",
                        vec![
                            AxisExtent::Fixed(1),
                            AxisExtent::Fixed(3),
                            free_side,
                            free_side,
                        ],
                    )?,
                    TensorContract::new(
                        "fetch_name_0",
                        vec![
                            AxisExtent::Fixed(1),
                            AxisExtent::Fixed(3),
                            free_side,
                            free_side,
                        ],
                    )?,
                    RunBudget::new(40_000_000, 40_000_000, 1)?,
                );
                let backend = load_backend(&contract, artifacts.unwarping_sha256.is_some())?;
                Some((backend, contract))
            }
            None => None,
        };

        Ok(Self {
            detector,
            detector_contract,
            recognizer,
            recognizer_contract,
            dictionary: dictionary.inner.clone(),
            orientation,
            document_orientation,
            unwarping,
        })
    }

    /// Runs the configured document preprocessing over one decoded page.
    ///
    /// Stages run in upstream's order: orientation, then unwarping. A stage the
    /// caller enabled without supplying its artifact is a typed error rather
    /// than a silent skip — an option that quietly does nothing is worse than
    /// one that refuses.
    fn preprocess_document(
        &self,
        page: crate::crop::InterleavedImage,
        options: &OcrOptions,
    ) -> Result<crate::document_pipeline::DocumentPreprocessing> {
        use crate::document_pipeline::DocumentPreprocessing;

        let mut result = DocumentPreprocessing::unchanged(page);

        if options.document.orientation {
            let Some((backend, contract)) = self.document_orientation.as_ref() else {
                return Err(Error::InvalidInput {
                    field: "document.orientation_artifact",
                    violation: InputViolation::Empty,
                });
            };
            let angle =
                crate::document_orientation::classify_page(backend, contract, result.image())?;
            if angle != 0 {
                let rotation = crate::document_orientation::DocumentRotation::new(
                    result.image().dimensions(),
                    angle,
                )?;
                let rotated = crate::document_orientation::rotate_page(result.image(), angle)?;
                result = DocumentPreprocessing::rotated(rotated, rotation);
            }
        }

        if options.document.unwarping {
            let Some((backend, contract)) = self.unwarping.as_ref() else {
                return Err(Error::InvalidInput {
                    field: "document.unwarping_artifact",
                    violation: InputViolation::Empty,
                });
            };
            let flattened = crate::unwarp::unwarp(backend, contract, result.image())?;
            result = result.unwarp(flattened);
        }

        Ok(result)
    }

    /// Recognizes text with document preprocessing, reporting the coordinate
    /// space the result is in.
    ///
    /// Use this rather than [`OcrEngine::recognize_png`] when unwarping is
    /// enabled: unwarping has no inverse, so the returned polygons describe the
    /// processed page rather than the caller's, and
    /// [`DocumentResult::coordinate_space`] is how that is stated rather than
    /// assumed.
    pub fn recognize_document(&self, png: &[u8], options: &OcrOptions) -> Result<DocumentResult> {
        let encoded = EncodedImage::new(png)?;
        let decoded = crate::image::decode_classic_bgr(encoded)?;
        let preprocessed = self.preprocess_document(decoded, options)?;
        let space = preprocessed.coordinate_space();
        let lines = self.recognize_image(preprocessed.image(), options)?;

        // Coordinates come back in the processed page's space. Where the chain
        // is invertible they are mapped home; where it is not, they are left as
        // they are and the space says so.
        let lines = match space {
            crate::document_pipeline::CoordinateSpace::Source => {
                let mut mapped = Vec::with_capacity(lines.len());
                for line in lines {
                    mapped.push(map_line(&preprocessed, line)?);
                }
                mapped
            }
            crate::document_pipeline::CoordinateSpace::Processed => lines,
        };
        Ok(DocumentResult {
            lines,
            coordinate_space: space,
        })
    }

    /// Recognizes text in a PNG read from an explicit local path.
    ///
    /// The read is bounded during the read, not after it, so an oversized file
    /// is refused without being allocated. See [`crate::input`].
    pub fn recognize_path(
        &self,
        path: impl AsRef<std::path::Path>,
        options: &OcrOptions,
    ) -> Result<Vec<TextLine>> {
        let bytes = crate::input::read_encoded_file(path)?;
        self.recognize_png(&bytes, options)
    }

    /// Recognizes text in a PNG read from a stream.
    ///
    /// The same bound applies, and it is the only thing that can stop a stream
    /// that never ends.
    pub fn recognize_reader(
        &self,
        reader: impl std::io::Read,
        options: &OcrOptions,
    ) -> Result<Vec<TextLine>> {
        let bytes = crate::input::read_encoded_from(reader)?;
        self.recognize_png(&bytes, options)
    }

    /// Runs the classic pipeline over an already decoded page.
    fn recognize_image(
        &self,
        image: &crate::crop::InterleavedImage,
        options: &OcrOptions,
    ) -> Result<Vec<TextLine>> {
        use crate::pipeline::{ClassicModels, ClassicThresholds, run_classic_ocr};

        let lines = run_classic_ocr(
            &ClassicModels {
                detector: (&self.detector, &self.detector_contract),
                recognizer: (&self.recognizer, &self.recognizer_contract),
                dictionary: &self.dictionary,
                orientation: self.orientation.as_ref().map(|(backend, contract)| {
                    (backend as &dyn crate::backend::InferenceBackend, contract)
                }),
            },
            image,
            ClassicThresholds {
                box_threshold: options.box_threshold,
                unclip_ratio: options.unclip_ratio,
                drop_score: options.drop_score,
                orientation_threshold: options.orientation_threshold,
            },
            &options.control.begin(),
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

    /// Recognizes text in one PNG image, reusing the loaded sessions.
    ///
    /// Each call is independent: no state carries between images, so the same
    /// input always produces the same result, and a failed call leaves the
    /// engine usable. `options` is taken by reference because it now carries the
    /// cancellation flag, which a caller will usually want to keep.
    pub fn recognize_png(&self, png: &[u8], options: &OcrOptions) -> Result<Vec<TextLine>> {
        // Unwarping would make the returned coordinates describe an image the
        // caller never supplied, and this signature has no way to say so.
        // Refusing is the only honest answer; `recognize_document` is the one
        // that can report a coordinate space.
        if options.document.unwarping {
            return Err(Error::Unsupported {
                capability: "unwarping through recognize_png; use recognize_document",
            });
        }
        let encoded = EncodedImage::new(png)?;
        let decoded = crate::image::decode_classic_bgr(encoded)?;
        let preprocessed = self.preprocess_document(decoded, options)?;
        let lines = self.recognize_image(preprocessed.image(), options)?;
        let mut mapped = Vec::with_capacity(lines.len());
        for line in lines {
            mapped.push(map_line(&preprocessed, line)?);
        }
        Ok(mapped)
    }

    /// Detects text regions without recognizing them.
    ///
    /// The detector and the reading-order sort run; cropping, orientation, and
    /// recognition do not. That makes this **cheaper than
    /// [`OcrEngine::recognize_png`] by the recognizer's whole cost**, which on
    /// a dense page is most of the run.
    ///
    /// # What this deliberately does not do
    ///
    /// It does not apply `drop_score`. That threshold filters on **recognition**
    /// confidence, and there is no recognition here — applying the detector's
    /// score to it instead would silently mean something else. `box_threshold`
    /// and `unclip_ratio` do apply, because they are the detector's own.
    ///
    /// It refuses unwarping for the same reason `recognize_png` does: the
    /// returned coordinates would describe an image the caller never supplied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] when unwarping is requested, and the same
    /// decode and model errors `recognize_png` returns.
    pub fn detect_png(&self, png: &[u8], options: &OcrOptions) -> Result<Vec<DetectedRegion>> {
        if options.document.unwarping {
            return Err(Error::Unsupported {
                capability: "unwarping through detect_png; use recognize_document",
            });
        }
        let encoded = EncodedImage::new(png)?;
        let decoded = crate::image::decode_classic_bgr(encoded)?;
        let preprocessed = self.preprocess_document(decoded, options)?;

        let schedule = options.control.begin();
        schedule.check("detector")?;
        let detected = crate::detector::detect_boxes(
            &self.detector,
            &self.detector_contract,
            preprocessed.image(),
            options.box_threshold,
            options.unclip_ratio,
        )?;

        let mut regions = Vec::with_capacity(detected.len());
        for entry in &detected {
            let mut corners = [crate::types::Point::new(0.0, 0.0)?; 4];
            for (slot, (x, y)) in corners.iter_mut().zip(&entry.corners) {
                *slot = crate::types::Point::new(*x as f32, *y as f32)?;
            }
            regions.push((Quadrilateral::new(corners)?, entry.score));
        }
        // The same reading-order sort the full pipeline establishes, so the two
        // entry points agree about order rather than only about content.
        let mut quadrilaterals: Vec<Quadrilateral> =
            regions.iter().map(|(quad, _)| *quad).collect();
        crate::geometry::classic_sort_quadrilaterals(&mut quadrilaterals);

        let mut sorted = Vec::with_capacity(regions.len());
        for quadrilateral in quadrilaterals {
            let score = regions
                .iter()
                .find(|(candidate, _)| *candidate == quadrilateral)
                .map_or(0.0, |(_, score)| *score);
            let mapped = map_quadrilateral(&preprocessed, quadrilateral)?;
            sorted.push(DetectedRegion {
                quadrilateral: mapped,
                score,
            });
        }
        Ok(sorted)
    }
}

/// One detected text region, before any recognition.
///
/// Roadmap item `MODAPI-001`: a caller who wants boxes and not text should not
/// have to pay for recognition, and a caller who has their own boxes should be
/// able to see what this port's detector would have produced.
#[cfg(feature = "onnxruntime")]
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectedRegion {
    /// The region's four corners, in the source image's coordinates.
    pub quadrilateral: Quadrilateral,
    /// The detector's mean probability inside the pre-unclip box.
    ///
    /// This is the **detector's** score, not a recognition confidence. The two
    /// are different numbers with different ranges of meaning, and a caller
    /// comparing one against a `TextLine::score` is comparing two things.
    pub score: f64,
}

#[cfg(feature = "onnxruntime")]
impl DetectedRegion {
    /// Serialises detected regions as `paddleocr-rust/detection-result/v1`.
    ///
    /// A free function rather than a method on a slice wrapper, because the
    /// document describes a **set** of regions and one region alone is not a
    /// document.
    #[must_use]
    pub fn slice_to_json(regions: &[Self], width: u32, height: u32, id: Option<&str>) -> String {
        let rows: Vec<([(f32, f32); 4], f64)> = regions
            .iter()
            .map(|region| {
                let mut corners = [(0.0_f32, 0.0_f32); 4];
                for (slot, point) in corners
                    .iter_mut()
                    .zip(region.quadrilateral.points().iter().copied())
                {
                    *slot = (point.x(), point.y());
                }
                (corners, region.score)
            })
            .collect();
        crate::structure_json::detection_to_json(&rows, width, height, id)
    }
}

/// Recognized lines together with the image their coordinates describe.
#[cfg(feature = "onnxruntime")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DocumentResult {
    /// The recognized lines.
    pub lines: Vec<TextLine>,
    /// Which image `lines`' quadrilaterals describe.
    ///
    /// `Processed` means an unwarping step ran and the coordinates **cannot**
    /// be mapped back to the page the caller supplied.
    pub coordinate_space: crate::document_pipeline::CoordinateSpace,
}

/// Maps one quadrilateral back to the caller's page.
///
/// The same rule `map_line` applies, extracted so the detection-only path does
/// not duplicate it: a coordinate that cannot be mapped is an error rather than
/// an approximation.
#[cfg(feature = "onnxruntime")]
fn map_quadrilateral(
    preprocessed: &crate::document_pipeline::DocumentPreprocessing,
    quadrilateral: Quadrilateral,
) -> Result<Quadrilateral> {
    let mut corners = [crate::types::Point::new(0.0, 0.0)?; 4];
    for (slot, point) in corners
        .iter_mut()
        .zip(quadrilateral.points().iter().copied())
    {
        match preprocessed.to_source(f64::from(point.x()), f64::from(point.y()))? {
            Some((x, y)) => *slot = crate::types::Point::new(x as f32, y as f32)?,
            None => {
                return Err(Error::InvalidInput {
                    field: "document.coordinate_space",
                    violation: InputViolation::OutOfRange,
                });
            }
        }
    }
    Quadrilateral::new(corners)
}

/// Maps one line's quadrilateral back to the caller's page.
///
/// Only called where the chain is invertible; a `None` here would mean the
/// caller was handed a coordinate space the code thought was mappable, so it is
/// a contract error rather than a silent pass-through.
#[cfg(feature = "onnxruntime")]
fn map_line(
    preprocessed: &crate::document_pipeline::DocumentPreprocessing,
    line: TextLine,
) -> Result<TextLine> {
    let mut corners = [crate::types::Point::new(0.0, 0.0)?; 4];
    for (slot, point) in corners
        .iter_mut()
        .zip(line.quadrilateral.points().iter().copied())
    {
        match preprocessed.to_source(f64::from(point.x()), f64::from(point.y()))? {
            Some((x, y)) => *slot = crate::types::Point::new(x as f32, y as f32)?,
            None => {
                return Err(Error::InvalidInput {
                    field: "document.coordinate_space",
                    violation: InputViolation::OutOfRange,
                });
            }
        }
    }
    Ok(TextLine {
        quadrilateral: crate::types::Quadrilateral::new(corners)?,
        text: line.text,
        score: line.score,
    })
}

/// Recognizes text in one PNG image using explicitly provisioned artifacts.
///
/// This loads both models, runs one image, and drops them. That is the right
/// shape for a single-image invocation and the wrong one for a batch: use
/// [`OcrEngine`] to pay session creation once across many images.
#[cfg(feature = "onnxruntime")]
pub fn recognize_png(
    artifacts: &Artifacts<'_>,
    dictionary: &Dictionary,
    png: &[u8],
    options: OcrOptions,
) -> Result<Vec<TextLine>> {
    OcrEngine::load(artifacts, dictionary)?.recognize_png(png, &options)
}

/// The placeholder recorded when a caller declares no expected digest.
#[cfg(feature = "onnxruntime")]
pub(crate) const UNCHECKED_DIGEST: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Loads one backend, hashing the artifact only when a digest was declared.
///
/// The file is streamed either way so the adapter's size and file-type gates
/// still run; only the comparison differs.
#[cfg(feature = "onnxruntime")]
fn load_backend(
    contract: &crate::backend::ModelContract,
    verify: bool,
) -> Result<crate::backend_ort::OrtBackend> {
    use crate::backend_ort::OrtBackend;
    if verify {
        let mut digest = crate::digest::Sha256::new();
        OrtBackend::load(contract, &mut digest, 1, 1)
    } else {
        let mut unchecked = UncheckedDigest;
        OrtBackend::load(contract, &mut unchecked, 1, 1)
    }
}

/// Reports the placeholder digest, so an undeclared identity is not a failure.
#[cfg(feature = "onnxruntime")]
pub(crate) struct UncheckedDigest;

#[cfg(feature = "onnxruntime")]
impl crate::backend::Sha256Stream for UncheckedDigest {
    fn update(&mut self, _bytes: &[u8]) {}
    fn finish(&mut self) -> String {
        UNCHECKED_DIGEST.to_owned()
    }
}

/// Checks that the public surface keeps the properties it documents.
#[cfg(all(test, feature = "onnxruntime"))]
mod concurrency_position {
    use super::*;

    /// Resolves to `true` only when `T: Sync`.
    ///
    /// The inherent constant is chosen ahead of the trait's default when its
    /// bound holds, which is the stable-Rust way to observe an auto trait
    /// without a compile failure. Asserting `!Sync` in a normal test is
    /// otherwise impossible: a type that *is* `Sync` would simply compile.
    struct Probe<T>(core::marker::PhantomData<T>);

    trait NotSync {
        const IS_SYNC: bool = false;
    }

    impl<T> NotSync for Probe<T> {}

    impl<T: Sync> Probe<T> {
        const IS_SYNC: bool = true;
    }

    /// The engine must stay `!Sync`, because that is what stops a caller from
    /// sharing one session across threads.
    ///
    /// If a future change makes it `Sync` — by adding a lock, say — this fails,
    /// and it should: the documented position is one engine per thread, and a
    /// lock would turn a compile error into a hidden queue.
    #[test]
    // The lint objects to asserting a constant, which is precisely the point:
    // the value is decided at compile time by whether the auto trait holds, and
    // there is nothing to evaluate at runtime.
    #[allow(clippy::assertions_on_constants)]
    fn the_engine_is_not_shareable_across_threads() {
        assert!(
            !<Probe<OcrEngine>>::IS_SYNC,
            "OcrEngine became Sync; the documented concurrency position no longer holds"
        );
        // The probe itself must be able to see a Sync type, or the assertion
        // above would pass for the wrong reason.
        assert!(<Probe<u32>>::IS_SYNC, "the probe is not detecting Sync");
    }
}

/// Optional check that a reused engine is equivalent to reloading per image.
///
/// This is the `API-001` claim that session reuse changes cost and nothing
/// else. It is ignored by default because it needs provisioned artifacts.
///
/// ```sh
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
/// PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
/// PADDLEOCR_RUST_DICTIONARY=<dict.txt> \
///   cargo test --release --features onnxruntime --lib -- --ignored --nocapture engine_reuse
/// ```
#[cfg(all(test, feature = "onnxruntime"))]
mod engine_reuse {
    use super::*;

    const PAGES: [(&str, &[u8]); 3] = [
        (
            "reading-order",
            include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png"),
        ),
        (
            "unicode",
            include_bytes!("../tests/fixtures/classic-v1-e2e-unicode/input.png"),
        ),
        (
            "tall-crop",
            include_bytes!("../tests/fixtures/classic-v1-e2e-tall-crop/input.png"),
        ),
    ];

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) => value,
            Err(_) => panic!("set {name}"),
        }
    }

    #[test]
    #[ignore = "API-001: needs explicitly provisioned models"]
    fn a_reused_engine_returns_the_same_results_as_reloading() {
        let library = env("PADDLEOCR_RUST_ORT_DYLIB");
        let detector = env("PADDLEOCR_RUST_DETECTOR_ONNX");
        let recognizer = env("PADDLEOCR_RUST_RECOGNIZER_ONNX");
        let dictionary_text = match std::fs::read_to_string(env("PADDLEOCR_RUST_DICTIONARY")) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let dictionary = match parse_dictionary(&dictionary_text, true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let artifacts = Artifacts::new(&library, &detector, &recognizer);
        let options = OcrOptions::default();

        let loading = std::time::Instant::now();
        let engine = match OcrEngine::load(&artifacts, &dictionary) {
            Ok(engine) => engine,
            Err(error) => panic!("load: {error}"),
        };
        println!(
            "[engine] load took {:.3} s",
            loading.elapsed().as_secs_f64()
        );

        for (name, png) in PAGES {
            let reused = match engine.recognize_png(png, &options) {
                Ok(lines) => lines,
                Err(error) => panic!("{name} reused: {error}"),
            };
            let fresh = match recognize_png(&artifacts, &dictionary, png, options.clone()) {
                Ok(lines) => lines,
                Err(error) => panic!("{name} fresh: {error}"),
            };
            assert_eq!(reused, fresh, "{name}: reuse changed the result");
            println!("[engine] {name}: {} lines, identical", reused.len());
        }

        // Running the same page twice through one engine must also agree, which
        // is what "no state carries between images" means concretely.
        let first = match engine.recognize_png(PAGES[0].1, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("repeat: {error}"),
        };
        let second = match engine.recognize_png(PAGES[0].1, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("repeat: {error}"),
        };
        assert_eq!(first, second, "a repeated image changed its own result");
    }
}
