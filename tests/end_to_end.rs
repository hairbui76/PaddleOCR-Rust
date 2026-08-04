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
        match OcrEngine::load(
            &Artifacts {
                library,
                detector,
                detector_sha256: None,
                recognizer,
                recognizer_sha256: None,
            },
            dictionary,
        ) {
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

        let permissive = OcrOptions {
            drop_score: 0.0,
            ..OcrOptions::default()
        };
        let impossible = OcrOptions {
            drop_score: 1.0,
            ..OcrOptions::default()
        };

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
        let no_boxes = OcrOptions {
            box_threshold: 1.1,
            ..OcrOptions::default()
        };
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
            &Artifacts {
                library: &library,
                detector: "/nonexistent/detector.onnx",
                detector_sha256: None,
                recognizer: &recognizer,
                recognizer_sha256: None,
            },
            &dictionary,
        );
        assert!(missing.is_err(), "a missing artifact must not load");

        // A declared digest that does not match the file must be refused
        // before the runtime ever sees the bytes.
        let wrong_digest = OcrEngine::load(
            &Artifacts {
                library: &library,
                detector: &detector,
                detector_sha256: Some(
                    "0000000000000000000000000000000000000000000000000000000000000001",
                ),
                recognizer: &recognizer,
                recognizer_sha256: None,
            },
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
            &Artifacts {
                library: &library,
                detector: &recognizer,
                detector_sha256: None,
                recognizer: &detector,
                recognizer_sha256: None,
            },
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
            &Artifacts {
                library: &library,
                detector: &recognizer,
                detector_sha256: Some(&detector_digest),
                recognizer: &detector,
                recognizer_sha256: None,
            },
            &dictionary,
        );
        assert!(
            matches!(swapped_with_digest, Err(Error::Model { .. })),
            "a declared digest must catch the swap, got {swapped_with_digest:?}"
        );

        // A file that exists and is not a model at all.
        let not_a_model = OcrEngine::load(
            &Artifacts {
                library: &library,
                detector: &env("PADDLEOCR_RUST_DICTIONARY"),
                detector_sha256: None,
                recognizer: &recognizer,
                recognizer_sha256: None,
            },
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
