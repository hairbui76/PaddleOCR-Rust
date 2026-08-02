//! Offline integrity checks for the committed self-authored fixture corpus.

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

        if fixture_id == "classic-v1-crop-oracle" {
            verify_crop_oracle(&metadata, &directory, &context);
        }
    }

    let expected_ids = BTreeSet::from([
        "classic-v1-crop-oracle".to_owned(),
        "classic-v1-ctc-greedy-path".to_owned(),
        "classic-v1-db-map-boundaries".to_owned(),
        "classic-v1-geometry-min-area-candidate".to_owned(),
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
    assert!(
        value_field(metadata, "artifacts", context).is_null(),
        "{context} must not introduce a model-backed fixture before artifact review"
    );

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
        assert_eq!(
            string_field(descriptor, "comparison_profile", context),
            "m2-unit-v1",
            "{context} expected fixture must use the frozen unit comparison profile"
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
