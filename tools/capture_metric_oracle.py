#!/usr/bin/env python3
"""Capture the detection and recognition evaluation metrics.

Roadmap item: `METRIC-001`, the detection and recognition halves.

Every compatibility row in this repository says **no accuracy claim**, because
no fixture here asserts what a model detects. Those rows point at a gap that
this item closes the tooling half of: without upstream's own metric, "this port
agrees with upstream" and "this port is as good as upstream" cannot even be
asked as separate questions.

Frozen from the **pinned PaddleOCR checkout**, not PaddleX -- these live in
`ppocr/metrics/`, which is in the checkout this project pinned first:

    DetectionIoUEvaluator   iou_constraint 0.5, area_precision_constraint 0.5
                            greedy first-match over the gt x det grid
    DetMetric               combine_results -> precision, recall, hmean
    RecMetric               exact-match accuracy and normalised edit distance

Executed, not transcribed. Needs `shapely` and `rapidfuzz` -- upstream's own
dependencies for these files -- plus the pinned checkout on `sys.path`.

Usage:
    python3 tools/capture_metric_oracle.py <output.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

CHECKOUT = Path(__file__).resolve().parent.parent / "PaddleOCR"


def _load_evaluator():
    """Loads `eval_det_iou.py` by path, bypassing the package `__init__`.

    Importing `ppocr.metrics` pulls in `e2e_metric`, which needs `scipy` --
    a dependency this capture does not need and the read-only checkout must not
    be made to acquire. Loading the one file directly reads exactly what is
    being frozen and nothing else.
    """
    import importlib.util

    path = CHECKOUT / "ppocr" / "metrics" / "eval_det_iou.py"
    spec = importlib.util.spec_from_file_location("_eval_det_iou", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.DetectionIoUEvaluator


DetectionIoUEvaluator = _load_evaluator()

from rapidfuzz.distance import Levenshtein  # noqa: E402

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/metric-oracle-capture/v1"


def quad(left: float, top: float, right: float, bottom: float) -> list[list[float]]:
    return [[left, top], [right, top], [right, bottom], [left, bottom]]


# Detection cases, each chosen for one behaviour of the matcher.
DETECTION_CASES = [
    (
        # A clean one-to-one match.
        "exact_match",
        [(quad(0, 0, 10, 10), False)],
        [quad(0, 0, 10, 10)],
    ),
    (
        # IoU exactly at the constraint. The comparison is strictly greater, so
        # this must NOT match.
        "iou_exactly_at_the_threshold",
        [(quad(0, 0, 10, 10), False)],
        [quad(0, 0, 10, 20)],
    ),
    (
        # Comfortably above it.
        "iou_above_the_threshold",
        [(quad(0, 0, 10, 10), False)],
        [quad(0, 0, 10, 12)],
    ),
    (
        # Nothing overlaps.
        "no_overlap",
        [(quad(0, 0, 10, 10), False)],
        [quad(100, 100, 110, 110)],
    ),
    (
        # A don't-care ground truth absorbs the detection covering it.
        "dont_care_absorbs_a_detection",
        [(quad(0, 0, 10, 10), True), (quad(50, 0, 60, 10), False)],
        [quad(0, 0, 10, 10), quad(50, 0, 60, 10)],
    ),
    (
        # Two detections compete for one ground truth; the matcher is greedy in
        # index order, not best-first.
        "greedy_first_match_wins",
        [(quad(0, 0, 10, 10), False)],
        [quad(0, 0, 10, 11), quad(0, 0, 10, 10)],
    ),
    (
        # Nothing detected at all.
        "no_detections",
        [(quad(0, 0, 10, 10), False)],
        [],
    ),
    (
        # Nothing to find, and something found anyway.
        "no_ground_truth",
        [],
        [quad(0, 0, 10, 10)],
    ),
    (
        # Everything is don't-care.
        "all_dont_care",
        [(quad(0, 0, 10, 10), True)],
        [quad(0, 0, 10, 10)],
    ),
]

# Recognition cases, each chosen for one branch of the normalisation.
RECOGNITION_CASES = [
    ("identical", "hello", "hello"),
    ("case_differs", "Hello", "hello"),
    ("space_only", "hello world", "helloworld"),
    ("one_substitution", "hello", "hallo"),
    ("one_deletion", "hello", "hell"),
    ("empty_prediction", "", "hello"),
    ("both_empty", "", ""),
    ("punctuation", "a-b.c", "abc"),
    ("cjk", "你好世界", "你好世间"),
    ("digits_and_letters", "AB12", "ab12"),
]


def normalize_text(text: str) -> str:
    """Upstream's `_normalize_text`: keep digits and ASCII letters, lowercase."""
    import string

    return "".join(
        filter(lambda c: c in (string.digits + string.ascii_letters), text)
    ).lower()


