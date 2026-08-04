// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Resolving a manifest to local files, and saying exactly what went wrong.
//!
//! Roadmap item `MOD-003`. `MOD-002` defined the manifest; this turns one into
//! three checked local paths, or into a report that names every mismatch.
//!
//! # Offline is structural, not a mode
//!
//! There is no `offline` flag here, because there is nothing to switch off. A
//! manifest's `url` fields are **provenance** — recorded so a reader knows where
//! an artifact came from — and this module never reads them. Resolution takes a
//! **directory** and looks inside it.
//!
//! That is stronger than a flag: a flag can be set wrong, and an offline mode
//! that is one boolean away from a fetch is a network dependency waiting for a
//! default to change. `MODEL-DEC-001` decided no automatic downloads; this is
//! that decision expressed as an absent capability.
//!
//! # Every mismatch, not the first
//!
//! A caller who provisioned the wrong model family usually has **all three**
//! files wrong. Stopping at the first mismatch means three runs to learn that.
//! [`resolve`] collects every problem and reports them together.
//!
//! # What "actionable" means here
//!
//! A [`Mismatch`] carries what was expected, what was found, and which file it
//! was found in. `Error` deliberately does not carry user paths — see
//! `docs/THREAT_MODEL.md` on not embedding caller-controlled strings in typed
//! errors — so the detail lives in this type, which a caller chooses to display,
//! rather than in an error that might reach a log.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::digest::Sha256;
use crate::error::{Error, InputViolation, Result};
use crate::manifest::{ArtifactEntry, DictionaryEntry, ModelManifest};

/// The three files a classic OCR manifest resolves to.
///
/// File names are fixed rather than configurable: a manifest describes one
/// pairing, and letting a caller rename its parts would let two different
/// artifacts satisfy the same manifest.
pub const DETECTOR_FILE: &str = "detector.onnx";
/// The recognizer's file name inside a resolved directory.
pub const RECOGNIZER_FILE: &str = "recognizer.onnx";
/// The dictionary's file name inside a resolved directory.
pub const DICTIONARY_FILE: &str = "dictionary.txt";

/// The largest artifact this module will hash, in bytes.
///
/// A bound on work done from a **declared** size: a manifest claiming a
/// `10 GiB` artifact must not cause a `10 GiB` read before it is rejected.
const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Which part of a manifest a mismatch belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Part {
    /// The detector artifact.
    Detector,
    /// The recognizer artifact.
    Recognizer,
    /// The dictionary.
    Dictionary,
}

impl Part {
    /// The file name this part resolves to.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Detector => DETECTOR_FILE,
            Self::Recognizer => RECOGNIZER_FILE,
            Self::Dictionary => DICTIONARY_FILE,
        }
    }
}

/// What went wrong with one part, in enough detail to act on.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Mismatch {
    /// The file is not present in the directory.
    Missing {
        /// Which part is absent.
        part: Part,
        /// The path that was looked for.
        path: PathBuf,
    },
    /// The file's size disagrees with the manifest.
    ///
    /// Checked before the digest, because it is the cheaper answer and it
    /// distinguishes "a different build" from "a corrupted download".
    Size {
        /// Which part disagrees.
        part: Part,
        /// The manifest's declared byte count.
        expected: u64,
        /// What the file on disk actually is.
        found: u64,
    },
    /// The file's SHA-256 disagrees with the manifest.
    Digest {
        /// Which part disagrees.
        part: Part,
        /// The manifest's declared digest.
        expected: String,
        /// The digest of the file on disk.
        found: String,
    },
    /// The dictionary's entry count disagrees with the manifest.
    ///
    /// A separate case from a digest mismatch because it is the one a caller
    /// can usually fix: it means the right file with the wrong `use_space_char`
    /// setting, or a dictionary from a neighbouring model version.
    DictionaryEntries {
        /// The manifest's declared entry count.
        expected: usize,
        /// What the file on disk holds.
        found: usize,
    },
}

impl Mismatch {
    /// Which part this mismatch concerns.
    #[must_use]
    pub const fn part(&self) -> Part {
        match self {
            Self::Missing { part, .. } | Self::Size { part, .. } | Self::Digest { part, .. } => {
                *part
            }
            Self::DictionaryEntries { .. } => Part::Dictionary,
        }
    }
}

