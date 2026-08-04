// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The classic OCR pipeline: one decoded image to recognized lines.
//!
//! The sequence is frozen by `docs/CLASSIC_OCR_CONTRACT.md`:
//!
//! detect -> stable reading-order sort -> perspective crop -> tall-crop
//! rotation -> aspect-sorted recognition batch -> original-order restore ->
//! inclusive score filter.
//!
//! Two orderings are live here and are easy to confuse. The **reading order**
//! is established once, right after detection, and every later step preserves
//! it. The **aspect order** exists only inside the recognizer and is undone
//! before results are returned. A line's index in the output therefore refers
//! to reading order throughout.
//!
//! The tall-crop rotation is not applied here: it belongs to the crop plan,
//! which rotates a crop whose height/width ratio reaches 1.5.

use crate::backend::{InferenceBackend, ModelContract};
use crate::control::RunSchedule;
use crate::crop::{InterleavedImage, classic_perspective_crop};
use crate::detector::detect_boxes;
use crate::dictionary::CtcDictionary;
use crate::error::Result;
use crate::geometry::{classic_perspective_crop_plan, classic_sort_quadrilaterals};
use crate::orientation::{classify, orientation_input_size, rotate_180};
use crate::recognizer::recognize;
use crate::resize::classic_linear_resize;
use crate::score_filter::retain_by_score;
use crate::types::{ImageDimensions, Point, Quadrilateral};

/// One recognized text line with its source-space quadrilateral.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OcrLine {
    /// Four corners in the original image's coordinates.
    pub(crate) quadrilateral: Quadrilateral,
    /// Decoded text.
    pub(crate) text: String,
    /// CTC confidence.
    pub(crate) score: f64,
}

/// The two models and the dictionary the pipeline runs against.
pub(crate) struct ClassicModels<'a> {
    /// Detector backend and its validated contract.
    pub(crate) detector: (&'a dyn InferenceBackend, &'a ModelContract),
    /// Recognizer backend and its validated contract.
    pub(crate) recognizer: (&'a dyn InferenceBackend, &'a ModelContract),
    /// Dictionary bound to the recognizer artifact.
    pub(crate) dictionary: &'a CtcDictionary,
    /// Optional text-line orientation classifier and its validated contract.
    ///
    /// `None` matches upstream's default: `use_angle_cls` is `False`, so a
    /// caller who has not provisioned the classifier gets exactly the behaviour
    /// they had before it existed.
    pub(crate) orientation: Option<(&'a dyn InferenceBackend, &'a ModelContract)>,
}

/// The frozen thresholds the classic pipeline applies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassicThresholds {
    /// Minimum mean probability for a detected box.
    pub(crate) box_threshold: f64,
    /// Polygon expansion ratio.
    pub(crate) unclip_ratio: f64,
    /// Minimum recognition confidence; equality is retained.
    pub(crate) drop_score: f64,
    /// Minimum orientation confidence to act on; equality does **not** rotate.
    pub(crate) orientation_threshold: f64,
}

