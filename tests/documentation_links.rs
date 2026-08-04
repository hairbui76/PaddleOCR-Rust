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

/// Every document the roadmap names exists, and every section it cites is there.
///
/// This closes a failure this session actually hit. A change described a rewrite
/// of `P8_ARTIFACT_AVAILABILITY.md` section 5; the edit targeted a heading whose
/// wording did not match, matched nothing, and **succeeded silently** — leaving
/// the roadmap pointing at a section that said the opposite of what the roadmap
/// claimed.
///
/// A broken link fails loudly. A link that resolves to the wrong content does
/// not, and the only cheap defence is to check that the cited **section** is
/// there at all.
#[test]
fn roadmap_references_resolve_to_real_documents_and_sections() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let roadmap = match std::fs::read_to_string(root.join("ROADMAP.md")) {
        Ok(value) => value,
        Err(error) => panic!("ROADMAP.md: {error}"),
    };

    // Every named document must exist.
    let mut documents = BTreeSet::new();
    for token in roadmap.split(|c: char| !(c.is_ascii_alphanumeric() || "/._-".contains(c))) {
        if token.starts_with("docs/") && token.ends_with(".md") {
            documents.insert(token.to_owned());
        }
    }
    assert!(
        documents.len() > 10,
        "only {} documents cited",
        documents.len()
    );
    let missing: Vec<&String> = documents
        .iter()
        .filter(|name| !root.join(name).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "the roadmap names missing documents: {missing:?}"
    );

    // Every `<doc> section N` reference must find a `## N.` heading.
    let mut checked = 0_usize;
    let words: Vec<&str> = roadmap.split_whitespace().collect();
    for window in words.windows(3) {
        let document = window[0].trim_matches(['`', ',', '.']);
        if !(document.starts_with("docs/") && document.ends_with(".md")) {
            continue;
        }
        if window[1] != "section" {
            continue;
        }
        let Ok(number) = window[2].trim_matches(['`', ',', '.']).parse::<u32>() else {
            continue;
        };
        let text = match std::fs::read_to_string(root.join(document)) {
            Ok(value) => value,
            Err(error) => panic!("{document}: {error}"),
        };
        let heading = format!("## {number}.");
        assert!(
            text.lines().any(|line| line.starts_with(&heading)),
            "{document} has no section {number}, but the roadmap cites it"
        );
        checked += 1;
    }
    assert!(checked > 0, "no section references were found to check");
}

/// Every roadmap identifier cited in a document is a real roadmap row.
///
/// The inverse direction of the check above. A document naming `TBLSTRUCT-002`
/// reads like a plan and is a typo.
///
/// Identifiers belong to one of three namespaces, and each is validated against
/// the document that **owns** it rather than exempted:
///
/// | Prefix | Owner |
/// |---|---|
/// | `M2-`, `M3-`, `M4-` | `docs/COMPATIBILITY.md` |
/// | `RISK-` | `docs/RISK_REGISTER.md` |
/// | everything else | `ROADMAP.md` |
///
/// Exempting a prefix would let a typo inside it through. Checking each against
/// its owner means `M4-TBLSTRUCT-002` fails just as `TBLSTRUCT-002` would.
#[test]
fn identifiers_cited_in_documents_are_real_roadmap_rows() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let roadmap = match std::fs::read_to_string(root.join("ROADMAP.md")) {
        Ok(value) => value,
        Err(error) => panic!("ROADMAP.md: {error}"),
    };

    // Row identifiers are the first cell of a table row.
    let mut rows = BTreeSet::new();
    for line in roadmap.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        if let Some((id, _)) = rest.split_once('`') {
            rows.insert(id.to_owned());
        }
    }
    assert!(rows.len() > 50, "only {} roadmap rows found", rows.len());

    // Identifiers owned by another document, validated against it.
    let owned = |file: &str, prefixes: &[&str]| -> (Vec<String>, BTreeSet<String>) {
        let text = match std::fs::read_to_string(root.join(file)) {
            Ok(value) => value,
            Err(error) => panic!("{file}: {error}"),
        };
        let mut ids = BTreeSet::new();
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("| `") else {
                continue;
            };
            if let Some((id, _)) = rest.split_once('`') {
                ids.insert(id.to_owned());
            }
        }
        (prefixes.iter().map(|p| (*p).to_owned()).collect(), ids)
    };
    let namespaces = [
        owned("docs/COMPATIBILITY.md", &["M2-", "M3-", "M4-"]),
        owned("docs/RISK_REGISTER.md", &["RISK-"]),
    ];
    for (prefixes, ids) in &namespaces {
        assert!(
            !ids.is_empty(),
            "no identifiers found for the namespace owning {prefixes:?}"
        );
    }

    let mut unknown = BTreeSet::new();
    let entries = match std::fs::read_dir(root.join("docs")) {
        Ok(value) => value,
        Err(error) => panic!("docs: {error}"),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => panic!("{}: {error}", path.display()),
        };
        for token in text.split('`') {
            // An identifier is upper-case letters, a hyphen, and three digits.
            let looks_like_id = token.len() >= 7
                && token.ends_with(|c: char| c.is_ascii_digit())
                && token
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                && token.contains('-');
            if !looks_like_id || rows.contains(token) {
                continue;
            }
            // If it belongs to another namespace, it must exist **there**.
            let mut claimed = false;
            for (prefixes, ids) in &namespaces {
                if prefixes.iter().any(|prefix| token.starts_with(prefix)) {
                    claimed = true;
                    if !ids.contains(token) {
                        unknown.insert(format!("{}: {token} (not in its owner)", path.display()));
                    }
                    break;
                }
            }
            if !claimed {
                unknown.insert(format!("{}: {token}", path.display()));
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "documents cite identifiers that are not roadmap rows:\n  {}",
        unknown.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}
