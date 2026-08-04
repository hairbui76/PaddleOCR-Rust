# Config Reconciliation

Roadmap item: `CONFIG-001`
Baseline: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: **complete** — every config in the pinned checkout is classified

## 1. The hard requirement is not the classification

`CONFIG-001` asks for each baseline config to be classified, and then adds the
constraint that actually costs something:

> no generic family claim from one config

It would have been easy to mark every `DBPostProcess` config `Verified` after
matching one of them. It would also have been false: this port froze thresholds,
tensor names, and shapes from **one** file, and a sibling config with a different
`limit_side_len` or a different backbone is not covered by that.

## 2. The result

| Status | Count |
|---|---|
| **Verified** | **2** |
| Postprocess implemented, parameters unverified | 84 |
| Deferred | 53 |
| **Total** | **139** |

The two `Verified` files are `configs/det/PP-OCRv6/PP-OCRv6_medium_det.yml` and
`configs/rec/PP-OCRv6/PP-OCRv6_medium_rec.yml` — the exact files whose parameters
this port froze and matched against a captured oracle.

The `84` are configs whose **postprocess** this port implements — `DBPostProcess`,
`CTCLabelDecode`, `TableLabelDecode`, `ClsPostProcess` — but whose parameters
came from somewhere else. Their status says exactly that and claims nothing more.

## 3. `Out of scope` is deliberately unassigned

The roadmap lists it as a valid status. It is also a **user-approved exclusion**,
and a reconciliation is evidence, not a decision. Nothing here assigns it, and a
test asserts the status never appears.

## 4. Two corrections to this project's own numbers

The roadmap estimated **`~155`** configs. The pinned checkout holds **`139`**.
The measured count is what is recorded; an estimate that survives into a
completed audit is an audit that did not look.

And the record is committed as a **fixture** rather than scanned at test time.
The upstream checkout is a read-only symlink that a clean checkout of this
repository does not have, and the whole suite must run without it. One test
compares the record against the checkout **when it is present**, which is the
only thing that stops the record from drifting away from the tree it describes.

## 5. What a status does and does not mean

A classification is by declared `Architecture.algorithm` and `PostProcess.name`.
It says **which upstream component a config needs**. It does not say whether this
port would reproduce that config's results, and no row's compatibility position
changes because of this document.
