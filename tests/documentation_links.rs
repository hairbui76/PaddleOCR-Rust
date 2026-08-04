// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `COMP-002`: the compatibility ledger's references must resolve.
//!
//! The ledger's value is that each verified row names its module, its contract,
//! and its fixture. A named thing that no longer exists is worse than an
//! unnamed one: it reads as evidence and is not.
//!
//! This test walks every `src/*.rs`, `docs/*.md`, and `tests/fixtures/*`
//! reference in the documentation and asserts the target exists. It is cheap
//! and it is the only thing that stops the links from rotting silently.

use std::collections::BTreeSet;
use std::path::Path;

/// Documents whose references are checked.
///
/// The ledger and the contracts it points at. Not every document in `docs/`:
/// planning records legitimately name files that were never created, and
/// forcing them to resolve would turn a plan into a promise.
const CHECKED: [&str; 8] = [
    "docs/COMPATIBILITY.md",
    "docs/SPECIALIZED_API.md",
    "docs/TABLE_PIPELINE_CONTRACT.md",
    "docs/TABLE_STRUCTURE_CONTRACT.md",
    "docs/TABLE_CELLS_CONTRACT.md",
    "docs/TABLE_CLASSIFICATION_CONTRACT.md",
    "docs/READING_ORDER_CONTRACT.md",
    "docs/RECONSTRUCTION_CONTRACT.md",
];

/// Extracts every token that looks like a repository path.
fn referenced_paths(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for token in text.split(|c: char| {
        !(c.is_ascii_alphanumeric() || c == '/' || c == '.' || c == '_' || c == '-')
    }) {
        let token = token.trim_end_matches(['.', ',']);
        let looks_like_path = (token.starts_with("src/") && token.ends_with(".rs"))
            || (token.starts_with("docs/") && token.ends_with(".md"))
            || token.starts_with("tests/fixtures/")
            // This repository's tools live directly in `tools/`. Upstream's
            // are under `tools/infer/`, and the ledger's "upstream references"
            // column names them legitimately, so a nested path is not ours to
            // resolve.
            || (token.starts_with("tools/") && token.matches('/').count() == 1);
        if looks_like_path && !token.contains("**") {
            found.insert(token.to_owned());
        }
    }
    found
}

/// Every path a checked document names must exist.
#[test]
fn documented_paths_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut missing = Vec::new();
    let mut checked = 0_usize;

    for document in CHECKED {
        let path = root.join(document);
        let text = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => panic!("{document}: {error}"),
        };
        for reference in referenced_paths(&text) {
            checked += 1;
            if !root.join(&reference).exists() {
                missing.push(format!("{document} -> {reference}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "documentation names {} path(s) that do not exist:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
    // A guard against the extractor silently matching nothing, which would make
    // this test pass for the wrong reason.
    assert!(checked > 20, "only {checked} references were checked");
}

/// Bare fixture identifiers in the ledger must name real fixture directories.
///
/// The ledger writes `classic-v1-table-cells` rather than
/// `tests/fixtures/classic-v1-table-cells`, so the path extractor above does
/// not see them. They are the references most worth checking: a fixture name is
/// the closest thing this ledger has to a citation.
#[test]
fn ledger_fixture_names_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = match std::fs::read_to_string(root.join("docs/COMPATIBILITY.md")) {
        Ok(value) => value,
        Err(error) => panic!("ledger: {error}"),
    };

    let mut names = BTreeSet::new();
    for token in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        if token.starts_with("classic-v1-") && token.len() > "classic-v1-".len() {
            names.insert(token.to_owned());
        }
    }
    assert!(names.len() >= 6, "only {} fixture names found", names.len());

    let missing: Vec<&String> = names
        .iter()
        .filter(|name| !root.join("tests/fixtures").join(name).is_dir())
        .collect();
    assert!(
        missing.is_empty(),
        "the ledger names fixtures that do not exist: {missing:?}"
    );
}

/// Every verified row states whether it makes an accuracy claim.
///
/// `COMP-002` requires it, and the requirement exists because every oracle in
/// this repository pins preprocessing and postprocessing rather than detection
/// quality. A row that omits the disclaimer reads as though it were an accuracy
/// claim.
#[test]
fn specialized_rows_disclaim_accuracy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = match std::fs::read_to_string(root.join("docs/COMPATIBILITY.md")) {
        Ok(value) => value,
        Err(error) => panic!("ledger: {error}"),
    };

    let mut missing = Vec::new();
    for line in text.lines() {
        // Rows for capabilities that run a model, which is where an accuracy
        // claim could be read into the row.
        let is_model_row = line.starts_with("| `M3-")
            || line.starts_with("| `M4-")
            || line.starts_with("| `M2-DET-001`")
            || line.starts_with("| `M2-REC-001`");
        if is_model_row && !line.contains("accuracy claim") {
            let id = line.split('`').nth(1).unwrap_or("?");
            missing.push(id.to_owned());
        }
    }
    assert!(
        missing.is_empty(),
        "these rows run a model and do not state an accuracy position: {missing:?}"
    );
}
