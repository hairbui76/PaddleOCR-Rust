// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The recognizer's CTC class-to-text dictionary.
//!
//! `docs/LOCAL_ONNX_CANDIDATE_INSPECTION.md` records the exact index
//! construction this module implements, derived from the pinned PaddleX
//! `CTCLabelDecode` source rather than guessed from the artifact:
//!
//! ```text
//! index 0                    = CTC blank
//! index 1 ..= entry_count    = the configured entries, in file order
//! index entry_count + 1      = a single appended U+0020 space
//! ```
//!
//! The rule the inspection insists on is **exact-scalar preservation**: each
//! class maps to its original scalar and the resulting UTF-8 is emitted
//! unchanged. Default NFC or NFKC normalization, case folding, and whitespace
//! cleanup are all disallowed, because the recorded dictionary contains an
//! entry that is a distinct scalar from the appended space (`U+3000`), and
//! normalising would silently merge classes the model separates.
//!
//! This module never reads a model. It accepts an already parsed entry list, so
//! the caller owns file access, and it holds only what it was given.

use crate::ctc::CtcGreedyPath;
use crate::error::{Error, InputViolation, ModelProblem, Result};

/// Maximum number of configured dictionary entries.
///
/// The recorded candidate declares 18,708 entries; this bound leaves room for
/// another artifact without admitting an unbounded allocation.
const MAX_DICTIONARY_ENTRIES: usize = 65_534;

/// The scalar appended as the final class by the pinned decoder construction.
const APPENDED_SPACE: char = ' ';

/// An ordered CTC dictionary bound to one recognizer artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CtcDictionary {
    /// Entries in configured order; index `n` here is class index `n + 1`.
    entries: Vec<String>,
    appends_space: bool,
}

impl CtcDictionary {
    /// Returns the configured entries in class order.
    pub(crate) fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Builds a dictionary from entries already parsed by the caller.
    ///
    /// `appends_space` mirrors the artifact's `use_space_char`. The recorded
    /// candidate sets it, which is why its class count is entries + 2.
    pub(crate) fn new(entries: Vec<String>, appends_space: bool) -> Result<Self> {
        if entries.is_empty() {
            return Err(Error::InvalidInput {
                field: "dictionary.entries",
                violation: InputViolation::Empty,
            });
        }
        if entries.len() > MAX_DICTIONARY_ENTRIES {
            return Err(Error::ResourceLimit {
                resource: "dictionary.entries",
                limit: MAX_DICTIONARY_ENTRIES as u64,
                actual: entries.len() as u64,
            });
        }
        if entries.iter().any(String::is_empty) {
            return Err(Error::InvalidInput {
                field: "dictionary.entries",
                violation: InputViolation::Empty,
            });
        }
        Ok(Self {
            entries,
            appends_space,
        })
    }

    /// Returns the number of configured entries, excluding blank and space.
    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns the total class count the model output must declare.
    ///
    /// This is the value that must equal the recognizer tensor's last axis. A
    /// mismatch is a model contract error rather than a decoding fallback.
    pub(crate) fn class_count(&self) -> usize {
        self.entries.len() + 1 + usize::from(self.appends_space)
    }

    /// Returns the text for one class index, or `None` for the blank.
    ///
    /// An index at or beyond [`Self::class_count`] is a contract error.
    pub(crate) fn text_for(&self, class_index: u32) -> Result<Option<&str>> {
        let class_index = class_index as usize;
        if class_index == 0 {
            return Ok(None);
        }
        if class_index <= self.entries.len() {
            return Ok(Some(&self.entries[class_index - 1]));
        }
        if self.appends_space && class_index == self.entries.len() + 1 {
            // Deliberately not an entry lookup: the space is appended by the
            // decoder construction, not present in the configured list.
            return Ok(Some(" "));
        }
        Err(Error::Model {
            problem: ModelProblem::TensorContract,
        })
    }

