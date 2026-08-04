// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Script classification for dictionary contents.
//!
//! Roadmap item `LANG-001` requires that only *verified* model and language
//! mappings are added, and that generic multilingual support is never inferred
//! from one artifact. This module exists to make that distinction checkable
//! rather than rhetorical.
//!
//! What it reports is a fact about a dictionary file: how many Unicode scalars
//! in it fall in each range. What it deliberately does **not** report is a
//! language, or a supported one. The two are routinely conflated, and the gap
//! is large. The pinned `PP-OCRv6` dictionary contains emoji and box-drawing
//! characters; nobody would claim this port recognises emoji from a photograph
//! because the dictionary can spell them. A class exists in the output layer.
//! That is all a census can tell you.
//!
//! Ranges are named after Unicode blocks, not languages, for the same reason.
//! `CjkUnified` is a block; Chinese, Japanese, and Korean are languages, and no
//! count of scalars decides which of them a model was trained for.

/// A Unicode range a dictionary entry's scalars can fall into.
///
/// These are blocks, not languages. The names follow Unicode's own, and the
/// coarse groupings — `Symbols`, `Emoji`, `Other` — exist so that a census
/// accounts for every scalar rather than quietly dropping the ones that do not
/// fit a tidy story.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Script {
    /// `U+0000`–`U+007F`.
    Ascii,
    /// `U+0080`–`U+024F`, Latin-1 Supplement through Latin Extended-B.
    Latin,
    /// `U+0370`–`U+03FF`.
    Greek,
    /// `U+0400`–`U+04FF`.
    Cyrillic,
    /// `U+2000`–`U+206F`, general punctuation.
    Punctuation,
    /// `U+3000`–`U+303F`, CJK symbols and punctuation.
    CjkSymbols,
    /// `U+3040`–`U+309F`.
    Hiragana,
    /// `U+30A0`–`U+30FF`.
    Katakana,
    /// `U+3400`–`U+4DBF`, CJK Unified Ideographs Extension A.
    CjkExtensionA,
    /// `U+4E00`–`U+9FFF`, CJK Unified Ideographs.
    CjkUnified,
    /// `U+AC00`–`U+D7AF`, precomposed Hangul syllables.
    HangulSyllables,
    /// `U+FF00`–`U+FFEF`, halfwidth and fullwidth forms.
    HalfwidthAndFullwidth,
    /// Arrows, mathematical operators, technical and geometric shapes.
    Symbols,
    /// `U+1F300`–`U+1FAFF`, pictographs.
    Emoji,
    /// Everything else, so a census never silently loses a scalar.
    Other,
}

impl Script {
    /// Classifies one scalar by the Unicode range it falls in.
    #[must_use]
    pub fn of(scalar: char) -> Self {
        let code = scalar as u32;
        match code {
            0x0000..=0x007F => Self::Ascii,
            0x0080..=0x024F => Self::Latin,
            0x0370..=0x03FF => Self::Greek,
            0x0400..=0x04FF => Self::Cyrillic,
            0x2000..=0x206F => Self::Punctuation,
            // Arrows, mathematical operators, technical, box drawing, block
            // elements, geometric shapes, miscellaneous and dingbat symbols.
            0x2190..=0x2BFF => Self::Symbols,
            0x3000..=0x303F => Self::CjkSymbols,
            0x3040..=0x309F => Self::Hiragana,
            0x30A0..=0x30FF => Self::Katakana,
            0x3400..=0x4DBF => Self::CjkExtensionA,
            0x4E00..=0x9FFF => Self::CjkUnified,
            0xAC00..=0xD7AF => Self::HangulSyllables,
            0xFF00..=0xFFEF => Self::HalfwidthAndFullwidth,
            0x1F300..=0x1FAFF => Self::Emoji,
            _ => Self::Other,
        }
    }

