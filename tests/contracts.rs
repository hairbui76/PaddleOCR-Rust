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
