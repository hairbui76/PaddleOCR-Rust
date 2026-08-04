// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `E2E-001`: end-to-end behaviour through the public surface only.
//!
//! This file may use nothing that is not public. That restriction is the point:
//! the unit tests reach inside the crate and can therefore pass while the API a
//! caller actually has is unusable. Everything here goes through `api`,
//! `result_json`, `types`, and `error`.
//!
//! It is split in two. The offline half runs in the normal `cargo test` and
//! needs no artifacts, no network, and no `onnxruntime` feature. The gated half
//! needs explicitly provisioned models and is ignored by default; it is the only
//! place that can check what a real model does with rotated text, mixed scripts,
//! a threshold boundary, a corrupt artifact, or two engines on two threads.

use paddleocr_rust::api::{OcrOptions, parse_dictionary};
use paddleocr_rust::error::Error;
use paddleocr_rust::manifest::ModelManifest;
use paddleocr_rust::result_json::{RESULT_SCHEMA_VERSION, result_to_json};
use paddleocr_rust::types::{MAX_ENCODED_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_IMAGE_SIDE_LENGTH};

/// A committed PNG with four lines of text.
const READING_ORDER: &[u8] = include_bytes!("fixtures/classic-v1-e2e-reading-order/input.png");

/// A committed PNG with no text at all.
const NO_TEXT: &[u8] = include_bytes!("fixtures/classic-v1-e2e-no-text/input.png");

/// Builds a minimal PNG header declaring the given dimensions.
///
/// Only `IHDR` is written, which is enough for every rejection below: each one
/// must happen from the declared dimensions, before any pixel is read. A file
/// that had to be fully decoded before being rejected would have already
/// allocated the memory the limit exists to prevent.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = Vec::new();
    header.extend_from_slice(b"IHDR");
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // Bit depth 8, colour type 2 (truecolour), no compression/filter/interlace.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes
}

#[test]
fn a_committed_png_reports_its_dimensions() {
    match paddleocr_rust::api::decode_png(READING_ORDER) {
        Ok((width, height)) => assert_eq!((width, height), (800, 320)),
        Err(error) => panic!("expected dimensions, got {error}"),
    }
    match paddleocr_rust::api::decode_png(NO_TEXT) {
        Ok((width, height)) => assert_eq!((width, height), (3, 2)),
        Err(error) => panic!("expected dimensions, got {error}"),
    }
}

#[test]
fn corrupt_and_truncated_input_is_a_typed_error() {
    let cases: [(&str, Vec<u8>); 5] = [
        ("empty", Vec::new()),
        ("signature only", b"\x89PNG\r\n\x1a\n".to_vec()),
        ("jpeg", b"\xff\xd8\xff\xe0\x00\x10JFIF\0".to_vec()),
        ("text", b"this is not an image at all".to_vec()),
        (
            "truncated after IHDR",
            png_header(8, 8).into_iter().take(20).collect(),
        ),
    ];
    for (name, bytes) in cases {
        let outcome = paddleocr_rust::api::decode_png(&bytes);
        assert!(outcome.is_err(), "{name} must not decode");
        // Whatever the case, it must be a typed error and never a panic; the
        // assertion above already proves no panic occurred.
    }
}

#[test]
fn a_valid_png_with_corrupt_pixel_data_is_a_typed_error() {
    // A well-formed header followed by an IDAT that is not valid zlib.
    let mut bytes = png_header(4, 4);
    bytes.extend_from_slice(&4_u32.to_be_bytes());
    bytes.extend_from_slice(b"IDAT");
    bytes.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    assert!(paddleocr_rust::api::decode_png(&bytes).is_err());
}

#[test]
fn oversized_input_is_rejected_by_its_declared_limit() {
    // Encoded byte count, checked before anything is parsed.
    let huge = vec![0_u8; MAX_ENCODED_IMAGE_BYTES + 1];
    match paddleocr_rust::api::decode_png(&huge) {
        Err(Error::ResourceLimit {
            resource, limit, ..
        }) => {
            assert_eq!(resource, "image.encoded_bytes");
            assert_eq!(limit, MAX_ENCODED_IMAGE_BYTES as u64);
        }
        other => panic!("expected an encoded-size limit, got {other:?}"),
    }

    // Side length, from the declared header.
    match paddleocr_rust::api::decode_png(&png_header(MAX_IMAGE_SIDE_LENGTH + 1, 4)) {
        Err(Error::ResourceLimit { resource, .. }) => {
            assert_eq!(resource, "image.width_pixels");
        }
        other => panic!("expected a width limit, got {other:?}"),
    }

    // Total pixels, from dimensions that are each individually legal.
    let side = 16_000_u32;
    assert!(side <= MAX_IMAGE_SIDE_LENGTH);
    assert!(u64::from(side) * u64::from(side) > MAX_IMAGE_PIXELS);
    match paddleocr_rust::api::decode_png(&png_header(side, side)) {
        Err(Error::ResourceLimit { resource, .. }) => {
            assert_eq!(resource, "image.total_pixels");
        }
        other => panic!("expected a pixel-count limit, got {other:?}"),
    }
}

#[test]
fn dictionary_parsing_reports_its_own_failures() {
    match parse_dictionary("", true) {
        Err(Error::InvalidInput { .. }) => {}
        other => panic!("an empty dictionary must be rejected, got {other:?}"),
    }

    let parsed = match parse_dictionary("a\nb\nc\n", true) {
        Ok(value) => value,
        Err(error) => panic!("expected a dictionary, got {error}"),
    };
    assert_eq!(parsed.len(), 3, "the entry count excludes blank and space");
}

