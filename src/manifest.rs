// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The versioned model manifest.
//!
//! Roadmap item `MOD-002`. A manifest is the single place a caller states which
//! artifacts a run uses and what they must be: task, family, version, format,
//! backend, source revision, byte count, SHA-256, tensor contract, dictionary
//! fingerprint, licence review, and the upstream baseline it was verified
//! against. `docs/ADR_MODEL_DEC_001_ARTIFACT_POLICY.md` is the policy this
//! encodes.
//!
//! # Why this format
//!
//! The syntax is `key = value`, one per line, with dotted keys for nesting and
//! `#` comments. It is not JSON, TOML, or YAML, and the reason is the same one
//! that produced the hand-rolled result writer: this crate has no runtime
//! serialisation dependency, and adding one for a file with about thirty fields
//! would trade a well-specified fifty-line parser for a supply-chain surface.
//!
//! The parser is deliberately strict. An unknown key is an error rather than
//! being ignored, because the failure mode of a lenient manifest parser is a
//! typo that silently disables a check — `detector.sha265` would leave an
//! artifact unverified while looking verified.
//!
//! # What a manifest does not do
//!
//! It records a URL and never fetches it. It never selects an artifact by
//! search, cache, or environment. Resolving a manifest to loaded models is
//! `MOD-003`; this module only defines and validates the document.

use std::collections::BTreeMap;

use crate::error::{Error, InputViolation, Result};

/// The frozen manifest schema identifier.
pub const MANIFEST_SCHEMA_VERSION: &str = "paddleocr-rust/model-manifest/v1";

/// Largest manifest this parser accepts, in bytes.
///
/// A manifest is a few kilobytes of key-value text. The bound exists because
/// the file is caller-supplied and every caller-supplied input in this crate is
/// bounded before it is parsed.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// Largest number of lines accepted, bounding the parse independently of size.
const MAX_MANIFEST_LINES: usize = 2_048;

/// One artifact's identity and tensor contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEntry {
    /// Source repository or distribution URL, recorded and never fetched.
    pub url: String,
    /// Exact source revision, because a family name is not provenance.
    pub revision: String,
    /// Lowercase hexadecimal SHA-256 of the exact artifact file.
    pub sha256: String,
    /// Exact byte count of the artifact file.
    pub bytes: u64,
    /// Name of the model's input tensor.
    pub input_name: String,
    /// Name of the model's output tensor.
    pub output_name: String,
}

/// The dictionary an artifact pair is bound to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictionaryEntry {
    /// Lowercase hexadecimal SHA-256 of the exact dictionary file.
    pub sha256: String,
    /// Number of configured entries, excluding blank and any appended space.
    pub entries: usize,
    /// Whether the artifact appends a space class, mirroring `use_space_char`.
    pub appends_space: bool,
}

/// A complete, validated model manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelManifest {
    /// Task this manifest serves, for example `ocr.classic`.
    pub task: String,
    /// Model family, for example `PP-OCRv6_medium`.
    pub family: String,
    /// Manifest version, distinct from the family's own versioning.
    pub version: String,
    /// Artifact representation, for example `onnx`.
    pub format: String,
    /// Backend the format is consumed through, for example `onnxruntime`.
    pub backend: String,
    /// Upstream commit this pairing was verified against.
    pub upstream_commit: String,
    /// Licence review identifier for the artifact terms.
    pub license_review: String,
    /// The detector artifact.
    pub detector: ArtifactEntry,
    /// The recognizer artifact.
    pub recognizer: ArtifactEntry,
    /// The dictionary bound to the recognizer.
    pub dictionary: DictionaryEntry,
}

impl ModelManifest {
    /// Parses and validates a manifest document.
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_MANIFEST_BYTES {
            return Err(Error::ResourceLimit {
                resource: "manifest.bytes",
                limit: MAX_MANIFEST_BYTES as u64,
                actual: text.len() as u64,
            });
        }
        let fields = parse_fields(text)?;
        let manifest = Self {
            task: take(&fields, "task")?,
            family: take(&fields, "family")?,
            version: take(&fields, "version")?,
            format: take(&fields, "format")?,
            backend: take(&fields, "backend")?,
            upstream_commit: take(&fields, "upstream.commit")?,
            license_review: take(&fields, "license.review")?,
            detector: artifact(&fields, "detector")?,
            recognizer: artifact(&fields, "recognizer")?,
            dictionary: DictionaryEntry {
                sha256: digest(&take(&fields, "dictionary.sha256")?, "dictionary.sha256")?,
                entries: number(&take(&fields, "dictionary.entries")?, "dictionary.entries")?
                    as usize,
                appends_space: boolean(
                    &take(&fields, "dictionary.appends_space")?,
                    "dictionary.appends_space",
                )?,
            },
        };