    /// Returns the stable name used in reports and documentation.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Latin => "Latin",
            Self::Greek => "Greek",
            Self::Cyrillic => "Cyrillic",
            Self::Punctuation => "Punctuation",
            Self::CjkSymbols => "CJK Symbols",
            Self::Hiragana => "Hiragana",
            Self::Katakana => "Katakana",
            Self::CjkExtensionA => "CJK Extension A",
            Self::CjkUnified => "CJK Unified",
            Self::HangulSyllables => "Hangul Syllables",
            Self::HalfwidthAndFullwidth => "Halfwidth and Fullwidth",
            Self::Symbols => "Symbols",
            Self::Emoji => "Emoji",
            Self::Other => "Other",
        }
    }
}

/// How many scalars of one script a dictionary holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScriptCount {
    /// The script counted.
    pub script: Script,
    /// The number of scalars, counting repeats across entries.
    pub scalars: usize,
}

/// Counts the scripts present across dictionary entries.
///
/// The result is sorted by descending count, then by script, so it is stable
/// for two dictionaries with the same contents regardless of entry order.
#[must_use]
pub fn census<'a>(entries: impl Iterator<Item = &'a str>) -> Vec<ScriptCount> {
    let mut counts: std::collections::BTreeMap<Script, usize> = std::collections::BTreeMap::new();
    for entry in entries {
        for scalar in entry.chars() {
            *counts.entry(Script::of(scalar)).or_insert(0) += 1;
        }
    }
    let mut census: Vec<ScriptCount> = counts
        .into_iter()
        .map(|(script, scalars)| ScriptCount { script, scalars })
        .collect();
    census.sort_by(|left, right| {
        right
            .scalars
            .cmp(&left.scalars)
            .then_with(|| left.script.cmp(&right.script))
    });
    census
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_land_in_their_declared_ranges() {
        assert_eq!(Script::of('A'), Script::Ascii);
        assert_eq!(Script::of('\u{00e9}'), Script::Latin);
        assert_eq!(Script::of('\u{03b1}'), Script::Greek);
        assert_eq!(Script::of('\u{0416}'), Script::Cyrillic);
        assert_eq!(Script::of('\u{2014}'), Script::Punctuation);
        assert_eq!(Script::of('\u{2192}'), Script::Symbols);
        assert_eq!(Script::of('\u{3000}'), Script::CjkSymbols);
        assert_eq!(Script::of('\u{3042}'), Script::Hiragana);
        assert_eq!(Script::of('\u{30a2}'), Script::Katakana);
        assert_eq!(Script::of('\u{3402}'), Script::CjkExtensionA);
        assert_eq!(Script::of('\u{4f60}'), Script::CjkUnified);
        assert_eq!(Script::of('\u{d55c}'), Script::HangulSyllables);
        assert_eq!(Script::of('\u{ff21}'), Script::HalfwidthAndFullwidth);
        assert_eq!(Script::of('\u{1f600}'), Script::Emoji);
        assert_eq!(Script::of('\u{05d0}'), Script::Other, "Hebrew is unlisted");
    }

    /// The ideographic space is `CjkSymbols`, not `Ascii`, which is the same
    /// exact-scalar distinction the dictionary itself enforces.
    #[test]
    fn the_ideographic_space_is_not_an_ascii_space() {
        assert_eq!(Script::of(' '), Script::Ascii);
        assert_eq!(Script::of('\u{3000}'), Script::CjkSymbols);
    }

    #[test]
    fn a_census_counts_every_scalar_and_sorts_by_size() {
        let entries = ["a", "b", "c", "\u{4f60}", "\u{4f60}\u{597d}", "\u{1f600}"];
        let census = census(entries.into_iter());
        // Both have three scalars, so the tie breaks by script order, and
        // `Ascii` is declared before `CjkUnified`.
        assert_eq!(
            census[0],
            ScriptCount {
                script: Script::Ascii,
                scalars: 3
            }
        );
        assert_eq!(
            census[1],
            ScriptCount {
                script: Script::CjkUnified,
                scalars: 3
            }
        );
        let total: usize = census.iter().map(|entry| entry.scalars).sum();
        assert_eq!(total, 7, "no scalar may be lost");
    }

    #[test]
    fn an_empty_census_is_empty_rather_than_a_zero_row() {
        assert!(census(std::iter::empty()).is_empty());
    }
}