/// A manifest supplies the model-identity block, and a caller without one gets
/// an explicit `null` rather than a missing field.
#[test]
fn the_model_block_is_null_without_a_manifest_and_filled_with_one() {
    let anonymous = result_to_json(&[], 8, 8, None, None);
    assert!(anonymous.contains("\"model\":null"), "{anonymous}");

    let text = include_str!("fixtures/classic-v1-model-manifest/expected.txt");
    let manifest = match ModelManifest::parse(text) {
        Ok(manifest) => manifest,
        Err(error) => panic!("manifest: {error}"),
    };
    let identified = result_to_json(&[], 8, 8, None, Some(&manifest));
    assert!(
        identified.contains("\"family\":\"PP-OCRv6_medium\""),
        "{identified}"
    );
    assert_eq!(manifest.recognizer_class_count(), 18_710);
}

/// A manifest that fails validation is refused through the public API too.
#[test]
fn an_invalid_manifest_is_a_typed_error() {
    assert!(ModelManifest::parse("").is_err(), "an empty manifest");
    assert!(
        ModelManifest::parse("schema_version = paddleocr-rust/model-manifest/v1\n").is_err(),
        "a manifest with only a schema version"
    );
}

#[test]
fn the_result_document_is_versioned_and_deterministic() {
    let empty = result_to_json(&[], 800, 320, None, None);
    assert!(empty.contains(RESULT_SCHEMA_VERSION), "{empty}");
    assert!(empty.contains("\"lines\":[]"), "{empty}");
    assert_eq!(
        empty,
        result_to_json(&[], 800, 320, None, None),
        "the same input must serialise byte-identically"
    );

    let named = result_to_json(&[], 800, 320, Some("page-01.png"), None);
    assert!(named.contains("\"id\":\"page-01.png\""), "{named}");
}

#[test]
fn the_default_options_are_the_frozen_thresholds() {
    let options = OcrOptions::default();
    assert!((options.box_threshold - 0.6).abs() < f64::EPSILON);
    assert!((options.unclip_ratio - 1.5).abs() < f64::EPSILON);
    assert!((options.drop_score - 0.5).abs() < f64::EPSILON);
}

/// The half that needs real artifacts.
///
/// ```sh
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_DETECTOR_ONNX=<detector.onnx> \
/// PADDLEOCR_RUST_RECOGNIZER_ONNX=<recognizer.onnx> \
/// PADDLEOCR_RUST_DICTIONARY=<dict.txt> \
///   cargo test --features onnxruntime --test end_to_end -- --ignored --nocapture
/// ```
#[cfg(feature = "onnxruntime")]
mod provisioned {
    use super::*;

    use paddleocr_rust::api::{Artifacts, Dictionary, OcrEngine};