/// Runs the complete classic pipeline over one decoded BGR image.
///
/// # Failure semantics
///
/// This path fails whole-input. Any error from any stage — a rejected tensor, a
/// backend failure, an exhausted time budget, a cancellation — abandons the
/// entire request and returns that error. No partial line list is ever returned.
///
/// That is a deliberate choice, not a limitation of the implementation. The
/// result document has no field that marks a result as incomplete, so a caller
/// receiving four lines from a nine-line page could not tell it apart from a
/// four-line page. Per-item recovery would have to be visible in the output type
/// to be safe, and that is an `API-001` decision this item does not preempt.
pub(crate) fn run_classic_ocr(
    models: &ClassicModels<'_>,
    image: &InterleavedImage,
    thresholds: ClassicThresholds,
    schedule: &RunSchedule<'_>,
) -> Result<Vec<OcrLine>> {
    let (detector, detector_contract) = models.detector;
    let (recognizer, recognizer_contract) = models.recognizer;
    let dictionary = models.dictionary;
    let ClassicThresholds {
        box_threshold,
        unclip_ratio,
        drop_score,
        orientation_threshold,
    } = thresholds;

    schedule.check("detector")?;
    let detected = detect_boxes(
        detector,
        detector_contract,
        image,
        box_threshold,
        unclip_ratio,
    )?;
    if detected.is_empty() {
        return Ok(Vec::new());
    }

    let mut quadrilaterals = Vec::with_capacity(detected.len());
    for entry in &detected {
        let mut corners = [Point::new(0.0, 0.0)?; 4];
        for (slot, (x, y)) in corners.iter_mut().zip(&entry.corners) {
            *slot = Point::new(*x as f32, *y as f32)?;
        }
        quadrilaterals.push(Quadrilateral::new(corners)?);
    }
    // Establish reading order once; every later step preserves it.
    classic_sort_quadrilaterals(&mut quadrilaterals);

    // Cropping is bounded by the region count, which the detector already caps,
    // but it is the last cheap place to stop before recognition dominates.
    schedule.check("crop")?;
    let mut crops: Vec<InterleavedImage> = Vec::with_capacity(quadrilaterals.len());
    for quadrilateral in &quadrilaterals {
        let plan = classic_perspective_crop_plan(*quadrilateral)?;
        crops.push(classic_perspective_crop(image, plan)?);
    }
    // Optional orientation stage, between cropping and recognition, which is
    // where upstream places it. A crop the classifier is confident is upside
    // down is replaced by its rotated self, so recognition sees the corrected
    // image; the detected polygon is untouched, because rotating a crop does not
    // move the region it came from.
    if let Some((backend, orientation_contract)) = models.orientation {
        schedule.check("orientation")?;
        let (width, height) = orientation_input_size();
        let target = ImageDimensions::new(width, height)?;
        let mut resized = Vec::with_capacity(crops.len());
        for crop in &crops {
            resized.push(classic_linear_resize(crop, target)?);
        }
        let borrowed: Vec<&InterleavedImage> = resized.iter().collect();
        let verdicts = classify(
            backend,
            orientation_contract,
            &borrowed,
            orientation_threshold,
        )?;
        for (crop, verdict) in crops.iter_mut().zip(&verdicts) {
            if verdict.rotate {
                *crop = rotate_180(crop)?;
            }
        }
    }

    let borrowed: Vec<&InterleavedImage> = crops.iter().collect();
    let recognized = recognize(
        recognizer,
        recognizer_contract,
        dictionary,
        &borrowed,
        schedule,
    )?;

    let paired: Vec<(OcrLine, f64)> = quadrilaterals
        .into_iter()
        .zip(recognized)
        .map(|(quadrilateral, line)| {
            let score = line.score;
            (
                OcrLine {
                    quadrilateral,
                    text: line.text,
                    score,
                },
                score,
            )
        })
        .collect();
    retain_by_score(paired, drop_score)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::{AxisExtent, BackendTensor, ModelArtifact, RunBudget, TensorContract};
    use crate::types::ImageDimensions;

    struct FakeDetector {
        map: Vec<f32>,
        width: usize,
        height: usize,
    }

    impl InferenceBackend for FakeDetector {
        fn run(&self, _input: &BackendTensor) -> Result<(String, BackendTensor)> {
            let tensor = BackendTensor::new(vec![1, 1, self.height, self.width], self.map.clone())?;
            Ok(("fetch_name_0".to_owned(), tensor))
        }
    }

    struct FakeRecognizer {
        classes: usize,
        score: f32,
    }

    impl InferenceBackend for FakeRecognizer {
        fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
            let batch = input.shape()[0];
            let time = 2_usize;
            let mut values = vec![0.0_f32; batch * time * self.classes];
            for row in 0..batch {
                for step in 0..time {
                    // Class index 1 is the dictionary's first entry.
                    values[(row * time + step) * self.classes + 1] = self.score;
                }
            }
            let tensor = BackendTensor::new(vec![batch, time, self.classes], values)?;
            Ok(("fetch_name_0".to_owned(), tensor))
        }
    }

    fn free() -> AxisExtent {
        AxisExtent::Bounded {
            minimum: 1,
            maximum: 8192,
        }
    }

    fn contract(input: Vec<AxisExtent>, output: Vec<AxisExtent>) -> ModelContract {
        let artifact = match ModelArtifact::new("/nonexistent/m.onnx", "0".repeat(64)) {
            Ok(value) => value,
            Err(error) => panic!("artifact: {error}"),
        };
        let input = match TensorContract::new("x", input) {
            Ok(value) => value,
            Err(error) => panic!("input contract: {error}"),
        };
        let output = match TensorContract::new("fetch_name_0", output) {
            Ok(value) => value,
            Err(error) => panic!("output contract: {error}"),
        };
        let budget = match RunBudget::new(40_000_000, 40_000_000, 64) {
            Ok(value) => value,
            Err(error) => panic!("budget: {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn image(width: u32, height: u32) -> InterleavedImage {
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, 3, vec![160_u8; (width * height * 3) as usize]) {
            Ok(value) => value,
            Err(error) => panic!("image: {error}"),
        }
    }

    fn dictionary() -> CtcDictionary {
        match CtcDictionary::new(vec!["x".to_owned(), "y".to_owned()], true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        }
    }

    /// Runs the pipeline over one centred region with the given confidence.
    fn run_with_confidence(confidence: f32) -> Vec<OcrLine> {
        match run_with_schedule(confidence, &crate::control::unbounded_schedule()) {
            Ok(lines) => lines,
            Err(error) => panic!("pipeline failed: {error}"),
        }
    }

    /// Runs the same pipeline under a caller-supplied schedule.
    fn run_with_schedule(
        confidence: f32,
        schedule: &crate::control::RunSchedule<'_>,
    ) -> Result<Vec<OcrLine>> {
        let source = image(64, 48);
        let plan = crate::geometry::classic_detector_resize_plan(source.dimensions());
        let resized = plan.resized();
        let (map_width, map_height) = (resized.width() as usize, resized.height() as usize);

        let mut map = vec![0.0_f32; map_width * map_height];
        for y in map_height / 4..map_height * 3 / 4 {
            for x in map_width / 4..map_width * 3 / 4 {
                map[y * map_width + x] = 0.9;
            }
        }

        let dictionary = dictionary();
        let classes = dictionary.class_count();
        let detector = FakeDetector {
            map,
            width: map_width,
            height: map_height,
        };
        let detector_contract = contract(
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(3), free(), free()],
            vec![
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(map_height),
                AxisExtent::Fixed(map_width),
            ],
        );
        let recognizer = FakeRecognizer {
            classes,
            score: confidence,
        };
        let recognizer_contract = contract(
            vec![
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(3),
                AxisExtent::Fixed(48),
                free(),
            ],
            vec![AxisExtent::Fixed(1), free(), AxisExtent::Fixed(classes)],
        );

        run_classic_ocr(
            &ClassicModels {
                detector: (&detector, &detector_contract),
                recognizer: (&recognizer, &recognizer_contract),
                dictionary: &dictionary,
                orientation: None,
            },
            &source,
            ClassicThresholds {
                box_threshold: 0.5,
                unclip_ratio: 1.5,
                drop_score: 0.5,
                orientation_threshold: crate::orientation::ORIENTATION_THRESHOLD,
            },
            schedule,
        )
    }

    /// A cancelled run returns a typed error and no lines at all.
    ///
    /// The point is the *and no lines*: a partial result would be
    /// indistinguishable from a complete one, since nothing in the result
    /// document marks it as truncated.
    #[test]
    fn a_cancelled_run_returns_no_partial_result() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let control = crate::control::RunControl::unbounded()
            .with_cancel_flag(Arc::new(AtomicBool::new(true)));
        let outcome = run_with_schedule(0.9, &control.begin());
        assert!(
            matches!(outcome, Err(crate::error::Error::Cancelled)),
            "expected a cancellation, got {outcome:?}"
        );
    }

    /// An exhausted budget stops at the first stage boundary, before the
    /// detector ever runs.
    #[test]
    fn an_exhausted_budget_stops_before_the_detector() {
        let control =
            crate::control::RunControl::unbounded().with_time_budget(std::time::Duration::ZERO);
        match run_with_schedule(0.9, &control.begin()) {
            Err(crate::error::Error::TimedOut { stage }) => {
                assert_eq!(stage, "detector", "the first checkpoint is the detector");
            }
            other => panic!("expected a timeout, got {other:?}"),
        }
    }

    /// An unbounded control is exactly the previous behaviour.
    #[test]
    fn an_unbounded_control_changes_nothing() {
        let control = crate::control::RunControl::unbounded();
        let with_control = match run_with_schedule(0.9, &control.begin()) {
            Ok(lines) => lines,
            Err(error) => panic!("pipeline failed: {error}"),
        };
        assert_eq!(with_control, run_with_confidence(0.9));
    }

    #[test]
    fn one_region_becomes_one_recognized_line() {
        let lines = run_with_confidence(0.9);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "x", "class 1 maps to the first entry");
        assert!(lines[0].score > 0.5);
    }

    #[test]
    fn a_low_confidence_line_is_filtered_out() {
        // A 0.2 confidence sits below the 0.5 drop score.
        assert!(
            run_with_confidence(0.2).is_empty(),
            "a 0.2 confidence must not survive 0.5"
        );
    }
}

