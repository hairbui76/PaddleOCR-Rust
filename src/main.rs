#![forbid(unsafe_code)]

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
     \x20                 --dictionary <dict.txt> [--json] \\\n\
     \x20                 [--detector-sha256 <hex>] [--recognizer-sha256 <hex>] <image.png>\n\
     \n\
     All paths are explicit. Only PNG input is supported; see \n\
     docs/IMAGE_DECODER_DECISION.md for why JPEG is deferred.\n"
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
    let mut image = None;
    let mut json = false;
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
            "--json" => {
                json = true;
                index += 1;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}"));
            }
            other => {
                if image.is_some() {
                    return Err("only one image may be given".to_owned());
                }
                image = Some(other.to_owned());
                index += 1;
            }
        }
    }

    let (library, detector, recognizer, dictionary, image) =
        match (library, detector, recognizer, dictionary, image) {
            (Some(a), Some(b), Some(c), Some(d), Some(e)) => (a, b, c, d, e),
            _ => return Err(format!("missing required arguments\n\n{}", usage())),
        };

    let bytes = std::fs::read(&image).map_err(|error| format!("cannot read the image: {error}"))?;
    let (width, height) =
        paddleocr_rust::api::decode_png(&bytes).map_err(|error| format!("{error}"))?;

    let dictionary_text = std::fs::read_to_string(&dictionary)
        .map_err(|error| format!("cannot read the dictionary: {error}"))?;
    let parsed = paddleocr_rust::api::parse_dictionary(&dictionary_text, true)
        .map_err(|error| format!("{error}"))?;

    eprintln!("image: {image} ({width}x{height} PNG)");
    eprintln!("dictionary: {} entries", parsed.len());

    let lines = paddleocr_rust::api::recognize_png(
        &paddleocr_rust::api::Artifacts {
            library: &library,
            detector: &detector,
            detector_sha256: detector_sha256.as_deref(),
            recognizer: &recognizer,
            recognizer_sha256: recognizer_sha256.as_deref(),
        },
        &parsed,
        &bytes,
        paddleocr_rust::api::OcrOptions::default(),
    )
    .map_err(|error| format!("{error}"))?;

    if json {
        println!(
            "{}",
            paddleocr_rust::result_json::result_to_json(&lines, width, height)
        );
    } else {
        for line in &lines {
            println!("{:.6}\t{}", line.score, line.text);
        }
        if lines.is_empty() {
            eprintln!("no text detected");
        }
    }
    Ok(ExitCode::SUCCESS)
}
