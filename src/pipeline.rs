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
use crate::crop::{InterleavedImage, classic_perspective_crop};
use crate::detector::detect_boxes;
use crate::dictionary::CtcDictionary;
use crate::error::Result;
use crate::geometry::{classic_perspective_crop_plan, classic_sort_quadrilaterals};
use crate::recognizer::recognize;
use crate::score_filter::retain_by_score;
use crate::types::{Point, Quadrilateral};

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
}

/// Runs the complete classic pipeline over one decoded BGR image.
pub(crate) fn run_classic_ocr(
    models: &ClassicModels<'_>,
    image: &InterleavedImage,
    thresholds: ClassicThresholds,
) -> Result<Vec<OcrLine>> {
    let (detector, detector_contract) = models.detector;
    let (recognizer, recognizer_contract) = models.recognizer;
    let dictionary = models.dictionary;
    let ClassicThresholds {
        box_threshold,
        unclip_ratio,
        drop_score,
    } = thresholds;

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

    let mut crops = Vec::with_capacity(quadrilaterals.len());
    for quadrilateral in &quadrilaterals {
        let plan = classic_perspective_crop_plan(*quadrilateral)?;
        crops.push(classic_perspective_crop(image, plan)?);
    }
    let borrowed: Vec<&InterleavedImage> = crops.iter().collect();
    let recognized = recognize(recognizer, recognizer_contract, dictionary, &borrowed)?;

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

        match run_classic_ocr(
            &ClassicModels {
                detector: (&detector, &detector_contract),
                recognizer: (&recognizer, &recognizer_contract),
                dictionary: &dictionary,
            },
            &source,
            ClassicThresholds {
                box_threshold: 0.5,
                unclip_ratio: 1.5,
                drop_score: 0.5,
            },
        ) {
            Ok(lines) => lines,
            Err(error) => panic!("pipeline failed: {error}"),
        }
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
#[cfg(all(test, feature = "onnxruntime"))]
mod g1 {
    use super::*;

    use std::path::Path;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};
    use crate::backend_ort::{OrtBackend, initialize_runtime};
    use crate::image::decode_classic_bgr;
    use crate::types::EncodedImage;

    const REORDER_PNG: &[u8] =
        include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png");
    const EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-e2e-reading-order/expected.json");

    struct Sha256(Vec<u8>);

    impl crate::backend::Sha256Stream for Sha256 {
        fn update(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        fn finish(&mut self) -> String {
            crate::backend_ort::tests::sha256_hex_for_tests(&self.0)
        }
    }

    /// Unwraps in this developer-only gate without `expect`, which the crate
    /// lints deny.
    fn must<T, E: core::fmt::Display>(value: core::result::Result<T, E>, what: &str) -> T {
        match value {
            Ok(value) => value,
            Err(error) => panic!("{what}: {error}"),
        }
    }

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) => value,
            Err(_) => panic!("set {name}"),
        }
    }

    #[test]
    #[ignore = "gate G1: needs explicitly provisioned models"]
    fn the_pipeline_reproduces_the_recorded_reading_order_fixture() {
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
                ModelArtifact::new(
                    env("PADDLEOCR_RUST_DETECTOR_ONNX"),
                    "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1",
                ),
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
                ModelArtifact::new(
                    env("PADDLEOCR_RUST_RECOGNIZER_ONNX"),
                    "9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba",
                ),
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

        let encoded = must(EncodedImage::new(REORDER_PNG), "encoded png");
        let image = must(decode_classic_bgr(encoded), "decode png");

        let lines = must(
            run_classic_ocr(
                &ClassicModels {
                    detector: (&detector, &detector_contract),
                    recognizer: (&recognizer, &recognizer_contract),
                    dictionary: &dictionary,
                },
                &image,
                ClassicThresholds {
                    box_threshold: 0.6,
                    unclip_ratio: 1.5,
                    drop_score: 0.5,
                },
            ),
            "run the pipeline",
        );

        let expected: serde_json::Value = must(serde_json::from_str(EXPECTED), "expected json");
        let recorded = match expected["lines"].as_array() {
            Some(lines) => lines.clone(),
            None => panic!("the expected fixture must record lines"),
        };
        let wanted: Vec<String> = recorded
            .iter()
            .map(|line| line["text"].as_str().unwrap_or_default().to_owned())
            .collect();
        let got: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();

        println!("G1 expected: {wanted:?}");
        println!("G1 actual  : {got:?}");
        for line in &lines {
            println!("  score {:.6} text {:?}", line.score, line.text);
        }
        assert_eq!(got, wanted, "the pipeline must reproduce the recorded text");

        // Confidences must match the recording too, not just the text.
        for (line, record) in lines.iter().zip(&recorded) {
            let recorded_score = record["confidence"].as_f64().unwrap_or_default();
            assert!(
                (line.score - recorded_score).abs() < 1e-5,
                "confidence for {:?}: got {}, recorded {recorded_score}",
                line.text,
                line.score
            );
        }
    }
}