/// Optional end-to-end check against explicitly provisioned real models.
///
/// This is gate `G1` from `docs/ADR_RT004_RUNTIME_SELECTION.md`. It is ignored
/// by default because it needs an ONNX Runtime library, both model artifacts,
/// and a dictionary that this repository never ships.
///
/// ```sh
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
/// PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
/// PADDLEOCR_RUST_DICTIONARY=<dict.txt> \
///   cargo test --features onnxruntime --lib -- --ignored --nocapture g1
/// ```
/// Shared provisioning for the optional gates that need real artifacts.
///
/// `G1` (semantics) and `G3` (resources) load exactly the same models under
/// exactly the same contracts and thread policy. Sharing the setup is what
/// makes the two results comparable: a latency number measured against a
/// different contract, thread count, or artifact would not be evidence about
/// the configuration `G1` verified.
#[cfg(all(test, feature = "onnxruntime"))]
pub(crate) mod gate_support {
    use super::*;

    use std::path::Path;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};
    use crate::backend_ort::{OrtBackend, initialize_runtime};

    /// The pinned detector artifact digest, from the reviewed fixtures.
    pub(crate) const DETECTOR_SHA256: &str =
        "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1";

    /// The pinned recognizer artifact digest, from the reviewed fixtures.
    pub(crate) const RECOGNIZER_SHA256: &str =
        "9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba";

    /// The thresholds both gates run under.
    pub(crate) const THRESHOLDS: ClassicThresholds = ClassicThresholds {
        box_threshold: 0.6,
        unclip_ratio: 1.5,
        drop_score: 0.5,
        orientation_threshold: crate::orientation::ORIENTATION_THRESHOLD,
    };

    pub(crate) struct Sha256(Vec<u8>);

    impl crate::backend::Sha256Stream for Sha256 {
        fn update(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        fn finish(&mut self) -> String {
            crate::backend_ort::tests::sha256_hex_for_tests(&self.0)
        }
    }

    /// Unwraps in these developer-only gates without `expect`, which the crate
    /// lints deny.
    pub(crate) fn must<T, E: core::fmt::Display>(
        value: core::result::Result<T, E>,
        what: &str,
    ) -> T {
        match value {
            Ok(value) => value,
            Err(error) => panic!("{what}: {error}"),
        }
    }

    pub(crate) fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) => value,
            Err(_) => panic!("set {name}"),
        }
    }

    /// Everything a gate needs to run the classic pipeline.
    pub(crate) struct Provisioned {
        pub(crate) detector: OrtBackend,
        pub(crate) detector_contract: ModelContract,
        pub(crate) recognizer: OrtBackend,
        pub(crate) recognizer_contract: ModelContract,
        pub(crate) dictionary: CtcDictionary,
    }

    impl Provisioned {
        /// Borrows the loaded models as the pipeline expects them.
        pub(crate) fn models(&self) -> ClassicModels<'_> {
            ClassicModels {
                detector: (&self.detector, &self.detector_contract),
                recognizer: (&self.recognizer, &self.recognizer_contract),
                dictionary: &self.dictionary,
                orientation: None,
            }
        }
    }

    /// Loads both models and the dictionary from the environment, single
    /// threaded, verifying artifact identity before either session is created.
    pub(crate) fn load() -> Provisioned {
        must(
            initialize_runtime(Path::new(&env("PADDLEOCR_RUST_ORT_DYLIB"))),
            "initialise the runtime",
        );

        let free = AxisExtent::Bounded {
            minimum: 1,
            maximum: 8192,
        };
        let detector_contract = ModelContract::new(
            must(
                ModelArtifact::new(env("PADDLEOCR_RUST_DETECTOR_ONNX"), DETECTOR_SHA256),
                "detector artifact",
            ),
            must(
                TensorContract::new(
                    "x",
                    vec![AxisExtent::Fixed(1), AxisExtent::Fixed(3), free, free],
                ),
                "detector input",
            ),
            must(
                TensorContract::new(
                    "fetch_name_0",
                    vec![AxisExtent::Fixed(1), AxisExtent::Fixed(1), free, free],
                ),
                "detector output",
            ),
            must(RunBudget::new(40_000_000, 40_000_000, 1), "detector budget"),
        );

        let dictionary_text = must(
            std::fs::read_to_string(env("PADDLEOCR_RUST_DICTIONARY")),
            "dictionary file",
        );
        let entries: Vec<String> = dictionary_text.lines().map(str::to_owned).collect();
        let dictionary = must(CtcDictionary::new(entries, true), "dictionary");
        let classes = dictionary.class_count();

        let recognizer_contract = ModelContract::new(
            must(
                ModelArtifact::new(env("PADDLEOCR_RUST_RECOGNIZER_ONNX"), RECOGNIZER_SHA256),
                "recognizer artifact",
            ),
            must(
                TensorContract::new(
                    "x",
                    vec![free, AxisExtent::Fixed(3), AxisExtent::Fixed(48), free],
                ),
                "recognizer input",
            ),
            must(
                TensorContract::new("fetch_name_0", vec![free, free, AxisExtent::Fixed(classes)]),
                "recognizer output",
            ),
            must(
                RunBudget::new(40_000_000, 40_000_000, 64),
                "recognizer budget",
            ),
        );

        let mut detector_digest = Sha256(Vec::new());
        let detector = must(
            OrtBackend::load(&detector_contract, &mut detector_digest, 1, 1),
            "load the detector",
        );
        let mut recognizer_digest = Sha256(Vec::new());
        let recognizer = must(
            OrtBackend::load(&recognizer_contract, &mut recognizer_digest, 1, 1),
            "load the recognizer",
        );

        Provisioned {
            detector,
            detector_contract,
            recognizer,
            recognizer_contract,
            dictionary,
        }
    }
}