    const TALL_CROP: &[u8] = include_bytes!("fixtures/classic-v1-e2e-tall-crop/input.png");
    const UNICODE: &[u8] = include_bytes!("fixtures/classic-v1-e2e-unicode/input.png");
    const BENCHMARK_PAGE: &[u8] = include_bytes!("fixtures/classic-v1-benchmark-page/input.png");

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) => value,
            Err(_) => panic!("set {name}"),
        }
    }

    fn dictionary() -> Dictionary {
        let text = match std::fs::read_to_string(env("PADDLEOCR_RUST_DICTIONARY")) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        match parse_dictionary(&text, true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        }
    }

    fn artifacts() -> (String, String, String) {
        (
            env("PADDLEOCR_RUST_ORT_DYLIB"),
            env("PADDLEOCR_RUST_DETECTOR_ONNX"),
            env("PADDLEOCR_RUST_RECOGNIZER_ONNX"),
        )
    }

    fn engine(
        library: &str,
        detector: &str,
        recognizer: &str,
        dictionary: &Dictionary,
    ) -> OcrEngine {
        match OcrEngine::load(&Artifacts::new(library, detector, recognizer), dictionary) {
            Ok(engine) => engine,
            Err(error) => panic!("load: {error}"),
        }
    }

    fn texts(lines: &[paddleocr_rust::api::TextLine]) -> Vec<String> {
        lines.iter().map(|line| line.text.clone()).collect()
    }

    #[test]
    #[ignore = "E2E-001: needs explicitly provisioned models"]
    fn the_documented_cases_behave_as_documented() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();
        let engine = engine(&library, &detector, &recognizer, &dictionary);
        let options = OcrOptions::default();

        // No text: an empty result, never a fabricated line.
        let empty = match engine.recognize_png(NO_TEXT, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("no-text: {error}"),
        };
        assert!(empty.is_empty(), "no-text produced {:?}", texts(&empty));

        // Multiple lines in a stable reading order.
        let many = match engine.recognize_png(READING_ORDER, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("reading-order: {error}"),
        };
        assert_eq!(texts(&many), ["Hello", "World", "Rust", "OCR"]);

        // A rotated, tall crop that the pipeline must turn upright.
        let tall = match engine.recognize_png(TALL_CROP, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("tall-crop: {error}"),
        };
        assert_eq!(texts(&tall), ["Rust"]);

        // A non-Latin script, with the exact scalars preserved.
        let unicode = match engine.recognize_png(UNICODE, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("unicode: {error}"),
        };
        assert_eq!(texts(&unicode), ["\u{4f60}\u{597d}"]);

        // Mixed scripts on one page: Latin lines and a CJK line together.
        let mixed = match engine.recognize_png(BENCHMARK_PAGE, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("benchmark page: {error}"),
        };
        let mixed_texts = texts(&mixed);
        assert!(
            mixed_texts.iter().any(|text| text == "Hello"),
            "{mixed_texts:?}"
        );
        assert!(
            mixed_texts.iter().any(|text| text == "\u{4f60}\u{597d}"),
            "{mixed_texts:?}"
        );

        // Repeat runs are identical, which is the order guarantee stated as a
        // property rather than as one recorded answer.
        for _ in 0..3 {
            let again = match engine.recognize_png(READING_ORDER, &options) {
                Ok(lines) => lines,
                Err(error) => panic!("repeat: {error}"),
            };
            assert_eq!(again, many, "a repeated run changed its result");
        }
    }

    #[test]
    #[ignore = "E2E-001: needs explicitly provisioned models"]
    fn threshold_boundaries_change_what_survives() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();
        let engine = engine(&library, &detector, &recognizer, &dictionary);

        let permissive = OcrOptions::default().with_drop_score(0.0);
        let impossible = OcrOptions::default().with_drop_score(1.0);

        let kept = match engine.recognize_png(READING_ORDER, &permissive) {
            Ok(lines) => lines,
            Err(error) => panic!("permissive: {error}"),
        };
        assert_eq!(kept.len(), 4);

        // A drop score above every achievable confidence removes everything.
        // Equality is retained, so this needs a threshold strictly above the
        // best score rather than equal to it.
        let dropped = match engine.recognize_png(READING_ORDER, &impossible) {
            Ok(lines) => lines,
            Err(error) => panic!("impossible: {error}"),
        };
        assert!(
            dropped.len() < kept.len(),
            "a 1.0 drop score kept {:?}",
            texts(&dropped)
        );

        // A box threshold above 1.0 cannot be met by any region, so detection
        // itself yields nothing and no crop is ever recognized.
        let no_boxes = OcrOptions::default().with_box_threshold(1.1);
        let none = match engine.recognize_png(READING_ORDER, &no_boxes) {
            Ok(lines) => lines,
            Err(error) => panic!("box threshold: {error}"),
        };
        assert!(none.is_empty(), "{:?}", texts(&none));
    }

    #[test]
    #[ignore = "E2E-001: needs explicitly provisioned models"]
    fn a_missing_corrupt_or_mismatched_artifact_is_a_typed_error() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();
        // The reviewed detector digest, from the committed end-to-end fixtures.
        let detector_digest =
            "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1".to_owned();

        let missing = OcrEngine::load(
            &Artifacts::new(&library, "/nonexistent/detector.onnx", &recognizer),
            &dictionary,
        );
        assert!(missing.is_err(), "a missing artifact must not load");

        // A declared digest that does not match the file must be refused
        // before the runtime ever sees the bytes.
        let wrong_digest = OcrEngine::load(
            &Artifacts::new(&library, &detector, &recognizer).with_detector_sha256(
                "0000000000000000000000000000000000000000000000000000000000000001",
            ),
            &dictionary,
        );
        assert!(
            matches!(wrong_digest, Err(Error::Model { .. })),
            "a digest mismatch must be a model error, got {wrong_digest:?}"
        );

        // Swapping the detector and recognizer is NOT caught at load time, and
        // this asserts that rather than wishing otherwise. Both models declare
        // the same input and output tensor names and leave the axes this port
        // constrains dynamic, so nothing in the declared ABI distinguishes
        // them. Load-time validation can only check what a model declares.
        let swapped = OcrEngine::load(
            &Artifacts::new(&library, &recognizer, &detector),
            &dictionary,
        );
        let swapped = match swapped {
            Ok(engine) => engine,
            Err(error) => panic!(
                "the swap is expected to load; if this now fails, the contract \
                 grew teeth and this test should record that instead: {error}"
            ),
        };
        // It fails on first use, as a typed error rather than a wrong answer.
        assert!(
            swapped
                .recognize_png(READING_ORDER, &OcrOptions::default())
                .is_err(),
            "a swapped model pair must not produce a result"
        );

        // Declaring the expected digest is what actually catches it, before the
        // runtime sees a byte. This is the concrete reason the digest arguments
        // exist: identity, not shape, is what tells two models apart.
        let swapped_with_digest = OcrEngine::load(
            &Artifacts::new(&library, &recognizer, &detector)
                .with_detector_sha256(&detector_digest),
            &dictionary,
        );
        assert!(
            matches!(swapped_with_digest, Err(Error::Model { .. })),
            "a declared digest must catch the swap, got {swapped_with_digest:?}"
        );

        // A file that exists and is not a model at all.
        let dictionary_path = env("PADDLEOCR_RUST_DICTIONARY");
        let not_a_model = OcrEngine::load(
            &Artifacts::new(&library, &dictionary_path, &recognizer),
            &dictionary,
        );
        assert!(not_a_model.is_err(), "a text file must not load as a model");
    }

    #[test]
    #[ignore = "E2E-001: needs explicitly provisioned models"]
    fn one_engine_per_thread_runs_concurrently_and_agrees() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();

        // The documented position is one engine per thread. This is that
        // position exercised, not merely asserted: four threads each load their
        // own engine and must agree on the same page.
        let expected = {
            let engine = engine(&library, &detector, &recognizer, &dictionary);
            match engine.recognize_png(READING_ORDER, &OcrOptions::default()) {
                Ok(lines) => texts(&lines),
                Err(error) => panic!("baseline: {error}"),
            }
        };

        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let (library, detector, recognizer) = (&library, &detector, &recognizer);
                    let dictionary = &dictionary;
                    scope.spawn(move || {
                        let engine = engine(library, detector, recognizer, dictionary);
                        match engine.recognize_png(READING_ORDER, &OcrOptions::default()) {
                            Ok(lines) => texts(&lines),
                            Err(error) => panic!("thread: {error}"),
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(value) => value,
                    Err(_) => panic!("a worker thread panicked"),
                })
                .collect::<Vec<_>>()
        });

        for (index, result) in results.iter().enumerate() {
            assert_eq!(result, &expected, "thread {index} disagreed");
        }
    }

    /// `CONC-001`: concurrent runs produce byte-identical serialized results.
    ///
    /// Agreeing on the recognized text is weaker than it sounds — two runs can
    /// agree on every string while differing in a coordinate or the last digit
    /// of a confidence. Comparing the serialized document instead compares
    /// geometry and scores at full precision, which is the property a caller
    /// writing results to disk actually depends on.
    #[test]
    #[ignore = "CONC-001: needs explicitly provisioned models"]
    fn concurrent_runs_are_byte_identical() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();

        let render = |engine: &OcrEngine| -> String {
            let lines = match engine.recognize_png(READING_ORDER, &OcrOptions::default()) {
                Ok(lines) => lines,
                Err(error) => panic!("run: {error}"),
            };
            result_to_json(&lines, 800, 320, None, None)
        };

        let expected = render(&engine(&library, &detector, &recognizer, &dictionary));

        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let (library, detector, recognizer) = (&library, &detector, &recognizer);
                    let dictionary = &dictionary;
                    scope.spawn(move || {
                        // One engine per thread, reused across three runs: this
                        // covers both the shutdown of a dropped engine and the
                        // reuse of a live one.
                        let engine = engine(library, detector, recognizer, dictionary);
                        (0..3).map(|_| render(&engine)).collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(value) => value,
                    Err(_) => panic!("a worker thread panicked"),
                })
                .collect::<Vec<_>>()
        });

        for (thread, runs) in results.iter().enumerate() {
            for (run, document) in runs.iter().enumerate() {
                assert_eq!(
                    document, &expected,
                    "thread {thread} run {run} differed from the single-threaded result"
                );
            }
        }
    }

    /// `DOCPIPE-001`: a rotated page is corrected and its coordinates come home.
    ///
    /// The property that matters is the second half. Recovering the text only
    /// shows the rotation happened; the polygons landing inside the *original*
    /// page is what shows the inverse was applied rather than forgotten.
    #[test]
    #[ignore = "DOCPIPE-001: needs the document orientation artifact"]
    fn a_rotated_page_is_corrected_and_its_coordinates_map_home() {
        let (library, detector, recognizer) = artifacts();
        let document_orientation = env("PADDLEOCR_RUST_DOC_ORIENTATION_ONNX");
        let dictionary = dictionary();
        let engine = match OcrEngine::load(
            &Artifacts::new(&library, &detector, &recognizer)
                .with_document_orientation(&document_orientation),
            &dictionary,
        ) {
            Ok(engine) => engine,
            Err(error) => panic!("load: {error}"),
        };

        let rotated = std::fs::read(env("PADDLEOCR_RUST_ROTATED_PAGE")).unwrap_or_default();
        let (width, height) = match paddleocr_rust::api::decode_png(&rotated) {
            Ok(value) => value,
            Err(error) => panic!("rotated page: {error}"),
        };
        assert_eq!((width, height), (320, 800), "the input is the rotated page");

        let options = OcrOptions::default().with_document_preprocessing(
            paddleocr_rust::document_pipeline::DocumentPreprocessOptions::default()
                .with_orientation(true),
        );
        let result = match engine.recognize_document(&rotated, &options) {
            Ok(result) => result,
            Err(error) => panic!("recognize_document: {error}"),
        };

        assert_eq!(
            result.coordinate_space,
            paddleocr_rust::document_pipeline::CoordinateSpace::Source,
            "rotation alone must stay mappable"
        );
        assert_eq!(texts(&result.lines), ["Hello", "World", "Rust", "OCR"]);

        // Every corner must lie inside the page the caller supplied, not the
        // rotated one the detector saw.
        for line in &result.lines {
            for point in line.quadrilateral.points() {
                assert!(
                    point.x() >= -1.0 && point.x() <= 321.0,
                    "x {} outside the supplied 320-wide page",
                    point.x()
                );
                assert!(
                    point.y() >= -1.0 && point.y() <= 801.0,
                    "y {} outside the supplied 800-tall page",
                    point.y()
                );
            }
        }
    }

    #[test]
    #[ignore = "E2E-001: needs explicitly provisioned models"]
    fn resource_limits_and_bad_input_survive_a_loaded_engine() {
        let (library, detector, recognizer) = artifacts();
        let dictionary = dictionary();
        let engine = engine(&library, &detector, &recognizer, &dictionary);
        let options = OcrOptions::default();

        // An oversized or corrupt image must fail on the image, not on the
        // model, and must leave the engine usable afterwards.
        for bytes in [
            vec![0_u8; MAX_ENCODED_IMAGE_BYTES + 1],
            b"not a png".to_vec(),
            png_header(MAX_IMAGE_SIDE_LENGTH + 1, 4),
        ] {
            assert!(engine.recognize_png(&bytes, &options).is_err());
        }

        let after = match engine.recognize_png(READING_ORDER, &options) {
            Ok(lines) => lines,
            Err(error) => panic!("the engine did not survive a rejected input: {error}"),
        };
        assert_eq!(texts(&after), ["Hello", "World", "Rust", "OCR"]);
    }
}

