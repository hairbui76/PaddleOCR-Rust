// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Upstream's detection and recognition evaluation metrics.
//!
//! Roadmap item `METRIC-001`, the detection and recognition halves.
//!
//! # Why this exists
//!
//! Every compatibility row in this repository says **no accuracy claim**,
//! because no fixture here asserts what a model detects. That is honest and it
//! is also a gap: without upstream's own metric, *"this port agrees with
//! upstream"* and *"this port is as good as upstream"* cannot be asked as
//! separate questions. This is the tooling half of closing it.
//!
//! Frozen from the **pinned PaddleOCR checkout** rather than PaddleX —
//! `ppocr/metrics/` is in the checkout this project pinned first.
//!
//! # The matcher is greedy in index order, not best-first
//!
//! This is the behaviour most likely to be reimplemented differently by
//! accident. `evaluate_image` walks the ground-truth × detection grid in index
//! order and takes the **first** pair that clears the threshold, even when a
//! later detection matches the same region better.
//!
//! The captured `greedy_first_match_wins` case is exactly that: two detections
//! for one region, the second an exact match, and the **first** is the one that
//! matches. A best-first matcher would give the same match count here and a
//! different one on a denser page.
//!
//! # The threshold is strictly greater
//!
//! `iou > 0.5`, so a detection at exactly `0.5` does **not** match. The corpus
//! pins that boundary with a pair whose IoU is exactly `0.5`.
//!
//! # What is not here
//!
//! Table, KIE, and super-resolution metrics. They score modules this port does
//! not have, three of which have no published ONNX export — see
//! `docs/P8_ARTIFACT_AVAILABILITY.md`. Distributed reduction is also absent:
//! there is nothing to reduce across, since this port has no distributed
//! evaluation.
#![allow(dead_code)]

/// A polygon in evaluation coordinates.
pub type EvalPolygon = Vec<(f64, f64)>;

/// `DetectionIoUEvaluator`'s IoU threshold. A match needs **more** than this.
pub const IOU_CONSTRAINT: f64 = 0.5;

/// How much of a detection a don't-care region must cover to absorb it.
pub const AREA_PRECISION_CONSTRAINT: f64 = 0.5;

/// `RecMetric`'s epsilon, added to the denominator of both ratios.
///
/// It is why a perfect run scores `0.999_99…` rather than `1.0`, and
/// reproducing it is the difference between agreeing with upstream and being
/// almost right.
pub const METRIC_EPS: f64 = 1e-5;

/// One image's detection counts, before they become ratios.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ImageCounts {
    /// Ground-truth regions that are not don't-care.
    pub gt_care: usize,
    /// Detections not absorbed by a don't-care region.
    pub det_care: usize,
    /// Detections matched to a ground-truth region.
    pub matched: usize,
}

/// Precision, recall, and their harmonic mean.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectionMetrics {
    /// Matched detections over caring detections.
    pub precision: f64,
    /// Matched detections over caring ground truth.
    pub recall: f64,
    /// Harmonic mean of the two, `0` when both are `0`.
    pub hmean: f64,
}

/// Shoelace area of a polygon, unsigned.
fn area(polygon: &[(f64, f64)]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let mut twice = 0.0;
    for index in 0..polygon.len() {
        let (x1, y1) = polygon[index];
        let (x2, y2) = polygon[(index + 1) % polygon.len()];
        twice += x1 * y2 - x2 * y1;
    }
    twice.abs() / 2.0
}

/// Clips `subject` against one directed edge of a convex clipper.
///
/// Sutherland–Hodgman. Correct for **convex** clippers, which is what a
/// detection quadrilateral is; a self-intersecting polygon would need a
/// different algorithm, and upstream's `Polygon(...).is_valid` check rejects
/// those before they reach here.
fn clip_to_edge(
    subject: &[(f64, f64)],
    edge_start: (f64, f64),
    edge_end: (f64, f64),
) -> Vec<(f64, f64)> {
    let inside = |point: (f64, f64)| {
        (edge_end.0 - edge_start.0) * (point.1 - edge_start.1)
            - (edge_end.1 - edge_start.1) * (point.0 - edge_start.0)
            >= 0.0
    };
    let intersect = |a: (f64, f64), b: (f64, f64)| {
        let (x1, y1) = edge_start;
        let (x2, y2) = edge_end;
        let (x3, y3) = a;
        let (x4, y4) = b;
        let denominator = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
        if denominator.abs() < f64::EPSILON {
            return a;
        }
        let first = x1 * y2 - y1 * x2;
        let second = x3 * y4 - y3 * x4;
        (
            (first * (x3 - x4) - (x1 - x2) * second) / denominator,
            (first * (y3 - y4) - (y1 - y2) * second) / denominator,
        )
    };

    let mut output = Vec::with_capacity(subject.len() + 4);
    for index in 0..subject.len() {
        let current = subject[index];
        let previous = subject[(index + subject.len() - 1) % subject.len()];
        let (current_in, previous_in) = (inside(current), inside(previous));
        if current_in {
            if !previous_in {
                output.push(intersect(previous, current));
            }
            output.push(current);
        } else if previous_in {
            output.push(intersect(previous, current));
        }
    }
    output
}