/// Optional end-to-end check against explicitly provisioned real models.
///
/// This is gate `G1` from `docs/ADR_RT004_RUNTIME_SELECTION.md`. It is ignored
/// by default because it needs an ONNX Runtime library, both model artifacts,
/// and a dictionary that this repository never ships.
///
/// ```sh
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
/// PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
/// PADDLEOCR_RUST_DICTIONARY=<dict.txt> \
///   cargo test --features onnxruntime --lib -- --ignored --nocapture g1
/// ```
#[cfg(all(test, feature = "onnxruntime"))]
mod g1 {
    use super::gate_support::{load, must};
    use super::*;

    use crate::image::decode_classic_bgr;
    use crate::types::EncodedImage;

    /// The four committed end-to-end fixtures: input bytes and expectation.
    const FIXTURES: [(&str, &[u8], &str); 4] = [
        (
            "reading-order",
            include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png"),
            include_str!("../tests/fixtures/classic-v1-e2e-reading-order/expected.json"),
        ),
        (
            "no-text",
            include_bytes!("../tests/fixtures/classic-v1-e2e-no-text/input.png"),
            include_str!("../tests/fixtures/classic-v1-e2e-no-text/expected.json"),
        ),
        (
            "tall-crop",
            include_bytes!("../tests/fixtures/classic-v1-e2e-tall-crop/input.png"),
            include_str!("../tests/fixtures/classic-v1-e2e-tall-crop/expected.json"),
        ),
        (
            "unicode",
            include_bytes!("../tests/fixtures/classic-v1-e2e-unicode/input.png"),
            include_str!("../tests/fixtures/classic-v1-e2e-unicode/expected.json"),
        ),
    ];