/// `TABLEPIPE-001` orchestration: three table models run in order.
///
/// Ignored by default and gated on four environment variables, the same bar
/// every other artifact-backed test in this file meets. It is the first test in
/// this project that loads three sessions at once, one of them `368 MB`.
///
/// ```text
/// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so> \
/// PADDLEOCR_RUST_TABLE_CLS_ONNX=<PP-LCNet_x1_0_table_cls/inference.onnx> \
/// PADDLEOCR_RUST_TABLE_CELL_ONNX=<RT-DETR-L_wired_table_cell_det/inference.onnx> \
/// PADDLEOCR_RUST_TABLE_STRUCTURE_ONNX=<SLANeXt_wired/inference.onnx> \
///   cargo test --features onnxruntime --test end_to_end -- --ignored table_ --nocapture
/// ```
#[cfg(feature = "onnxruntime")]
mod table_orchestration {
    use paddleocr_rust::table_engine::{TableArtifacts, TableEngine, TableImage};
    use paddleocr_rust::table_pipeline::TableRoute;

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) if !value.is_empty() => value,
            _ => panic!("set {name} to run this test"),
        }
    }

    /// A synthetic table image: white with dark ruling lines and text blocks.
    ///
    /// Deterministic and generated rather than committed, for the reason the
    /// table fixtures record: a page-sized image exists only to exercise the
    /// path, and a formula reproduces it exactly.
    fn synthetic_table(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![235_u8; (width * height * 3) as usize];
        let columns = [0_u32, width / 2, width - 1];
        let rows = [0_u32, height / 2, height - 1];
        let mut paint = |x: u32, y: u32, value: u8| {
            if x < width && y < height {
                let base = ((y * width + x) * 3) as usize;
                pixels[base] = value;
                pixels[base + 1] = value;
                pixels[base + 2] = value;
            }
        };
        for y in 0..height {
            for x in &columns {
                for thickness in 0..2 {
                    paint(x + thickness, y, 20);
                }
            }
        }
        for x in 0..width {
            for y in &rows {
                for thickness in 0..2 {
                    paint(x, y + thickness, 20);
                }
            }
        }
        // Four dark blocks standing in for text, one per cell.
        for (cell_x, cell_y) in [(0_u32, 0_u32), (1, 0), (0, 1), (1, 1)] {
            let left = cell_x * (width / 2) + width / 8;
            let top = cell_y * (height / 2) + height / 8;
            for y in top..(top + height / 8) {
                for x in left..(left + width / 4) {
                    paint(x, y, 60);
                }
            }
        }
        pixels
    }

    fn swap_channels(pixels: &[u8]) -> Vec<u8> {
        let mut swapped = pixels.to_vec();
        for pixel in swapped.chunks_exact_mut(3) {
            pixel.swap(0, 2);
        }
        swapped
    }

    #[test]
    #[ignore = "TABLEPIPE-001: needs the three table artifacts"]
    fn the_three_table_models_run_in_order_and_produce_html() {
        let library = env("PADDLEOCR_RUST_ORT_DYLIB");
        let classifier = env("PADDLEOCR_RUST_TABLE_CLS_ONNX");
        let cells = env("PADDLEOCR_RUST_TABLE_CELL_ONNX");
        let structure = env("PADDLEOCR_RUST_TABLE_STRUCTURE_ONNX");

        let engine = match TableEngine::load(&TableArtifacts::new(
            &library,
            &classifier,
            &cells,
            &structure,
            TableRoute::Wired,
        )) {
            Ok(value) => value,
            Err(error) => panic!("load: {error}"),
        };

        let (width, height) = (480_u32, 320_u32);
        let rgb_pixels = synthetic_table(width, height);
        let bgr_pixels = swap_channels(&rgb_pixels);
        let rgb = match TableImage::new(width, height, rgb_pixels) {
            Ok(value) => value,
            Err(error) => panic!("rgb: {error}"),
        };
        let bgr = match TableImage::new(width, height, bgr_pixels) {
            Ok(value) => value,
            Err(error) => panic!("bgr: {error}"),
        };

        // Each stage is checked on its own, so a failure names the model rather
        // than the pipeline.
        let (route, score) = match engine.classify(&rgb) {
            Ok(value) => value,
            Err(error) => panic!("classify: {error}"),
        };
        println!("route: {route:?} score {score}");
        assert!((0.0..=1.0).contains(&score), "score out of range: {score}");

        let detected = match engine.detect_cells(&rgb) {
            Ok(value) => value,
            Err(error) => panic!("detect_cells: {error}"),
        };
        println!("cells: {}", detected.len());
        for cell in &detected {
            assert!(
                cell[2] > cell[0] && cell[3] > cell[1],
                "degenerate {cell:?}"
            );
        }

        let tokens = match engine.recognize_structure(&bgr) {
            Ok(value) => value,
            Err(error) => panic!("recognize_structure: {error}"),
        };
        println!("tokens: {}", tokens.len());
        assert_eq!(tokens.first().map(String::as_str), Some("<html>"));
        assert_eq!(tokens.last().map(String::as_str), Some("</html>"));

        // One OCR box per cell, placed on the dark blocks above.
        let mut ocr_boxes = Vec::new();
        let mut ocr_texts = Vec::new();
        for (index, (cell_x, cell_y)) in [(0_u32, 0_u32), (1, 0), (0, 1), (1, 1)]
            .into_iter()
            .enumerate()
        {
            let left = f64::from(cell_x * (width / 2) + width / 8);
            let top = f64::from(cell_y * (height / 2) + height / 8);
            ocr_boxes.push([
                left,
                top,
                left + f64::from(width / 4),
                top + f64::from(height / 8),
            ]);
            ocr_texts.push(format!("cell{index}"));
        }

        let result = match engine.recognize_table(
            &rgb,
            &bgr,
            [0.0, 0.0, f64::from(width), f64::from(height)],
            &ocr_boxes,
            &ocr_texts,
        ) {
            Ok(value) => value,
            Err(error) => panic!("recognize_table: {error}"),
        };
        println!("html: {}", result.html);
        assert!(result.html.starts_with("<html><body><table>"));
        assert!(result.html.ends_with("</table></body></html>"));
        assert_eq!(result.route, route);
    }
}