/// Intersection area of two convex polygons.
pub fn intersection_area(first: &[(f64, f64)], second: &[(f64, f64)]) -> f64 {
    if first.len() < 3 || second.len() < 3 {
        return 0.0;
    }
    // Sutherland–Hodgman needs a counter-clockwise clipper; the signed area's
    // sign says which way this one runs, and reversing costs one allocation
    // against a wrong answer.
    let mut clipper = second.to_vec();
    let mut twice = 0.0;
    for index in 0..clipper.len() {
        let (x1, y1) = clipper[index];
        let (x2, y2) = clipper[(index + 1) % clipper.len()];
        twice += x1 * y2 - x2 * y1;
    }
    if twice < 0.0 {
        clipper.reverse();
    }

    let mut output = first.to_vec();
    for index in 0..clipper.len() {
        if output.is_empty() {
            return 0.0;
        }
        output = clip_to_edge(
            &output,
            clipper[index],
            clipper[(index + 1) % clipper.len()],
        );
    }
    area(&output)
}

/// Intersection over union of two convex polygons.
#[must_use]
pub fn polygon_iou(first: &[(f64, f64)], second: &[(f64, f64)]) -> f64 {
    let intersection = intersection_area(first, second);
    let union = area(first) + area(second) - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Evaluates one image's detections against its ground truth.
///
/// `ignore` marks don't-care ground truth. A detection more than
/// [`AREA_PRECISION_CONSTRAINT`] covered by any don't-care region is itself
/// treated as don't-care, which is how upstream stops an unlabelled region from
/// counting as a false positive.
#[must_use]
pub fn evaluate_image(
    ground_truth: &[EvalPolygon],
    ignore: &[bool],
    predicted: &[EvalPolygon],
) -> ImageCounts {
    let dont_care_gt: Vec<usize> = (0..ground_truth.len())
        .filter(|index| ignore.get(*index).copied().unwrap_or(false))
        .collect();

    let mut dont_care_det = vec![false; predicted.len()];
    for (index, detection) in predicted.iter().enumerate() {
        let detection_area = area(detection);
        for gt_index in &dont_care_gt {
            let covered = intersection_area(&ground_truth[*gt_index], detection);
            let precision = if detection_area == 0.0 {
                0.0
            } else {
                covered / detection_area
            };
            if precision > AREA_PRECISION_CONSTRAINT {
                dont_care_det[index] = true;
                break;
            }
        }
    }

    let mut gt_matched = vec![false; ground_truth.len()];
    let mut det_matched = vec![false; predicted.len()];
    let mut matched = 0_usize;
    // Index order, first match wins. Not best-first; see the module docs.
    for gt_index in 0..ground_truth.len() {
        for det_index in 0..predicted.len() {
            if gt_matched[gt_index]
                || det_matched[det_index]
                || dont_care_gt.contains(&gt_index)
                || dont_care_det[det_index]
            {
                continue;
            }
            if polygon_iou(&predicted[det_index], &ground_truth[gt_index]) > IOU_CONSTRAINT {
                gt_matched[gt_index] = true;
                det_matched[det_index] = true;
                matched += 1;
            }
        }
    }

    ImageCounts {
        gt_care: ground_truth.len() - dont_care_gt.len(),
        det_care: predicted.len() - dont_care_det.iter().filter(|value| **value).count(),
        matched,
    }
}

/// Combines per-image counts into precision, recall, and hmean.
///
/// The counts are summed **before** the ratios are taken, which is not the same
/// as averaging per-image ratios: a page with one region would otherwise weigh
/// as much as a page with fifty.
#[must_use]
pub fn combine_results(counts: &[ImageCounts]) -> DetectionMetrics {
    let gt_care: usize = counts.iter().map(|entry| entry.gt_care).sum();
    let det_care: usize = counts.iter().map(|entry| entry.det_care).sum();
    let matched: usize = counts.iter().map(|entry| entry.matched).sum();

    let recall = if gt_care == 0 {
        0.0
    } else {
        matched as f64 / gt_care as f64
    };
    let precision = if det_care == 0 {
        0.0
    } else {
        matched as f64 / det_care as f64
    };
    let hmean = if recall + precision == 0.0 {
        0.0
    } else {
        2.0 * recall * precision / (recall + precision)
    };
    DetectionMetrics {
        precision,
        recall,
        hmean,
    }
}

/// How `RecMetric` normalises a pair before comparing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RecognitionOptions {
    /// Remove every space from both strings. Upstream's default is `true`.
    pub ignore_space: bool,
    /// Keep only ASCII digits and letters, then lowercase. Default `false`.
    ///
    /// Destructive for non-Latin scripts: it reduces `你好` to the empty
    /// string, which then compares equal to any other all-removed text. That is
    /// upstream's behaviour and the corpus pins it rather than correcting it.
    pub is_filter: bool,
}

