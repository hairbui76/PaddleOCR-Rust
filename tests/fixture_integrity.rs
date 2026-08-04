// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Offline integrity checks for the committed fixture corpus.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use sha2::{Digest, Sha256};

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
const UPSTREAM_BASELINE: &str = "2661c7c0ef5c613e8f93c6e93b2e052399f0f854";
const CROP_CHANNEL_GRID_FIXTURE_ID: &str = "classic-v1-crop-channel-grid";
const RESIZE_LINEAR_GRID_FIXTURE_ID: &str = "classic-v1-resize-linear-grid";
const RESIZE_LINEAR_GRID_CAPTURE_SHA256: &str =
    "a8bccebc3cd2738d73cfa5d4bc803957ed069bc49532503484363749757130dc";
const CROP_CHANNEL_GRID_CAPTURE_SHA256: &str =
    "3b0ee3e3b231d272ac6b7751812c5af5f6390d7bd38a57d6fe5e9a89f4c620fb";
const E2E_NO_TEXT_FIXTURE_ID: &str = "classic-v1-e2e-no-text";
const E2E_NO_TEXT_INPUT_SHA256: &str =
    "c422c83b3b20d3b206d47643e3f5e6aa3d87ece61e6433ddd5be5bda8906bccd";
const E2E_NO_TEXT_BGR_SHA256: &str =
    "0c2a426327d6cf918e1547312404a9151f9f486bf006c3f77665ae997b33cf3d";
const E2E_NO_TEXT_DETECTOR_SHA256: &str =
    "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1";
const E2E_NO_TEXT_RECOGNIZER_SHA256: &str =
    "9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba";
const E2E_NO_TEXT_DICTIONARY_SHA256: &str =
    "b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d";
const E2E_NO_TEXT_DETECTOR_REVISION: &str = "61323801669c338b7891481ec7bac61ce31b576a";
const E2E_NO_TEXT_RECOGNIZER_REVISION: &str = "50c7eacafc52fa7bcf4194e8cd08e46f8558504b";
const E2E_NO_TEXT_CAPTURE_SHA256: &str =
    "28df5acdb8f9c06493d947eb2df3a85194af0efe37213ea5d55de524e7c4c299";
const E2E_NO_TEXT_EXPECTED_SHA256: &str =
    "386596c7e38f5b67e7cdb8d2d77aaa5fa6305edf8d46170336bda0b781c9fe81";
const E2E_NO_TEXT_FRESH_OUTPUT_SHA256: &str =
    "c6a0c1a835356437be001f438f636d83c1c7b53135ace79917f7f9134d594306";
const E2E_NO_TEXT_SOURCE_RECORD_SHA256: &str =
    "d73f50c93bf0760ecb958526e423f0b229578865c39c8ee3232a4eb9f0b430c6";
const E2E_READING_ORDER_FIXTURE_ID: &str = "classic-v1-e2e-reading-order";
const E2E_READING_ORDER_INPUT_SHA256: &str =
    "1617b343fa384344a2b260bc4e57c836c93b9d3d35247dd5ea548df331042ea1";
const E2E_READING_ORDER_BGR_SHA256: &str =
    "eec2d2d8b45309575caf21d1ab59cf7763731410cd55f61d7af5e880a76f80b4";
const E2E_READING_ORDER_CAPTURE_SHA256: &str =
    "f6c146d484e39fd52a2c16370a33be228f3274f927c9e4ed4f077df792f23ef7";
const E2E_READING_ORDER_EXPECTED_SHA256: &str =
    "579727f354d95304df8c90fa774c908b160866ee0807e089663d4eb3f978ea73";
const E2E_READING_ORDER_FRESH_OUTPUT_SHA256: &str =
    "cfeaaa7eda940a2710a9027af1490b6360b5e5530a20a9763fb380a11e7b631f";
const E2E_READING_ORDER_SOURCE_RECORD_SHA256: &str =
    "ae3c765b262b6cd0e46a211cc6e27d8b00233ede490b89cbe216a326d9f09135";
const E2E_TALL_CROP_FIXTURE_ID: &str = "classic-v1-e2e-tall-crop";
const E2E_TALL_CROP_INPUT_SHA256: &str =
    "95e9d9c3e198de854feb4c1b6b42cb8c6aedb3768313664879ba55c847683c20";
const E2E_TALL_CROP_BGR_SHA256: &str =
    "c16f7c6e47c92d92d897fee5e7ecdf32e5847bff18335bb3600da7965e65204d";
const E2E_TALL_CROP_CAPTURE_SHA256: &str =
    "8ac605bcad9ab4a2dd4e52edd61d1009e2cb223b97b47cd8ed9ccc1b5fcf248e";
const E2E_TALL_CROP_EXPECTED_SHA256: &str =
    "1db18357fa6d70c66b6d31ed9b9e22eb1b5e2665572d712b86685c100817b692";
const E2E_TALL_CROP_FRESH_OUTPUT_SHA256: &str =
    "263ab6e86d0a863452dab7869249c2b898859e4e2fc9f983c868b07408462d07";
const E2E_TALL_CROP_SOURCE_RECORD_SHA256: &str =
    "44f6b2d0257290784efc7023c7f95d2bd49b7b149b8f26630fd42ef0cb86acdc";
const E2E_UNICODE_FIXTURE_ID: &str = "classic-v1-e2e-unicode";
const E2E_UNICODE_INPUT_SHA256: &str =
    "17ce44aad0a8ce5a3db571fc6d7ca57fa22e1dec979326ce02ff37d77157c94c";
const E2E_UNICODE_BGR_SHA256: &str =
    "37c63cdf220706ab8c9808e9f257399a6ce32d6a1eb9d72f9457b761cd9a2d0c";
const E2E_UNICODE_CAPTURE_SHA256: &str =
    "26ab4d089c2597ab9b0780143963e727191c4364373b88adfca89ba21a63b88a";
const E2E_UNICODE_EXPECTED_SHA256: &str =
    "6835e967d31683380d41aeb7921ce1b4bf7f13f3afaab9e70eeb0fd3493332a5";
const E2E_UNICODE_FRESH_OUTPUT_SHA256: &str =
    "143f6bb51f2fe9bd3aed4e73dc210ceaafd2a8ff6397deeec9720f9df0c83c35";
const E2E_UNICODE_SOURCE_RECORD_SHA256: &str =
    "a201b8eb2ef306d753d62f76f7f76986a35c5f2414a8528f5079b72e01a7c617";
const SCORE_FILTER_FIXTURE_ID: &str = "classic-v1-ctc-score-boundary";
const SCORE_FILTER_INPUT_SHA256: &str =
    "f1b13a4af8568815bf28f42828479a383676fc04993e52714fb02101dbafc4af";
const SCORE_FILTER_CAPTURE_SHA256: &str =
    "54bf5d9280d442052ec6b17fb38675efa3224c2d03f0ade11ca8feea0da56a4c";
const SCORE_FILTER_EXPECTED_SHA256: &str =
    "3e8a69017de686cb8d75039455edec16bb0f536205ced7797a93504f3ae7678e";
const SCORE_FILTER_FRESH_OUTPUT_SHA256: &str =
    "b58c613b400325a58b9f5a08ca4e44463dfc7ca33636c4d778b17fc0cf6f26c4";
const SCORE_FILTER_SOURCE_RECORD_SHA256: &str =
    "b7ebdfd334db75c8be1baf5648db4420c1969c631b53cce5c0169d1d0babd4f8";

#[test]
fn committed_fixture_metadata_and_payloads_are_integrity_checked() {
    let fixture_directories = fixture_directories();
    let mut fixture_ids = BTreeSet::new();

    for directory in fixture_directories {
        let context = format!("fixture directory {}", directory.display());
        let metadata_path = directory.join("metadata.json");
        let metadata = read_json_file(&metadata_path);
        verify_common_metadata(&metadata, &directory, &context);

        let fixture_id = string_field(&metadata, "fixture_id", &context);
        assert!(
            fixture_ids.insert(fixture_id.to_owned()),
            "duplicate fixture identifier {fixture_id:?}"
        );

        verify_asset_descriptor(
            object_field(&metadata, "input", &context),
            &directory,
            "input",
            true,
            &context,
        );
        verify_asset_descriptor(
            object_field(&metadata, "expected", &context),
            &directory,
            "expected",
            false,
            &context,
        );
        let expected_profile = match fixture_id {
            "classic-v1-image-inputs" => "m2-image-input-oracle-v1",
            "classic-v1-e2e-no-text"
            | "classic-v1-e2e-reading-order"
            | E2E_TALL_CROP_FIXTURE_ID
            | E2E_UNICODE_FIXTURE_ID => "m2-e2e-v1",
            _ => "m2-unit-v1",
        };
        assert_eq!(
            string_field(
                object_field(&metadata, "expected", &context),
                "comparison_profile",
                &context
            ),
            expected_profile,
            "{context} fixture comparison profile changed without an integrity-gate update"
        );

        if fixture_id == "classic-v1-crop-oracle" {
            verify_crop_oracle(&metadata, &directory, &context);
        }
        if fixture_id == "classic-v1-crop-scalar-grid" {
            verify_crop_scalar_grid_oracle(&metadata, &directory, &context);
        }
        if fixture_id == CROP_CHANNEL_GRID_FIXTURE_ID {
            verify_crop_channel_grid_oracle(&metadata, &directory, &context);
        }
        if fixture_id == RESIZE_LINEAR_GRID_FIXTURE_ID {
            verify_resize_linear_grid_oracle(&metadata, &directory, &context);
        }
        if fixture_id == "classic-v1-image-inputs" {
            verify_image_input_oracle(&metadata, &directory, &context);
        }
        if fixture_id == "classic-v1-e2e-no-text" {
            verify_e2e_no_text_oracle(&metadata, &directory, &context);
        }
        if fixture_id == E2E_READING_ORDER_FIXTURE_ID {
            verify_e2e_reading_order_oracle(&metadata, &directory, &context);
        }
        if fixture_id == E2E_TALL_CROP_FIXTURE_ID {
            verify_e2e_tall_crop_oracle(&metadata, &directory, &context);
        }
        if fixture_id == E2E_UNICODE_FIXTURE_ID {
            verify_e2e_unicode_oracle(&metadata, &directory, &context);
        }
        if fixture_id == SCORE_FILTER_FIXTURE_ID {
            verify_score_filter_oracle(&metadata, &directory, &context);
        }
    }

    let expected_ids = BTreeSet::from([
        "classic-v1-crop-oracle".to_owned(),
        "classic-v1-crop-scalar-grid".to_owned(),
        CROP_CHANNEL_GRID_FIXTURE_ID.to_owned(),
        RESIZE_LINEAR_GRID_FIXTURE_ID.to_owned(),
        "classic-v1-contour-grid".to_owned(),
        "classic-v1-min-area-box-grid".to_owned(),
        "classic-v1-unclip-score-grid".to_owned(),
        "classic-v1-db-components".to_owned(),
        "classic-v1-ctc-greedy-path".to_owned(),
        "classic-v1-db-map-boundaries".to_owned(),
        "classic-v1-e2e-no-text".to_owned(),
        E2E_READING_ORDER_FIXTURE_ID.to_owned(),
        E2E_TALL_CROP_FIXTURE_ID.to_owned(),
        E2E_UNICODE_FIXTURE_ID.to_owned(),
        SCORE_FILTER_FIXTURE_ID.to_owned(),
        "classic-v1-geometry-min-area-candidate".to_owned(),
        "classic-v1-image-inputs".to_owned(),
        "classic-v1-benchmark-page".to_owned(),
        "classic-v1-preprocess-input".to_owned(),
        "classic-v1-model-manifest".to_owned(),
        "classic-v1-orientation".to_owned(),
        "classic-v1-document-orientation".to_owned(),
        "classic-v1-page-rotation".to_owned(),
        "classic-v1-unwarp".to_owned(),
        "classic-v1-cubic-resize".to_owned(),
        "classic-v1-layout".to_owned(),
        "classic-v1-table-classification".to_owned(),
        "classic-v1-table-cells".to_owned(),
    ]);
    assert_eq!(
        fixture_ids, expected_ids,
        "fixture corpus changed without an integrity-gate update"
    );
}

fn fixture_directories() -> Vec<PathBuf> {
    let root = Path::new(FIXTURE_ROOT);
    let entries = must_ok(
        fs::read_dir(root),
        &format!("read fixture root {}", root.display()),
    );
    let mut directories = Vec::new();

    for entry in entries {
        let entry = must_ok(entry, "read fixture-root entry");
        let path = entry.path();
        let metadata = must_ok(
            fs::symlink_metadata(&path),
            &format!("inspect fixture-root entry {}", path.display()),
        );
        assert!(
            !metadata.file_type().is_symlink(),
            "fixture-root entry {} must not be a symlink",
            path.display()
        );
        if metadata.is_dir() {
            directories.push(path);
        } else {
            assert_eq!(
                entry.file_name(),
                "README.md",
                "unexpected non-directory entry in fixture root"
            );
        }
    }

    directories.sort();
    assert!(!directories.is_empty(), "fixture corpus must not be empty");
    directories
}

fn verify_common_metadata(metadata: &Value, directory: &Path, context: &str) {
    let directory_name = match directory.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => panic!("{context} has a non-UTF-8 directory name"),
    };
    assert_eq!(
        string_field(metadata, "fixture_id", context),
        directory_name,
        "{context} fixture_id must match its directory"
    );
    assert_non_empty(string_field(metadata, "kind", context), "kind", context);
    assert_iso_date(string_field(metadata, "reviewed_on", context), context);
    assert!(
        value_field(metadata, "contains_personal_data", context)
            .as_bool()
            .is_some(),
        "{context} contains_personal_data must be a boolean"
    );
    if matches!(
        string_field(metadata, "fixture_id", context),
        "classic-v1-e2e-no-text"
            | E2E_READING_ORDER_FIXTURE_ID
            | E2E_TALL_CROP_FIXTURE_ID
            | E2E_UNICODE_FIXTURE_ID
    ) {
        assert!(
            value_field(metadata, "artifacts", context).is_object(),
            "{context} reviewed end-to-end fixture must declare its exact candidate artifacts"
        );
    } else {
        assert!(
            value_field(metadata, "artifacts", context).is_null(),
            "{context} must not introduce a model-backed fixture before artifact review"
        );
    }

    let upstream = object_field(metadata, "upstream", context);
    assert_eq!(
        string_field(upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} must identify the pinned upstream baseline"
    );
    let reference_paths = array_field(upstream, "reference_paths", context);
    assert!(
        !reference_paths.is_empty(),
        "{context} must declare at least one upstream reference path"
    );
    for reference_path in reference_paths {
        assert_non_empty(
            value_as_str(reference_path, "upstream reference path", context),
            "upstream reference path",
            context,
        );
    }

    let tests = array_field(metadata, "tests", context);
    assert!(!tests.is_empty(), "{context} must name a consuming test");
    for test_name in tests {
        let test_name = value_as_str(test_name, "test name", context);
        assert!(
            test_name.contains("::tests::"),
            "{context} test name {test_name:?} must identify a module test"
        );
    }

    let limitations = array_field(metadata, "limitations", context);
    assert!(
        !limitations.is_empty(),
        "{context} must record at least one limitation"
    );
}