        let schema = take(&fields, "schema_version")?;
        if schema != MANIFEST_SCHEMA_VERSION {
            return Err(Error::InvalidInput {
                field: "manifest.schema_version",
                violation: InputViolation::OutOfRange,
            });
        }
        if manifest.dictionary.entries == 0 {
            return Err(Error::InvalidInput {
                field: "manifest.dictionary.entries",
                violation: InputViolation::Empty,
            });
        }
        // An unknown key is refused rather than ignored: a lenient parser turns
        // `detector.sha265` into an artifact that looks verified and is not.
        if let Some(unknown) = fields.keys().find(|key| !is_known(key)) {
            let _ = unknown;
            return Err(Error::InvalidInput {
                field: "manifest.unknown_key",
                violation: InputViolation::OutOfRange,
            });
        }
        Ok(manifest)
    }

    /// Returns the class count the recognizer's output must have.
    ///
    /// This is the dictionary entry count plus the CTC blank, plus the appended
    /// space when the artifact uses one. It is derived rather than stored so a
    /// manifest cannot state a class count that disagrees with its own
    /// dictionary fields.
    #[must_use]
    pub fn recognizer_class_count(&self) -> usize {
        self.dictionary.entries + 1 + usize::from(self.dictionary.appends_space)
    }
}

/// Every key this schema defines. Anything else is a typo or a later version.
const KNOWN_KEYS: [&str; 21] = [
    "schema_version",
    "task",
    "family",
    "version",
    "format",
    "backend",
    "upstream.commit",
    "license.review",
    "detector.url",
    "detector.revision",
    "detector.sha256",
    "detector.bytes",
    "detector.input.name",
    "detector.output.name",
    "recognizer.url",
    "recognizer.revision",
    "recognizer.sha256",
    "recognizer.bytes",
    "recognizer.input.name",
    "recognizer.output.name",
    "dictionary.sha256",
];

fn is_known(key: &str) -> bool {
    KNOWN_KEYS.contains(&key) || matches!(key, "dictionary.entries" | "dictionary.appends_space")
}

/// Splits a manifest into its key-value pairs.
fn parse_fields(text: &str) -> Result<BTreeMap<String, String>> {
    let mut fields = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if index >= MAX_MANIFEST_LINES {
            return Err(Error::ResourceLimit {
                resource: "manifest.lines",
                limit: MAX_MANIFEST_LINES as u64,
                actual: (index + 1) as u64,
            });
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(Error::InvalidInput {
                field: "manifest.line",
                violation: InputViolation::OutOfRange,
            });
        };
        let (key, value) = (key.trim(), value.trim());
        if key.is_empty() || value.is_empty() {
            return Err(Error::InvalidInput {
                field: "manifest.line",
                violation: InputViolation::Empty,
            });
        }
        // A repeated key is an error, not a last-one-wins: two digests for one
        // artifact means the document does not say what it appears to say.
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(Error::InvalidInput {
                field: "manifest.duplicate_key",
                violation: InputViolation::OutOfRange,
            });
        }
    }
    Ok(fields)
}

fn take(fields: &BTreeMap<String, String>, key: &'static str) -> Result<String> {
    match fields.get(key) {
        Some(value) => Ok(value.clone()),
        None => Err(Error::InvalidInput {
            field: "manifest.missing_key",
            violation: InputViolation::Empty,
        }),
    }
}

fn artifact(fields: &BTreeMap<String, String>, prefix: &str) -> Result<ArtifactEntry> {
    let key = |suffix: &str| -> String { format!("{prefix}.{suffix}") };
    let get = |suffix: &str| -> Result<String> {
        match fields.get(&key(suffix)) {
            Some(value) => Ok(value.clone()),
            None => Err(Error::InvalidInput {
                field: "manifest.missing_key",
                violation: InputViolation::Empty,
            }),
        }
    };
    Ok(ArtifactEntry {
        url: get("url")?,
        revision: get("revision")?,
        sha256: digest(&get("sha256")?, "manifest.sha256")?,
        bytes: number(&get("bytes")?, "manifest.bytes")?,
        input_name: get("input.name")?,
        output_name: get("output.name")?,
    })
}