impl RecognitionOptions {
    /// Upstream's constructor defaults: `ignore_space` on, `is_filter` off.
    #[must_use]
    pub const fn upstream_defaults() -> Self {
        Self {
            ignore_space: true,
            is_filter: false,
        }
    }
}

/// Exact-match accuracy and normalised edit distance.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct RecognitionMetrics {
    /// Correct predictions over the total, with [`METRIC_EPS`] in the
    /// denominator.
    pub accuracy: f64,
    /// `1 −` mean normalised edit distance, with the same epsilon.
    pub normalized_edit_distance: f64,
}

/// `_normalize_text`: ASCII digits and letters only, lowercased.
fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Levenshtein distance in **characters**, not bytes.
///
/// Upstream compares Python `str`, whose unit is a code point. Comparing UTF-8
/// bytes would score a one-character CJK substitution as three edits.
fn levenshtein(first: &[char], second: &[char]) -> usize {
    if first.is_empty() {
        return second.len();
    }
    if second.is_empty() {
        return first.len();
    }
    let mut previous: Vec<usize> = (0..=second.len()).collect();
    let mut current = vec![0_usize; second.len() + 1];
    for (i, a) in first.iter().enumerate() {
        current[0] = i + 1;
        for (j, b) in second.iter().enumerate() {
            let cost = usize::from(a != b);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        previous.clone_from(&current);
    }
    previous[second.len()]
}

/// `rapidfuzz`'s `normalized_distance`: edits over the longer length.
///
/// Two empty strings score `0`, not a division by zero.
#[must_use]
pub fn normalized_edit_distance(first: &str, second: &str) -> f64 {
    let a: Vec<char> = first.chars().collect();
    let b: Vec<char> = second.chars().collect();
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 0.0;
    }
    levenshtein(&a, &b) as f64 / longest as f64
}

