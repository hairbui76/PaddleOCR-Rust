# Table Pipeline Contract

Roadmap item: `TABLEPIPE-001`
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: **composition, route, suppression, and cropping matched**; model
orchestration is the remaining `P9` plumbing

`TableRecognitionV2` turns three model outputs — structure tokens, cell boxes,
and OCR boxes with text — into an HTML table. All three models are now ported,
so what remained was the part that composes them.

Almost all of it is **pure functions over boxes and token lists** — no image, no
artifact, no inference session — which is why it was portable ahead of the
plumbing. It is where the table actually gets built, and it is the cheapest part
of the pipeline to pin exactly.

Sections 9 and 10 record what is matched and what is not.

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

## 6. The route, and a threshold the config does not carry

`predict_single_table_recognition_res` branches on the classifier's label:
`wired_table` selects the wired structure and cell models, `wireless_table` the
wireless pair. There is **no `else`**. A third label leaves both predictions
unbound and upstream raises `UnboundLocalError`; this port's `table_route`
returns `None`, which is the same refusal expressed where a caller can act on it.

The cell detector is then called with **`threshold=0.3`**, written into the
pipeline with a comment saying it improves cell recall. The artifact's own
`draw_threshold` is `0.5`. Taking the config value — the obvious thing to do
after `TBLCELL-001` froze it — would silently drop cells the reference pipeline
keeps, so the two constants are separate and a compile-time assertion holds them
in order.

## 7. NMS uses IoU, and therefore does not suppress containment

`cells_det_results_nms` suppresses above an IoU of `0.3`. Because it is IoU and
not the matcher's asymmetric score, a cell box **fully containing** another
survives: the captured `containment` case keeps both boxes, at an IoU of `0.01`,
where the matcher would have scored the same pair `1.0` in one direction.

Two overlap measures with opposite behaviour on the same geometry, both in the
same pipeline, both captured.

Ties in the score sort go to the **higher** index — `argsort()[::-1]` on equal
scores — which the `tied_scores` case pins.

## 8. Cropping discards, it does not clip

`get_region_ocr_det_boxes` keeps only boxes **fully** inside the table region and
re-expresses them relative to its top-left corner. A box crossing any edge is
**discarded entirely**, not clipped to the boundary. A box exactly on the
boundary is kept: the comparison is inclusive on all four edges.

The corpus covers inside, crossing left, crossing bottom, exactly on the
boundary, and entirely outside.

## 9. Oracle results

Every captured case is reproduced: eight geometry cases across all three
functions plus the swapped order, four token lists for row starts, the cell sort
with its flags and their alignment, the cell-to-OCR matching, **both HTML cases
byte for byte**, four NMS cases, and five cropping cases.

The capture executes the pinned PaddleX functions rather than transcribing
them, and where upstream raises it records the exception instead of choosing
kinder inputs.

## 10. What is left

Running the four models in order against real artifacts. That needs artifact
plumbing — four sessions live at once, one of them `368 MB` — which is what the
rest of `P9` is for.

Everything around that is now done and matched: the composition logic, the
route, the detection threshold, the cell suppression, and the cropping. So this
row stays `In progress`, and what remains is plumbing rather than behaviour.
