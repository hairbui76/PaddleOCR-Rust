# Reading Order Contract

Roadmap item: `STRUCT-001` (first slice)
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: **the four XY-cut primitives implemented and matched**; the heuristics
above them are not ported

## 1. What was portable, and why this slice first

`PP-StructureV3` orders layout blocks with `xycut_enhanced`, which is `1,830`
lines. Almost all of it is per-label heuristics: how a document title is
attached to its section, how a caption follows its figure, how a region absorbs
its children.

Underneath sit **four pure functions** that are the algorithm itself:

| Function | What it does |
|---|---|
| `projection_by_bboxes` | boxes → a 1D occupancy histogram on one axis |
| `split_projection_profile` | histogram → segments, split at gaps |
| `recursive_yx_cut` | project on `Y`, then `X`, then recurse |
| `recursive_xy_cut` | the mirror |

They take integer boxes and return an ordering. No model, no image, no artifact
— the same property that made `docs/TABLE_PIPELINE_CONTRACT.md`'s composition
logic portable ahead of its plumbing, and the reason it is the first slice.

## 2. The two cut orders are not the same reading order

This is the finding worth carrying forward. On a two-column page with two rows:

| Cut | Order |
|---|---|
| `yx` — rows first | `[0, 1, 2, 3]` — **row major** |
| `xy` — columns first | `[0, 2, 1, 3]` — **column major** |

That is the entire difference between reading a page **across** and reading it
**down**. Neither is "the" reading order; which one a document wants depends on
the document.

So both are captured and both are implemented, and the fixture contains a case
where they disagree specifically so that a future change cannot quietly collapse
them into one.

On a page with a full-width header over two columns the two agree, which is why
that case is in the corpus too: a corpus where they always differ would be as
misleading as one where they never do.

## 3. Details the source decides and a reading would not

- **Gaps split on strictly greater.** `split_projection_profile` compares
  `index_diffs > min_gap`, so a gap exactly equal to `min_gap` does **not**
  split. The corpus has that boundary both ways.
- **Segment ends are exclusive**, and the last end is `last + 1`. Two runs in
  `[1,1,0,0,0,1,1]` come out as starts `[0, 5]` and ends `[1, 7]`.
- **The recursion resets `min_gap`.** Both cut functions take a `min_gap`
  parameter, use it for their *second* axis, and then recurse **without passing
  it**, so every level below the first uses the default of `1`. Reproduced as
  found.
- **A single segment on the second axis short-circuits.** When the inner split
  yields one interval, the chunk is emitted in sorted order rather than
  recursed, which is what terminates the recursion.
- **Negative coordinates take a different branch.** The histogram's length
  becomes the magnitude of the smallest coordinate, and intervals accumulate
  with absolute values. It is reproduced rather than tidied.

## 4. What this port does that upstream does not

Two bounds, both because the histogram is allocated from **caller-supplied
numbers**:

- An **empty** box list is refused rather than allocating a zero-length
  histogram and returning an empty ordering that looks like a valid answer.
- An **absurd extent** is refused. A box claiming to span `900,000,000` pixels
  is hostile input, not a layout, and upstream would try to allocate it.

Both are the same treatment `docs/THREAT_MODEL.md` gives image dimensions.

## 5. A permutation guard on every case

Every captured ordering is asserted to be a **permutation** of the input
indices — each block appearing exactly once.

An ordering that drops or duplicates a block is worse than one that is merely in
the wrong order: a wrong order is visible in the output, while a dropped block
means content silently disappears from a structured document. The guard runs on
every case in the corpus, in both cut directions.

## 6. A limit on what is claimed

NumPy's default `argsort` is **not stable**, so upstream's tie order is formally
unspecified. This port sorts stably, the corpus contains a case with tied top
edges, and the two agree there.

That is the most that can be claimed: agreement on the captured ties, not a
guarantee for all of them. It is recorded rather than papered over.

## 7. Status

Implemented in `src/reading_order.rs` and matched against
`tests/fixtures/classic-v1-reading-order`, captured by **executing** the pinned
PaddleX functions.

`STRUCT-001` stays `In progress`. The label-aware heuristics above these
functions are not ported, and the full orchestration is blocked for a reason
that is not about effort: `FORM-001`, `SEAL-001`, `CHART-001`, and `KIE-001`
have no published ONNX export, per `docs/P8_ARTIFACT_AVAILABILITY.md`.
