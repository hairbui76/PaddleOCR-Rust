#![forbid(unsafe_code)]
// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Command-line entrypoint for the classic OCR path.
//!
//! Every artifact is supplied explicitly: nothing is downloaded, cached, or
//! read from an environment variable. Without the `onnxruntime` feature there
//! is no backend compiled in, so the binary reports that and exits rather than
//! pretending to work.

use std::process::ExitCode;

fn usage() -> &'static str {
    "usage: paddleocr-rust --ort-dylib <libonnxruntime.so> \\\n\
     \x20                 --detector <detector.onnx> --recognizer <recognizer.onnx> \\\n\
     \x20                 --dictionary <dict.txt> [--json] [--time-budget-ms <n>] \\\n\
     \x20                 [--manifest <manifest.txt>] \\\n\
     \x20                 [--detector-sha256 <hex>] [--recognizer-sha256 <hex>] <image.png>...\n\
     \n\
     All paths are explicit. Only PNG input is supported; see \n\
     docs/IMAGE_DECODER_DECISION.md for why JPEG is deferred.\n\
     \n\
     Several images may be given. The models are loaded once and reused, which \n\
     is the whole reason to pass them together rather than running this per \n\
     file. With more than one image the text output gains a leading path \n\
     column, as grep does, and --json emits one JSON document per line.\n"
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|value| value == "--help" || value == "-h")
    {
        print!("{}", usage());
        return ExitCode::SUCCESS;
    }

    #[cfg(not(feature = "onnxruntime"))]
    {
        let _ = arguments;
        eprintln!(
            "paddleocr-rust: this build has no inference backend compiled in.\n\
             Rebuild with `--features onnxruntime` to run the classic pipeline."
        );
        eprint!("\n{}", usage());
        ExitCode::from(2)
    }

    #[cfg(feature = "onnxruntime")]
    {
        match run(&arguments) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("paddleocr-rust: {message}");
                ExitCode::from(2)
            }
        }
    }
}