/// Scores predicted strings against their targets.
#[must_use]
pub fn recognition_metrics(
    pairs: &[(String, String)],
    options: RecognitionOptions,
) -> RecognitionMetrics {
    let mut correct = 0_usize;
    let mut edit = 0.0_f64;
    for (prediction, target) in pairs {
        let mut prediction = prediction.clone();
        let mut target = target.clone();
        if options.ignore_space {
            prediction = prediction.replace(' ', "");
            target = target.replace(' ', "");
        }
        if options.is_filter {
            prediction = normalize_text(&prediction);
            target = normalize_text(&target);
        }
        edit += normalized_edit_distance(&prediction, &target);
        if prediction == target {
            correct += 1;
        }
    }
    let total = pairs.len() as f64;
    RecognitionMetrics {
        accuracy: correct as f64 / (total + METRIC_EPS),
        normalized_edit_distance: 1.0 - edit / (total + METRIC_EPS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-metrics/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn read_polygons(value: &Value) -> Vec<EvalPolygon> {
        match value.as_array() {
            Some(values) => values
                .iter()
                .map(|polygon| match polygon.as_array() {
                    Some(points) => points
                        .iter()
                        .map(|point| match point.as_array() {
                            Some(pair) => (
                                pair[0].as_f64().unwrap_or(f64::NAN),
                                pair[1].as_f64().unwrap_or(f64::NAN),
                            ),
                            None => panic!("point"),
                        })
                        .collect(),
                    None => panic!("polygon"),
                })
                .collect(),
            None => panic!("polygons"),
        }
    }

    #[test]
    fn the_captured_constants_match() {
        let fixture = fixture();
        let constants = &fixture["constants"];
        assert!(
            (constants["iou_constraint"].as_f64().unwrap_or(0.0) - IOU_CONSTRAINT).abs() < 1e-12
        );
        assert!(
            (constants["area_precision_constraint"]
                .as_f64()
                .unwrap_or(0.0)
                - AREA_PRECISION_CONSTRAINT)
                .abs()
                < 1e-12
        );
        assert!((constants["eps"].as_f64().unwrap_or(0.0) - METRIC_EPS).abs() < 1e-15);
    }

    #[test]
    fn the_captured_detection_cases_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["detection"].as_array() {
            Some(value) => value,
            None => panic!("detection"),
        };
        assert_eq!(cases.len(), 9);
        let mut all = Vec::new();
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let ground_truth = read_polygons(&case["ground_truth"]);
            let predicted = read_polygons(&case["predicted"]);
            let ignore: Vec<bool> = match case["ignore"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_bool().unwrap_or(false))
                    .collect(),
                None => panic!("{name}: ignore"),
            };

            let counts = evaluate_image(&ground_truth, &ignore, &predicted);
            assert_eq!(
                counts.gt_care,
                case["gt_care"].as_u64().unwrap_or(0) as usize,
                "{name}: gt_care"
            );
            assert_eq!(
                counts.det_care,
                case["det_care"].as_u64().unwrap_or(0) as usize,
                "{name}: det_care"
            );
            assert_eq!(
                counts.matched,
                case["matched"].as_u64().unwrap_or(0) as usize,
                "{name}: matched"
            );

            let metrics = combine_results(&[counts]);
            for (label, actual, expected) in [
                ("precision", metrics.precision, case["precision"].as_f64()),
                ("recall", metrics.recall, case["recall"].as_f64()),
                ("hmean", metrics.hmean, case["hmean"].as_f64()),
            ] {
                let expected = expected.unwrap_or(f64::NAN);
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "{name}: {label}: {actual} vs {expected}"
                );
            }
            all.push(counts);
        }

        // The corpus, summed before the ratios are taken.
        let corpus = combine_results(&all);
        let expected = &fixture["detection_corpus"];
        for (label, actual, key) in [
            ("precision", corpus.precision, "precision"),
            ("recall", corpus.recall, "recall"),
            ("hmean", corpus.hmean, "hmean"),
        ] {
            let want = expected[key].as_f64().unwrap_or(f64::NAN);
            assert!(
                (actual - want).abs() < 1e-12,
                "corpus {label}: {actual} vs {want}"
            );
        }
    }

    /// A detection at exactly the threshold does not match.
    #[test]
    fn the_iou_threshold_is_strictly_greater() {
        let ground_truth = vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]];
        // Half the area of the union: IoU is exactly 0.5.
        let predicted = vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)]];
        assert!((polygon_iou(&predicted[0], &ground_truth[0]) - 0.5).abs() < 1e-12);
        assert_eq!(
            evaluate_image(&ground_truth, &[false], &predicted).matched,
            0,
            "an IoU of exactly the constraint must not match"
        );
    }

    /// The first detection in index order wins, not the best one.
    #[test]
    fn matching_is_greedy_in_index_order() {
        let ground_truth = vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]];
        let worse = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 11.0), (0.0, 11.0)];
        let exact = ground_truth[0].clone();
        let counts = evaluate_image(&ground_truth, &[false], &[worse, exact]);
        assert_eq!(counts.matched, 1);
        assert_eq!(counts.det_care, 2, "the unmatched detection still counts");
        // Precision is halved by the extra detection, which is the observable
        // consequence of matching the worse one first.
        let metrics = combine_results(&[counts]);
        assert!((metrics.precision - 0.5).abs() < 1e-12);
    }

    #[test]
    fn the_captured_recognition_cases_are_reproduced() {
        let fixture = fixture();
        let cases = match fixture["recognition"].as_array() {
            Some(value) => value,
            None => panic!("recognition"),
        };
        assert_eq!(cases.len(), 40);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let pairs = vec![(
                case["prediction"].as_str().unwrap_or_default().to_owned(),
                case["target"].as_str().unwrap_or_default().to_owned(),
            )];
            let options = RecognitionOptions {
                ignore_space: case["ignore_space"].as_bool().unwrap_or(false),
                is_filter: case["is_filter"].as_bool().unwrap_or(false),
            };
            let metrics = recognition_metrics(&pairs, options);
            for (label, actual, key) in [
                ("acc", metrics.accuracy, "acc"),
                (
                    "norm_edit_dis",
                    metrics.normalized_edit_distance,
                    "norm_edit_dis",
                ),
            ] {
                let want = case[key].as_f64().unwrap_or(f64::NAN);
                assert!(
                    (actual - want).abs() < 1e-9,
                    "{name}: {label}: {actual} vs {want}"
                );
            }
        }
    }

    #[test]
    fn the_captured_recognition_corpus_is_reproduced() {
        let fixture = fixture();
        let corpus = &fixture["recognition_corpus"];
        let pairs: Vec<(String, String)> = match corpus["pairs"].as_array() {
            Some(values) => values
                .iter()
                .map(|pair| match pair.as_array() {
                    Some(entry) => (
                        entry[0].as_str().unwrap_or_default().to_owned(),
                        entry[1].as_str().unwrap_or_default().to_owned(),
                    ),
                    None => panic!("pair"),
                })
                .collect(),
            None => panic!("pairs"),
        };
        let metrics = recognition_metrics(&pairs, RecognitionOptions::upstream_defaults());
        assert!((metrics.accuracy - corpus["acc"].as_f64().unwrap_or(f64::NAN)).abs() < 1e-9);
        assert!(
            (metrics.normalized_edit_distance
                - corpus["norm_edit_dis"].as_f64().unwrap_or(f64::NAN))
            .abs()
                < 1e-9
        );
    }

    /// The epsilon means a perfect run does not score `1.0`.
    ///
    /// Reproducing it is the difference between agreeing with upstream and
    /// being almost right, and a reader comparing two numbers needs to know
    /// which one they are looking at.
    #[test]
    fn a_perfect_run_scores_just_under_one() {
        let pairs = vec![("a".to_owned(), "a".to_owned())];
        let metrics = recognition_metrics(&pairs, RecognitionOptions::upstream_defaults());
        assert!(metrics.accuracy < 1.0, "{}", metrics.accuracy);
        assert!(metrics.accuracy > 0.9999, "{}", metrics.accuracy);
    }

    /// Edit distance counts characters, not bytes.
    #[test]
    fn edit_distance_counts_characters() {
        // One CJK substitution is one edit, not three.
        assert!((normalized_edit_distance("你好世界", "你好世间") - 0.25).abs() < 1e-12);
        assert!((normalized_edit_distance("", "") - 0.0).abs() < 1e-12);
        assert!((normalized_edit_distance("", "abc") - 1.0).abs() < 1e-12);
    }

    /// `is_filter` erases non-Latin text, and the corpus records it.
    #[test]
    fn the_filter_erases_non_latin_text() {
        assert_eq!(normalize_text("你好世界"), "");
        // Which makes two different CJK strings compare equal under it.
        let pairs = vec![("你好".to_owned(), "再见".to_owned())];
        let metrics = recognition_metrics(
            &pairs,
            RecognitionOptions {
                ignore_space: true,
                is_filter: true,
            },
        );
        assert!(
            metrics.accuracy > 0.9999,
            "both normalise to the empty string"
        );
    }

    /// Polygon intersection handles disjoint, nested, and identical shapes.
    #[test]
    fn polygon_intersection_covers_the_ordinary_cases() {
        let unit = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        let far = vec![(10.0, 10.0), (12.0, 10.0), (12.0, 12.0), (10.0, 12.0)];
        let inner = vec![(0.5, 0.5), (1.5, 0.5), (1.5, 1.5), (0.5, 1.5)];

        assert!((intersection_area(&unit, &unit) - 4.0).abs() < 1e-9);
        assert!(intersection_area(&unit, &far).abs() < 1e-9);
        assert!((intersection_area(&unit, &inner) - 1.0).abs() < 1e-9);
        assert!((polygon_iou(&unit, &unit) - 1.0).abs() < 1e-12);
        assert!(polygon_iou(&unit, &far).abs() < 1e-12);
        assert!((polygon_iou(&unit, &inner) - 0.25).abs() < 1e-9);
    }

    /// A clockwise polygon gives the same answer as its reverse.
    #[test]
    fn winding_order_does_not_change_the_area() {
        let clockwise = vec![(0.0, 0.0), (0.0, 2.0), (2.0, 2.0), (2.0, 0.0)];
        let counter = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        assert!((intersection_area(&clockwise, &counter) - 4.0).abs() < 1e-9);
        assert!((intersection_area(&counter, &clockwise) - 4.0).abs() < 1e-9);
    }
}