    #[test]
    #[ignore = "gate G1: needs explicitly provisioned models"]
    fn the_pipeline_reproduces_every_recorded_end_to_end_fixture() {
        let provisioned = load();

        let mut failures = Vec::new();
        for (name, png, expectation) in FIXTURES {
            let encoded = must(EncodedImage::new(png), "encoded png");
            let image = must(decode_classic_bgr(encoded), "decode png");
            let lines = must(
                run_classic_ocr(
                    &provisioned.models(),
                    &image,
                    super::gate_support::THRESHOLDS,
                    &crate::control::unbounded_schedule(),
                ),
                "run the pipeline",
            );

            let expected: serde_json::Value = must(serde_json::from_str(expectation), "expected");
            let recorded = match expected["lines"].as_array() {
                Some(lines) => lines.clone(),
                None => panic!("{name}: the expected fixture must record lines"),
            };
            let wanted: Vec<String> = recorded
                .iter()
                .map(|line| line["text"].as_str().unwrap_or_default().to_owned())
                .collect();
            let got: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();

            println!("[{name}] expected {wanted:?}");
            println!("[{name}] actual   {got:?}");
            if got != wanted {
                failures.push(format!("{name}: expected {wanted:?}, got {got:?}"));
                continue;
            }
            for (line, record) in lines.iter().zip(&recorded) {
                let recorded_score = record["confidence"].as_f64().unwrap_or_default();
                if (line.score - recorded_score).abs() >= 1e-5 {
                    failures.push(format!(
                        "{name}: confidence for {:?} was {} against {recorded_score}",
                        line.text, line.score
                    ));
                }
            }
        }

        assert!(failures.is_empty(), "end-to-end mismatches: {failures:#?}");
    }
}

