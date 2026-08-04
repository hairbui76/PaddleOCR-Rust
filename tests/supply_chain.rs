// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `SUPPLY-001`: the committed SBOM must describe the committed lockfile.
//!
//! A software bill of materials is only worth having if it cannot drift. A
//! hand-maintained one goes stale on the first dependency change, and a stale
//! SBOM is worse than none, because it asserts a composition that is no longer
//! true.
//!
//! This test makes drift a build failure. It parses `Cargo.lock` directly —
//! without a TOML dependency, since the subset involved is four keys — and
//! requires an exact correspondence with `sbom/paddleocr-rust.cdx.json`: same
//! packages, same versions, same checksums, both directions.
//!
//! It also enforces the two policies `SUPPLY-001` names that can be checked
//! mechanically: every dependency is exactly pinned, and every licence in the
//! graph is one that has been reviewed.

use std::collections::BTreeMap;

use serde_json::Value;

const LOCKFILE: &str = include_str!("../Cargo.lock");
const SBOM: &str = include_str!("../sbom/paddleocr-rust.cdx.json");
const MANIFEST: &str = include_str!("../Cargo.toml");

/// This project's own package, which the SBOM describes rather than lists.
const ROOT: &str = "paddleocr-rust";

/// Licence expressions accepted in this dependency graph.
///
/// The list is explicit rather than a permissive-by-pattern rule: a new
/// expression should fail this test and be looked at, even when it is obviously
/// fine, because "obviously fine" is the judgement the review exists to make.
const REVIEWED_LICENSES: [&str; 9] = [
    "MIT OR Apache-2.0",
    "Apache-2.0 OR MIT",
    "MIT/Apache-2.0",
    "MIT",
    "ISC",
    "0BSD OR MIT OR Apache-2.0",
    "MIT OR Zlib OR Apache-2.0",
    "Unlicense OR MIT",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
];

/// Extracts `name -> (version, checksum)` from the lockfile.
///
/// The lockfile is a tiny, stable subset of TOML: `[[package]]` headers and four
/// quoted string keys. Parsing it here avoids adding a TOML dependency to check
/// a file whose whole purpose is to bound dependencies.
fn locked_packages() -> BTreeMap<String, (String, Option<String>)> {
    let mut packages = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut checksum: Option<String> = None;

    let mut flush =
        |name: &mut Option<String>,
         version: &mut Option<String>,
         checksum: &mut Option<String>,
         packages: &mut BTreeMap<String, (String, Option<String>)>| {
            if let (Some(name), Some(version)) = (name.take(), version.take()) {
                packages.insert(name, (version, checksum.take()));
            } else {
                *checksum = None;
            }
        };

    for line in LOCKFILE.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            flush(&mut name, &mut version, &mut checksum, &mut packages);
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        match key {
            "name" => name = Some(value),
            "version" if name.is_some() => version = Some(value),
            "checksum" => checksum = Some(value),
            _ => {}
        }
    }
    flush(&mut name, &mut version, &mut checksum, &mut packages);
    packages
}

fn sbom_components() -> BTreeMap<String, (String, Option<String>, String)> {
    let document: Value = match serde_json::from_str(SBOM) {
        Ok(value) => value,
        Err(error) => panic!("the SBOM is not valid JSON: {error}"),
    };
    let components = match document["components"].as_array() {
        Some(components) => components,
        None => panic!("the SBOM must list components"),
    };
    let mut map = BTreeMap::new();
    for component in components {
        let name = component["name"].as_str().unwrap_or_default().to_owned();
        let version = component["version"].as_str().unwrap_or_default().to_owned();
        let checksum = component["hashes"]
            .as_array()
            .and_then(|hashes| hashes.first())
            .and_then(|hash| hash["content"].as_str())
            .map(str::to_owned);
        let license = component["licenses"]
            .as_array()
            .and_then(|licenses| licenses.first())
            .and_then(|license| license["expression"].as_str())
            .unwrap_or_default()
            .to_owned();
        map.insert(name, (version, checksum, license));
    }
    map
}

#[test]
fn the_sbom_describes_exactly_the_locked_dependencies() {
    let locked = locked_packages();
    let sbom = sbom_components();

    let expected: BTreeMap<_, _> = locked
        .iter()
        .filter(|(name, _)| name.as_str() != ROOT)
        .collect();

    let missing: Vec<&str> = expected
        .keys()
        .filter(|name| !sbom.contains_key(**name))
        .map(|name| name.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "locked but absent from the SBOM: {missing:?} — regenerate with \
         tools/generate_sbom.py and review the additions"
    );

    let extra: Vec<&str> = sbom
        .keys()
        .filter(|name| !expected.contains_key(name))
        .map(String::as_str)
        .collect();
    assert!(
        extra.is_empty(),
        "in the SBOM but no longer locked: {extra:?} — regenerate with \
         tools/generate_sbom.py"
    );

    for (name, (version, checksum)) in &expected {
        let (sbom_version, sbom_checksum, _) = &sbom[*name];
        assert_eq!(
            sbom_version, version,
            "{name}: the SBOM records a different version than the lockfile"
        );
        assert_eq!(
            sbom_checksum, checksum,
            "{name}: the SBOM records a different checksum than the lockfile"
        );
    }
}

#[test]
fn every_component_carries_a_checksum() {
    // The lockfile checksum is the SHA-256 of the published .crate archive, and
    // it is the only provenance link an auditor can verify without trusting
    // this project. A component without one is a component nobody can check.
    for (name, (_, checksum, _)) in sbom_components() {
        assert!(
            checksum.is_some(),
            "{name} has no checksum in the SBOM; a registry dependency always has one"
        );
    }
}

#[test]
fn every_licence_in_the_graph_has_been_reviewed() {
    let mut unreviewed = Vec::new();
    for (name, (_, _, license)) in sbom_components() {
        if !REVIEWED_LICENSES.contains(&license.as_str()) {
            unreviewed.push(format!("{name}: {license}"));
        }
    }
    assert!(
        unreviewed.is_empty(),
        "unreviewed licence expressions: {unreviewed:?} — add to REVIEWED_LICENSES \
         only after checking the terms, and update docs/LIC_002_AUDIT.md"
    );
}

#[test]
fn every_direct_dependency_is_exactly_pinned() {
    // An exact pin is what makes the audited graph the built graph. A caret or
    // wildcard requirement would let a rebuild pick up a version nobody in this
    // repository has looked at.
    let mut loose = Vec::new();
    let mut in_dependencies = false;
    for line in MANIFEST.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependencies = trimmed.contains("dependencies");
            continue;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once('=') else {
            continue;
        };
        // Both `png = "=0.18.1"` and the table form carry the requirement in a
        // quoted string; the pin must be the first character of it.
        let Some(start) = rest.find('"') else {
            continue;
        };
        let requirement = &rest[start + 1..];
        let Some(end) = requirement.find('"') else {
            continue;
        };
        let requirement = &requirement[..end];
        if !requirement.starts_with('=') {
            loose.push(format!("{}: {requirement}", name.trim()));
        }
    }
    assert!(
        loose.is_empty(),
        "dependencies without an exact pin: {loose:?}"
    );
}