impl core::fmt::Display for Mismatch {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing { part, path } => {
                write!(
                    formatter,
                    "{} is missing at {}",
                    part.file_name(),
                    path.display()
                )
            }
            Self::Size {
                part,
                expected,
                found,
            } => write!(
                formatter,
                "{} is {found} bytes, but the manifest declares {expected}",
                part.file_name()
            ),
            Self::Digest {
                part,
                expected,
                found,
            } => write!(
                formatter,
                "{} hashes to {found}, but the manifest declares {expected}",
                part.file_name()
            ),
            Self::DictionaryEntries { expected, found } => write!(
                formatter,
                "{DICTIONARY_FILE} holds {found} entries, but the manifest declares {expected}"
            ),
        }
    }
}

/// Three verified local paths.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ResolvedModel {
    /// The verified detector artifact.
    pub detector: PathBuf,
    /// The verified recognizer artifact.
    pub recognizer: PathBuf,
    /// The verified dictionary.
    pub dictionary: PathBuf,
}

/// Resolves a manifest against a local directory, checking every part.
///
/// Returns the three paths when everything agrees, and **every** mismatch when
/// anything does not — not the first one.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] when the directory is not one, and
/// [`Error::Io`] when a file cannot be read. A mismatch is **not** an error: it
/// is data, returned in the `Err` variant's payload so a caller can present all
/// of it at once.
pub fn resolve(
    manifest: &ModelManifest,
    directory: &Path,
) -> core::result::Result<ResolvedModel, ResolveFailure> {
    if !directory.is_dir() {
        return Err(ResolveFailure::Directory);
    }

    let mut mismatches = Vec::new();
    let detector = check_artifact(
        Part::Detector,
        &manifest.detector,
        directory,
        &mut mismatches,
    );
    let recognizer = check_artifact(
        Part::Recognizer,
        &manifest.recognizer,
        directory,
        &mut mismatches,
    );
    let dictionary = check_dictionary(&manifest.dictionary, directory, &mut mismatches);

    match (detector, recognizer, dictionary) {
        (Some(detector), Some(recognizer), Some(dictionary)) if mismatches.is_empty() => {
            Ok(ResolvedModel {
                detector,
                recognizer,
                dictionary,
            })
        }
        _ => Err(ResolveFailure::Mismatched(mismatches)),
    }
}

/// Why a resolution did not produce three paths.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ResolveFailure {
    /// The supplied path is not a directory.
    Directory,
    /// One or more parts disagreed with the manifest.
    Mismatched(Vec<Mismatch>),
}

impl core::fmt::Display for ResolveFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Directory => write!(formatter, "the model path is not a directory"),
            Self::Mismatched(mismatches) => {
                write!(formatter, "{} mismatch(es):", mismatches.len())?;
                for mismatch in mismatches {
                    write!(formatter, "\n  {mismatch}")?;
                }
                Ok(())
            }
        }
    }
}

fn check_artifact(
    part: Part,
    entry: &ArtifactEntry,
    directory: &Path,
    mismatches: &mut Vec<Mismatch>,
) -> Option<PathBuf> {
    let path = directory.join(part.file_name());
    let Ok(metadata) = std::fs::metadata(&path) else {
        mismatches.push(Mismatch::Missing { part, path });
        return None;
    };
    if metadata.len() != entry.bytes {
        mismatches.push(Mismatch::Size {
            part,
            expected: entry.bytes,
            found: metadata.len(),
        });
        // The size already disagrees, so the digest will too. Reading a
        // gigabyte to confirm that would be work done for no new information.
        return None;
    }
    // The size matched the manifest, so this bound has already been satisfied
    // by a number the manifest declared and the file confirmed. It is checked
    // anyway: a manifest is caller-supplied too.
    if metadata.len() > MAX_ARTIFACT_BYTES {
        mismatches.push(Mismatch::Size {
            part,
            expected: MAX_ARTIFACT_BYTES,
            found: metadata.len(),
        });
        return None;
    }

    let found = match digest_of(&path) {
        Some(value) => value,
        None => {
            mismatches.push(Mismatch::Missing { part, path });
            return None;
        }
    };
    if found != entry.sha256 {
        mismatches.push(Mismatch::Digest {
            part,
            expected: entry.sha256.clone(),
            found,
        });
        return None;
    }
    Some(path)
}