/// Optional resource measurement against explicitly provisioned real models.
///
/// This is gate `G3` from `docs/ADR_RT004_RUNTIME_SELECTION.md`, for the two
/// budgets in `docs/QUALITY_PROFILE.md` that can only be observed in process:
/// warm end-to-end latency, median at most `5 s` and p95 at most `10 s` across
/// twenty runs of the same 1280x720 fixture after model warmup, single
/// threaded.
///
/// Cold CLI latency, peak resident memory, and stripped binary size are process
/// level and are measured outside this test; `docs/G3_RESOURCE_EVIDENCE.md`
/// records the commands and results for all four.
///
/// The measurement runs the whole in-process path per iteration — decode,
/// detect, crop, recognize, filter — because that is what the budget names. It
/// deliberately does not reload the models per iteration; that is the cold
/// path, and reporting it here would make the warm figure meaningless.
///
/// ```sh
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
/// PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
/// PADDLEOCR_RUST_DICTIONARY=<dict.txt> \
///   cargo test --release --features onnxruntime --lib -- --ignored --nocapture g3
/// ```
#[cfg(all(test, feature = "onnxruntime"))]
mod g3 {
    use super::gate_support::{THRESHOLDS, load, must};
    use super::*;

    use std::time::Instant;

    use crate::image::decode_classic_bgr;
    use crate::types::EncodedImage;