fn verify_asset_descriptor(
    descriptor: &Value,
    fixture_directory: &Path,
    label: &str,
    requires_provenance: bool,
    context: &str,
) {
    let path = string_field(descriptor, "path", context);
    assert_non_empty(path, &format!("{label}.path"), context);
    let digest = string_field(descriptor, "sha256", context);
    assert_sha256_format(digest, &format!("{label}.sha256"), context);
    assert_non_empty(
        string_field(descriptor, "sha256_scope", context),
        &format!("{label}.sha256_scope"),
        context,
    );

    if requires_provenance {
        assert_eq!(
            string_field(descriptor, "license", context),
            "Apache-2.0",
            "{context} self-authored fixture input must state Apache-2.0"
        );
        assert_non_empty(
            string_field(descriptor, "provenance", context),
            &format!("{label}.provenance"),
            context,
        );
    } else {
        assert_non_empty(
            string_field(descriptor, "schema_version", context),
            &format!("{label}.schema_version"),
            context,
        );
        let comparison_profile = string_field(descriptor, "comparison_profile", context);
        assert!(
            matches!(
                comparison_profile,
                "m2-unit-v1" | "m2-image-input-oracle-v1" | "m2-e2e-v1"
            ),
            "{context} expected fixture has unsupported comparison profile {comparison_profile:?}"
        );
    }

    if !path.contains('#') {
        let bytes = read_fixture_file(fixture_directory, path, context);
        assert_digest(&bytes, digest, &format!("{context} {label} payload"));
    }
}

fn verify_crop_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    let expected = object_field(metadata, "expected", context);
    assert_eq!(
        string_field(input, "path", context),
        "capture.json#/cases/*/input",
        "{context} crop input path changed without a payload-integrity update"
    );
    assert_eq!(
        string_field(expected, "path", context),
        "capture.json#/cases/*/output",
        "{context} crop expected path changed without a payload-integrity update"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "generator", context),
        "tools/capture_crop_oracle.py",
        "{context} crop generator changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, "capture.json", context);
    assert_digest(
        &capture_bytes,
        string_field(oracle, "capture_sha256", context),
        &format!("{context} capture document"),
    );

    let capture = parse_json_bytes(&capture_bytes, &format!("{context} capture document"));
    let cases = array_field(&capture, "cases", context);
    assert_eq!(
        cases.len(),
        15,
        "{context} expected fifteen reviewed crop cases"
    );

    let mut case_ids = BTreeSet::new();
    let mut input_bytes = Vec::new();
    let mut output_bytes = Vec::new();
    for case in cases {
        let fixture_id = string_field(case, "fixture_id", context);
        assert!(
            case_ids.insert(fixture_id.to_owned()),
            "{context} duplicate crop-case identifier {fixture_id:?}"
        );
        input_bytes.extend(decode_crop_payload(case, "input", context));
        output_bytes.extend(decode_crop_payload(case, "output", context));
    }
    let expected_case_ids = BTreeSet::from([
        "classic-v1-crop-oracle-border-replicate-bgr-3x2".to_owned(),
        "classic-v1-crop-oracle-cubic-rounding-bgr-8x10".to_owned(),
        "classic-v1-crop-oracle-cubic-weight-order-bgr-5x10".to_owned(),
        "classic-v1-crop-oracle-edge-projective-bgr-5x4".to_owned(),
        "classic-v1-crop-oracle-identity-bgr-3x2".to_owned(),
        "classic-v1-crop-oracle-interior-projective-bgr-7x6".to_owned(),
        "classic-v1-crop-oracle-perspective-lu-bgr-12x13".to_owned(),
        "classic-v1-crop-oracle-phase-projective-bgr-8x8".to_owned(),
        "classic-v1-crop-oracle-projective-bgr-4x3".to_owned(),
        "classic-v1-crop-oracle-sampling-matrix-bgr-12x11".to_owned(),
        "classic-v1-crop-oracle-single-pixel-bgr-3x3".to_owned(),
        "classic-v1-crop-oracle-tall-projective-bgr-4x7".to_owned(),
        "classic-v1-crop-oracle-tall-rotation-bgr-2x3".to_owned(),
        "classic-v1-crop-oracle-tall-thin-projective-bgr-3x9".to_owned(),
        "classic-v1-crop-oracle-ties-even-bgr-4x7".to_owned(),
    ]);
    assert_eq!(
        case_ids, expected_case_ids,
        "{context} crop case set changed without an integrity-gate update"
    );
    assert_digest(
        &input_bytes,
        string_field(input, "sha256", context),
        &format!("{context} concatenated crop inputs"),
    );
    assert_digest(
        &output_bytes,
        string_field(expected, "sha256", context),
        &format!("{context} concatenated crop outputs"),
    );

    let geometry_oracle = object_field(metadata, "geometry_oracle", context);
    assert_eq!(
        string_field(geometry_oracle, "comparison_profile", context),
        "m2-unit-v1",
        "{context} geometry oracle must use the frozen unit comparison profile"
    );
    let geometry_path = string_field(geometry_oracle, "path", context);
    let geometry_bytes = read_fixture_file(fixture_directory, geometry_path, context);
    assert_digest(
        &geometry_bytes,
        string_field(geometry_oracle, "sha256", context),
        &format!("{context} geometry oracle"),
    );
}

fn verify_crop_scalar_grid_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    let expected = object_field(metadata, "expected", context);
    assert_eq!(
        string_field(input, "path", context),
        "capture.json#/cases/*/input",
        "{context} crop scalar-grid input path changed without a payload-integrity update"
    );
    assert_eq!(
        string_field(expected, "path", context),
        "capture.json#/cases/*/output",
        "{context} crop scalar-grid expected path changed without a payload-integrity update"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "generator", context),
        "tools/capture_crop_oracle.py",
        "{context} crop scalar-grid generator changed without review"
    );
    assert_eq!(
        string_field(oracle, "suite", context),
        "scalar-grid",
        "{context} crop scalar-grid suite changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, "capture.json", context);
    assert_digest(
        &capture_bytes,
        string_field(oracle, "capture_sha256", context),
        &format!("{context} crop scalar-grid capture document"),
    );

    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} crop scalar-grid capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        "paddleocr-rust/crop-oracle/v1",
        "{context} crop scalar-grid capture schema changed without review"
    );
    verify_scalar_grid_capture_environment(
        object_field(oracle, "environment", context),
        object_field(&capture, "environment", context),
        context,
    );

    let cases = array_field(&capture, "cases", context);
    let expected_case_ids = [
        "classic-v1-crop-scalar-grid-00-bgr-3x3",
        "classic-v1-crop-scalar-grid-01-bgr-4x7",
        "classic-v1-crop-scalar-grid-02-bgr-5x11",
        "classic-v1-crop-scalar-grid-03-bgr-6x4",
        "classic-v1-crop-scalar-grid-04-bgr-7x13",
        "classic-v1-crop-scalar-grid-05-bgr-8x5",
        "classic-v1-crop-scalar-grid-06-bgr-9x15",
        "classic-v1-crop-scalar-grid-07-bgr-10x6",
        "classic-v1-crop-scalar-grid-08-bgr-11x16",
        "classic-v1-crop-scalar-grid-09-bgr-12x8",
        "classic-v1-crop-scalar-grid-10-bgr-13x14",
        "classic-v1-crop-scalar-grid-11-bgr-14x9",
        "classic-v1-crop-scalar-grid-12-bgr-15x12",
        "classic-v1-crop-scalar-grid-13-bgr-16x10",
        "classic-v1-crop-scalar-grid-14-bgr-3x16",
        "classic-v1-crop-scalar-grid-15-bgr-4x12",
        "classic-v1-crop-scalar-grid-16-bgr-5x15",
        "classic-v1-crop-scalar-grid-17-bgr-6x9",
        "classic-v1-crop-scalar-grid-18-bgr-7x14",
        "classic-v1-crop-scalar-grid-19-bgr-8x3",
        "classic-v1-crop-scalar-grid-20-bgr-9x11",
        "classic-v1-crop-scalar-grid-21-bgr-10x4",
        "classic-v1-crop-scalar-grid-22-bgr-11x13",
        "classic-v1-crop-scalar-grid-23-bgr-12x6",
        "classic-v1-crop-scalar-grid-24-bgr-1x1",
        "classic-v1-crop-scalar-grid-25-bgr-1x7",
        "classic-v1-crop-scalar-grid-26-bgr-7x1",
        "classic-v1-crop-scalar-grid-27-bgr-2x2",
        "classic-v1-crop-scalar-grid-28-bgr-2x9",
        "classic-v1-crop-scalar-grid-29-bgr-9x2",
        "classic-v1-crop-scalar-grid-30-bgr-17x19",
        "classic-v1-crop-scalar-grid-31-bgr-31x3",
        "classic-v1-crop-scalar-grid-32-bgr-3x31",
        "classic-v1-crop-scalar-grid-33-bgr-16x16",
        "classic-v1-crop-scalar-grid-34-bgr-13x17",
        "classic-v1-crop-scalar-grid-35-bgr-12x12",
    ];
    assert_eq!(
        cases.len(),
        expected_case_ids.len(),
        "{context} expected thirty-six reviewed crop scalar-grid cases"
    );

    let mut input_bytes = Vec::new();
    let mut output_bytes = Vec::new();
    for (case, expected_fixture_id) in cases.iter().zip(expected_case_ids) {
        assert_eq!(
            string_field(case, "fixture_id", context),
            expected_fixture_id,
            "{context} crop scalar-grid case order or identifier changed without an integrity-gate update"
        );
        input_bytes.extend(decode_crop_payload(case, "input", context));
        output_bytes.extend(decode_crop_payload(case, "output", context));
    }
    assert_digest(
        &input_bytes,
        string_field(input, "sha256", context),
        &format!("{context} concatenated crop scalar-grid inputs"),
    );
    assert_digest(
        &output_bytes,
        string_field(expected, "sha256", context),
        &format!("{context} concatenated crop scalar-grid outputs"),
    );
}

fn verify_scalar_grid_capture_environment(oracle: &Value, captured: &Value, context: &str) {
    for field in ["numpy", "opencv", "opencv_build_information_sha256"] {
        assert_eq!(
            string_field(captured, field, context),
            string_field(oracle, field, context),
            "{context} crop scalar-grid environment disagrees on {field}"
        );
    }
    let captured_distribution = object_field(captured, "opencv_distribution", context);
    let captured_distribution = format!(
        "{} {}",
        string_field(captured_distribution, "name", context),
        string_field(captured_distribution, "version", context)
    );
    assert_eq!(
        captured_distribution,
        string_field(oracle, "opencv_distribution", context),
        "{context} crop scalar-grid environment disagrees on OpenCV distribution"
    );
    let captured_python = string_field(captured, "python", context);
    assert!(
        captured_python.starts_with(string_field(oracle, "python", context)),
        "{context} crop scalar-grid environment disagrees on Python version"
    );
    assert_eq!(
        value_field(captured, "opencv_optimized", context).as_bool(),
        Some(false),
        "{context} crop scalar-grid capture must disable OpenCV optimized paths"
    );
    assert_eq!(
        value_field(oracle, "opencv_optimized", context).as_bool(),
        Some(false),
        "{context} crop scalar-grid metadata must record disabled OpenCV optimized paths"
    );
}

/// Checks the interleaved-channel and cubic-saturation crop oracle record.
///
/// This is the only committed crop capture whose sources are not all
/// three-channel BGR, so it additionally pins the declared channel-order label
/// of every case instead of assuming one colour convention.
fn verify_crop_channel_grid_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    let expected = object_field(metadata, "expected", context);
    assert_eq!(
        string_field(input, "path", context),
        "capture.json#/cases/*/input",
        "{context} crop channel-grid input path changed without a payload-integrity update"
    );
    assert_eq!(
        string_field(expected, "path", context),
        "capture.json#/cases/*/output",
        "{context} crop channel-grid expected path changed without a payload-integrity update"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "generator", context),
        "tools/capture_crop_oracle.py",
        "{context} crop channel-grid generator changed without review"
    );
    assert_eq!(
        string_field(oracle, "suite", context),
        "channel-grid",
        "{context} crop channel-grid suite changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        CROP_CHANNEL_GRID_CAPTURE_SHA256,
        "{context} crop channel-grid capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, "capture.json", context);
    assert_digest(
        &capture_bytes,
        CROP_CHANNEL_GRID_CAPTURE_SHA256,
        &format!("{context} crop channel-grid capture document"),
    );

    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} crop channel-grid capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        "paddleocr-rust/crop-oracle/v1",
        "{context} crop channel-grid capture schema changed without review"
    );
    verify_scalar_grid_capture_environment(
        object_field(oracle, "environment", context),
        object_field(&capture, "environment", context),
        context,
    );

    // Each reviewed case is pinned with the channel count its payloads must
    // declare, so a silently re-generated capture cannot change the covered
    // channel range.
    let expected_cases = [
        ("classic-v1-crop-channel-grid-1ch-step-edge-half-phase", 1),
        ("classic-v1-crop-channel-grid-1ch-step-edge-projective", 1),
        (
            "classic-v1-crop-channel-grid-1ch-checkerboard-half-phase",
            1,
        ),
        (
            "classic-v1-crop-channel-grid-1ch-checkerboard-projective",
            1,
        ),
        (
            "classic-v1-crop-channel-grid-1ch-isolated-spike-half-phase",
            1,
        ),
        (
            "classic-v1-crop-channel-grid-1ch-isolated-spike-projective",
            1,
        ),
        ("classic-v1-crop-channel-grid-2ch-step-edge-half-phase", 2),
        ("classic-v1-crop-channel-grid-2ch-step-edge-projective", 2),
        (
            "classic-v1-crop-channel-grid-2ch-checkerboard-half-phase",
            2,
        ),
        (
            "classic-v1-crop-channel-grid-2ch-checkerboard-projective",
            2,
        ),
        (
            "classic-v1-crop-channel-grid-2ch-isolated-spike-half-phase",
            2,
        ),
        (
            "classic-v1-crop-channel-grid-2ch-isolated-spike-projective",
            2,
        ),
        ("classic-v1-crop-channel-grid-4ch-step-edge-half-phase", 4),
        ("classic-v1-crop-channel-grid-4ch-step-edge-projective", 4),
        (
            "classic-v1-crop-channel-grid-4ch-checkerboard-half-phase",
            4,
        ),
        (
            "classic-v1-crop-channel-grid-4ch-checkerboard-projective",
            4,
        ),
        (
            "classic-v1-crop-channel-grid-4ch-isolated-spike-half-phase",
            4,
        ),
        (
            "classic-v1-crop-channel-grid-4ch-isolated-spike-projective",
            4,
        ),
        (
            "classic-v1-crop-channel-grid-3ch-step-edge-quarter-phase",
            3,
        ),
        (
            "classic-v1-crop-channel-grid-3ch-checkerboard-quarter-phase",
            3,
        ),
        (
            "classic-v1-crop-channel-grid-3ch-isolated-spike-quarter-phase",
            3,
        ),
    ];
    let cases = array_field(&capture, "cases", context);
    assert_eq!(
        cases.len(),
        expected_cases.len(),
        "{context} expected twenty-one reviewed crop channel-grid cases"
    );

    let mut input_bytes = Vec::new();
    let mut output_bytes = Vec::new();
    let mut observed_channels = BTreeSet::new();
    for (case, (expected_fixture_id, channels)) in cases.iter().zip(expected_cases) {
        assert_eq!(
            string_field(case, "fixture_id", context),
            expected_fixture_id,
            "{context} crop channel-grid case order or identifier changed without an integrity-gate update"
        );
        let channel_order = if channels == 3 {
            "BGR".to_owned()
        } else {
            format!("opaque-{channels}")
        };
        observed_channels.insert(channels);
        input_bytes.extend(decode_crop_payload_with_channel_order(
            case,
            "input",
            &channel_order,
            context,
        ));
        output_bytes.extend(decode_crop_payload_with_channel_order(
            case,
            "output",
            &channel_order,
            context,
        ));
    }
    assert_eq!(
        observed_channels,
        BTreeSet::from([1, 2, 3, 4]),
        "{context} crop channel-grid must keep covering one through four channels"
    );
    assert_digest(
        &input_bytes,
        string_field(input, "sha256", context),
        &format!("{context} concatenated crop channel-grid inputs"),
    );
    assert_digest(
        &output_bytes,
        string_field(expected, "sha256", context),
        &format!("{context} concatenated crop channel-grid outputs"),
    );
}

