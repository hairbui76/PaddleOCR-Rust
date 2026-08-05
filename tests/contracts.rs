// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration checks that keep the M2 Must contract matrix synchronized.

const M2_MUST_ROWS: [&str; 7] = [
    "M2-DET-001",
    "M2-REC-001",
    "M2-GEO-001",
    "M2-OCR-001",
    "M2-API-001",
    "M2-CLI-001",
    "M2-MODEL-001",
];

#[test]
fn m2_contract_coverage_maps_every_must_compatibility_row() {
    let compatibility = include_str!("../docs/COMPATIBILITY.md");
    let coverage = include_str!("../docs/M2_CONTRACT_COVERAGE.md");

    for identifier in M2_MUST_ROWS {
        let compatibility_row = format!("| `{identifier}` | Must |");
        let coverage_row = format!("| `{identifier}` |");
        assert!(
            compatibility.contains(&compatibility_row),
            "missing Must compatibility row for {identifier}"
        );
        assert!(
            coverage.contains(&coverage_row),
            "missing contract-coverage row for {identifier}"
        );
    }
}

#[test]
fn m2_contract_coverage_records_the_open_decision_boundaries() {
    let coverage = include_str!("../docs/M2_CONTRACT_COVERAGE.md");

    for decision in ["`D-006`", "`D-007`", "`D-008`"] {
        assert!(
            coverage.contains(decision),
            "missing open-decision boundary {decision}"
        );
    }
    assert!(coverage.contains("No supported artifact, download, cache, conversion, or backend."));
    assert!(coverage.contains("Bootstrap-only; not an OCR CLI."));
}

#[test]
fn crop_sampler_retains_the_portable_cpu_operation_profile() {
    let crop = include_str!("../src/crop.rs");
    let policy = include_str!("../docs/CROP_ORACLE_CAPTURE.md");

    for forbidden in [
        "std::arch",
        "is_x86_feature_detected",
        "target_feature",
        "f32::mul_add",
    ] {
        assert!(
            !crop.contains(forbidden),
            "portable crop sampler must not introduce {forbidden}"
        );
    }

    assert!(policy.contains("## Portable crop operation profile"));
    assert!(policy.contains("target-cpu=x86-64"));
    assert!(policy.contains("target-feature=-avx,-avx2,-fma"));
}

/// Checks whose fixture metadata names them as module tests.
mod tests {
    /// `PDF-001` entry gate: the committed corpus must be the corpus that was
    /// measured.
    ///
    /// The fidelity and malformed measurements in
    /// `docs/PDF_ENTRY_GATE_EVIDENCE.md` were taken against these exact bytes by an
    /// external harness this project deliberately does not depend on. Nothing in the
    /// offline gate can re-render a PDF, so the one thing it *can* enforce is that
    /// the files have not drifted away from the numbers written about them — a
    /// measurement whose input silently changed is worse than no measurement.
    #[test]
    fn the_pdf_entry_gate_corpus_matches_its_recorded_digests() {
        use paddleocr_rust::digest::sha256_hex;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/classic-v1-pdf-entry-gate");
        let recorded: serde_json::Value = match std::fs::read_to_string(root.join("expected.json"))
        {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(error) => panic!("expected.json: {error}"),
            },
            Err(error) => panic!("expected.json: {error}"),
        };
        let files = match recorded["files"].as_object() {
            Some(files) => files,
            None => panic!("expected.json has no files map"),
        };
        // The gate names four page kinds plus the malformed corpus; a shrunken
        // corpus would still pass a per-file loop.
        assert_eq!(files.len(), 22, "the corpus changed size");

        for (name, entry) in files {
            let path = root.join(name);
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => panic!("{name}: {error}"),
            };
            assert_eq!(
                bytes.len() as u64,
                entry["bytes"].as_u64().unwrap_or(u64::MAX),
                "{name}: byte count"
            );
            assert_eq!(
                sha256_hex(&bytes),
                entry["sha256"].as_str().unwrap_or_default(),
                "{name}: digest"
            );
        }

        // Both halves of the gate must still be represented.
        let fidelity = files
            .keys()
            .filter(|name| name.starts_with("fidelity/"))
            .count();
        let malformed = files
            .keys()
            .filter(|name| name.starts_with("malformed/"))
            .count();
        assert_eq!(fidelity, 8, "fidelity corpus size");
        assert_eq!(malformed, 14, "malformed corpus size");

        // The recorded measurement must name the case whose renderer bit-identically
        // reproduced the reference, since that is the claim the evidence rests on.
        let cases = recorded["fidelity_measurement"]["cases"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let scanned = cases
            .iter()
            .find(|case| case["case"].as_str() == Some("scanned_flate"))
            .unwrap_or_else(|| panic!("no scanned_flate measurement"));
        assert_eq!(
            scanned["max_component_difference"].as_i64(),
            Some(0),
            "the recorded scan-path measurement is no longer bit-identical"
        );
    }
}
