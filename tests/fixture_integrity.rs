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
            "classic-v1-e2e-no-text" | "classic-v1-e2e-reading-order" => "m2-e2e-v1",
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
        if fixture_id == "classic-v1-image-inputs" {
            verify_image_input_oracle(&metadata, &directory, &context);
        }
        if fixture_id == "classic-v1-e2e-no-text" {
            verify_e2e_no_text_oracle(&metadata, &directory, &context);
        }
        if fixture_id == E2E_READING_ORDER_FIXTURE_ID {
            verify_e2e_reading_order_oracle(&metadata, &directory, &context);
        }
    }

    let expected_ids = BTreeSet::from([
        "classic-v1-crop-oracle".to_owned(),
        "classic-v1-crop-scalar-grid".to_owned(),
        "classic-v1-db-components".to_owned(),
        "classic-v1-ctc-greedy-path".to_owned(),
        "classic-v1-db-map-boundaries".to_owned(),
        "classic-v1-e2e-no-text".to_owned(),
        E2E_READING_ORDER_FIXTURE_ID.to_owned(),
        "classic-v1-geometry-min-area-candidate".to_owned(),
        "classic-v1-image-inputs".to_owned(),
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
        "classic-v1-e2e-no-text" | E2E_READING_ORDER_FIXTURE_ID
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
    let payload = object_field(case, role, context);
    assert_eq!(
        string_field(payload, "channel_order", context),
        "BGR",
        "{context} crop payload must remain BGR"
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
}