/// Checks the OpenCV `INTER_LINEAR` resize oracle record.
fn verify_resize_linear_grid_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    let expected = object_field(metadata, "expected", context);
    assert_eq!(
        string_field(input, "path", context),
        "capture.json#/cases/*/input",
        "{context} resize input path changed without a payload-integrity update"
    );
    assert_eq!(
        string_field(expected, "path", context),
        "capture.json#/cases/*/output",
        "{context} resize expected path changed without a payload-integrity update"
    );
    assert_eq!(
        string_field(expected, "schema_version", context),
        "paddleocr-rust/resize-oracle/v1",
        "{context} resize expected schema changed without review"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "generator", context),
        "tools/capture_resize_oracle.py",
        "{context} resize generator changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        RESIZE_LINEAR_GRID_CAPTURE_SHA256,
        "{context} resize capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, "capture.json", context);
    assert_digest(
        &capture_bytes,
        RESIZE_LINEAR_GRID_CAPTURE_SHA256,
        &format!("{context} resize capture document"),
    );

    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} resize capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        "paddleocr-rust/resize-oracle/v1",
        "{context} resize capture schema changed without review"
    );
    verify_scalar_grid_capture_environment(
        object_field(oracle, "environment", context),
        object_field(&capture, "environment", context),
        context,
    );
    assert_eq!(
        string_field(
            object_field(&capture, "algorithm", context),
            "interpolation",
            context
        ),
        "INTER_LINEAR",
        "{context} resize interpolation changed without review"
    );

    let cases = array_field(&capture, "cases", context);
    assert_eq!(
        cases.len(),
        34,
        "{context} expected thirty-four reviewed resize cases"
    );

    let mut input_bytes = Vec::new();
    let mut output_bytes = Vec::new();
    let mut seen = BTreeSet::new();
    for case in cases {
        let fixture_id = string_field(case, "fixture_id", context);
        assert!(
            fixture_id.starts_with("classic-v1-resize-linear-"),
            "{context} resize case identifier {fixture_id:?} is outside the reviewed namespace"
        );
        assert!(
            seen.insert(fixture_id.to_owned()),
            "{context} duplicate resize case {fixture_id:?}"
        );
        let target = object_field(case, "target_size", context);
        for axis in ["width", "height"] {
            let value = value_field(target, axis, context).as_u64();
            assert!(
                matches!(value, Some(value) if value > 0),
                "{context} resize case {fixture_id:?} must declare a positive {axis}"
            );
        }
        input_bytes.extend(decode_crop_payload(case, "input", context));
        output_bytes.extend(decode_crop_payload(case, "output", context));
    }
    assert_digest(
        &input_bytes,
        string_field(input, "sha256", context),
        &format!("{context} concatenated resize inputs"),
    );
    assert_digest(
        &output_bytes,
        string_field(expected, "sha256", context),
        &format!("{context} concatenated resize outputs"),
    );
}

fn verify_image_input_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    let negative_input = object_field(metadata, "negative_input", context);
    let expected = object_field(metadata, "expected", context);
    assert_eq!(
        string_field(input, "path", context),
        "capture.json#/cases/*/encoded_image",
        "{context} image input path changed without an integrity-gate update"
    );
    assert_eq!(
        string_field(negative_input, "path", context),
        "capture.json#/negative_cases/*/encoded_input",
        "{context} negative image input path changed without an integrity-gate update"
    );
    assert_eq!(
        string_field(expected, "path", context),
        "capture.json#/cases/*/opencv_imread_color",
        "{context} image oracle output path changed without an integrity-gate update"
    );
    assert_eq!(
        string_field(expected, "comparison_profile", context),
        "m2-image-input-oracle-v1",
        "{context} image oracle must use its dedicated comparison profile"
    );
    verify_asset_descriptor(
        negative_input,
        fixture_directory,
        "negative input",
        true,
        context,
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "generator", context),
        "tools/capture_image_decoder_oracle.py",
        "{context} image generator changed without review"
    );
    assert_eq!(
        string_field(oracle, "operation", context),
        "cv2.imdecode(encoded, cv2.IMREAD_COLOR)",
        "{context} image oracle operation changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, "capture.json", context);
    assert_digest(
        &capture_bytes,
        string_field(oracle, "capture_sha256", context),
        &format!("{context} image capture document"),
    );

    let capture = parse_json_bytes(&capture_bytes, &format!("{context} image capture document"));
    assert_eq!(
        string_field(&capture, "schema_version", context),
        "paddleocr-rust/image-input-oracle/v1",
        "{context} image capture schema changed without review"
    );
    let captured_oracle = object_field(&capture, "oracle", context);
    assert_eq!(
        string_field(captured_oracle, "operation", context),
        string_field(oracle, "operation", context),
        "{context} image capture and metadata disagree on the oracle operation"
    );
    verify_image_capture_environment(
        object_field(oracle, "environment", context),
        object_field(&capture, "environment", context),
        context,
    );

    let cases = array_field(&capture, "cases", context);
    assert_eq!(
        cases.len(),
        15,
        "{context} expected fifteen valid image cases"
    );
    let mut case_ids = BTreeSet::new();
    let mut valid_inputs = Vec::new();
    let mut outputs = Vec::new();
    for case in cases {
        let fixture_id = string_field(case, "fixture_id", context);
        assert!(
            case_ids.insert(fixture_id.to_owned()),
            "{context} duplicate valid image case identifier {fixture_id:?}"
        );
        assert!(
            matches!(string_field(case, "format", context), "png" | "jpeg"),
            "{context} valid image case {fixture_id:?} has an unsupported format label"
        );
        valid_inputs.extend(decode_image_payload(case, "encoded_image", false, context));
        outputs.extend(decode_image_payload(
            case,
            "opencv_imread_color",
            true,
            context,
        ));
    }
    let expected_case_ids = BTreeSet::from([
        "classic-v1-image-input-jpeg-baseline-3x2".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-1".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-2".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-3".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-4".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-5".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-6".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-7".to_owned(),
        "classic-v1-image-input-jpeg-exif-orientation-8".to_owned(),
        "classic-v1-image-input-jpeg-progressive-3x2".to_owned(),
        "classic-v1-image-input-png-grayscale-3x2".to_owned(),
        "classic-v1-image-input-png-grayscale16-3x2".to_owned(),
        "classic-v1-image-input-png-indexed-trns-3x2".to_owned(),
        "classic-v1-image-input-png-rgb-3x2".to_owned(),
        "classic-v1-image-input-png-rgba-3x2".to_owned(),
    ]);
    assert_eq!(
        case_ids, expected_case_ids,
        "{context} valid image case set changed without an integrity-gate update"
    );
    assert_digest(
        &valid_inputs,
        string_field(input, "sha256", context),
        &format!("{context} concatenated valid image inputs"),
    );
    assert_digest(
        &outputs,
        string_field(expected, "sha256", context),
        &format!("{context} concatenated image oracle outputs"),
    );

    let negative_cases = array_field(&capture, "negative_cases", context);
    assert_eq!(
        negative_cases.len(),
        5,
        "{context} expected five negative image cases"
    );
    let mut negative_ids = BTreeSet::new();
    let mut negative_inputs = Vec::new();
    for case in negative_cases {
        let fixture_id = string_field(case, "fixture_id", context);
        assert!(
            negative_ids.insert(fixture_id.to_owned()),
            "{context} duplicate negative image case identifier {fixture_id:?}"
        );
        assert!(
            matches!(
                string_field(case, "required_outcome", context),
                "invalid_input_empty"
                    | "unsupported_format"
                    | "malformed_input"
                    | "resource_limit_before_project_pixel_allocation"
                    | "content_detection_ignores_filename_hint"
            ),
            "{context} negative image case {fixture_id:?} has an unknown required outcome"
        );
        negative_inputs.extend(decode_image_payload(case, "encoded_input", false, context));
    }
    let expected_negative_ids = BTreeSet::from([
        "classic-v1-image-input-content-name-confusion".to_owned(),
        "classic-v1-image-input-empty".to_owned(),
        "classic-v1-image-input-oversized-png-header".to_owned(),
        "classic-v1-image-input-truncated-png".to_owned(),
        "classic-v1-image-input-unknown-bytes".to_owned(),
    ]);
    assert_eq!(
        negative_ids, expected_negative_ids,
        "{context} negative image case set changed without an integrity-gate update"
    );
    assert_digest(
        &negative_inputs,
        string_field(negative_input, "sha256", context),
        &format!("{context} concatenated negative image inputs"),
    );
}

fn verify_e2e_no_text_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    assert_eq!(
        string_field(input, "path", context),
        "input.png",
        "{context} no-text fixture input path changed without review"
    );
    assert_eq!(
        string_field(input, "sha256", context),
        E2E_NO_TEXT_INPUT_SHA256,
        "{context} no-text fixture input hash changed without review"
    );

    let artifacts = object_field(metadata, "artifacts", context);
    assert_eq!(
        string_field(artifacts, "representation", context),
        "onnx",
        "{context} no-text fixture must retain the reviewed ONNX representation"
    );
    assert_eq!(
        string_field(artifacts, "terms_review", context),
        "LIC-001",
        "{context} no-text fixture must identify its terms review"
    );
    assert_eq!(
        value_field(artifacts, "local_only_candidate", context).as_bool(),
        Some(true),
        "{context} no-text fixture must remain a local-only candidate"
    );
    verify_e2e_candidate(
        object_field(artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    let dictionary = object_field(artifacts, "dictionary", context);
    assert_eq!(
        string_field(dictionary, "source_path", context),
        "ppocr/utils/dict/ppocrv6_dict.txt",
        "{context} no-text fixture dictionary source changed without review"
    );
    assert_eq!(
        string_field(dictionary, "sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} no-text fixture dictionary hash changed without review"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "capture_path", context),
        "capture.json",
        "{context} no-text fixture capture path changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_schema_version", context),
        "paddleocr-rust/classic-onnx-oracle-capture/v1",
        "{context} no-text fixture capture schema changed without review"
    );
    let capture_path = string_field(oracle, "capture_path", context);
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        E2E_NO_TEXT_CAPTURE_SHA256,
        "{context} no-text fixture capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(fixture_directory, capture_path, context);
    assert_digest(
        &capture_bytes,
        string_field(oracle, "capture_sha256", context),
        &format!("{context} no-text fixture capture document"),
    );
    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} no-text fixture capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        string_field(oracle, "capture_schema_version", context),
        "{context} no-text fixture capture schema disagrees with metadata"
    );
    assert_eq!(
        string_field(&capture, "fixture_id", context),
        E2E_NO_TEXT_FIXTURE_ID,
        "{context} no-text capture fixture identifier changed without review"
    );

    let capture_input = object_field(&capture, "input", context);
    assert_eq!(
        string_field(capture_input, "source_fixture", context),
        "classic-v1-image-inputs",
        "{context} no-text capture input source changed without review"
    );
    assert_eq!(
        string_field(capture_input, "source_fixture_id", context),
        "classic-v1-image-input-png-rgb-3x2",
        "{context} no-text capture input identifier changed without review"
    );
    assert_eq!(
        string_field(capture_input, "png_sha256", context),
        E2E_NO_TEXT_INPUT_SHA256,
        "{context} no-text capture PNG hash changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_sha256", context),
        E2E_NO_TEXT_BGR_SHA256,
        "{context} no-text capture BGR hash changed without review"
    );
    assert_eq!(
        value_field(capture_input, "bgr_shape", context),
        &serde_json::json!([2, 3, 3]),
        "{context} no-text capture BGR shape changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_channel_order", context),
        "BGR",
        "{context} no-text capture BGR channel order changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_dtype", context),
        "uint8",
        "{context} no-text capture BGR dtype changed without review"
    );

    let capture_upstream = object_field(&capture, "upstream", context);
    assert_eq!(
        string_field(capture_upstream, "repository", context),
        "https://github.com/PaddlePaddle/PaddleOCR.git",
        "{context} no-text capture upstream repository changed without review"
    );
    assert_eq!(
        string_field(capture_upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} no-text capture upstream baseline changed without review"
    );
    for field in ["status_before", "status_after"] {
        assert_eq!(
            string_field(capture_upstream, field, context),
            "clean",
            "{context} no-text capture upstream {field} must be clean"
        );
    }

    let capture_artifacts = object_field(&capture, "artifacts", context);
    assert_eq!(
        string_field(capture_artifacts, "terms_review", context),
        "LIC-001",
        "{context} no-text capture must identify its terms review"
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    let capture_dictionary = object_field(capture_artifacts, "dictionary", context);
    assert_eq!(
        string_field(capture_dictionary, "sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} no-text capture dictionary hash changed without review"
    );

    let execution = object_field(&capture, "execution", context);
    for (field, expected) in [
        ("python", "3.12.3"),
        ("paddlepaddle", "3.3.1"),
        (
            "paddle_inference",
            "not invoked; use_onnx=true selected ONNX Runtime",
        ),
        ("onnxruntime", "1.28.0"),
        ("selected_execution_provider", "CPUExecutionProvider"),
        ("numpy", "1.26.4"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("gpu", "disabled"),
    ] {
        assert_eq!(
            string_field(execution, field, context),
            expected,
            "{context} no-text capture execution field {field} changed without review"
        );
    }
    let session_options = object_field(execution, "onnx_session_options", context);
    assert_eq!(
        unsigned_field(session_options, "intra_op_num_threads", context),
        1,
        "{context} no-text capture must pin one intra-op thread"
    );
    assert_eq!(
        unsigned_field(session_options, "inter_op_num_threads", context),
        1,
        "{context} no-text capture must pin one inter-op thread"
    );
    assert_eq!(
        value_field(session_options, "enable_mem_pattern", context).as_bool(),
        Some(false),
        "{context} no-text capture must disable ONNX Runtime memory patterns"
    );
    let classic_options = object_field(execution, "classic_options", context);
    for field in [
        "use_gpu",
        "use_onnx",
        "use_angle_cls",
        "benchmark",
        "show_log",
        "cls_argument",
    ] {
        let expected = field == "use_onnx";
        assert_eq!(
            value_field(classic_options, field, context).as_bool(),
            Some(expected),
            "{context} no-text capture classic option {field} changed without review"
        );
    }

    let reproducibility = object_field(&capture, "reproducibility", context);
    assert_eq!(
        value_field(reproducibility, "harness_retained_in_repository", context).as_bool(),
        Some(false),
        "{context} must not retain the external no-text capture harness"
    );
    assert_eq!(
        value_field(reproducibility, "fresh_process_stdout_identical", context).as_bool(),
        Some(true),
        "{context} no-text capture fresh-process outputs must agree"
    );
    let fresh_runs = array_field(reproducibility, "fresh_process_runs", context);
    assert_eq!(
        fresh_runs.len(),
        2,
        "{context} no-text capture must retain exactly two fresh-process digests"
    );
    let first_run = value_as_object(&fresh_runs[0], "first fresh process run", context);
    let second_run = value_as_object(&fresh_runs[1], "second fresh process run", context);
    assert_eq!(
        string_field(first_run, "id", context),
        "run-1",
        "{context} no-text capture first fresh run changed without review"
    );
    assert_eq!(
        string_field(second_run, "id", context),
        "run-2",
        "{context} no-text capture second fresh run changed without review"
    );
    assert_eq!(
        string_field(first_run, "stdout_sha256", context),
        E2E_NO_TEXT_FRESH_OUTPUT_SHA256,
        "{context} no-text capture first fresh-run stdout hash changed without review"
    );
    assert_eq!(
        string_field(second_run, "stdout_sha256", context),
        E2E_NO_TEXT_FRESH_OUTPUT_SHA256,
        "{context} no-text capture second fresh-run stdout hash changed without review"
    );
    assert_sha256_format(
        string_field(first_run, "stdout_sha256", context),
        "fresh_process_runs.stdout_sha256",
        context,
    );

    let source_result = object_field(&capture, "source_result", context);
    let source_record = object_field(source_result, "record", context);
    let source_record_bytes = must_ok(
        serde_json::to_vec(source_record),
        &format!("serialize {context} no-text source record"),
    );
    assert_digest(
        &source_record_bytes,
        string_field(source_result, "canonical_json_sha256", context),
        &format!("{context} no-text source record"),
    );
    assert_eq!(
        string_field(source_result, "canonical_json_sha256", context),
        E2E_NO_TEXT_SOURCE_RECORD_SHA256,
        "{context} no-text source record digest changed without review"
    );
    assert_eq!(
        string_field(source_record, "fixture_id", context),
        E2E_NO_TEXT_FIXTURE_ID,
        "{context} no-text source-result identifier changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_png_sha256", context),
        E2E_NO_TEXT_INPUT_SHA256,
        "{context} no-text source-result PNG hash changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_bgr_sha256", context),
        E2E_NO_TEXT_BGR_SHA256,
        "{context} no-text source-result BGR hash changed without review"
    );
    for (field, expected) in [
        ("detector_sha256", E2E_NO_TEXT_DETECTOR_SHA256),
        ("recognizer_sha256", E2E_NO_TEXT_RECOGNIZER_SHA256),
        ("dictionary_sha256", E2E_NO_TEXT_DICTIONARY_SHA256),
    ] {
        assert_eq!(
            string_field(source_record, field, context),
            expected,
            "{context} no-text source-result field {field} changed without review"
        );
    }
    assert_eq!(
        value_field(source_result, "raw_detector_tensors_retained", context).as_bool(),
        Some(false),
        "{context} must not retain raw detector tensors"
    );
    assert_eq!(
        value_field(source_result, "raw_recognizer_tensors_retained", context).as_bool(),
        Some(false),
        "{context} must not retain raw recognizer tensors"
    );

    let expected_bytes = read_fixture_file(fixture_directory, "expected.json", context);
    assert_digest(
        &expected_bytes,
        E2E_NO_TEXT_EXPECTED_SHA256,
        &format!("{context} no-text expected result"),
    );
    let expected = parse_json_bytes(
        &expected_bytes,
        &format!("{context} no-text expected result"),
    );
    assert_eq!(
        string_field(&expected, "schema_version", context),
        "paddleocr-rust/ocr-result/v1",
        "{context} no-text expected result schema changed without review"
    );
    let expected_input = object_field(&expected, "input", context);
    assert!(
        value_field(expected_input, "id", context).is_null(),
        "{context} no-text expected input identifier must remain null"
    );
    assert!(
        value_field(expected_input, "page_index", context).is_null(),
        "{context} no-text expected page index must remain null"
    );
    assert_eq!(
        unsigned_field(expected_input, "width", context),
        3,
        "{context} no-text expected width changed without review"
    );
    assert_eq!(
        unsigned_field(expected_input, "height", context),
        2,
        "{context} no-text expected height changed without review"
    );
    let expected_models = object_field(&expected, "models", context);
    let expected_detector = object_field(expected_models, "detector", context);
    let expected_recognizer = object_field(expected_models, "recognizer", context);
    assert_eq!(
        string_field(expected_detector, "family", context),
        "PP-OCRv6_medium_det",
        "{context} no-text expected detector family changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "version", context),
        format!("m2-onnx-det-v6-medium@{E2E_NO_TEXT_DETECTOR_REVISION}"),
        "{context} no-text expected detector provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "artifact_sha256", context),
        E2E_NO_TEXT_DETECTOR_SHA256,
        "{context} no-text expected detector hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "family", context),
        "PP-OCRv6_medium_rec",
        "{context} no-text expected recognizer family changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "version", context),
        format!("m2-onnx-rec-v6-medium@{E2E_NO_TEXT_RECOGNIZER_REVISION}"),
        "{context} no-text expected recognizer provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "artifact_sha256", context),
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        "{context} no-text expected recognizer hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "dictionary_sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} no-text expected dictionary hash changed without review"
    );
    assert!(
        array_field(&expected, "lines", context).is_empty(),
        "{context} no-text expected result must contain no lines"
    );
    assert_eq!(
        value_field(source_record, "lines", context),
        value_field(&expected, "lines", context),
        "{context} no-text source result and expected result differ"
    );
}

