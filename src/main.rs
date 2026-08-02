#![forbid(unsafe_code)]

//! Command-line entrypoint for the future native OCR interface.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "paddleocr-rust: OCR inference is not implemented yet; no model runtime or artifacts are available"
    );
    ExitCode::from(2)
}