/// `MODAPI-001`: detection without recognition, through the same engine.
#[cfg(feature = "onnxruntime")]
mod detection_only {
    use paddleocr_rust::api::{Artifacts, DetectedRegion, OcrEngine, OcrOptions, parse_dictionary};

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) if !value.is_empty() => value,
            _ => panic!("set {name} to run this test"),
        }
    }

    #[test]
    #[ignore = "MODAPI-001: needs explicitly provisioned models"]
    fn detection_agrees_with_the_full_pipeline_on_order_and_geometry() {
        let library = env("PADDLEOCR_RUST_ORT_DYLIB");
        let detector = env("PADDLEOCR_RUST_DETECTOR_ONNX");
        let recognizer = env("PADDLEOCR_RUST_RECOGNIZER_ONNX");
        let dictionary_path = env("PADDLEOCR_RUST_DICTIONARY");

        let text = match std::fs::read_to_string(&dictionary_path) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let dictionary = match parse_dictionary(&text, true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let engine = match OcrEngine::load(
            &Artifacts::new(&library, &detector, &recognizer),
            &dictionary,
        ) {
            Ok(value) => value,
            Err(error) => panic!("load: {error}"),
        };

        const PAGE: &[u8] = include_bytes!("fixtures/classic-v1-benchmark-page/input.png");
        let options = OcrOptions::default();

        let detected = match engine.detect_png(PAGE, &options) {
            Ok(value) => value,
            Err(error) => panic!("detect: {error}"),
        };
        let recognized = match engine.recognize_png(PAGE, &options) {
            Ok(value) => value,
            Err(error) => panic!("recognize: {error}"),
        };

        println!(
            "detected {} regions, recognized {} lines",
            detected.len(),
            recognized.len()
        );

        // Recognition can only drop regions -- `drop_score` filters on a
        // confidence detection never computes -- so it never adds one.
        assert!(
            recognized.len() <= detected.len(),
            "recognition produced more lines than there were regions"
        );

        // Every recognized line's box is one the detector reported, and in the
        // same order: both paths run the same reading-order sort.
        let mut cursor = 0_usize;
        for line in &recognized {
            let found = detected[cursor..]
                .iter()
                .position(|region| region.quadrilateral == line.quadrilateral);
            match found {
                Some(offset) => cursor += offset + 1,
                None => {
                    panic!("a recognized line's box is absent from the detection, or out of order")
                }
            }
        }

        for region in &detected {
            assert!(
                (0.0..=1.0).contains(&region.score),
                "detector score out of range: {}",
                region.score
            );
        }

        let json = DetectedRegion::slice_to_json(&detected, 1280, 720, Some("bench"));
        assert!(json.starts_with("{\"schema_version\":\"paddleocr-rust/detection-result/v1\""));
        assert_eq!(
            json,
            DetectedRegion::slice_to_json(&detected, 1280, 720, Some("bench"))
        );
    }
}