fn verify_e2e_reading_order_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    assert_eq!(
        string_field(input, "path", context),
        "input.png",
        "{context} reading-order fixture input path changed without review"
    );
    assert_eq!(
        string_field(input, "sha256", context),
        E2E_READING_ORDER_INPUT_SHA256,
        "{context} reading-order fixture input hash changed without review"
    );

    let artifacts = object_field(metadata, "artifacts", context);
    assert_eq!(
        string_field(artifacts, "representation", context),
        "onnx",
        "{context} reading-order fixture must retain the reviewed ONNX representation"
    );
    assert_eq!(
        string_field(artifacts, "terms_review", context),
        "LIC-001",
        "{context} reading-order fixture must identify its terms review"
    );
    assert_eq!(
        value_field(artifacts, "local_only_candidate", context).as_bool(),
        Some(true),
        "{context} reading-order fixture must remain a local-only candidate"
    );
    verify_e2e_candidate(
        object_field(artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    let dictionary = object_field(artifacts, "dictionary", context);
    assert_eq!(
        string_field(dictionary, "source_path", context),
        "ppocr/utils/dict/ppocrv6_dict.txt",
        "{context} reading-order fixture dictionary source changed without review"
    );
    assert_eq!(
        string_field(dictionary, "sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} reading-order fixture dictionary hash changed without review"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "capture_path", context),
        "capture.json",
        "{context} reading-order fixture capture path changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_schema_version", context),
        "paddleocr-rust/classic-onnx-oracle-capture/v1",
        "{context} reading-order fixture capture schema changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        E2E_READING_ORDER_CAPTURE_SHA256,
        "{context} reading-order fixture capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(
        fixture_directory,
        string_field(oracle, "capture_path", context),
        context,
    );
    assert_digest(
        &capture_bytes,
        E2E_READING_ORDER_CAPTURE_SHA256,
        &format!("{context} reading-order fixture capture document"),
    );
    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} reading-order fixture capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        string_field(oracle, "capture_schema_version", context),
        "{context} reading-order fixture capture schema disagrees with metadata"
    );
    assert_eq!(
        string_field(&capture, "fixture_id", context),
        E2E_READING_ORDER_FIXTURE_ID,
        "{context} reading-order capture fixture identifier changed without review"
    );

    let capture_input = object_field(&capture, "input", context);
    assert_eq!(
        string_field(capture_input, "png_sha256", context),
        E2E_READING_ORDER_INPUT_SHA256,
        "{context} reading-order capture PNG hash changed without review"
    );
    assert_eq!(
        unsigned_field(capture_input, "png_byte_length", context),
        8_988,
        "{context} reading-order capture PNG byte length changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_sha256", context),
        E2E_READING_ORDER_BGR_SHA256,
        "{context} reading-order capture BGR hash changed without review"
    );
    assert_eq!(
        value_field(capture_input, "bgr_shape", context),
        &serde_json::json!([320, 800, 3]),
        "{context} reading-order capture BGR shape changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_channel_order", context),
        "BGR",
        "{context} reading-order capture BGR channel order changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_dtype", context),
        "uint8",
        "{context} reading-order capture BGR dtype changed without review"
    );
    let renderer = object_field(capture_input, "renderer", context);
    for (field, expected) in [
        ("kind", "cv2.putText"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("font_face", "FONT_HERSHEY_SIMPLEX"),
        ("line_type", "LINE_AA"),
    ] {
        assert_eq!(
            string_field(renderer, field, context),
            expected,
            "{context} reading-order renderer field {field} changed without review"
        );
    }
    assert_eq!(
        value_field(renderer, "canvas_bgr", context),
        &serde_json::json!([255, 255, 255]),
        "{context} reading-order renderer canvas changed without review"
    );
    assert_eq!(
        value_field(renderer, "foreground_bgr", context),
        &serde_json::json!([0, 0, 0]),
        "{context} reading-order renderer foreground changed without review"
    );
    assert_eq!(
        value_field(renderer, "font_asset_bundled", context).as_bool(),
        Some(false),
        "{context} reading-order fixture must not bundle a font asset"
    );
    let layout = array_field(renderer, "layout", context);
    let expected_layout = [
        ("Hello", serde_json::json!([40, 120]), 2.0, 4),
        ("World", serde_json::json!([510, 120]), 2.0, 4),
        ("Rust", serde_json::json!([40, 280]), 2.0, 4),
        ("OCR", serde_json::json!([510, 280]), 2.0, 4),
    ];
    assert_eq!(
        layout.len(),
        expected_layout.len(),
        "{context} reading-order renderer layout count changed without review"
    );
    for (entry, (text, origin, scale, thickness)) in layout.iter().zip(expected_layout) {
        assert_eq!(
            string_field(entry, "text", context),
            text,
            "{context} reading-order renderer text changed without review"
        );
        assert_eq!(
            value_field(entry, "origin", context),
            &origin,
            "{context} reading-order renderer origin changed without review"
        );
        assert_eq!(
            value_field(entry, "scale", context).as_f64(),
            Some(scale),
            "{context} reading-order renderer scale changed without review"
        );
        assert_eq!(
            unsigned_field(entry, "thickness", context),
            thickness,
            "{context} reading-order renderer thickness changed without review"
        );
    }
    let encoding = object_field(capture_input, "encoding", context);
    assert_eq!(
        string_field(encoding, "operation", context),
        "cv2.imencode('.png', image, [cv2.IMWRITE_PNG_COMPRESSION, 9])",
        "{context} reading-order PNG encoder changed without review"
    );
    assert_eq!(
        string_field(encoding, "round_trip_operation", context),
        "cv2.imdecode(encoded, cv2.IMREAD_COLOR)",
        "{context} reading-order PNG round-trip operation changed without review"
    );
    assert_eq!(
        value_field(encoding, "bgr_round_trip_equal", context).as_bool(),
        Some(true),
        "{context} reading-order PNG must round-trip to the rendered BGR input"
    );

    let capture_upstream = object_field(&capture, "upstream", context);
    assert_eq!(
        string_field(capture_upstream, "repository", context),
        "https://github.com/PaddlePaddle/PaddleOCR.git",
        "{context} reading-order capture upstream repository changed without review"
    );
    assert_eq!(
        string_field(capture_upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} reading-order capture upstream baseline changed without review"
    );
    for field in ["status_before", "status_after"] {
        assert_eq!(
            string_field(capture_upstream, field, context),
            "clean",
            "{context} reading-order capture upstream {field} must be clean"
        );
    }

    let capture_artifacts = object_field(&capture, "artifacts", context);
    assert_eq!(
        string_field(capture_artifacts, "terms_review", context),
        "LIC-001",
        "{context} reading-order capture must identify its terms review"
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    assert_eq!(
        string_field(
            object_field(capture_artifacts, "dictionary", context),
            "sha256",
            context
        ),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} reading-order capture dictionary hash changed without review"
    );

    let execution = object_field(&capture, "execution", context);
    for (field, expected) in [
        ("python", "3.12.3"),
        ("paddlepaddle", "3.3.1"),
        (
            "paddle_inference",
            "not invoked; use_onnx=true selected ONNX Runtime",
        ),
        ("onnxruntime", "1.28.0"),
        ("selected_execution_provider", "CPUExecutionProvider"),
        ("numpy", "1.26.4"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("gpu", "disabled"),
    ] {
        assert_eq!(
            string_field(execution, field, context),
            expected,
            "{context} reading-order execution field {field} changed without review"
        );
    }
    let session_options = object_field(execution, "onnx_session_options", context);
    assert_eq!(
        unsigned_field(session_options, "intra_op_num_threads", context),
        1,
        "{context} reading-order capture must pin one intra-op thread"
    );
    assert_eq!(
        unsigned_field(session_options, "inter_op_num_threads", context),
        1,
        "{context} reading-order capture must pin one inter-op thread"
    );
    assert_eq!(
        value_field(session_options, "enable_mem_pattern", context).as_bool(),
        Some(false),
        "{context} reading-order capture must disable ONNX Runtime memory patterns"
    );
    let classic_options = object_field(execution, "classic_options", context);
    for field in [
        "use_gpu",
        "use_onnx",
        "use_angle_cls",
        "benchmark",
        "show_log",
        "cls_argument",
    ] {
        assert_eq!(
            value_field(classic_options, field, context).as_bool(),
            Some(field == "use_onnx"),
            "{context} reading-order classic option {field} changed without review"
        );
    }

    let reproducibility = object_field(&capture, "reproducibility", context);
    assert_eq!(
        value_field(reproducibility, "harness_retained_in_repository", context).as_bool(),
        Some(false),
        "{context} must not retain the external reading-order capture harness"
    );
    assert_eq!(
        value_field(reproducibility, "fresh_process_stdout_identical", context).as_bool(),
        Some(true),
        "{context} reading-order capture fresh-process outputs must agree"
    );
    let fresh_runs = array_field(reproducibility, "fresh_process_runs", context);
    assert_eq!(
        fresh_runs.len(),
        2,
        "{context} reading-order capture must retain exactly two fresh-process digests"
    );
    for (index, run) in fresh_runs.iter().enumerate() {
        assert_eq!(
            string_field(run, "id", context),
            format!("run-{}", index + 1),
            "{context} reading-order fresh run identifier changed without review"
        );
        assert_eq!(
            string_field(run, "stdout_sha256", context),
            E2E_READING_ORDER_FRESH_OUTPUT_SHA256,
            "{context} reading-order fresh-run stdout hash changed without review"
        );
    }

    let source_result = object_field(&capture, "source_result", context);
    let source_record = object_field(source_result, "record", context);
    assert_sha256_format(
        string_field(source_result, "canonical_json_sha256", context),
        "source_result.canonical_json_sha256",
        context,
    );
    assert_eq!(
        string_field(source_result, "canonical_json_sha256", context),
        E2E_READING_ORDER_SOURCE_RECORD_SHA256,
        "{context} reading-order source record digest changed without review"
    );
    assert_eq!(
        string_field(source_record, "fixture_id", context),
        E2E_READING_ORDER_FIXTURE_ID,
        "{context} reading-order source-result identifier changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_png_sha256", context),
        E2E_READING_ORDER_INPUT_SHA256,
        "{context} reading-order source-result PNG hash changed without review"
    );
    assert_eq!(
        unsigned_field(source_record, "input_png_byte_length", context),
        8_988,
        "{context} reading-order source-result PNG byte length changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_bgr_sha256", context),
        E2E_READING_ORDER_BGR_SHA256,
        "{context} reading-order source-result BGR hash changed without review"
    );
    assert_eq!(
        value_field(source_record, "input_bgr_shape", context),
        &serde_json::json!([320, 800, 3]),
        "{context} reading-order source-result BGR shape changed without review"
    );
    for (field, expected) in [
        ("detector_sha256", E2E_NO_TEXT_DETECTOR_SHA256),
        ("recognizer_sha256", E2E_NO_TEXT_RECOGNIZER_SHA256),
        ("dictionary_sha256", E2E_NO_TEXT_DICTIONARY_SHA256),
    ] {
        assert_eq!(
            string_field(source_record, field, context),
            expected,
            "{context} reading-order source-result field {field} changed without review"
        );
    }
    for field in [
        "raw_detector_tensors_retained",
        "raw_recognizer_tensors_retained",
        "timing_values_retained",
    ] {
        assert_eq!(
            value_field(source_result, field, context).as_bool(),
            Some(false),
            "{context} reading-order source result must not retain {field}"
        );
    }

    let expected_bytes = read_fixture_file(fixture_directory, "expected.json", context);
    assert_digest(
        &expected_bytes,
        E2E_READING_ORDER_EXPECTED_SHA256,
        &format!("{context} reading-order expected result"),
    );
    let expected = parse_json_bytes(
        &expected_bytes,
        &format!("{context} reading-order expected result"),
    );
    assert_eq!(
        string_field(&expected, "schema_version", context),
        "paddleocr-rust/ocr-result/v1",
        "{context} reading-order expected result schema changed without review"
    );
    let expected_input = object_field(&expected, "input", context);
    assert!(
        value_field(expected_input, "id", context).is_null(),
        "{context} reading-order expected input identifier must remain null"
    );
    assert!(
        value_field(expected_input, "page_index", context).is_null(),
        "{context} reading-order expected page index must remain null"
    );
    assert_eq!(
        unsigned_field(expected_input, "width", context),
        800,
        "{context} reading-order expected width changed without review"
    );
    assert_eq!(
        unsigned_field(expected_input, "height", context),
        320,
        "{context} reading-order expected height changed without review"
    );
    let expected_models = object_field(&expected, "models", context);
    let expected_detector = object_field(expected_models, "detector", context);
    let expected_recognizer = object_field(expected_models, "recognizer", context);
    assert_eq!(
        string_field(expected_detector, "family", context),
        "PP-OCRv6_medium_det",
        "{context} reading-order expected detector family changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "version", context),
        format!("m2-onnx-det-v6-medium@{E2E_NO_TEXT_DETECTOR_REVISION}"),
        "{context} reading-order expected detector provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "artifact_sha256", context),
        E2E_NO_TEXT_DETECTOR_SHA256,
        "{context} reading-order expected detector hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "family", context),
        "PP-OCRv6_medium_rec",
        "{context} reading-order expected recognizer family changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "version", context),
        format!("m2-onnx-rec-v6-medium@{E2E_NO_TEXT_RECOGNIZER_REVISION}"),
        "{context} reading-order expected recognizer provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "artifact_sha256", context),
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        "{context} reading-order expected recognizer hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "dictionary_sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} reading-order expected dictionary hash changed without review"
    );

    let lines = array_field(&expected, "lines", context);
    let expected_lines = [
        (
            "Hello",
            serde_json::json!([[33, 61], [195, 63], [194, 133], [32, 131]]),
        ),
        (
            "World",
            serde_json::json!([[503, 64], [682, 64], [682, 132], [503, 132]]),
        ),
        (
            "Rust",
            serde_json::json!([[33, 224], [186, 224], [186, 294], [33, 294]]),
        ),
        (
            "OCR",
            serde_json::json!([[504, 224], [644, 224], [644, 292], [504, 292]]),
        ),
    ];
    assert_eq!(
        lines.len(),
        expected_lines.len(),
        "{context} reading-order expected line count changed without review"
    );
    for (line, (text, quad)) in lines.iter().zip(expected_lines) {
        assert_eq!(
            string_field(line, "text", context),
            text,
            "{context} reading-order text order changed without review"
        );
        assert_eq!(
            value_field(line, "quad", context),
            &quad,
            "{context} reading-order quadrilateral changed without review"
        );
        let confidence = match value_field(line, "confidence", context).as_f64() {
            Some(value) => value,
            None => panic!("{context} reading-order confidence must be a JSON number"),
        };
        assert!(
            (0.0..=1.0).contains(&confidence),
            "{context} reading-order confidence must remain in the closed unit interval"
        );
    }
    assert_eq!(
        value_field(source_record, "lines", context),
        value_field(&expected, "lines", context),
        "{context} reading-order source result and expected result differ"
    );
}