    /// The 1280x720 page the resource budgets are stated against.
    const BENCHMARK_PAGE: &[u8] =
        include_bytes!("../tests/fixtures/classic-v1-benchmark-page/input.png");

    /// The run count the budget names.
    const RUNS: usize = 20;

    /// `docs/QUALITY_PROFILE.md`: warm median at most five seconds.
    const MEDIAN_BUDGET_SECONDS: f64 = 5.0;

    /// `docs/QUALITY_PROFILE.md`: warm p95 at most ten seconds.
    const P95_BUDGET_SECONDS: f64 = 10.0;

    /// Returns the value at a percentile using the nearest-rank definition.
    ///
    /// Nearest rank is chosen over interpolation because it always reports a
    /// time that was actually observed. With twenty samples the p95 is the
    /// nineteenth sorted value, so an interpolated figure would be a number no
    /// run produced.
    fn nearest_rank(sorted: &[f64], percentile: f64) -> f64 {
        let rank = (percentile / 100.0 * sorted.len() as f64).ceil().max(1.0) as usize;
        sorted[rank.min(sorted.len()) - 1]
    }

    #[test]
    #[ignore = "gate G3: needs explicitly provisioned models"]
    fn warm_end_to_end_latency_stays_inside_the_declared_budget() {
        let provisioned = load();
        let models = provisioned.models();

        let decode = || {
            let encoded = must(EncodedImage::new(BENCHMARK_PAGE), "encoded png");
            must(decode_classic_bgr(encoded), "decode png")
        };

        // One discarded run performs the runtime's own first-call allocation and
        // any lazy kernel setup. Counting it would report cold cost as warm.
        let schedule = crate::control::unbounded_schedule();
        let warmup = must(
            run_classic_ocr(&models, &decode(), THRESHOLDS, &schedule),
            "warmup run",
        );
        println!("[g3] warmup detected {} lines", warmup.len());

        let mut samples = Vec::with_capacity(RUNS);
        let mut line_counts = Vec::with_capacity(RUNS);
        for index in 0..RUNS {
            let image = decode();
            let started = Instant::now();
            let lines = must(
                run_classic_ocr(&models, &image, THRESHOLDS, &schedule),
                "measured run",
            );
            let elapsed = started.elapsed().as_secs_f64();
            println!("[g3] run {index}: {elapsed:.3} s, {} lines", lines.len());
            samples.push(elapsed);
            line_counts.push(lines.len());
        }

        // A run that recognized a different number of lines is not the same
        // work, so it is not a comparable sample.
        assert!(
            line_counts.windows(2).all(|pair| pair[0] == pair[1]),
            "the runs were not equivalent work: {line_counts:?}"
        );

        let mut sorted = samples.clone();
        sorted.sort_by(|left, right| {
            left.partial_cmp(right)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let median = nearest_rank(&sorted, 50.0);
        let p95 = nearest_rank(&sorted, 95.0);
        println!(
            "[g3] runs={RUNS} min={:.3}s median={median:.3}s p95={p95:.3}s max={:.3}s lines={}",
            sorted[0],
            sorted[sorted.len() - 1],
            line_counts[0]
        );

        assert!(
            median <= MEDIAN_BUDGET_SECONDS,
            "warm median {median:.3} s exceeds the {MEDIAN_BUDGET_SECONDS} s budget"
        );
        assert!(
            p95 <= P95_BUDGET_SECONDS,
            "warm p95 {p95:.3} s exceeds the {P95_BUDGET_SECONDS} s budget"
        );
    }

    /// The decoded page must not need more than the declared decode envelope.
    ///
    /// This is the one adapter-boundary resource claim that does not need a
    /// process-level measurement: the budget is a compile-time constant and the
    /// page's decoded size is known exactly.
    #[test]
    fn the_benchmark_page_stays_inside_the_decode_envelope() {
        let encoded = must(EncodedImage::new(BENCHMARK_PAGE), "encoded png");
        let image = must(decode_classic_bgr(encoded), "decode png");
        assert_eq!(image.pixels().len(), 1280 * 720 * 3);
    }
}
