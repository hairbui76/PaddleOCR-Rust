// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `CONFIG-001`: every upstream config accounted for, per file.
//!
//! The row requires each baseline config to be classified as Verified, an
//! intentional difference, Deferred, or a user-approved exclusion — and adds the
//! constraint that matters most: **no generic family claim from one config**.
//!
//! That constraint is what the tests below enforce. It is easy to write
//! "PP-OCRv6 detection is verified" after checking one file; the reconciliation
//! names **two** files as `Verified` and every other `DBPostProcess` config as
//! *postprocess implemented, parameters unverified*, which is a different and
//! true statement.
//!
//! # Why this is a record rather than a scan
//!
//! The classification is committed as a fixture. The upstream checkout is a
//! read-only symlink that a clean checkout of this repository does not have, and
//! the whole test suite must run without it. So the record stands alone, and one
//! test compares it against the checkout **when that is present** — which is the
//! only thing stopping the record from drifting away from the tree it describes.
//!
//! Nothing here is executable code: the reconciliation is evidence, and its
//! consumers are tests.
#![allow(dead_code)]

/// The committed reconciliation, as JSON.
pub const RECONCILIATION: &str =
    include_str!("../tests/fixtures/classic-v1-config-reconciliation/expected.json");

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    const RECORD: &str = super::RECONCILIATION;

    fn record() -> serde_json::Value {
        match serde_json::from_str(RECORD) {
            Ok(value) => value,
            Err(error) => panic!("reconciliation: {error}"),
        }
    }

    /// Every row carries a status and a reason, and the counts add up.
    #[test]
    fn every_config_is_classified_with_a_reason() {
        let record = record();
        let rows = match record["rows"].as_array() {
            Some(value) => value,
            None => panic!("rows"),
        };
        let total = record["total"].as_u64().unwrap_or(0) as usize;
        assert_eq!(rows.len(), total, "the declared total must match the rows");
        assert!(total > 100, "only {total} configs were reconciled");

        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for row in rows {
            let config = row["config"].as_str().unwrap_or_default();
            assert!(
                config.starts_with("configs/") && config.ends_with(".yml"),
                "unexpected config path {config:?}"
            );
            let status = row["status"].as_str().unwrap_or_default();
            assert!(!status.is_empty(), "{config}: no status");
            let reason = row["reason"].as_str().unwrap_or_default();
            assert!(
                reason.len() > 20,
                "{config}: the reason must say something: {reason:?}"
            );
            *counts.entry(status.to_owned()).or_default() += 1;
        }

        let declared = match record["counts"].as_object() {
            Some(value) => value,
            None => panic!("counts"),
        };
        for (status, count) in &counts {
            assert_eq!(
                declared.get(status).and_then(serde_json::Value::as_u64),
                Some(*count as u64),
                "{status}: declared count disagrees with the rows"
            );
        }
        assert_eq!(
            declared.len(),
            counts.len(),
            "a declared status has no rows"
        );
    }

    /// `Verified` is claimed per file, never per family.
    ///
    /// The check has teeth: it finds every other config sharing a verified file's
    /// postprocess and asserts that **none** of them is also `Verified`. A future
    /// change that promoted a family would fail here.
    #[test]
    fn no_family_claim_is_made_from_one_config() {
        let record = record();
        let rows = match record["rows"].as_array() {
            Some(value) => value,
            None => panic!("rows"),
        };

        let verified: Vec<&serde_json::Value> = rows
            .iter()
            .filter(|row| row["status"].as_str() == Some("Verified"))
            .collect();
        assert!(!verified.is_empty(), "nothing is verified");
        assert!(
            verified.len() <= 4,
            "{} configs claim Verified; each needs its own capture",
            verified.len()
        );

        for row in &verified {
            let post = row["postprocess"].as_str().unwrap_or_default();
            let siblings = rows.iter().filter(|other| {
                other["postprocess"].as_str() == Some(post) && other["config"] != row["config"]
            });
            for sibling in siblings {
                assert_ne!(
                    sibling["status"].as_str(),
                    Some("Verified"),
                    "{} was promoted to Verified by sharing a postprocess with {}",
                    sibling["config"].as_str().unwrap_or("?"),
                    row["config"].as_str().unwrap_or("?")
                );
            }
        }
    }

    /// `Out of scope` needs a user decision and must not appear.
    #[test]
    fn out_of_scope_is_not_self_assigned() {
        let record = record();
        let rows = match record["rows"].as_array() {
            Some(value) => value,
            None => panic!("rows"),
        };
        for row in rows {
            assert_ne!(
                row["status"].as_str(),
                Some("Out of scope"),
                "{}: out of scope is a user-approved exclusion, not one this \
             reconciliation may assign",
                row["config"].as_str().unwrap_or("?")
            );
        }
    }

    /// When the pinned checkout is present, the record still describes it.
    ///
    /// Skipped without it, because the whole test suite must run in a clean
    /// checkout with no upstream. Present, it is the only thing that stops the
    /// record from drifting away from the tree it claims to reconcile.
    #[test]
    fn the_record_matches_the_checkout_when_it_is_present() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("PaddleOCR/configs");
        if !root.is_dir() {
            eprintln!("skipped: the pinned checkout is not present");
            return;
        }

        let mut found = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(directory) = stack.pop() {
            let entries = match std::fs::read_dir(&directory) {
                Ok(value) => value,
                Err(error) => panic!("read {}: {error}", directory.display()),
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "yml") {
                    // `root` is `PaddleOCR/configs`, so stripping its parent
                    // already yields a `configs/...` path. Prepending another
                    // `configs/` is how the first version of this test doubled it.
                    let relative = match path.strip_prefix(root.parent().unwrap_or(&root)) {
                        Ok(value) => value,
                        Err(error) => panic!("strip: {error}"),
                    };
                    found.push(relative.display().to_string());
                }
            }
        }
        found.sort();

        let record = record();
        let mut recorded: Vec<String> = match record["rows"].as_array() {
            Some(rows) => rows
                .iter()
                .map(|row| row["config"].as_str().unwrap_or_default().to_owned())
                .collect(),
            None => panic!("rows"),
        };
        recorded.sort();

        assert_eq!(
            recorded, found,
            "the reconciliation and the checkout disagree about which configs exist"
        );
    }
}
