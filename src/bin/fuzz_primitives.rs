// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Stdin-oriented developer-only fuzz target for current pure primitives.

use std::io::{self, Read};

use paddleocr_rust::fuzz::{MAX_INPUT_BYTES, exercise};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock().take(MAX_INPUT_BYTES as u64);
    let mut input = Vec::new();
    reader.read_to_end(&mut input)?;
    exercise(&input);
    Ok(())
}
