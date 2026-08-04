// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Reports which Unicode scripts a recognizer dictionary can spell.
//!
//! Roadmap item: `LANG-001`.
//!
//! A dictionary is the only thing that decides which scalars a recognizer can
//! ever emit, and it is supplied by the caller. This prints what is actually in
//! one, so a claim about language support can be checked instead of assumed.
//!
//! What it reports is a fact about a file: how many scalars fall in each Unicode
//! range. It is **not** a support claim. The pinned `PP-OCRv6` dictionary
//! contains 672 emoji scalars; that does not make this port an emoji recogniser.
//! See `docs/LANGUAGE_SUPPORT.md` for the difference.
//!
//! ```sh
//! cargo run --example dictionary_census -- <dictionary.txt>
//! ```

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: dictionary_census <dictionary.txt>");
        return ExitCode::from(2);
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::from(2);
        }
    };

    // `true` matches the pinned candidate's `use_space_char`, which is why the
    // class count is entries plus two: one blank, one appended space.
    let dictionary = match paddleocr_rust::api::parse_dictionary(&text, true) {
        Ok(dictionary) => dictionary,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::from(2);
        }
    };

    println!("entries: {}", dictionary.len());
    println!("classes: {}", dictionary.class_count());
    println!();
    println!("| Script | Scalars |");
    println!("|---|---|");
    let mut total = 0;
    for row in dictionary.script_census() {
        println!("| {} | {} |", row.script.name(), row.scalars);
        total += row.scalars;
    }
    println!();
    println!("total scalars: {total}");
    ExitCode::SUCCESS
}