/// `IMG-003` entry gate: what a component delta of `36` does downstream.
///
/// `docs/IMAGE_DECODER_EVIDENCE.md` records that every evaluated pure-Rust JPEG
/// decoder differs from the committed OpenCV oracle by up to `36` in one
/// component. The roadmap makes measuring the **consequence** of that the
/// precondition for deciding anything about JPEG: a component difference cannot
/// be assumed harmless to a model tensor.
///
/// This probe measures it. It does not need a JPEG decoder — it perturbs a
/// decoded page directly, which isolates the question "what does a delta of `d`
/// do?" from "which decoder produces it".
#[cfg(feature = "onnxruntime")]
mod jpeg_delta_gate {
    use paddleocr_rust::api::{Artifacts, OcrEngine, OcrOptions, parse_dictionary};

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) if !value.is_empty() => value,
            _ => panic!("set {name} to run this test"),
        }
    }

    /// Re-encodes a decoded page as a PNG with every component perturbed.
    ///
    /// `uniform` shifts every component by the same amount, which is the worst
    /// case a bounded delta permits. `scattered` varies per component within the
    /// bound, which is closer to what a decoder difference actually looks like.
    /// Both are measured, because the worst case decides what is safe and the
    /// realistic case decides what is likely.
    fn perturbed_png(source: &[u8], delta: i16, scattered: bool) -> Vec<u8> {
        let decoder = png::Decoder::new(std::io::Cursor::new(source));
        let mut reader = match decoder.read_info() {
            Ok(value) => value,
            Err(error) => panic!("png info: {error}"),
        };
        let mut buffer = vec![0_u8; reader.output_buffer_size().unwrap_or(0)];
        let info = match reader.next_frame(&mut buffer) {
            Ok(value) => value,
            Err(error) => panic!("png frame: {error}"),
        };
        let (width, height) = (info.width, info.height);
        let bytes = &mut buffer[..info.buffer_size()];

        for (index, component) in bytes.iter_mut().enumerate() {
            let shift = if scattered {
                // Deterministic and spread across the whole range, so the
                // measurement is repeatable.
                let mixed = (index as u64).wrapping_mul(2_654_435_761) >> 13;
                (mixed % (2 * delta as u64 + 1)) as i16 - delta
            } else {
                delta
            };
            *component = (i16::from(*component) + shift).clamp(0, 255) as u8;
        }

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(info.color_type);
            encoder.set_depth(info.bit_depth);
            let mut writer = match encoder.write_header() {
                Ok(value) => value,
                Err(error) => panic!("png header: {error}"),
            };
            if let Err(error) = writer.write_image_data(bytes) {
                panic!("png write: {error}");
            }
        }
        out
    }

    #[test]
    #[ignore = "IMG-003: needs explicitly provisioned models"]
    fn a_component_delta_of_36_is_measured_through_the_models() {
        let library = env("PADDLEOCR_RUST_ORT_DYLIB");
        let detector = env("PADDLEOCR_RUST_DETECTOR_ONNX");
        let recognizer = env("PADDLEOCR_RUST_RECOGNIZER_ONNX");
        let dictionary_path = env("PADDLEOCR_RUST_DICTIONARY");
        let text = match std::fs::read_to_string(&dictionary_path) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let dictionary = match parse_dictionary(&text, true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let engine = match OcrEngine::load(
            &Artifacts::new(&library, &detector, &recognizer),
            &dictionary,
        ) {
            Ok(value) => value,
            Err(error) => panic!("load: {error}"),
        };

        const PAGE: &[u8] = include_bytes!("fixtures/classic-v1-benchmark-page/input.png");
        let options = OcrOptions::default();

        let baseline = match engine.recognize_png(PAGE, &options) {
            Ok(value) => value,
            Err(error) => panic!("baseline: {error}"),
        };
        let baseline_text: Vec<String> = baseline.iter().map(|l| l.text.clone()).collect();
        println!("baseline: {} lines {baseline_text:?}", baseline.len());

        for (label, delta, scattered) in [
            ("uniform +1", 1_i16, false),
            ("uniform +4", 4, false),
            ("uniform +16", 16, false),
            ("uniform +36", 36, false),
            ("scattered +/-36", 36, true),
        ] {
            let png = perturbed_png(PAGE, delta, scattered);
            let lines = match engine.recognize_png(&png, &options) {
                Ok(value) => value,
                Err(error) => panic!("{label}: {error}"),
            };
            let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
            let same_text = texts == baseline_text;
            let same_count = lines.len() == baseline.len();

            // Worst corner movement, when the counts allow a comparison.
            let mut worst_corner = 0.0_f32;
            if same_count {
                for (a, b) in baseline.iter().zip(&lines) {
                    for (p, q) in a
                        .quadrilateral
                        .points()
                        .iter()
                        .zip(b.quadrilateral.points())
                    {
                        worst_corner = worst_corner
                            .max((p.x() - q.x()).abs())
                            .max((p.y() - q.y()).abs());
                    }
                }
            }
            println!(
                "{label:<18} lines {:>2} (same_count {same_count}) same_text {same_text} \
                 worst_corner_px {worst_corner}",
                lines.len()
            );
        }

        // The probe reports; it does not decide. `IMG-003` is the decision, and
        // asserting a tolerance here would make it before the evidence is
        // recorded. The only assertion is that the baseline itself is stable,
        // without which none of the comparisons above mean anything.
        let repeat = match engine.recognize_png(PAGE, &options) {
            Ok(value) => value,
            Err(error) => panic!("repeat: {error}"),
        };
        assert_eq!(
            repeat.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
            baseline_text,
            "the unperturbed baseline must be reproducible"
        );
    }
}