fn verify_e2e_tall_crop_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    assert_eq!(
        string_field(input, "path", context),
        "input.png",
        "{context} tall-crop fixture input path changed without review"
    );
    assert_eq!(
        string_field(input, "sha256", context),
        E2E_TALL_CROP_INPUT_SHA256,
        "{context} tall-crop fixture input hash changed without review"
    );

    let artifacts = object_field(metadata, "artifacts", context);
    assert_eq!(
        string_field(artifacts, "representation", context),
        "onnx",
        "{context} tall-crop fixture must retain the reviewed ONNX representation"
    );
    assert_eq!(
        string_field(artifacts, "terms_review", context),
        "LIC-001",
        "{context} tall-crop fixture must identify its terms review"
    );
    assert_eq!(
        value_field(artifacts, "local_only_candidate", context).as_bool(),
        Some(true),
        "{context} tall-crop fixture must remain a local-only candidate"
    );
    verify_e2e_candidate(
        object_field(artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    let dictionary = object_field(artifacts, "dictionary", context);
    assert_eq!(
        string_field(dictionary, "source_path", context),
        "ppocr/utils/dict/ppocrv6_dict.txt",
        "{context} tall-crop fixture dictionary source changed without review"
    );
    assert_eq!(
        string_field(dictionary, "sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} tall-crop fixture dictionary hash changed without review"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "capture_path", context),
        "capture.json",
        "{context} tall-crop fixture capture path changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_schema_version", context),
        "paddleocr-rust/classic-onnx-oracle-capture/v1",
        "{context} tall-crop fixture capture schema changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        E2E_TALL_CROP_CAPTURE_SHA256,
        "{context} tall-crop fixture capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(
        fixture_directory,
        string_field(oracle, "capture_path", context),
        context,
    );
    assert_digest(
        &capture_bytes,
        E2E_TALL_CROP_CAPTURE_SHA256,
        &format!("{context} tall-crop fixture capture document"),
    );
    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} tall-crop fixture capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        string_field(oracle, "capture_schema_version", context),
        "{context} tall-crop fixture capture schema disagrees with metadata"
    );
    assert_eq!(
        string_field(&capture, "fixture_id", context),
        E2E_TALL_CROP_FIXTURE_ID,
        "{context} tall-crop capture fixture identifier changed without review"
    );

    let capture_input = object_field(&capture, "input", context);
    assert_eq!(
        string_field(capture_input, "png_sha256", context),
        E2E_TALL_CROP_INPUT_SHA256,
        "{context} tall-crop capture PNG hash changed without review"
    );
    assert_eq!(
        unsigned_field(capture_input, "png_byte_length", context),
        6_913,
        "{context} tall-crop capture PNG byte length changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_sha256", context),
        E2E_TALL_CROP_BGR_SHA256,
        "{context} tall-crop capture BGR hash changed without review"
    );
    assert_eq!(
        value_field(capture_input, "bgr_shape", context),
        &serde_json::json!([900, 360, 3]),
        "{context} tall-crop capture BGR shape changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_channel_order", context),
        "BGR",
        "{context} tall-crop capture BGR channel order changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_dtype", context),
        "uint8",
        "{context} tall-crop capture BGR dtype changed without review"
    );
    let renderer = object_field(capture_input, "renderer", context);
    for (field, expected) in [
        ("kind", "cv2.putText+cv2.rotate"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("font_face", "FONT_HERSHEY_SIMPLEX"),
        ("line_type", "LINE_AA"),
        ("text", "Rust"),
        ("rotation", "cv2.ROTATE_90_CLOCKWISE"),
    ] {
        assert_eq!(
            string_field(renderer, field, context),
            expected,
            "{context} tall-crop renderer field {field} changed without review"
        );
    }
    assert_eq!(
        value_field(renderer, "horizontal_canvas_shape", context),
        &serde_json::json!([220, 760, 3]),
        "{context} tall-crop horizontal canvas shape changed without review"
    );
    assert_eq!(
        value_field(renderer, "horizontal_canvas_bgr", context),
        &serde_json::json!([255, 255, 255]),
        "{context} tall-crop horizontal canvas color changed without review"
    );
    assert_eq!(
        value_field(renderer, "text_origin", context),
        &serde_json::json!([20, 170]),
        "{context} tall-crop text origin changed without review"
    );
    assert_eq!(
        value_field(renderer, "text_scale", context).as_f64(),
        Some(4.0),
        "{context} tall-crop text scale changed without review"
    );
    assert_eq!(
        unsigned_field(renderer, "text_thickness", context),
        8,
        "{context} tall-crop text thickness changed without review"
    );
    assert_eq!(
        value_field(renderer, "outer_canvas_shape", context),
        &serde_json::json!([900, 360, 3]),
        "{context} tall-crop outer canvas shape changed without review"
    );
    assert_eq!(
        value_field(renderer, "outer_canvas_bgr", context),
        &serde_json::json!([255, 255, 255]),
        "{context} tall-crop outer canvas color changed without review"
    );
    assert_eq!(
        value_field(renderer, "placement_top_left", context),
        &serde_json::json!([80, 70]),
        "{context} tall-crop placement changed without review"
    );
    assert_eq!(
        value_field(renderer, "font_asset_bundled", context).as_bool(),
        Some(false),
        "{context} tall-crop fixture must not bundle a font asset"
    );
    let encoding = object_field(capture_input, "encoding", context);
    assert_eq!(
        string_field(encoding, "operation", context),
        "cv2.imencode('.png', image, [cv2.IMWRITE_PNG_COMPRESSION, 9])",
        "{context} tall-crop PNG encoder changed without review"
    );
    assert_eq!(
        string_field(encoding, "round_trip_operation", context),
        "cv2.imdecode(encoded, cv2.IMREAD_COLOR)",
        "{context} tall-crop PNG round-trip operation changed without review"
    );
    assert_eq!(
        value_field(encoding, "bgr_round_trip_equal", context).as_bool(),
        Some(true),
        "{context} tall-crop PNG must round-trip to the rendered BGR input"
    );

    let capture_upstream = object_field(&capture, "upstream", context);
    assert_eq!(
        string_field(capture_upstream, "repository", context),
        "https://github.com/PaddlePaddle/PaddleOCR.git",
        "{context} tall-crop capture upstream repository changed without review"
    );
    assert_eq!(
        string_field(capture_upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} tall-crop capture upstream baseline changed without review"
    );
    for field in ["status_before", "status_after"] {
        assert_eq!(
            string_field(capture_upstream, field, context),
            "clean",
            "{context} tall-crop capture upstream {field} must be clean"
        );
    }
    assert!(
        array_field(capture_upstream, "reference_paths", context)
            .iter()
            .any(|value| {
                value_as_str(value, "tall-crop upstream reference path", context)
                    == "tools/infer/utility.py:get_rotate_crop_image"
            }),
        "{context} tall-crop capture must name the classic crop implementation"
    );

    let capture_artifacts = object_field(&capture, "artifacts", context);
    assert_eq!(
        string_field(capture_artifacts, "terms_review", context),
        "LIC-001",
        "{context} tall-crop capture must identify its terms review"
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    assert_eq!(
        string_field(
            object_field(capture_artifacts, "dictionary", context),
            "sha256",
            context
        ),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} tall-crop capture dictionary hash changed without review"
    );

    let execution = object_field(&capture, "execution", context);
    for (field, expected) in [
        ("python", "3.12.3"),
        ("paddlepaddle", "3.3.1"),
        (
            "paddle_inference",
            "not invoked; use_onnx=true selected ONNX Runtime",
        ),
        ("onnxruntime", "1.28.0"),
        ("selected_execution_provider", "CPUExecutionProvider"),
        ("numpy", "1.26.4"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("gpu", "disabled"),
    ] {
        assert_eq!(
            string_field(execution, field, context),
            expected,
            "{context} tall-crop execution field {field} changed without review"
        );
    }
    let session_options = object_field(execution, "onnx_session_options", context);
    assert_eq!(
        unsigned_field(session_options, "intra_op_num_threads", context),
        1,
        "{context} tall-crop capture must pin one intra-op thread"
    );
    assert_eq!(
        unsigned_field(session_options, "inter_op_num_threads", context),
        1,
        "{context} tall-crop capture must pin one inter-op thread"
    );
    assert_eq!(
        value_field(session_options, "enable_mem_pattern", context).as_bool(),
        Some(false),
        "{context} tall-crop capture must disable ONNX Runtime memory patterns"
    );
    let classic_options = object_field(execution, "classic_options", context);
    for field in [
        "use_gpu",
        "use_onnx",
        "use_angle_cls",
        "benchmark",
        "show_log",
        "cls_argument",
    ] {
        assert_eq!(
            value_field(classic_options, field, context).as_bool(),
            Some(field == "use_onnx"),
            "{context} tall-crop classic option {field} changed without review"
        );
    }

    let reproducibility = object_field(&capture, "reproducibility", context);
    assert_eq!(
        value_field(reproducibility, "harness_retained_in_repository", context).as_bool(),
        Some(false),
        "{context} must not retain the external tall-crop capture harness"
    );
    assert_eq!(
        value_field(reproducibility, "fresh_process_stdout_identical", context).as_bool(),
        Some(true),
        "{context} tall-crop capture fresh-process outputs must agree"
    );
    let fresh_runs = array_field(reproducibility, "fresh_process_runs", context);
    assert_eq!(
        fresh_runs.len(),
        2,
        "{context} tall-crop capture must retain exactly two fresh-process digests"
    );
    for (index, run) in fresh_runs.iter().enumerate() {
        assert_eq!(
            string_field(run, "id", context),
            format!("run-{}", index + 1),
            "{context} tall-crop fresh run identifier changed without review"
        );
        assert_eq!(
            string_field(run, "stdout_sha256", context),
            E2E_TALL_CROP_FRESH_OUTPUT_SHA256,
            "{context} tall-crop fresh-run stdout hash changed without review"
        );
    }

    let source_result = object_field(&capture, "source_result", context);
    let source_record = object_field(source_result, "record", context);
    assert_sha256_format(
        string_field(source_result, "canonical_json_sha256", context),
        "source_result.canonical_json_sha256",
        context,
    );
    assert_eq!(
        string_field(source_result, "canonical_json_sha256", context),
        E2E_TALL_CROP_SOURCE_RECORD_SHA256,
        "{context} tall-crop source record digest changed without review"
    );
    assert_eq!(
        string_field(source_record, "fixture_id", context),
        E2E_TALL_CROP_FIXTURE_ID,
        "{context} tall-crop source-result identifier changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_png_sha256", context),
        E2E_TALL_CROP_INPUT_SHA256,
        "{context} tall-crop source-result PNG hash changed without review"
    );
    assert_eq!(
        unsigned_field(source_record, "input_png_byte_length", context),
        6_913,
        "{context} tall-crop source-result PNG byte length changed without review"
    );
    assert_eq!(
        string_field(source_record, "input_bgr_sha256", context),
        E2E_TALL_CROP_BGR_SHA256,
        "{context} tall-crop source-result BGR hash changed without review"
    );
    assert_eq!(
        value_field(source_record, "input_bgr_shape", context),
        &serde_json::json!([900, 360, 3]),
        "{context} tall-crop source-result BGR shape changed without review"
    );
    for (field, expected) in [
        ("detector_sha256", E2E_NO_TEXT_DETECTOR_SHA256),
        ("recognizer_sha256", E2E_NO_TEXT_RECOGNIZER_SHA256),
        ("dictionary_sha256", E2E_NO_TEXT_DICTIONARY_SHA256),
    ] {
        assert_eq!(
            string_field(source_record, field, context),
            expected,
            "{context} tall-crop source-result field {field} changed without review"
        );
    }
    let crop_diagnostics = array_field(source_record, "crop_diagnostics", context);
    assert_eq!(
        crop_diagnostics.len(),
        1,
        "{context} tall-crop source result must contain one crop diagnostic"
    );
    let crop_diagnostic = &crop_diagnostics[0];
    let pre_rotation_shape = array_field(crop_diagnostic, "pre_rotation_shape", context);
    assert_eq!(
        value_field(crop_diagnostic, "pre_rotation_shape", context),
        &serde_json::json!([307, 145, 3]),
        "{context} tall-crop pre-rotation shape changed without review"
    );
    assert_eq!(
        value_field(crop_diagnostic, "rotation_applied", context).as_bool(),
        Some(true),
        "{context} tall-crop rotation branch must remain applied"
    );
    assert_eq!(
        value_field(crop_diagnostic, "post_rotation_shape", context),
        &serde_json::json!([145, 307, 3]),
        "{context} tall-crop post-rotation shape changed without review"
    );
    let pre_rotation_height = match pre_rotation_shape[0].as_u64() {
        Some(value) => value,
        None => panic!("{context} tall-crop pre-rotation height must be an unsigned integer"),
    };
    let pre_rotation_width = match pre_rotation_shape[1].as_u64() {
        Some(value) => value,
        None => panic!("{context} tall-crop pre-rotation width must be an unsigned integer"),
    };
    let twice_height = match pre_rotation_height.checked_mul(2) {
        Some(value) => value,
        None => panic!("{context} tall-crop pre-rotation height multiplication overflows"),
    };
    let three_times_width = match pre_rotation_width.checked_mul(3) {
        Some(value) => value,
        None => panic!("{context} tall-crop pre-rotation width multiplication overflows"),
    };
    assert!(
        twice_height >= three_times_width,
        "{context} tall-crop pre-rotation dimensions must satisfy the classic branch"
    );
    for field in [
        "raw_detector_tensors_retained",
        "raw_recognizer_tensors_retained",
        "timing_values_retained",
    ] {
        assert_eq!(
            value_field(source_result, field, context).as_bool(),
            Some(false),
            "{context} tall-crop source result must not retain {field}"
        );
    }

    let expected_bytes = read_fixture_file(fixture_directory, "expected.json", context);
    assert_digest(
        &expected_bytes,
        E2E_TALL_CROP_EXPECTED_SHA256,
        &format!("{context} tall-crop expected result"),
    );
    let expected = parse_json_bytes(
        &expected_bytes,
        &format!("{context} tall-crop expected result"),
    );
    assert_eq!(
        string_field(&expected, "schema_version", context),
        "paddleocr-rust/ocr-result/v1",
        "{context} tall-crop expected result schema changed without review"
    );
    let expected_input = object_field(&expected, "input", context);
    assert!(
        value_field(expected_input, "id", context).is_null(),
        "{context} tall-crop expected input identifier must remain null"
    );
    assert!(
        value_field(expected_input, "page_index", context).is_null(),
        "{context} tall-crop expected page index must remain null"
    );
    assert_eq!(
        unsigned_field(expected_input, "width", context),
        360,
        "{context} tall-crop expected width changed without review"
    );
    assert_eq!(
        unsigned_field(expected_input, "height", context),
        900,
        "{context} tall-crop expected height changed without review"
    );
    let expected_models = object_field(&expected, "models", context);
    let expected_detector = object_field(expected_models, "detector", context);
    let expected_recognizer = object_field(expected_models, "recognizer", context);
    assert_eq!(
        string_field(expected_detector, "family", context),
        "PP-OCRv6_medium_det",
        "{context} tall-crop expected detector family changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "version", context),
        format!("m2-onnx-det-v6-medium@{E2E_NO_TEXT_DETECTOR_REVISION}"),
        "{context} tall-crop expected detector provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "artifact_sha256", context),
        E2E_NO_TEXT_DETECTOR_SHA256,
        "{context} tall-crop expected detector hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "family", context),
        "PP-OCRv6_medium_rec",
        "{context} tall-crop expected recognizer family changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "version", context),
        format!("m2-onnx-rec-v6-medium@{E2E_NO_TEXT_RECOGNIZER_REVISION}"),
        "{context} tall-crop expected recognizer provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "artifact_sha256", context),
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        "{context} tall-crop expected recognizer hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "dictionary_sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} tall-crop expected dictionary hash changed without review"
    );
    let lines = array_field(&expected, "lines", context);
    assert_eq!(
        lines.len(),
        1,
        "{context} tall-crop expected line count changed without review"
    );
    let line = &lines[0];
    assert_eq!(
        string_field(line, "text", context),
        "Rust",
        "{context} tall-crop recognized text changed without review"
    );
    assert_eq!(
        value_field(line, "quad", context),
        &serde_json::json!([[96, 76], [241, 74], [245, 381], [100, 383]]),
        "{context} tall-crop quadrilateral changed without review"
    );
    let confidence = match value_field(line, "confidence", context).as_f64() {
        Some(value) => value,
        None => panic!("{context} tall-crop confidence must be a JSON number"),
    };
    assert!(
        (0.0..=1.0).contains(&confidence),
        "{context} tall-crop confidence must remain in the closed unit interval"
    );
    assert_eq!(
        value_field(source_record, "lines", context),
        value_field(&expected, "lines", context),
        "{context} tall-crop source result and expected result differ"
    );
}

