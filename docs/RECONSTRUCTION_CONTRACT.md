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

## 7. What is deliberately absent

The **image**, **chart**, **formula**, and **seal** formatters.

They depend on P8 modules with no published ONNX export
(`docs/P8_ARTIFACT_AVAILABILITY.md`). Porting them would produce code with
nothing to check it against, which this project has five recorded bugs' worth of
reason not to do.

## 8. What is left

Assembling a whole document: walking ordered blocks, dispatching each to its
formatter, and joining the results — including the pretty-versus-plain switch
and the header/footer special cases.

That step needs a block type with a label and content, which is
`STRUCT-001`'s to define, and `STRUCT-001` is itself blocked on the four missing
artifacts. So `RECON-001` stays `In progress` with the formatters done and the
assembly waiting on a decision above it.