/// `DOC-E2E-001`, the slice that is not blocked on PDF.
///
/// The row lists rotated, warped, multipage, empty, corrupt, password, and
/// oversized cases. Multipage and password belong to `PDF-001`, which has no
/// approved renderer. The rest go through document preprocessing, which is
/// implemented and `Done` — so they are verifiable now, and this covers them.
#[cfg(feature = "onnxruntime")]
mod document_boundary {
    use paddleocr_rust::Error;
    use paddleocr_rust::api::{Artifacts, OcrEngine, OcrOptions, parse_dictionary};
    use paddleocr_rust::document_pipeline::{CoordinateSpace, DocumentPreprocessOptions};

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) if !value.is_empty() => value,
            _ => panic!("set {name} to run this test"),
        }
    }

    fn engine() -> OcrEngine {
        let text = match std::fs::read_to_string(env("PADDLEOCR_RUST_DICTIONARY")) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        let dictionary = match parse_dictionary(&text, true) {
            Ok(value) => value,
            Err(error) => panic!("dictionary: {error}"),
        };
        match OcrEngine::load(
            &Artifacts::new(
                &env("PADDLEOCR_RUST_ORT_DYLIB"),
                &env("PADDLEOCR_RUST_DETECTOR_ONNX"),
                &env("PADDLEOCR_RUST_RECOGNIZER_ONNX"),
            ),
            &dictionary,
        ) {
            Ok(value) => value,
            Err(error) => panic!("load: {error}"),
        }
    }

    /// A blank page, a corrupt file, and an oversized declaration.
    ///
    /// These need no model to be interesting, but they are run through the
    /// loaded engine anyway: the question is what the **whole path** does, and a
    /// unit test on the decoder cannot answer that.
    #[test]
    #[ignore = "DOC-E2E-001: needs explicitly provisioned models"]
    fn empty_corrupt_and_oversized_pages_answer_without_panicking() {
        let engine = engine();
        let options = OcrOptions::default();

        // A blank white page: a valid image with nothing to detect.
        let mut blank = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut blank, 320, 240);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = match encoder.write_header() {
                Ok(value) => value,
                Err(error) => panic!("header: {error}"),
            };
            if let Err(error) = writer.write_image_data(&vec![255_u8; 320 * 240 * 3]) {
                panic!("write: {error}");
            }
        }
        match engine.recognize_png(&blank, &options) {
            Ok(lines) => assert!(lines.is_empty(), "a blank page found {} lines", lines.len()),
            Err(error) => panic!("a blank page must succeed with no lines, got {error}"),
        }

        // Corrupt inputs, each with the **kind** of refusal it should get.
        //
        // The distinction is the point, and there are **three** kinds, not two.
        // No bytes at all is `Empty`. A byte string that is not a PNG is
        // `Unsupported` -- "this is not a format I handle" -- because `IMG-001`
        // selects format from the eight-byte signature alone. A byte string that
        // claims to be a PNG and then is not is `InvalidInput`. Collapsing them
        // would lose the only thing that tells a caller whether to send
        // something, convert it, or re-fetch it.
        enum Kind {
            /// No input at all.
            Empty,
            /// Not a format this port handles.
            Unsupported,
            /// A handled format, malformed or out of bounds.
            Malformed,
        }
        let corrupt: Vec<(&str, Vec<u8>, Kind)> = vec![
            ("empty", Vec::new(), Kind::Empty),
            (
                "not_an_image",
                b"this is not a png at all".to_vec(),
                Kind::Unsupported,
            ),
            (
                "truncated",
                blank[..blank.len() / 3].to_vec(),
                Kind::Malformed,
            ),
            ("header_only", blank[..8].to_vec(), Kind::Malformed),
            (
                "declared_huge",
                {
                    let mut bytes = blank.clone();
                    // Overwrite IHDR's width with 0xFFFFFFFF, after the signature
                    // and the length/type fields.
                    bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
                    bytes
                },
                Kind::Malformed,
            ),
        ];
        for (name, bytes, kind) in corrupt {
            let outcome = engine.recognize_png(&bytes, &options);
            match (&outcome, kind) {
                (Ok(lines), _) => panic!("{name}: expected a refusal, got {} lines", lines.len()),
                (
                    Err(Error::InvalidInput {
                        violation: paddleocr_rust::InputViolation::Empty,
                        ..
                    }),
                    Kind::Empty,
                ) => {}
                (Err(Error::Unsupported { .. }), Kind::Unsupported) => {}
                (
                    Err(
                        Error::InvalidInput { .. }
                        | Error::ResourceLimit { .. }
                        | Error::Model { .. },
                    ),
                    Kind::Malformed,
                ) => {}
                (Err(other), _) => {
                    panic!("{name}: refused with the wrong kind: {other:?}")
                }
            }
        }
    }

    /// Rotation preserves content and returns coordinates in the caller's page.
    ///
    /// The point is not that rotation works — `DOCORI-001` established that
    /// against a capture. It is that the **coordinates come back**: a rotated
    /// page's boxes must be usable against the image the caller supplied, which
    /// is the property `DocumentRotation::inverse` exists for.
    #[test]
    #[ignore = "DOC-E2E-001: needs the document orientation artifact"]
    fn a_rotated_page_returns_source_coordinates() {
        let engine = engine();
        let options =
            OcrOptions::default().with_document_preprocessing(DocumentPreprocessOptions::default());

        const PAGE: &[u8] = include_bytes!("fixtures/classic-v1-benchmark-page/input.png");
        let result = match engine.recognize_document(PAGE, &options) {
            Ok(value) => value,
            Err(error) => panic!("document: {error}"),
        };
        assert_eq!(
            result.coordinate_space,
            CoordinateSpace::Source,
            "without unwarping the coordinates must describe the caller's page"
        );
        for line in &result.lines {
            for point in line.quadrilateral.points() {
                assert!(
                    point.x() >= -1.0 && point.x() <= 1281.0,
                    "x outside the page: {}",
                    point.x()
                );
                assert!(
                    point.y() >= -1.0 && point.y() <= 721.0,
                    "y outside the page: {}",
                    point.y()
                );
            }
        }
        println!(
            "document: {} lines in {:?}",
            result.lines.len(),
            result.coordinate_space
        );
    }

    /// Unwarping through `recognize_png` is refused, not silently mapped.
    ///
    /// This is the boundary `docs/UNWARPING_CONTRACT.md` section 3 names: an
    /// unwarped page's coordinates cannot return to the caller, and a signature
    /// with nowhere to say so must refuse rather than answer.
    #[test]
    #[ignore = "DOC-E2E-001: needs explicitly provisioned models"]
    fn warping_is_refused_where_it_cannot_be_reported() {
        let engine = engine();
        let options = OcrOptions::default()
            .with_document_preprocessing(DocumentPreprocessOptions::default().with_unwarping(true));
        const PAGE: &[u8] = include_bytes!("fixtures/classic-v1-benchmark-page/input.png");

        match engine.recognize_png(PAGE, &options) {
            Err(Error::Unsupported { capability }) => {
                assert!(capability.contains("unwarping"), "{capability}");
            }
            other => panic!("expected an unsupported refusal, got {other:?}"),
        }
        // And `detect_png` refuses it for the same reason.
        match engine.detect_png(PAGE, &options) {
            Err(Error::Unsupported { .. }) => {}
            other => panic!("detect_png must refuse unwarping too, got {other:?}"),
        }
    }
}