fn verify_e2e_unicode_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let input = object_field(metadata, "input", context);
    assert_eq!(
        string_field(input, "path", context),
        "input.png",
        "{context} Unicode fixture input path changed without review"
    );
    assert_eq!(
        string_field(input, "sha256", context),
        E2E_UNICODE_INPUT_SHA256,
        "{context} Unicode fixture input hash changed without review"
    );

    let artifacts = object_field(metadata, "artifacts", context);
    assert_eq!(
        string_field(artifacts, "representation", context),
        "onnx",
        "{context} Unicode fixture must retain the reviewed ONNX representation"
    );
    assert_eq!(
        string_field(artifacts, "terms_review", context),
        "LIC-001",
        "{context} Unicode fixture must identify its terms review"
    );
    assert_eq!(
        value_field(artifacts, "local_only_candidate", context).as_bool(),
        Some(true),
        "{context} Unicode fixture must remain a local-only candidate"
    );
    verify_e2e_candidate(
        object_field(artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    let dictionary = object_field(artifacts, "dictionary", context);
    assert_eq!(
        string_field(dictionary, "source_path", context),
        "ppocr/utils/dict/ppocrv6_dict.txt",
        "{context} Unicode fixture dictionary source changed without review"
    );
    assert_eq!(
        string_field(dictionary, "sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} Unicode fixture dictionary hash changed without review"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "capture_path", context),
        "capture.json",
        "{context} Unicode fixture capture path changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_schema_version", context),
        "paddleocr-rust/classic-onnx-oracle-capture/v1",
        "{context} Unicode fixture capture schema changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        E2E_UNICODE_CAPTURE_SHA256,
        "{context} Unicode fixture capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(
        fixture_directory,
        string_field(oracle, "capture_path", context),
        context,
    );
    assert_digest(
        &capture_bytes,
        E2E_UNICODE_CAPTURE_SHA256,
        &format!("{context} Unicode fixture capture document"),
    );
    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} Unicode fixture capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        string_field(oracle, "capture_schema_version", context),
        "{context} Unicode fixture capture schema disagrees with metadata"
    );
    assert_eq!(
        string_field(&capture, "fixture_id", context),
        E2E_UNICODE_FIXTURE_ID,
        "{context} Unicode capture fixture identifier changed without review"
    );

    let capture_input = object_field(&capture, "input", context);
    assert_eq!(
        string_field(capture_input, "png_sha256", context),
        E2E_UNICODE_INPUT_SHA256,
        "{context} Unicode capture PNG hash changed without review"
    );
    assert_eq!(
        unsigned_field(capture_input, "png_byte_length", context),
        9_151,
        "{context} Unicode capture PNG byte length changed without review"
    );
    assert_eq!(
        string_field(capture_input, "bgr_sha256", context),
        E2E_UNICODE_BGR_SHA256,
        "{context} Unicode capture BGR hash changed without review"
    );
    assert_eq!(
        value_field(capture_input, "bgr_shape", context),
        &serde_json::json!([320, 800, 3]),
        "{context} Unicode capture BGR shape changed without review"
    );
    for (field, expected) in [("bgr_channel_order", "BGR"), ("bgr_dtype", "uint8")] {
        assert_eq!(
            string_field(capture_input, field, context),
            expected,
            "{context} Unicode capture input field {field} changed without review"
        );
    }

    let renderer = object_field(capture_input, "renderer", context);
    for (field, expected) in [
        ("kind", "PIL.ImageDraw.text+cv2.cvtColor"),
        ("pillow", "12.3.0"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("text", "你好"),
        ("text_utf8_hex", "e4bda0e5a5bd"),
        (
            "rgb_to_bgr_operation",
            "cv2.cvtColor(numpy.asarray(image), cv2.COLOR_RGB2BGR)",
        ),
    ] {
        assert_eq!(
            string_field(renderer, field, context),
            expected,
            "{context} Unicode renderer field {field} changed without review"
        );
    }
    assert_eq!(
        string_field(renderer, "text", context).as_bytes(),
        &[0xe4_u8, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd],
        "{context} Unicode renderer text must retain its reviewed UTF-8 bytes"
    );
    assert_eq!(
        value_field(renderer, "text_codepoints", context),
        &serde_json::json!(["U+4F60", "U+597D"]),
        "{context} Unicode renderer code points changed without review"
    );
    assert_eq!(
        value_field(renderer, "canvas_rgb_shape", context),
        &serde_json::json!([320, 800, 3]),
        "{context} Unicode renderer canvas shape changed without review"
    );
    assert_eq!(
        value_field(renderer, "canvas_rgb", context),
        &serde_json::json!([255, 255, 255]),
        "{context} Unicode renderer canvas color changed without review"
    );
    assert_eq!(
        value_field(renderer, "text_origin", context),
        &serde_json::json!([40, 45]),
        "{context} Unicode renderer text origin changed without review"
    );
    assert_eq!(
        unsigned_field(renderer, "font_size", context),
        128,
        "{context} Unicode renderer font size changed without review"
    );
    assert_eq!(
        value_field(renderer, "foreground_rgb", context),
        &serde_json::json!([0, 0, 0]),
        "{context} Unicode renderer foreground color changed without review"
    );
    let font = object_field(renderer, "font", context);
    for (field, expected) in [
        ("name", "Noto Sans CJK"),
        (
            "external_path",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ),
        (
            "sha256",
            "b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a",
        ),
        ("license", "OFL-1.1"),
        (
            "external_license_record_path",
            "/usr/share/doc/fonts-noto-cjk/copyright",
        ),
        (
            "external_license_record_sha256",
            "849f4ea9c214fa4ac3593b770c699f387534b11ce671264c1b10d85bdcb5997b",
        ),
    ] {
        assert_eq!(
            string_field(font, field, context),
            expected,
            "{context} Unicode renderer font field {field} changed without review"
        );
    }
    assert_eq!(
        unsigned_field(font, "collection_index", context),
        0,
        "{context} Unicode renderer font collection index changed without review"
    );
    assert_eq!(
        value_field(font, "bundled", context).as_bool(),
        Some(false),
        "{context} Unicode fixture must not bundle a font asset"
    );
    let encoding = object_field(capture_input, "encoding", context);
    assert_eq!(
        string_field(encoding, "operation", context),
        "cv2.imencode('.png', image, [cv2.IMWRITE_PNG_COMPRESSION, 9])",
        "{context} Unicode PNG encoder changed without review"
    );
    assert_eq!(
        string_field(encoding, "round_trip_operation", context),
        "cv2.imdecode(encoded, cv2.IMREAD_COLOR)",
        "{context} Unicode PNG round-trip operation changed without review"
    );
    assert_eq!(
        value_field(encoding, "bgr_round_trip_equal", context).as_bool(),
        Some(true),
        "{context} Unicode PNG must round-trip to the rendered BGR input"
    );

    let capture_upstream = object_field(&capture, "upstream", context);
    assert_eq!(
        string_field(capture_upstream, "repository", context),
        "https://github.com/PaddlePaddle/PaddleOCR.git",
        "{context} Unicode capture upstream repository changed without review"
    );
    assert_eq!(
        string_field(capture_upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} Unicode capture upstream baseline changed without review"
    );
    for field in ["status_before", "status_after"] {
        assert_eq!(
            string_field(capture_upstream, field, context),
            "clean",
            "{context} Unicode capture upstream {field} must be clean"
        );
    }
    assert!(
        array_field(capture_upstream, "reference_paths", context)
            .iter()
            .any(|value| {
                value_as_str(value, "Unicode upstream reference path", context)
                    == "ppocr/postprocess/rec_postprocess.py"
            }),
        "{context} Unicode capture must name the classic recognition postprocessor"
    );

    let capture_artifacts = object_field(&capture, "artifacts", context);
    assert_eq!(
        string_field(capture_artifacts, "terms_review", context),
        "LIC-001",
        "{context} Unicode capture must identify its terms review"
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "detector", context),
        "m2-onnx-det-v6-medium",
        E2E_NO_TEXT_DETECTOR_REVISION,
        E2E_NO_TEXT_DETECTOR_SHA256,
        context,
    );
    verify_e2e_candidate(
        object_field(capture_artifacts, "recognizer", context),
        "m2-onnx-rec-v6-medium",
        E2E_NO_TEXT_RECOGNIZER_REVISION,
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        context,
    );
    assert_eq!(
        string_field(
            object_field(capture_artifacts, "dictionary", context),
            "sha256",
            context
        ),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} Unicode capture dictionary hash changed without review"
    );

    let execution = object_field(&capture, "execution", context);
    for (field, expected) in [
        ("python", "3.12.3"),
        ("pillow", "12.3.0"),
        ("paddlepaddle", "3.3.1"),
        (
            "paddle_inference",
            "not invoked; use_onnx=true selected ONNX Runtime",
        ),
        ("onnxruntime", "1.28.0"),
        ("selected_execution_provider", "CPUExecutionProvider"),
        ("numpy", "1.26.4"),
        ("opencv_python", "4.11.0.86"),
        ("opencv", "4.11.0"),
        ("gpu", "disabled"),
    ] {
        assert_eq!(
            string_field(execution, field, context),
            expected,
            "{context} Unicode execution field {field} changed without review"
        );
    }
    let session_options = object_field(execution, "onnx_session_options", context);
    for field in ["intra_op_num_threads", "inter_op_num_threads"] {
        assert_eq!(
            unsigned_field(session_options, field, context),
            1,
            "{context} Unicode capture must pin one {field}"
        );
    }
    assert_eq!(
        value_field(session_options, "enable_mem_pattern", context).as_bool(),
        Some(false),
        "{context} Unicode capture must disable ONNX Runtime memory patterns"
    );
    let classic_options = object_field(execution, "classic_options", context);
    for field in [
        "use_gpu",
        "use_onnx",
        "use_angle_cls",
        "benchmark",
        "show_log",
        "cls_argument",
    ] {
        assert_eq!(
            value_field(classic_options, field, context).as_bool(),
            Some(field == "use_onnx"),
            "{context} Unicode classic option {field} changed without review"
        );
    }

    let reproducibility = object_field(&capture, "reproducibility", context);
    assert_eq!(
        value_field(reproducibility, "harness_retained_in_repository", context).as_bool(),
        Some(false),
        "{context} must not retain the external Unicode capture harness"
    );
    assert_eq!(
        value_field(reproducibility, "fresh_process_stdout_identical", context).as_bool(),
        Some(true),
        "{context} Unicode capture fresh-process outputs must agree"
    );
    let fresh_runs = array_field(reproducibility, "fresh_process_runs", context);
    assert_eq!(
        fresh_runs.len(),
        2,
        "{context} Unicode capture must retain exactly two fresh-process digests"
    );
    for (index, run) in fresh_runs.iter().enumerate() {
        assert_eq!(
            string_field(run, "id", context),
            format!("run-{}", index + 1),
            "{context} Unicode fresh run identifier changed without review"
        );
        assert_eq!(
            string_field(run, "stdout_sha256", context),
            E2E_UNICODE_FRESH_OUTPUT_SHA256,
            "{context} Unicode fresh-run stdout hash changed without review"
        );
    }

    let source_result = object_field(&capture, "source_result", context);
    assert_eq!(
        string_field(source_result, "canonical_json_sha256", context),
        E2E_UNICODE_SOURCE_RECORD_SHA256,
        "{context} Unicode source record digest changed without review"
    );
    let source_record = object_field(source_result, "record", context);
    assert_eq!(
        string_field(source_record, "fixture_id", context),
        E2E_UNICODE_FIXTURE_ID,
        "{context} Unicode source-result identifier changed without review"
    );
    for (field, expected) in [
        ("detector_sha256", E2E_NO_TEXT_DETECTOR_SHA256),
        ("recognizer_sha256", E2E_NO_TEXT_RECOGNIZER_SHA256),
        ("dictionary_sha256", E2E_NO_TEXT_DICTIONARY_SHA256),
        (
            "font_sha256",
            "b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a",
        ),
        (
            "font_license_record_sha256",
            "849f4ea9c214fa4ac3593b770c699f387534b11ce671264c1b10d85bdcb5997b",
        ),
        ("input_png_sha256", E2E_UNICODE_INPUT_SHA256),
        ("input_bgr_sha256", E2E_UNICODE_BGR_SHA256),
    ] {
        assert_eq!(
            string_field(source_record, field, context),
            expected,
            "{context} Unicode source-result field {field} changed without review"
        );
    }
    assert_eq!(
        unsigned_field(source_record, "input_png_byte_length", context),
        9_151,
        "{context} Unicode source-result PNG byte length changed without review"
    );
    assert_eq!(
        value_field(source_record, "input_bgr_shape", context),
        &serde_json::json!([320, 800, 3]),
        "{context} Unicode source-result BGR shape changed without review"
    );
    for field in [
        "raw_detector_tensors_retained",
        "raw_recognizer_tensors_retained",
        "timing_values_retained",
    ] {
        assert_eq!(
            value_field(source_result, field, context).as_bool(),
            Some(false),
            "{context} Unicode source result must not retain {field}"
        );
    }

    let expected_bytes = read_fixture_file(fixture_directory, "expected.json", context);
    assert_digest(
        &expected_bytes,
        E2E_UNICODE_EXPECTED_SHA256,
        &format!("{context} Unicode expected result"),
    );
    let expected = parse_json_bytes(
        &expected_bytes,
        &format!("{context} Unicode expected result"),
    );
    assert_eq!(
        string_field(&expected, "schema_version", context),
        "paddleocr-rust/ocr-result/v1",
        "{context} Unicode expected result schema changed without review"
    );
    let expected_input = object_field(&expected, "input", context);
    assert!(
        value_field(expected_input, "id", context).is_null(),
        "{context} Unicode expected input identifier must remain null"
    );
    assert!(
        value_field(expected_input, "page_index", context).is_null(),
        "{context} Unicode expected page index must remain null"
    );
    assert_eq!(
        unsigned_field(expected_input, "width", context),
        800,
        "{context} Unicode expected width changed without review"
    );
    assert_eq!(
        unsigned_field(expected_input, "height", context),
        320,
        "{context} Unicode expected height changed without review"
    );
    let expected_models = object_field(&expected, "models", context);
    let expected_detector = object_field(expected_models, "detector", context);
    let expected_recognizer = object_field(expected_models, "recognizer", context);
    assert_eq!(
        string_field(expected_detector, "family", context),
        "PP-OCRv6_medium_det",
        "{context} Unicode expected detector family changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "version", context),
        format!("m2-onnx-det-v6-medium@{E2E_NO_TEXT_DETECTOR_REVISION}"),
        "{context} Unicode expected detector provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_detector, "artifact_sha256", context),
        E2E_NO_TEXT_DETECTOR_SHA256,
        "{context} Unicode expected detector hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "family", context),
        "PP-OCRv6_medium_rec",
        "{context} Unicode expected recognizer family changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "version", context),
        format!("m2-onnx-rec-v6-medium@{E2E_NO_TEXT_RECOGNIZER_REVISION}"),
        "{context} Unicode expected recognizer provenance version changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "artifact_sha256", context),
        E2E_NO_TEXT_RECOGNIZER_SHA256,
        "{context} Unicode expected recognizer hash changed without review"
    );
    assert_eq!(
        string_field(expected_recognizer, "dictionary_sha256", context),
        E2E_NO_TEXT_DICTIONARY_SHA256,
        "{context} Unicode expected dictionary hash changed without review"
    );
    let lines = array_field(&expected, "lines", context);
    assert_eq!(
        lines.len(),
        1,
        "{context} Unicode expected line count changed without review"
    );
    let line = &lines[0];
    assert_eq!(
        string_field(line, "text", context),
        "你好",
        "{context} Unicode recognized text changed without review"
    );
    assert_eq!(
        string_field(line, "text", context).as_bytes(),
        &[0xe4_u8, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd],
        "{context} Unicode recognized text must retain its reviewed UTF-8 bytes"
    );
    assert_eq!(
        value_field(line, "quad", context),
        &serde_json::json!([[17, 59], [315, 59], [315, 233], [17, 233]]),
        "{context} Unicode quadrilateral changed without review"
    );
    let confidence = match value_field(line, "confidence", context).as_f64() {
        Some(value) => value,
        None => panic!("{context} Unicode confidence must be a JSON number"),
    };
    assert!(
        (0.0..=1.0).contains(&confidence),
        "{context} Unicode confidence must remain in the closed unit interval"
    );
    assert_eq!(
        value_field(source_record, "lines", context),
        value_field(&expected, "lines", context),
        "{context} Unicode source result and expected result differ"
    );
}