#[cfg(feature = "onnxruntime")]
fn run(arguments: &[String]) -> Result<ExitCode, String> {
    let mut library = None;
    let mut detector = None;
    let mut recognizer = None;
    let mut dictionary = None;
    let mut images: Vec<String> = Vec::new();
    let mut json = false;
    let mut time_budget_ms = None;
    let mut manifest_path = None;
    let mut detector_sha256 = None;
    let mut recognizer_sha256 = None;

    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        let take = |slot: &mut Option<String>| -> Result<(), String> {
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| format!("{argument} needs a value"))?;
            *slot = Some(value.clone());
            Ok(())
        };
        match argument {
            "--ort-dylib" => {
                take(&mut library)?;
                index += 2;
            }
            "--detector" => {
                take(&mut detector)?;
                index += 2;
            }
            "--recognizer" => {
                take(&mut recognizer)?;
                index += 2;
            }
            "--dictionary" => {
                take(&mut dictionary)?;
                index += 2;
            }
            "--detector-sha256" => {
                take(&mut detector_sha256)?;
                index += 2;
            }
            "--recognizer-sha256" => {
                take(&mut recognizer_sha256)?;
                index += 2;
            }
            "--time-budget-ms" => {
                take(&mut time_budget_ms)?;
                index += 2;
            }
            "--manifest" => {
                take(&mut manifest_path)?;
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}"));
            }
            other => {
                images.push(other.to_owned());
                index += 1;
            }
        }
    }

    let (library, detector, recognizer, dictionary) =
        match (library, detector, recognizer, dictionary) {
            (Some(a), Some(b), Some(c), Some(d)) if !images.is_empty() => (a, b, c, d),
            _ => return Err(format!("missing required arguments\n\n{}", usage())),
        };

    // The budget is checked at stage boundaries, so it bounds a run rather than
    // interrupting one; see the `control` module for what that guarantees. It
    // applies per image, not to the whole invocation: a caller passing fifty
    // pages means "no page may take longer than this", and a total budget would
    // silently abandon the tail of the list.
    let mut options = paddleocr_rust::api::OcrOptions::default();
    if let Some(value) = time_budget_ms {
        let milliseconds: u64 = value
            .parse()
            .map_err(|_| format!("--time-budget-ms needs a whole number, got {value:?}"))?;
        options.control = paddleocr_rust::control::RunControl::unbounded()
            .with_time_budget(std::time::Duration::from_millis(milliseconds));
    }

    // A manifest is provenance and identity, never a download instruction and
    // never a path resolver: the artifact paths above are still the caller's.
    // Its digests are applied as the expected ones when the caller did not give
    // them explicitly, so declaring a manifest is also declaring verification.
    let manifest = match &manifest_path {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read the manifest: {error}"))?;
            let parsed = paddleocr_rust::manifest::ModelManifest::parse(&text)
                .map_err(|error| format!("{path}: {error}"))?;
            eprintln!(
                "manifest: {} {} ({} via {})",
                parsed.family, parsed.version, parsed.format, parsed.backend
            );
            Some(parsed)
        }
        None => None,
    };
    if let Some(parsed) = &manifest {
        if detector_sha256.is_none() {
            detector_sha256 = Some(parsed.detector.sha256.clone());
        }
        if recognizer_sha256.is_none() {
            recognizer_sha256 = Some(parsed.recognizer.sha256.clone());
        }
    }

    let dictionary_text = std::fs::read_to_string(&dictionary)
        .map_err(|error| format!("cannot read the dictionary: {error}"))?;
    let parsed = paddleocr_rust::api::parse_dictionary(&dictionary_text, true)
        .map_err(|error| format!("{error}"))?;
    eprintln!("dictionary: {} entries", parsed.len());
    // A manifest that disagrees with the dictionary it was given describes a
    // different pairing than the one about to run, so it is refused rather than
    // recorded alongside a result it does not describe.
    if let Some(expected) = &manifest
        && expected.dictionary.entries != parsed.len()
    {
        return Err(format!(
            "the manifest declares {} dictionary entries but the file has {}",
            expected.dictionary.entries,
            parsed.len()
        ));
    }

    // Loading once is the point of the engine: session creation costs about
    // 1.4 s on the reference host, and paying it per image would make a
    // multi-page run several times slower than it needs to be.
    let engine = paddleocr_rust::api::OcrEngine::load(
        &{
            let mut artifacts =
                paddleocr_rust::api::Artifacts::new(&library, &detector, &recognizer);
            if let Some(digest) = detector_sha256.as_deref() {
                artifacts = artifacts.with_detector_sha256(digest);
            }
            if let Some(digest) = recognizer_sha256.as_deref() {
                artifacts = artifacts.with_recognizer_sha256(digest);
            }
            artifacts
        },
        &parsed,
    )
    .map_err(|error| format!("{error}"))?;

    let several = images.len() > 1;
    for image in &images {
        // Bounded during the read: `std::fs::read` would allocate a ten
        // gigabyte file in full and only then meet the 64 MiB limit, which
        // honours the limit's letter and defeats its purpose.
        let bytes = paddleocr_rust::input::read_encoded_file(image)
            .map_err(|error| format!("{image}: {error}"))?;
        let (width, height) =
            paddleocr_rust::api::decode_png(&bytes).map_err(|error| format!("{image}: {error}"))?;
        eprintln!("image: {image} ({width}x{height} PNG)");

        let lines = engine
            .recognize_png(&bytes, &options)
            .map_err(|error| format!("{image}: {error}"))?;

        if json {
            // One document per line: with several inputs that is JSONL, and
            // each document names its input so position is not the only thing
            // identifying it.
            let id = if several { Some(image.as_str()) } else { None };
            println!(
                "{}",
                paddleocr_rust::result_json::result_to_json(
                    &lines,
                    width,
                    height,
                    id,
                    manifest.as_ref(),
                )
            );
        } else {
            for line in &lines {
                if several {
                    println!("{image}\t{:.6}\t{}", line.score, line.text);
                } else {
                    println!("{:.6}\t{}", line.score, line.text);
                }
            }
            if lines.is_empty() {
                eprintln!("{image}: no text detected");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