def recognition_metrics(
    pairs: list[tuple[str, str]], ignore_space: bool, is_filter: bool
) -> dict:
    """`RecMetric.__call__`, reproduced with its own `eps`."""
    eps = 1e-5
    correct = 0
    total = 0
    edit = 0.0
    for pred, target in pairs:
        if ignore_space:
            pred = pred.replace(" ", "")
            target = target.replace(" ", "")
        if is_filter:
            pred = normalize_text(pred)
            target = normalize_text(target)
        edit += Levenshtein.normalized_distance(pred, target)
        if pred == target:
            correct += 1
        total += 1
    return {
        "acc": correct / (total + eps),
        "norm_edit_dis": 1 - edit / (total + eps),
    }


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    evaluator = DetectionIoUEvaluator()
    detections = []
    for name, ground_truth, predicted in DETECTION_CASES:
        gt_info = [
            {"points": points, "text": "", "ignore": ignore}
            for points, ignore in ground_truth
        ]
        det_info = [{"points": points, "text": ""} for points in predicted]
        result = evaluator.evaluate_image(gt_info, det_info)
        combined = evaluator.combine_results([result])
        detections.append(
            {
                "case": name,
                "ground_truth": [points for points, _ in ground_truth],
                "ignore": [ignore for _, ignore in ground_truth],
                "predicted": predicted,
                "gt_care": int(result["gtCare"]),
                "det_care": int(result["detCare"]),
                "matched": int(result["detMatched"]),
                "precision": float(combined["precision"]),
                "recall": float(combined["recall"]),
                "hmean": float(combined["hmean"]),
            }
        )

    # And every case combined into one corpus, which is how the metric is
    # actually used: per-image counts summed before the ratios are taken.
    corpus = []
    for name, ground_truth, predicted in DETECTION_CASES:
        gt_info = [
            {"points": points, "text": "", "ignore": ignore}
            for points, ignore in ground_truth
        ]
        det_info = [{"points": points, "text": ""} for points in predicted]
        corpus.append(evaluator.evaluate_image(gt_info, det_info))
    combined_corpus = evaluator.combine_results(corpus)

    recognitions = []
    for name, pred, target in RECOGNITION_CASES:
        for ignore_space in (True, False):
            for is_filter in (True, False):
                metrics = recognition_metrics([(pred, target)], ignore_space, is_filter)
                recognitions.append(
                    {
                        "case": f"{name}/space={ignore_space}/filter={is_filter}",
                        "prediction": pred,
                        "target": target,
                        "ignore_space": ignore_space,
                        "is_filter": is_filter,
                        "acc": metrics["acc"],
                        "norm_edit_dis": metrics["norm_edit_dis"],
                    }
                )

    # The whole recognition corpus at the defaults, so the accumulation is
    # pinned and not only the single-pair arithmetic.
    all_pairs = [(pred, target) for _, pred, target in RECOGNITION_CASES]
    corpus_metrics = recognition_metrics(all_pairs, True, False)

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "upstream": "PaddleOCR ppocr/metrics/{eval_det_iou,det_metric,rec_metric}.py",
        "constants": {
            "iou_constraint": evaluator.iou_constraint,
            "area_precision_constraint": evaluator.area_precision_constraint,
            "eps": 1e-5,
        },
        "detection": detections,
        "detection_corpus": {
            "precision": float(combined_corpus["precision"]),
            "recall": float(combined_corpus["recall"]),
            "hmean": float(combined_corpus["hmean"]),
        },
        "recognition": recognitions,
        "recognition_corpus": {
            "pairs": [[pred, target] for _, pred, target in RECOGNITION_CASES],
            "ignore_space": True,
            "is_filter": False,
            "acc": corpus_metrics["acc"],
            "norm_edit_dis": corpus_metrics["norm_edit_dis"],
        },
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(
        f"wrote {output} ({len(detections)} detection cases, "
        f"{len(recognitions)} recognition cases)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
