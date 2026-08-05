# Markdown Reconstruction Contract

Roadmap item: `RECON-001` (first slice)
Baselines: PaddleOCR `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`, PaddleX `3.7.2`
Status: **the per-label formatters implemented and matched**; document assembly
and four label handlers are not

## 1. What was portable

`PP-StructureV3` rebuilds a document by mapping each ordered layout block
through a **per-label formatter**, from `build_handle_funcs_dict`. Most of those
formatters are pure string functions — no model, no image, no artifact — so they
are capturable and matchable exactly.

That is the same property that made `docs/READING_ORDER_CONTRACT.md`'s
primitives portable ahead of the heuristics above them, and it is why the
formatters come before assembly.

## 2. Heading level comes from dots, not from nesting

`format_title` reassembles a numbered title, strips trailing periods, and counts
the **remaining dots**:

| Content | Markdown |
|---|---|
| `1 Introduction` | `## 1 Introduction` |
| `1. Introduction` | `### 1. Introduction` |
| `1.2 Methods` | `### 1.2 Methods` |
| `1.2.3 Results` | `#### 1.2.3 Results` |
| `A.B.C lettered` | `#### A.B.C lettered` |
| `Trailing dots...` | `## Trailing dots` |

Two rows are worth stopping on.

`A.B.C lettered` is **not** numbering the pattern recognises, so it survives
untouched — and then its dots are counted anyway, making it a third-level
heading. Upstream's behaviour, reproduced rather than corrected.

`1 Introduction` and `1. Introduction` differ by one level, because the trailing
period is kept as part of the numbering and then counted. A document whose
author is inconsistent about that period gets inconsistent heading levels.

### The hash count is one more than the level

The format string is `#{'#' * level}`, so a level-`1` title emits **two** hashes.
The name and the output disagree by one, which matters to anyone comparing this
port's documents with upstream's.

## 3. A newline that is added twice

`simplify_table` returns `"\n" + stripped`, and its documented call site is
`simplify_table("\n" + block.content)`.

So every reconstructed table is preceded by **two** newlines — a blank line.
That is reproduced rather than tidied, and the captured oracle records both,
because a port that emitted one newline would produce documents that render
subtly differently and diff against upstream's on every table.

## 4. `normalize_newlines` collapses before it expands

```python
block.content.replace("\n\n", "\n").replace("\n", "\n\n")
```

The order is load-bearing. Collapsing first means an existing paragraph break
survives as **one** blank line; reversing the two replacements would turn every
paragraph break into two. That is silent in a rendered document and obvious in a
diff, which is the worst combination.

## 5. `format_first_line` stops at the first non-empty field

It splits on a separator, walks fields until it finds a non-empty one, rewrites
it **if** it matches a template, and then **breaks either way**.

So `"intro abstract"` is left alone: the first non-empty field is `intro`, which
does not match, and the scan stops before reaching `abstract`. The corpus pins
that, along with the leading-whitespace case where the separator produces empty
fields that are skipped without stopping the scan.

## 6. No regular-expression dependency

The numbering pattern has four alternatives — Arabic with optional dotted
groups, parenthesised Arabic or CJK, bare CJK, and Roman with a required
trailing separator. It is implemented as a **hand-written matcher**.

This project has two dependencies. Adding a regex crate to parse four
alternatives would be a poor trade against a matcher that the sixteen captured
title cases check directly. Forms outside that corpus are unverified, and this
document says so rather than implying the pattern is complete.

The alternation order is preserved because it is load-bearing: `I` is tried
before `II`, and only the required trailing `.`-or-space makes the longer Roman
forms reachable.

## 7. Document assembly: the two multipage functions

Above the page converter sit two functions in `pipeline_v2.py`, ported in
`src/multipage.rs` and captured in `classic-v1-multipage`. Neither renders,
decodes, or infers anything, which is why they are frozen while `PDF-001` has no
approved renderer: a renderer is what makes several pages *reachable*, not what
makes these functions *correct*.

`concatenate_markdown_pages` reads exactly two values per page — the
continuation flags and the Markdown text — so a pair of booleans and a string is
its complete input, not a stand-in for one.

Four behaviours are captured because a reasonable reimplementation gets each
one wrong:

1. **Every document begins with a blank line.** The loop seeds the previous
   page's end flag to `true`, so the first page takes the separator branch. Code
   that joins *between* pages would differ on every document upstream produces.
2. **One CJK side is enough to drop the joining space.** The test is `or`, not
   `and`: a Chinese tail followed by English prose is joined with no separator.
3. **An empty continuing page still contributes a space.** With no character on
   one side, neither side tests as CJK, so upstream appends `" "` and then
   nothing.
4. **The merge separator keys on the raw last character.** Block contents
   already end in a space, so a merged paragraph gets *two*. Upstream does not
   trim, so neither does this port.

`merge_text_across_page` moves a paragraph that runs past a page break into the
last *surviving* block, dropping it from its own page — so a page can come back
empty, and several consecutive pages can collapse into one paragraph. The start
flag is computed against the previous block **on the same page**, which means a
page's first block is measured against nothing and `get_seg_flag`'s no-previous
branch decides it: the flag clears when the block's first text segment begins
within ten pixels of the block's own leading edge. That branch is the reason a
cross-page merge can happen at all.

The capture records which cases actually dropped a block, and a test asserts the
list is non-empty, because a corpus where nothing merged would be passed by an
implementation that never merges.

## 8. What is left

Only the **recognition-on** content variants of the image, chart, formula, and
seal handlers: the chart-to-table conversion, `$$`-wrapped formula content, and
a seal's image-plus-text pair. Those read a model's output, and the models
publish no ONNX export (`docs/P8_ARTIFACT_AVAILABILITY.md`). Porting them would
produce code with nothing to check it against, which this project has five
recorded bugs' worth of reason not to do.

Their recognition-**off** forms are not absent. With those flags off — this
port's supported mode — upstream routes all four labels through the plain image
handler, and `src/markdown_v2.rs` implements and matches it. An earlier revision
of this document said the four formatters were missing outright; that was true of
the leaf dispatch in `src/markdown.rs` and never true of the page converter.