    /// Checks that a model's declared class count matches this dictionary.
    pub(crate) fn require_class_count(&self, declared: usize) -> Result<()> {
        if declared != self.class_count() {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        Ok(())
    }

    /// Maps one greedy CTC path to text, preserving scalars exactly.
    ///
    /// The returned string is the concatenation of each retained class's text
    /// in timestep order. No normalization, case folding, trimming, or
    /// whitespace collapsing is applied.
    pub(crate) fn decode(&self, path: &CtcGreedyPath) -> Result<String> {
        let mut text = String::new();
        for class_index in path.class_indices() {
            if let Some(fragment) = self.text_for(*class_index)? {
                text.push_str(fragment);
            }
        }
        Ok(text)
    }

    /// Returns the appended space scalar, for callers that must document it.
    pub(crate) const fn appended_space(&self) -> Option<char> {
        if self.appends_space {
            Some(APPENDED_SPACE)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ctc::{CtcScoreMatrix, classic_ctc_greedy_indices};

    fn dictionary(entries: &[&str], appends_space: bool) -> CtcDictionary {
        let entries = entries.iter().map(|entry| (*entry).to_owned()).collect();
        match CtcDictionary::new(entries, appends_space) {
            Ok(dictionary) => dictionary,
            Err(error) => panic!("expected a valid dictionary, got {error}"),
        }
    }

    #[test]
    fn class_indices_follow_the_recorded_construction() {
        let dictionary = dictionary(&["a", "b", "c"], true);
        assert_eq!(dictionary.entry_count(), 3);
        // 1 blank + 3 entries + 1 appended space.
        assert_eq!(dictionary.class_count(), 5);

        assert!(matches!(dictionary.text_for(0), Ok(None)));
        assert!(matches!(dictionary.text_for(1), Ok(Some("a"))));
        assert!(matches!(dictionary.text_for(3), Ok(Some("c"))));
        assert!(matches!(dictionary.text_for(4), Ok(Some(" "))));
        assert!(matches!(
            dictionary.text_for(5),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn a_dictionary_without_an_appended_space_has_one_fewer_class() {
        let dictionary = dictionary(&["a", "b", "c"], false);
        assert_eq!(dictionary.class_count(), 4);
        assert_eq!(dictionary.appended_space(), None);
        assert!(matches!(
            dictionary.text_for(4),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    /// The appended space must stay distinct from a space-like entry.
    ///
    /// The recorded candidate contains `U+3000` as a configured entry while the
    /// decoder appends `U+0020`. Merging them would destroy a class the model
    /// separates, so this test pins that they decode differently.
    #[test]
    fn an_ideographic_space_entry_stays_distinct_from_the_appended_space() {
        let dictionary = dictionary(&["\u{3000}", "x"], true);
        assert!(matches!(dictionary.text_for(1), Ok(Some("\u{3000}"))));
        assert!(matches!(dictionary.text_for(3), Ok(Some(" "))));
        let entry = match dictionary.text_for(1) {
            Ok(Some(value)) => value,
            other => panic!("expected the configured entry, got {other:?}"),
        };
        let appended = match dictionary.text_for(3) {
            Ok(Some(value)) => value,
            other => panic!("expected the appended space, got {other:?}"),
        };
        assert_ne!(entry, appended);
    }

    #[test]
    fn decoding_preserves_scalars_without_normalization() {
        // A decomposed sequence must not be recomposed, and a full-width Latin
        // capital must not be folded or converted.
        let dictionary = dictionary(&["e\u{0301}", "\u{FF21}", "\u{00E9}"], false);
        let matrix_values = [
            // time 0 selects class 1, time 1 selects class 2, time 2 class 3.
            0.1, 0.9, 0.0, 0.0, //
            0.1, 0.0, 0.8, 0.0, //
            0.1, 0.0, 0.0, 0.7, //
        ];
        let matrix = match CtcScoreMatrix::new(3, 4, &matrix_values) {
            Ok(matrix) => matrix,
            Err(error) => panic!("expected a valid matrix, got {error}"),
        };
        let path = match classic_ctc_greedy_indices(matrix) {
            Ok(path) => path,
            Err(error) => panic!("expected a greedy path, got {error}"),
        };
        assert_eq!(path.class_indices(), [1, 2, 3]);

        let text = match dictionary.decode(&path) {
            Ok(text) => text,
            Err(error) => panic!("expected decoded text, got {error}"),
        };
        assert_eq!(text, "e\u{0301}\u{FF21}\u{00E9}");
        // Exactly the scalars supplied, in order: nothing was recomposed.
        assert_eq!(text.chars().count(), 4);
    }

    #[test]
    fn blanks_are_dropped_and_repeats_collapse_before_mapping() {
        let dictionary = dictionary(&["a", "b"], false);
        // Raw argmax path a, a, blank, a, b collapses to a, a, b.
        let matrix_values = [
            0.0, 0.9, 0.1, //
            0.0, 0.9, 0.1, //
            0.9, 0.0, 0.1, //
            0.0, 0.9, 0.1, //
            0.0, 0.1, 0.9, //
        ];
        let matrix = match CtcScoreMatrix::new(5, 3, &matrix_values) {
            Ok(matrix) => matrix,
            Err(error) => panic!("expected a valid matrix, got {error}"),
        };
        let path = match classic_ctc_greedy_indices(matrix) {
            Ok(path) => path,
            Err(error) => panic!("expected a greedy path, got {error}"),
        };
        let text = match dictionary.decode(&path) {
            Ok(text) => text,
            Err(error) => panic!("expected decoded text, got {error}"),
        };
        assert_eq!(text, "aab");
    }

    #[test]
    fn an_out_of_range_class_is_a_contract_error_not_a_panic() {
        let dictionary = dictionary(&["a"], false);
        // A model declaring three classes against a two-class dictionary.
        let matrix_values = [0.1, 0.2, 0.9];
        let matrix = match CtcScoreMatrix::new(1, 3, &matrix_values) {
            Ok(matrix) => matrix,
            Err(error) => panic!("expected a valid matrix, got {error}"),
        };
        let path = match classic_ctc_greedy_indices(matrix) {
            Ok(path) => path,
            Err(error) => panic!("expected a greedy path, got {error}"),
        };
        assert_eq!(path.class_indices(), [2]);
        assert!(matches!(
            dictionary.decode(&path),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn a_declared_class_count_mismatch_is_rejected_up_front() {
        let dictionary = dictionary(&["a", "b"], true);
        assert!(dictionary.require_class_count(4).is_ok());
        assert!(matches!(
            dictionary.require_class_count(3),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
        assert!(matches!(
            dictionary.require_class_count(5),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn dictionaries_reject_empty_and_oversized_entry_lists() {
        assert!(matches!(
            CtcDictionary::new(Vec::new(), true),
            Err(Error::InvalidInput {
                field: "dictionary.entries",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            CtcDictionary::new(vec!["a".to_owned(), String::new()], true),
            Err(Error::InvalidInput {
                field: "dictionary.entries",
                violation: InputViolation::Empty,
            })
        ));
        let oversized = vec!["a".to_owned(); MAX_DICTIONARY_ENTRIES + 1];
        assert!(matches!(
            CtcDictionary::new(oversized, true),
            Err(Error::ResourceLimit {
                resource: "dictionary.entries",
                ..
            })
        ));
    }

    /// The recorded candidate's arithmetic, checked without shipping its data.
    #[test]
    fn the_recorded_candidate_class_count_is_reproduced_by_the_construction() {
        // 18,708 self-authored placeholder entries stand in for the recorded
        // candidate's entry count. No dictionary content is committed here.
        let entries = vec!["x".to_owned(); 18_708];
        let dictionary = match CtcDictionary::new(entries, true) {
            Ok(dictionary) => dictionary,
            Err(error) => panic!("expected a valid dictionary, got {error}"),
        };
        assert_eq!(dictionary.class_count(), 18_710);
        assert!(dictionary.require_class_count(18_710).is_ok());
    }
}