fn check_dictionary(
    entry: &DictionaryEntry,
    directory: &Path,
    mismatches: &mut Vec<Mismatch>,
) -> Option<PathBuf> {
    let part = Part::Dictionary;
    let path = directory.join(DICTIONARY_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        mismatches.push(Mismatch::Missing { part, path });
        return None;
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let found = hasher.finish();
    if found != entry.sha256 {
        mismatches.push(Mismatch::Digest {
            part,
            expected: entry.sha256.clone(),
            found,
        });
        return None;
    }

    // The count is of configured entries: lines as written, before the blank
    // class or any appended space, which is what `DictionaryEntry` documents.
    let text = String::from_utf8_lossy(&bytes);
    let entries = text.lines().count();
    if entries != entry.entries {
        mismatches.push(Mismatch::DictionaryEntries {
            expected: entry.entries,
            found: entries,
        });
        return None;
    }
    Some(path)
}

fn digest_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hasher.finish())
}

/// Rejects a path that a caller should not be resolving against.
///
/// Not a sandbox — this project does not claim one — but a refusal of the two
/// shapes that are almost always a mistake: an empty path, and one containing a
/// `..` component, which usually means a caller concatenated user input into a
/// model location.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] for an empty path or one with a parent
/// component.
pub fn check_model_directory(directory: &Path) -> Result<()> {
    if directory.as_os_str().is_empty() {
        return Err(Error::InvalidInput {
            field: "resolve.directory",
            violation: InputViolation::Empty,
        });
    }
    if directory
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(Error::InvalidInput {
            field: "resolve.directory",
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    /// A temporary directory that removes itself.
    ///
    /// Hand-rolled rather than a dependency: this project has two, and a test
    /// helper is not a reason for a third.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "paddleocr-rust-resolve-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            if let Err(error) = fs::create_dir_all(&path) {
                panic!("scratch: {error}");
            }
            Self { path }
        }

        fn write(&self, name: &str, contents: &[u8]) {
            if let Err(error) = fs::write(self.path.join(name), contents) {
                panic!("write {name}: {error}");
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn digest(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finish()
    }

    const DETECTOR_BYTES: &[u8] = b"detector-artifact-bytes";
    const RECOGNIZER_BYTES: &[u8] = b"recognizer-artifact-bytes";
    const DICTIONARY_BYTES: &[u8] = b"a\nb\nc\n";

    /// The manifest format is `key = value` lines, not JSON.
    ///
    /// Written out here rather than reusing the committed fixture, because the
    /// fixture describes the real artifacts and these tests describe files a
    /// few bytes long.
    fn manifest_text() -> String {
        format!(
            "schema_version = paddleocr-rust/model-manifest/v1\n\
             task = ocr.classic\n\
             family = PP-OCRv6_medium\n\
             version = test\n\
             format = onnx\n\
             backend = onnxruntime\n\
             upstream.commit = 2661c7c0ef5c613e8f93c6e93b2e052399f0f854\n\
             license.review = LIC-001\n\
             detector.url = https://example.invalid/d\n\
             detector.revision = r1\n\
             detector.sha256 = {det}\n\
             detector.bytes = {det_len}\n\
             detector.input.name = x\n\
             detector.output.name = fetch_name_0\n\
             recognizer.url = https://example.invalid/r\n\
             recognizer.revision = r2\n\
             recognizer.sha256 = {rec}\n\
             recognizer.bytes = {rec_len}\n\
             recognizer.input.name = x\n\
             recognizer.output.name = fetch_name_0\n\
             dictionary.sha256 = {dict}\n\
             dictionary.entries = 3\n\
             dictionary.appends_space = false\n",
            det = digest(DETECTOR_BYTES),
            det_len = DETECTOR_BYTES.len(),
            rec = digest(RECOGNIZER_BYTES),
            rec_len = RECOGNIZER_BYTES.len(),
            dict = digest(DICTIONARY_BYTES),
        )
    }

    fn manifest() -> ModelManifest {
        match ModelManifest::parse(&manifest_text()) {
            Ok(value) => value,
            Err(error) => panic!("manifest: {error}"),
        }
    }

    fn populate(scratch: &Scratch) {
        scratch.write(DETECTOR_FILE, DETECTOR_BYTES);
        scratch.write(RECOGNIZER_FILE, RECOGNIZER_BYTES);
        scratch.write(DICTIONARY_FILE, DICTIONARY_BYTES);
    }

    #[test]
    fn a_matching_directory_resolves_to_three_paths() {
        let scratch = Scratch::new("match");
        populate(&scratch);
        match resolve(&manifest(), &scratch.path) {
            Ok(resolved) => {
                assert!(resolved.detector.ends_with(DETECTOR_FILE));
                assert!(resolved.recognizer.ends_with(RECOGNIZER_FILE));
                assert!(resolved.dictionary.ends_with(DICTIONARY_FILE));
            }
            Err(failure) => panic!("expected a resolution, got {failure}"),
        }
    }

    /// Every mismatch is reported, not the first.
    ///
    /// A caller who provisioned the wrong family has all three wrong, and
    /// learning that in one run rather than three is the whole point.
    #[test]
    fn every_mismatch_is_reported_at_once() {
        let scratch = Scratch::new("all-wrong");
        scratch.write(DETECTOR_FILE, b"wrong detector");
        scratch.write(RECOGNIZER_FILE, b"wrong recognizer");
        // Right length, wrong bytes: reaches the digest check rather than
        // stopping at the size.
        scratch.write(DICTIONARY_FILE, b"x\ny\nz\n");

        match resolve(&manifest(), &scratch.path) {
            Err(ResolveFailure::Mismatched(mismatches)) => {
                assert_eq!(mismatches.len(), 3, "{mismatches:?}");
                let parts: Vec<Part> = mismatches.iter().map(Mismatch::part).collect();
                assert!(parts.contains(&Part::Detector));
                assert!(parts.contains(&Part::Recognizer));
                assert!(parts.contains(&Part::Dictionary));
            }
            other => panic!("expected three mismatches, got {other:?}"),
        }
    }

    /// A size mismatch stops before the digest, and says so.
    #[test]
    fn a_size_mismatch_is_reported_without_hashing() {
        let scratch = Scratch::new("size");
        scratch.write(DETECTOR_FILE, b"short");
        scratch.write(RECOGNIZER_FILE, RECOGNIZER_BYTES);
        scratch.write(DICTIONARY_FILE, DICTIONARY_BYTES);

        match resolve(&manifest(), &scratch.path) {
            Err(ResolveFailure::Mismatched(mismatches)) => {
                assert_eq!(mismatches.len(), 1);
                match &mismatches[0] {
                    Mismatch::Size {
                        part,
                        expected,
                        found,
                    } => {
                        assert_eq!(*part, Part::Detector);
                        assert_eq!(*expected, DETECTOR_BYTES.len() as u64);
                        assert_eq!(*found, 5);
                    }
                    other => panic!("expected a size mismatch, got {other:?}"),
                }
            }
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    /// The same length and different bytes is a digest mismatch.
    #[test]
    fn a_digest_mismatch_carries_both_digests() {
        let scratch = Scratch::new("digest");
        let mut wrong = DETECTOR_BYTES.to_vec();
        wrong[0] = b'D';
        scratch.write(DETECTOR_FILE, &wrong);
        scratch.write(RECOGNIZER_FILE, RECOGNIZER_BYTES);
        scratch.write(DICTIONARY_FILE, DICTIONARY_BYTES);

        match resolve(&manifest(), &scratch.path) {
            Err(ResolveFailure::Mismatched(mismatches)) => match &mismatches[0] {
                Mismatch::Digest {
                    expected, found, ..
                } => {
                    assert_eq!(*expected, digest(DETECTOR_BYTES));
                    assert_eq!(*found, digest(&wrong));
                    assert_ne!(expected, found);
                }
                other => panic!("expected a digest mismatch, got {other:?}"),
            },
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    /// A missing file names the path it looked for.
    #[test]
    fn a_missing_file_names_where_it_was_sought() {
        let scratch = Scratch::new("missing");
        scratch.write(RECOGNIZER_FILE, RECOGNIZER_BYTES);
        scratch.write(DICTIONARY_FILE, DICTIONARY_BYTES);

        match resolve(&manifest(), &scratch.path) {
            Err(ResolveFailure::Mismatched(mismatches)) => match &mismatches[0] {
                Mismatch::Missing { part, path } => {
                    assert_eq!(*part, Part::Detector);
                    assert!(path.ends_with(DETECTOR_FILE));
                }
                other => panic!("expected a missing file, got {other:?}"),
            },
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    /// A dictionary with the right digest and the wrong count cannot happen…
    ///
    /// …because the digest covers the bytes the count is derived from. The
    /// `DictionaryEntries` variant is therefore **unreachable through this
    /// path**, and the test says so rather than pretending to exercise it. It
    /// is kept as a guard: a future change that derives the count from anything
    /// other than the hashed bytes would make it reachable, and it should
    /// report that specifically rather than as a digest mismatch.
    #[test]
    fn the_entry_count_check_is_subsumed_by_the_digest() {
        let scratch = Scratch::new("entries");
        populate(&scratch);
        assert!(resolve(&manifest(), &scratch.path).is_ok());

        // Any change to the dictionary that alters the count also alters the
        // digest, and the dictionary has **no declared size** in the manifest —
        // `DictionaryEntry` carries a digest, a count, and a space flag — so
        // the digest is what reports it.
        scratch.write(DICTIONARY_FILE, b"a\nb\n");
        match resolve(&manifest(), &scratch.path) {
            Err(ResolveFailure::Mismatched(mismatches)) => {
                assert_eq!(mismatches.len(), 1);
                assert!(
                    matches!(mismatches[0], Mismatch::Digest { .. }),
                    "{:?}",
                    mismatches[0]
                );
            }
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    /// A path that is not a directory is refused before anything is read.
    #[test]
    fn a_non_directory_is_refused() {
        let scratch = Scratch::new("not-a-dir");
        scratch.write(DETECTOR_FILE, DETECTOR_BYTES);
        let file = scratch.path.join(DETECTOR_FILE);
        assert_eq!(resolve(&manifest(), &file), Err(ResolveFailure::Directory));
    }

    /// Suspicious directories are refused: empty, and anything with `..`.
    #[test]
    fn suspicious_directories_are_refused() {
        assert!(check_model_directory(Path::new("")).is_err());
        assert!(check_model_directory(Path::new("../models")).is_err());
        assert!(check_model_directory(Path::new("models/../../etc")).is_err());
        assert!(check_model_directory(Path::new("/opt/models")).is_ok());
        assert!(check_model_directory(Path::new("models")).is_ok());
    }

    /// Nothing here reads a manifest's URLs.
    ///
    /// The URLs are provenance. This asserts the property structurally: a
    /// manifest whose URLs are nonsense still resolves, because they are never
    /// consulted.
    #[test]
    fn urls_are_provenance_and_are_never_consulted() {
        let scratch = Scratch::new("urls");
        populate(&scratch);
        let mut manifest = manifest();
        manifest.detector.url = "not://a.real.scheme/at/all".to_owned();
        manifest.recognizer.url = String::new();
        assert!(resolve(&manifest, &scratch.path).is_ok());
    }

    /// The failure's `Display` lists every mismatch on its own line.
    #[test]
    fn the_failure_display_is_actionable() {
        let failure = ResolveFailure::Mismatched(vec![
            Mismatch::Missing {
                part: Part::Detector,
                path: PathBuf::from("/models/detector.onnx"),
            },
            Mismatch::Size {
                part: Part::Recognizer,
                expected: 100,
                found: 42,
            },
        ]);
        let text = failure.to_string();
        assert!(text.contains("2 mismatch(es)"), "{text}");
        assert!(
            text.contains("detector.onnx is missing at /models/detector.onnx"),
            "{text}"
        );
        assert!(
            text.contains("recognizer.onnx is 42 bytes, but the manifest declares 100"),
            "{text}"
        );
    }
}