/// Accepts a lowercase 64-character hexadecimal digest and nothing else.
///
/// Uppercase is refused rather than folded: the digest is compared as text
/// elsewhere, and accepting two spellings of one value would mean a manifest and
/// a command line could disagree textually while meaning the same thing.
fn digest(value: &str, field: &'static str) -> Result<String> {
    let _ = field;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::InvalidInput {
            field: "manifest.sha256",
            violation: InputViolation::OutOfRange,
        });
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(Error::InvalidInput {
            field: "manifest.sha256",
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(value.to_owned())
}

fn number(value: &str, field: &'static str) -> Result<u64> {
    let _ = field;
    match value.parse::<u64>() {
        Ok(parsed) if parsed > 0 => Ok(parsed),
        _ => Err(Error::InvalidInput {
            field: "manifest.number",
            violation: InputViolation::OutOfRange,
        }),
    }
}

fn boolean(value: &str, field: &'static str) -> Result<bool> {
    let _ = field;
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::InvalidInput {
            field: "manifest.boolean",
            violation: InputViolation::OutOfRange,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str =
        include_str!("../tests/fixtures/classic-v1-model-manifest/expected.txt");

    #[test]
    fn the_committed_manifest_parses_and_matches_the_pinned_artifacts() {
        let manifest = match ModelManifest::parse(COMMITTED) {
            Ok(manifest) => manifest,
            Err(error) => panic!("committed manifest: {error}"),
        };
        assert_eq!(manifest.task, "ocr.classic");
        assert_eq!(manifest.family, "PP-OCRv6_medium");
        assert_eq!(manifest.format, "onnx");
        assert_eq!(manifest.backend, "onnxruntime");
        assert_eq!(
            manifest.detector.sha256,
            "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1"
        );
        assert_eq!(manifest.detector.bytes, 62_032_837);
        assert_eq!(
            manifest.recognizer.sha256,
            "9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba"
        );
        assert_eq!(manifest.recognizer.bytes, 76_554_979);
        assert_eq!(manifest.dictionary.entries, 18_708);
        assert!(manifest.dictionary.appends_space);
        // Entries plus the CTC blank plus the appended space, which is the
        // class count the recognizer's output contract is built from.
        assert_eq!(manifest.recognizer_class_count(), 18_710);
        assert_eq!(manifest.detector.input_name, "x");
        assert_eq!(manifest.detector.output_name, "fetch_name_0");
    }

    fn committed_with(replacement: &str, with: &str) -> String {
        assert!(
            COMMITTED.contains(replacement),
            "the committed manifest no longer contains {replacement:?}"
        );
        COMMITTED.replace(replacement, with)
    }

    /// A typo in a key is refused rather than silently leaving a check off.
    ///
    /// This is the failure this parser exists to prevent: `detector.sha265`
    /// would leave the artifact unverified while the manifest looks complete.
    #[test]
    fn an_unknown_key_is_refused() {
        let text = format!("{COMMITTED}\ndetector.sha265 = {}\n", "0".repeat(64));
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::InvalidInput {
                field: "manifest.unknown_key",
                ..
            })
        ));
    }

    #[test]
    fn a_duplicate_key_is_refused() {
        let text = format!("{COMMITTED}\ntask = ocr.classic\n");
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::InvalidInput {
                field: "manifest.duplicate_key",
                ..
            })
        ));
    }

    #[test]
    fn a_missing_key_is_refused() {
        let text: String = COMMITTED
            .lines()
            .filter(|line| !line.starts_with("detector.sha256"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::InvalidInput {
                field: "manifest.missing_key",
                ..
            })
        ));
    }

    #[test]
    fn a_malformed_digest_is_refused() {
        for bad in [
            "0".repeat(63),
            "0".repeat(65),
            "EB13B44B25BB36F89528B68720AF8A61D9CF381176107F465DB1757B65D086E1".to_owned(),
            "z".repeat(64),
        ] {
            let text = committed_with(
                "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1",
                &bad,
            );
            assert!(
                matches!(
                    ModelManifest::parse(&text),
                    Err(Error::InvalidInput {
                        field: "manifest.sha256",
                        ..
                    })
                ),
                "digest {bad:?} must be refused"
            );
        }
    }

    #[test]
    fn a_zero_or_malformed_byte_count_is_refused() {
        for bad in ["0", "-1", "many", "62032837.0"] {
            let text = committed_with("62032837", bad);
            assert!(
                ModelManifest::parse(&text).is_err(),
                "byte count {bad:?} must be refused"
            );
        }
    }

    #[test]
    fn a_wrong_schema_version_is_refused() {
        let text = committed_with(MANIFEST_SCHEMA_VERSION, "paddleocr-rust/model-manifest/v2");
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::InvalidInput {
                field: "manifest.schema_version",
                ..
            })
        ));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let text = format!("# a comment\n\n{COMMITTED}\n\n# trailing\n");
        assert!(ModelManifest::parse(&text).is_ok());
    }

    #[test]
    fn a_line_without_a_separator_is_refused() {
        let text = format!("{COMMITTED}\nthis line has no equals sign\n");
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::InvalidInput {
                field: "manifest.line",
                ..
            })
        ));
    }

    #[test]
    fn an_oversized_manifest_is_refused_before_parsing() {
        let text = "x".repeat(MAX_MANIFEST_BYTES + 1);
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::ResourceLimit {
                resource: "manifest.bytes",
                ..
            })
        ));
    }

    #[test]
    fn a_manifest_with_too_many_lines_is_refused() {
        let text = "# comment\n".repeat(MAX_MANIFEST_LINES + 1);
        assert!(matches!(
            ModelManifest::parse(&text),
            Err(Error::ResourceLimit {
                resource: "manifest.lines",
                ..
            })
        ));
    }

    #[test]
    fn an_appended_space_changes_the_derived_class_count() {
        let with_space = match ModelManifest::parse(COMMITTED) {
            Ok(manifest) => manifest,
            Err(error) => panic!("committed manifest: {error}"),
        };
        let text = committed_with(
            "dictionary.appends_space = true",
            "dictionary.appends_space = false",
        );
        let without_space = match ModelManifest::parse(&text) {
            Ok(manifest) => manifest,
            Err(error) => panic!("modified manifest: {error}"),
        };
        assert_eq!(
            with_space.recognizer_class_count(),
            without_space.recognizer_class_count() + 1
        );
    }
}