fn verify_score_filter_oracle(metadata: &Value, fixture_directory: &Path, context: &str) {
    let metadata_input = object_field(metadata, "input", context);
    assert_eq!(
        string_field(metadata_input, "path", context),
        "input.json",
        "{context} score-filter input path changed without review"
    );
    assert_eq!(
        string_field(metadata_input, "sha256", context),
        SCORE_FILTER_INPUT_SHA256,
        "{context} score-filter input hash changed without review"
    );
    assert!(
        value_field(metadata, "artifacts", context).is_null(),
        "{context} score-filter oracle must not introduce a model artifact"
    );

    let oracle = object_field(metadata, "oracle", context);
    assert_eq!(
        string_field(oracle, "capture_path", context),
        "capture.json",
        "{context} score-filter capture path changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_schema_version", context),
        "paddleocr-rust/classic-score-filter-oracle-capture/v1",
        "{context} score-filter capture schema changed without review"
    );
    assert_eq!(
        string_field(oracle, "capture_sha256", context),
        SCORE_FILTER_CAPTURE_SHA256,
        "{context} score-filter capture digest changed without review"
    );
    let capture_bytes = read_fixture_file(
        fixture_directory,
        string_field(oracle, "capture_path", context),
        context,
    );
    assert_digest(
        &capture_bytes,
        SCORE_FILTER_CAPTURE_SHA256,
        &format!("{context} score-filter capture document"),
    );
    let capture = parse_json_bytes(
        &capture_bytes,
        &format!("{context} score-filter capture document"),
    );
    assert_eq!(
        string_field(&capture, "schema_version", context),
        string_field(oracle, "capture_schema_version", context),
        "{context} score-filter capture schema disagrees with metadata"
    );
    assert_eq!(
        string_field(&capture, "fixture_id", context),
        SCORE_FILTER_FIXTURE_ID,
        "{context} score-filter capture fixture identifier changed without review"
    );

    let capture_input = object_field(&capture, "input", context);
    assert_eq!(
        string_field(capture_input, "fixture_path", context),
        "input.json",
        "{context} score-filter capture input path changed without review"
    );
    assert_eq!(
        string_field(capture_input, "fixture_sha256", context),
        SCORE_FILTER_INPUT_SHA256,
        "{context} score-filter capture input hash changed without review"
    );
    assert_eq!(
        value_field(capture_input, "synthetic_image_shape", context),
        &serde_json::json!([16, 60, 3]),
        "{context} score-filter synthetic image shape changed without review"
    );
    assert_eq!(
        string_field(capture_input, "synthetic_image_dtype", context),
        "uint8",
        "{context} score-filter synthetic image dtype changed without review"
    );
    assert_eq!(
        value_field(capture_input, "drop_score", context).as_f64(),
        Some(0.5),
        "{context} score-filter threshold changed without review"
    );
    for field in ["mock_detector", "mock_crop", "mock_recognizer"] {
        assert_non_empty(string_field(capture_input, field, context), field, context);
    }

    let capture_upstream = object_field(&capture, "upstream", context);
    assert_eq!(
        string_field(capture_upstream, "repository", context),
        "https://github.com/PaddlePaddle/PaddleOCR.git",
        "{context} score-filter capture upstream repository changed without review"
    );
    assert_eq!(
        string_field(capture_upstream, "commit", context),
        UPSTREAM_BASELINE,
        "{context} score-filter capture upstream baseline changed without review"
    );
    for field in ["status_before", "status_after"] {
        assert_eq!(
            string_field(capture_upstream, field, context),
            "clean",
            "{context} score-filter capture upstream {field} must be clean"
        );
    }
    assert!(
        array_field(capture_upstream, "reference_paths", context)
            .iter()
            .any(|value| {
                value_as_str(value, "score-filter upstream reference path", context)
                    == "tools/infer/predict_system.py:TextSystem.__call__"
            }),
        "{context} score-filter capture must name the classic filter loop"
    );

    let execution = object_field(&capture, "execution", context);
    for (field, expected) in [
        ("python", "3.12.3"),
        ("paddlepaddle", "3.3.1"),
        ("numpy", "1.26.4"),
        (
            "model_inference",
            "not invoked; fake collaborators exercised only the classic filter loop",
        ),
        ("gpu", "disabled"),
    ] {
        assert_eq!(
            string_field(execution, field, context),
            expected,
            "{context} score-filter execution field {field} changed without review"
        );
    }
    let process_environment = object_field(execution, "process_environment", context);
    for field in [
        "PYTHONDONTWRITEBYTECODE",
        "PYTHONNOUSERSITE",
        "OMP_NUM_THREADS",
        "MKL_NUM_THREADS",
        "OPENBLAS_NUM_THREADS",
    ] {
        assert_eq!(
            string_field(process_environment, field, context),
            "1",
            "{context} score-filter environment {field} changed without review"
        );
    }

    let reproducibility = object_field(&capture, "reproducibility", context);
    assert_eq!(
        string_field(reproducibility, "harness_sha256", context),
        "d57d413b4561dca0227a4d07b527a3399e1b2e38221023b9837f1ac4eb13ace4",
        "{context} score-filter harness digest changed without review"
    );
    assert_eq!(
        value_field(reproducibility, "harness_retained_in_repository", context).as_bool(),
        Some(false),
        "{context} must not retain the external score-filter capture harness"
    );
    assert_eq!(
        value_field(reproducibility, "fresh_process_stdout_identical", context).as_bool(),
        Some(true),
        "{context} score-filter fresh-process outputs must agree"
    );
    let fresh_runs = array_field(reproducibility, "fresh_process_runs", context);
    assert_eq!(
        fresh_runs.len(),
        2,
        "{context} score-filter capture must retain exactly two fresh-process digests"
    );
    for (index, run) in fresh_runs.iter().enumerate() {
        assert_eq!(
            string_field(run, "id", context),
            format!("run-{}", index + 1),
            "{context} score-filter fresh run identifier changed without review"
        );
        assert_eq!(
            string_field(run, "stdout_sha256", context),
            SCORE_FILTER_FRESH_OUTPUT_SHA256,
            "{context} score-filter fresh-run stdout hash changed without review"
        );
    }

    let source_result = object_field(&capture, "source_result", context);
    assert_eq!(
        string_field(source_result, "canonical_json_sha256", context),
        SCORE_FILTER_SOURCE_RECORD_SHA256,
        "{context} score-filter source record digest changed without review"
    );
    for field in [
        "timing_values_retained",
        "raw_detector_tensors_retained",
        "raw_recognizer_tensors_retained",
    ] {
        assert_eq!(
            value_field(source_result, field, context).as_bool(),
            Some(false),
            "{context} score-filter source result must not retain {field}"
        );
    }
    let source_record = object_field(source_result, "record", context);
    assert_eq!(
        string_field(source_record, "fixture_id", context),
        SCORE_FILTER_FIXTURE_ID,
        "{context} score-filter source-result identifier changed without review"
    );
    assert_eq!(
        string_field(source_record, "upstream_commit", context),
        UPSTREAM_BASELINE,
        "{context} score-filter source-result baseline changed without review"
    );
    assert_eq!(
        value_field(source_record, "drop_score", context).as_f64(),
        Some(0.5),
        "{context} score-filter source-result threshold changed without review"
    );

    let input_bytes = read_fixture_file(fixture_directory, "input.json", context);
    assert_digest(
        &input_bytes,
        SCORE_FILTER_INPUT_SHA256,
        &format!("{context} score-filter input document"),
    );
    let input = parse_json_bytes(
        &input_bytes,
        &format!("{context} score-filter input document"),
    );
    assert_eq!(
        string_field(&input, "schema_version", context),
        "paddleocr-rust/classic-score-filter-input/v1",
        "{context} score-filter input schema changed without review"
    );
    assert_eq!(
        string_field(&input, "fixture_id", context),
        SCORE_FILTER_FIXTURE_ID,
        "{context} score-filter input identifier changed without review"
    );
    assert_eq!(
        value_field(&input, "drop_score", context).as_f64(),
        Some(0.5),
        "{context} score-filter input threshold changed without review"
    );
    let input_pairs = array_field(&input, "pairs", context);
    assert_eq!(
        input_pairs.len(),
        3,
        "{context} score-filter input pair count changed without review"
    );
    let expected_pairs = [
        (
            "below",
            0.499_999_999_999_999_94,
            serde_json::json!([[2, 2], [12, 2], [12, 10], [2, 10]]),
        ),
        (
            "at",
            0.5,
            serde_json::json!([[22, 2], [32, 2], [32, 10], [22, 10]]),
        ),
        (
            "above",
            0.500_000_000_000_000_1,
            serde_json::json!([[42, 2], [52, 2], [52, 10], [42, 10]]),
        ),
    ];
    for (pair, (text, score, quad)) in input_pairs.iter().zip(expected_pairs) {
        assert_eq!(
            string_field(pair, "text", context),
            text,
            "{context} score-filter input text changed without review"
        );
        assert_eq!(
            value_field(pair, "score", context).as_f64(),
            Some(score),
            "{context} score-filter input score changed without review"
        );
        assert_eq!(
            value_field(pair, "box", context),
            &quad,
            "{context} score-filter input box changed without review"
        );
    }
    assert_eq!(
        value_field(source_record, "input_boxes", context),
        &serde_json::json!([
            [[2, 2], [12, 2], [12, 10], [2, 10]],
            [[22, 2], [32, 2], [32, 10], [22, 10]],
            [[42, 2], [52, 2], [52, 10], [42, 10]]
        ]),
        "{context} score-filter source input boxes changed without review"
    );
    assert_eq!(
        value_field(source_record, "input_results", context),
        &serde_json::json!([
            ["below", 0.49999999999999994],
            ["at", 0.5],
            ["above", 0.5000000000000001]
        ]),
        "{context} score-filter source input results changed without review"
    );

    let expected_bytes = read_fixture_file(fixture_directory, "expected.json", context);
    assert_digest(
        &expected_bytes,
        SCORE_FILTER_EXPECTED_SHA256,
        &format!("{context} score-filter expected result"),
    );
    let expected = parse_json_bytes(
        &expected_bytes,
        &format!("{context} score-filter expected result"),
    );
    assert_eq!(
        string_field(&expected, "schema_version", context),
        "paddleocr-rust/classic-score-filter/v1",
        "{context} score-filter expected schema changed without review"
    );
    assert_eq!(
        string_field(&expected, "fixture_id", context),
        SCORE_FILTER_FIXTURE_ID,
        "{context} score-filter expected identifier changed without review"
    );
    assert_eq!(
        value_field(&expected, "drop_score", context).as_f64(),
        Some(0.5),
        "{context} score-filter expected threshold changed without review"
    );
    assert_eq!(
        value_field(&expected, "retained_input_indexes", context),
        &serde_json::json!([1, 2]),
        "{context} score-filter retained indexes changed without review"
    );
    let lines = array_field(&expected, "lines", context);
    assert_eq!(
        lines.len(),
        2,
        "{context} score-filter expected line count changed without review"
    );
    let expected_lines = [
        (
            "at",
            0.5,
            serde_json::json!([[22, 2], [32, 2], [32, 10], [22, 10]]),
        ),
        (
            "above",
            0.500_000_000_000_000_1,
            serde_json::json!([[42, 2], [52, 2], [52, 10], [42, 10]]),
        ),
    ];
    for (line, (text, score, quad)) in lines.iter().zip(expected_lines) {
        assert_eq!(
            string_field(line, "text", context),
            text,
            "{context} score-filter retained text changed without review"
        );
        assert_eq!(
            value_field(line, "score", context).as_f64(),
            Some(score),
            "{context} score-filter retained score changed without review"
        );
        assert_eq!(
            value_field(line, "box", context),
            &quad,
            "{context} score-filter retained box changed without review"
        );
    }
    assert_eq!(
        value_field(source_record, "kept_boxes", context),
        &serde_json::json!([
            [[22, 2], [32, 2], [32, 10], [22, 10]],
            [[42, 2], [52, 2], [52, 10], [42, 10]]
        ]),
        "{context} score-filter source kept boxes changed without review"
    );
    assert_eq!(
        value_field(source_record, "kept_results", context),
        &serde_json::json!([["at", 0.5], ["above", 0.5000000000000001]]),
        "{context} score-filter source kept results changed without review"
    );
}

