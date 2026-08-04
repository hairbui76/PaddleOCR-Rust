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

        Ok(Self {
            detector,
            detector_contract,
            recognizer,
            recognizer_contract,
            dictionary: dictionary.inner.clone(),
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

    /// Recognizes text in one PNG image, reusing the loaded sessions.
    ///
    /// Each call is independent: no state carries between images, so the same
    /// input always produces the same result, and a failed call leaves the
    /// engine usable. `options` is taken by reference because it now carries the
    /// cancellation flag, which a caller will usually want to keep.
    pub fn recognize_png(&self, png: &[u8], options: &OcrOptions) -> Result<Vec<TextLine>> {
        use crate::pipeline::{ClassicModels, ClassicThresholds, run_classic_ocr};

        let encoded = EncodedImage::new(png)?;
        let image = crate::image::decode_classic_bgr(encoded)?;
        let lines = run_classic_ocr(
            &ClassicModels {
                detector: (&self.detector, &self.detector_contract),
                recognizer: (&self.recognizer, &self.recognizer_contract),
                dictionary: &self.dictionary,
            },
            &image,
            ClassicThresholds {
                box_threshold: options.box_threshold,
                unclip_ratio: options.unclip_ratio,
                drop_score: options.drop_score,
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
const UNCHECKED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
struct UncheckedDigest;

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
