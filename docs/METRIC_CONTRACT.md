# Evaluation Metric Contract

Roadmap item: `METRIC-001` (detection and recognition halves)
Baseline: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: **detection and recognition implemented and matched**; table, KIE, SR,
and distributed reduction are not

## 1. Why this exists

Every compatibility row in `docs/COMPATIBILITY.md` says **no accuracy claim**,
because no fixture in this repository asserts what a model detects. That is
honest, and it is also a gap: without upstream's own metric, *"this port agrees
with upstream"* and *"this port is as good as upstream"* cannot be asked as
separate questions.

This is the tooling half of closing that gap. It makes the accuracy question
**askable**. It does not answer one, and no row's position changes because of
it.

Frozen from `ppocr/metrics/` in the **PaddleOCR** checkout, not PaddleX — this
is one of the few remaining behaviours the first-pinned baseline still owns.

## 2. Three behaviours a reimplementation gets wrong by accident

### The matcher is greedy in index order, not best-first

`evaluate_image` walks the ground-truth × detection grid in index order and
takes the **first** pair that clears the threshold, even when a later detection
matches the same region better.

The captured `greedy_first_match_wins` case is exactly that: one region, two
detections, the **second** an exact match — and the **first** is the one that
matches.

| | Greedy index order | Best-first |
|---|---|---|
| matched | `1` | `1` |
| det_care | `2` | `2` |
| precision | `0.5` | `0.5` |

The two agree here and diverge on a denser page, where a detection consumed by
the wrong region leaves a better-matching region unmatched. Reproducing the
order is what makes the numbers comparable at all.

### The IoU threshold is strictly greater

`iou > 0.5`. A detection at **exactly** `0.5` does not match, and the corpus
pins that with a pair whose IoU is exactly `0.5` — a `10x10` region against a
`10x20` detection, intersection `100`, union `200`.

### `RecMetric` divides by `total + 1e-5`

So a **perfect run scores `0.99999`, not `1.0`**. Reproducing that epsilon is
the difference between agreeing with upstream and being almost right, and a
reader comparing two numbers needs to know which one they are looking at.

## 3. Two things this port had to get right on its own

**Edit distance counts characters, not bytes.** Upstream compares Python `str`,
whose unit is a code point. Comparing UTF-8 bytes would score one CJK
substitution as three edits — `你好世界` against `你好世间` is `0.25`, not `0.75`.

**Polygon intersection is Sutherland–Hodgman**, with a winding-order correction:
the clipper's signed area decides whether it needs reversing, because the
algorithm needs a counter-clockwise clipper and a detection quadrilateral may
arrive either way. Upstream calls shapely; this port has no geometry dependency
and computes it.

Correct for **convex** clippers, which a detection quadrilateral is. Upstream's
own `Polygon(...).is_valid` check rejects self-intersecting shapes before they
reach the evaluator, so the two agree on the inputs the evaluator actually sees.

## 4. A behaviour recorded rather than corrected

`is_filter` keeps only ASCII digits and letters, then lowercases. On non-Latin
text that **erases the string entirely**: `你好` becomes `""`, and two different
CJK predictions therefore compare **equal** under it.

That is upstream's behaviour. It is reproduced and pinned by a test rather than
fixed, because a metric that disagrees with the reference is not a metric this
project can compare against. It is also the reason `is_filter` defaults to
`false` and `docs/LANGUAGE_SUPPORT.md`'s scope matters: a CJK evaluation run with
`is_filter` on would report a meaningless accuracy of `1.0`.

## 5. Oracle results

Nine detection cases and forty recognition cases — ten pairs across every
combination of `ignore_space` and `is_filter` — plus both corpora combined.
Every one is reproduced.

| Case | gt_care | det_care | matched | precision |
|---|---|---|---|---|
| `exact_match` | `1` | `1` | `1` | `1.0` |
| `iou_exactly_at_the_threshold` | `1` | `1` | `0` | `0.0` |
| `dont_care_absorbs_a_detection` | `1` | `1` | `1` | `1.0` |
| `greedy_first_match_wins` | `1` | `2` | `1` | `0.5` |
| `no_ground_truth` | `0` | `1` | `0` | `0.0` |

The combined corpus scores `precision 0.5`, `recall 0.571…`, `hmean 0.533…` —
counts summed **before** the ratios, which is not the same as averaging
per-image ratios and matters as soon as pages differ in size.

The capture loads `eval_det_iou.py` **by file path**. Importing `ppocr.metrics`
pulls in `e2e_metric`, which needs `scipy`; loading the one file reads exactly
what is being frozen and does not push a dependency onto the read-only checkout.

## 6. What is not here

**Table, KIE, and super-resolution metrics.** They score modules this port does
not have, three of which have no published ONNX export
(`docs/P8_ARTIFACT_AVAILABILITY.md`). Implementing a metric for a module that
cannot run would be code with nothing to check it.

**Distributed reduction.** There is nothing to reduce across: this port has no
distributed evaluation, and adding the reduction before the thing it reduces
would freeze a shape with no user.