fn verify_e2e_candidate(
    candidate: &Value,
    expected_key: &str,
    expected_revision: &str,
    expected_sha256: &str,
    context: &str,
) {
    assert_eq!(
        string_field(candidate, "candidate_key", context),
        expected_key,
        "{context} end-to-end candidate key changed without review"
    );
    assert_eq!(
        string_field(candidate, "revision", context),
        expected_revision,
        "{context} end-to-end candidate revision changed without review"
    );
    assert_eq!(
        string_field(candidate, "sha256", context),
        expected_sha256,
        "{context} end-to-end candidate hash changed without review"
    );
}

fn verify_image_capture_environment(oracle: &Value, captured: &Value, context: &str) {
    for field in ["numpy", "opencv", "opencv_build_information_sha256"] {
        assert_eq!(
            string_field(captured, field, context),
            string_field(oracle, field, context),
            "{context} image oracle environment disagrees on {field}"
        );
    }
    let captured_distribution = object_field(captured, "opencv_distribution", context);
    let captured_distribution = format!(
        "{} {}",
        string_field(captured_distribution, "name", context),
        string_field(captured_distribution, "version", context)
    );
    assert_eq!(
        captured_distribution,
        string_field(oracle, "opencv_distribution", context),
        "{context} image oracle environment disagrees on OpenCV distribution"
    );
    let captured_python = string_field(captured, "python", context);
    assert!(
        captured_python.starts_with(string_field(oracle, "python", context)),
        "{context} image oracle environment disagrees on Python version"
    );
}

fn decode_image_payload(case: &Value, role: &str, is_bgr_output: bool, context: &str) -> Vec<u8> {
    let payload = object_field(case, role, context);
    let encoded = string_field(payload, "base64", context);
    let bytes = must_ok(
        STANDARD.decode(encoded),
        &format!("decode {context} {role} base64 payload"),
    );
    assert_digest(
        &bytes,
        string_field(payload, "sha256", context),
        &format!("{context} {role} payload"),
    );
    let byte_length = match value_field(payload, "byte_length", context).as_u64() {
        Some(value) => value,
        None => panic!("{context} {role} byte_length must be an unsigned integer"),
    };
    assert_eq!(
        usize::try_from(byte_length).ok(),
        Some(bytes.len()),
        "{context} {role} byte_length must match its base64 bytes"
    );

    if is_bgr_output {
        assert_eq!(
            string_field(payload, "channel_order", context),
            "BGR",
            "{context} {role} output must remain BGR"
        );
        assert_eq!(
            string_field(payload, "dtype", context),
            "uint8",
            "{context} {role} output must remain uint8"
        );
        let shape = array_field(payload, "shape", context);
        assert_eq!(
            shape.len(),
            3,
            "{context} {role} output must have an HWC shape"
        );
        let mut expected_length = 1_usize;
        for (axis_index, axis) in shape.iter().enumerate() {
            let axis = match axis.as_u64() {
                Some(value) => value,
                None => {
                    panic!("{context} {role} shape axis {axis_index} must be an unsigned integer")
                }
            };
            let axis = match usize::try_from(axis) {
                Ok(value) => value,
                Err(_) => panic!("{context} {role} shape axis {axis_index} does not fit usize"),
            };
            assert!(
                axis > 0,
                "{context} {role} shape axis {axis_index} must be non-zero"
            );
            expected_length = match expected_length.checked_mul(axis) {
                Some(value) => value,
                None => panic!("{context} {role} shape byte count overflows usize"),
            };
        }
        assert_eq!(
            shape[2].as_u64(),
            Some(3),
            "{context} {role} output must have three BGR channels"
        );
        assert_eq!(
            bytes.len(),
            expected_length,
            "{context} {role} byte count must match its HWC uint8 shape"
        );
    }

    bytes
}

fn decode_crop_payload(case: &Value, role: &str, context: &str) -> Vec<u8> {
    decode_crop_payload_with_channel_order(case, role, "BGR", context)
}

/// Decodes one captured crop payload that declares an explicit channel order.
///
/// Only a three-channel payload may claim `BGR`. Every other supported channel
/// count must use the deliberately colourless `opaque-<n>` label, because this
/// project has frozen no colour meaning for those counts.
fn decode_crop_payload_with_channel_order(
    case: &Value,
    role: &str,
    expected_channel_order: &str,
    context: &str,
) -> Vec<u8> {
    let payload = object_field(case, role, context);
    assert_eq!(
        string_field(payload, "channel_order", context),
        expected_channel_order,
        "{context} crop payload channel order changed without review"
    );
    assert_eq!(
        string_field(payload, "dtype", context),
        "uint8",
        "{context} crop payload must remain uint8"
    );
    let encoded = string_field(payload, "base64", context);
    let bytes = must_ok(
        STANDARD.decode(encoded),
        &format!("decode {context} {role} base64 payload"),
    );
    assert_digest(
        &bytes,
        string_field(payload, "sha256", context),
        &format!("{context} {role} payload"),
    );

    let mut expected_len = 1_usize;
    for (index, axis) in array_field(payload, "shape", context).iter().enumerate() {
        let axis = match axis.as_u64() {
            Some(axis) => axis,
            None => panic!("{context} {role} shape axis {index} must be an unsigned integer"),
        };
        let axis = match usize::try_from(axis) {
            Ok(axis) => axis,
            Err(_) => panic!("{context} {role} shape axis {index} does not fit usize"),
        };
        expected_len = match expected_len.checked_mul(axis) {
            Some(length) => length,
            None => panic!("{context} {role} shape byte count overflows usize"),
        };
    }
    assert_eq!(
        bytes.len(),
        expected_len,
        "{context} {role} decoded length must match its uint8 shape"
    );
    bytes
}

fn read_json_file(path: &Path) -> Value {
    let bytes = read_regular_file(path, &format!("read JSON fixture {}", path.display()));
    parse_json_bytes(&bytes, &format!("parse JSON fixture {}", path.display()))
}

fn parse_json_bytes(bytes: &[u8], context: &str) -> Value {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => panic!("{context} is not UTF-8: {error}"),
    };
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("{context} is not valid JSON: {error}"),
    }
}

fn read_fixture_file(fixture_directory: &Path, relative_path: &str, context: &str) -> Vec<u8> {
    let relative = Path::new(relative_path);
    assert!(
        !relative.is_absolute()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "{context} fixture path {relative_path:?} must be a relative normal path"
    );
    read_regular_file(
        &fixture_directory.join(relative),
        &format!("read {context} fixture file {relative_path:?}"),
    )
}

fn read_regular_file(path: &Path, context: &str) -> Vec<u8> {
    let metadata = must_ok(fs::symlink_metadata(path), &format!("inspect {context}"));
    assert!(
        !metadata.file_type().is_symlink() && metadata.is_file(),
        "{context} must be a regular non-symlink file"
    );
    must_ok(fs::read(path), context)
}

fn value_field<'a>(value: &'a Value, field: &str, context: &str) -> &'a Value {
    match value.get(field) {
        Some(value) => value,
        None => panic!("{context} is missing required field {field:?}"),
    }
}

fn object_field<'a>(value: &'a Value, field: &str, context: &str) -> &'a Value {
    let value = value_field(value, field, context);
    value_as_object(value, field, context)
}

fn value_as_object<'a>(value: &'a Value, field: &str, context: &str) -> &'a Value {
    assert!(
        value.is_object(),
        "{context} field {field:?} must be an object"
    );
    value
}

fn array_field<'a>(value: &'a Value, field: &str, context: &str) -> &'a [Value] {
    match value_field(value, field, context).as_array() {
        Some(values) => values,
        None => panic!("{context} field {field:?} must be an array"),
    }
}

fn string_field<'a>(value: &'a Value, field: &str, context: &str) -> &'a str {
    value_as_str(value_field(value, field, context), field, context)
}

fn unsigned_field(value: &Value, field: &str, context: &str) -> u64 {
    match value_field(value, field, context).as_u64() {
        Some(value) => value,
        None => panic!("{context} field {field:?} must be an unsigned integer"),
    }
}

fn value_as_str<'a>(value: &'a Value, field: &str, context: &str) -> &'a str {
    match value.as_str() {
        Some(value) => value,
        None => panic!("{context} field {field:?} must be a string"),
    }
}

fn assert_non_empty(value: &str, field: &str, context: &str) {
    assert!(
        !value.is_empty(),
        "{context} field {field:?} must not be empty"
    );
}

fn assert_iso_date(value: &str, context: &str) {
    let bytes = value.as_bytes();
    assert!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && [0, 1, 2, 3, 5, 6, 8, 9]
                .into_iter()
                .all(|index| bytes[index].is_ascii_digit()),
        "{context} reviewed_on must use YYYY-MM-DD"
    );
}

fn assert_digest(bytes: &[u8], expected: &str, context: &str) {
    assert_sha256_format(expected, "sha256", context);
    let actual = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(actual, expected, "{context} SHA-256 mismatch");
}

fn assert_sha256_format(value: &str, field: &str, context: &str) {
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{context} field {field:?} must be a lowercase SHA-256 hexadecimal digest"
    );
}

fn must_ok<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context} failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_v1_e2e_no_text_fixture_is_well_formed() {
        let directory = Path::new(FIXTURE_ROOT).join(E2E_NO_TEXT_FIXTURE_ID);
        let context = format!("fixture directory {}", directory.display());
        let metadata = read_json_file(&directory.join("metadata.json"));

        verify_common_metadata(&metadata, &directory, &context);
        verify_e2e_no_text_oracle(&metadata, &directory, &context);
    }

    #[test]
    fn classic_v1_e2e_reading_order_fixture_is_well_formed() {
        let directory = Path::new(FIXTURE_ROOT).join(E2E_READING_ORDER_FIXTURE_ID);
        let context = format!("fixture directory {}", directory.display());
        let metadata = read_json_file(&directory.join("metadata.json"));

        verify_common_metadata(&metadata, &directory, &context);
        verify_e2e_reading_order_oracle(&metadata, &directory, &context);
    }

    #[test]
    fn classic_v1_e2e_tall_crop_fixture_is_well_formed() {
        let directory = Path::new(FIXTURE_ROOT).join(E2E_TALL_CROP_FIXTURE_ID);
        let context = format!("fixture directory {}", directory.display());
        let metadata = read_json_file(&directory.join("metadata.json"));

        verify_common_metadata(&metadata, &directory, &context);
        verify_e2e_tall_crop_oracle(&metadata, &directory, &context);
    }

    #[test]
    fn classic_v1_e2e_unicode_fixture_is_well_formed() {
        let directory = Path::new(FIXTURE_ROOT).join(E2E_UNICODE_FIXTURE_ID);
        let context = format!("fixture directory {}", directory.display());
        let metadata = read_json_file(&directory.join("metadata.json"));

        verify_common_metadata(&metadata, &directory, &context);
        verify_e2e_unicode_oracle(&metadata, &directory, &context);
    }

    #[test]
    fn classic_v1_ctc_score_boundary_fixture_is_well_formed() {
        let directory = Path::new(FIXTURE_ROOT).join(SCORE_FILTER_FIXTURE_ID);
        let context = format!("fixture directory {}", directory.display());
        let metadata = read_json_file(&directory.join("metadata.json"));

        verify_common_metadata(&metadata, &directory, &context);
        verify_score_filter_oracle(&metadata, &directory, &context);
    }
}
