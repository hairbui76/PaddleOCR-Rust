# Table Pipeline Contract — first slice

Roadmap item: `TABLEPIPE-001`
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: **first slice implemented and matched**; the rest is `P9` plumbing

`TableRecognitionV2` turns three model outputs — structure tokens, cell boxes,
and OCR boxes with text — into an HTML table. All three models are now ported,
so what remained was the part that composes them.

That part is **pure functions over boxes and token lists**. It needs no image,
no artifact, and no inference session, which is why it is the first slice rather
than the pipeline wiring: it is where the table actually gets built, and it is
the cheapest part of the pipeline to pin exactly.

## 1. The matcher's score is not IoU, and is not symmetric

`match_table_and_ocr` thresholds on `compute_inter`, which is

```
intersection(cell, ocr) / area(ocr)
```

— intersection over the **second** box's area, not over the union. The captured
corpus makes the difference concrete:

| Case | `compute_inter` | `compute_inter` swapped | `compute_iou` |
|---|---|---|---|
| `first_contains_second` | `1.0000` | `0.0100` | `0.0100` |
| `second_contains_first` | `0.0100` | `1.0000` | `0.0100` |
| `partial_overlap` | `0.2500` | `0.2500` | `0.1429` |

A table cell fully containing a small OCR box is exactly the normal case, and it
scores `1.0` one way round and `0.01` the other. Substituting IoU — the obvious
"equivalent" — would put almost every real cell below the `0.7` threshold and
produce an empty table. The oracle captures **both orders** so the asymmetry is
asserted rather than assumed.

The threshold is strictly greater than `0.7`, and a zero-area OCR box scores `0`
through an explicit guard rather than dividing by zero.

## 2. Two branches nothing can reach

`match_table_and_ocr` takes two flag lists, `table_cells_flag` and
`row_start_index`, and has branches for them disagreeing:

```python
if table_cells_flag[k + 1] < row_start_index[k + 1]:
    ...
elif table_cells_flag[k + 1] > row_start_index[k + 1]:
    matched[real_len - 1].append(matched[real_len + s])   # KeyError
```

Its only caller passes **`table_cells_flag` for both**, so neither branch ever
runs. Handed genuinely different lists, upstream raises `KeyError`; the fixture
records that under `matching.unreachable_branch`, and a test asserts the
recorded behaviour so it cannot silently change.

This port takes a **single** flag list. That makes the unreachable branches
structurally impossible rather than reproducing a crash nothing can trigger,
which is a deliberate, recorded difference rather than an oversight.

## 3. Row grouping anchors on the first box, not a running mean

`sort_table_cells_boxes` sorts by top edge, then groups boxes within `10` pixels
of the **row's first box**. `current_y` is set when a row starts and not updated
as boxes join it, so three boxes at tops `0`, `9`, `18` form **two** rows — the
third is `18` from the anchor even though it is only `9` from its predecessor.

A running mean or a pairwise comparison would give one row. The test pins the
anchor behaviour with exactly that case.

## 4. A parameter named for something it is not passed

`get_html_result`'s fourth parameter is named `table_cells_flag`. Its only caller
passes `row_start_index`. The name is upstream's and the behaviour is the
caller's; this port names the parameter for what it receives and records why.

## 5. Assembly details worth naming

- `<td></td>` is emitted as an opening `<td>`, then content, then `</td>` —
  never as the literal token.
- A cell with **no** matching OCR box still closes its tag, producing an empty
  `<td></td>`. A cell whose match list is *empty* would `continue` and leave the
  tag unclosed; that state is only reachable through the dead branch in §2.
- Multiple OCR boxes in one cell are joined with a single space, with leading
  spaces and `<b>`/`</b>` markers stripped from each fragment, and a `<b>`
  wrapper re-applied around the whole run if the first fragment carried one.
- OCR boxes matching no cell are **dropped**. The corpus includes one such box
  so the drop is asserted.

## 6. Oracle results

Every captured case is reproduced: eight geometry cases across all three
functions plus the swapped order, four token lists for row starts, the cell sort
with its flags and their alignment, the cell-to-OCR matching, and **both HTML
cases byte for byte**.

The capture executes the pinned PaddleX functions rather than transcribing
them, and where upstream raises it records the exception instead of choosing
kinder inputs.

## 7. What is left

Cropping tables out of a page, choosing between the wired and wireless cell
detectors from `TBLCLS-001`'s answer, and running the four models in order. Those
need artifact plumbing — four models loaded at once, one of them `368 MB` — which
is what `P9` is for.

So this row stays `In progress`: the composition logic is done and matched, and
the orchestration around it is not.
