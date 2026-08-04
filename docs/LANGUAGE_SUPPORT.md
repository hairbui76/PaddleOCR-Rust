# Language and Script Support

Roadmap item: `LANG-001`
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: One verified artifact pair; no general multilingual claim

This document exists to keep two things apart that are constantly conflated:

1. **What a dictionary contains** — which scalars the recognizer's output layer
   has a class for. This is a fact about a file and can be counted exactly.
2. **What has been verified** — which text this port has actually been observed
   to read correctly, from a recorded input, against a recorded expectation.

The second is much smaller than the first, and the gap is not a rounding error.
The pinned dictionary contains 672 emoji scalars. Nothing about that makes this
port an emoji recogniser from a photograph. A class exists in the output layer.
That is all a count can tell you.

## The verified mapping

There is exactly one, and adding a second requires the same evidence this one
has, not an inference from it.

| Field | Value |
|---|---|
| Detector | `PP-OCRv6_medium` det, SHA-256 `eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1` |
| Recognizer | `PP-OCRv6_medium` rec, SHA-256 `9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba` |
| Dictionary | `ppocr/utils/dict/ppocrv6_dict.txt`, SHA-256 `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |
| Entries | `18,708`; class count `18,710` — one CTC blank plus one appended space |
| Verified scripts | ASCII Latin words, and the two CJK Unified scalars `U+4F60 U+597D` |
| Evidence | Gate `G1` end-to-end fixtures, and `tests/end_to_end.rs` |

"Verified scripts" is deliberately narrow. Four committed fixtures and one real
book page have been read correctly; between them they contain English words and
one two-character Chinese greeting. That is the honest extent of it.

## What the dictionary contains

Counted with `cargo run --example dictionary_census -- <dictionary.txt>` over the
dictionary named above. Every scalar is accounted for; the counts sum to the
entry count because every entry in this dictionary is exactly one scalar.

| Script | Scalars |
|---|---|
| CJK Unified | 15,565 |
| Symbols | 928 |
| Emoji | 672 |
| Other | 445 |
| Latin | 429 |
| Halfwidth and Fullwidth | 139 |
| CJK Extension A | 137 |
| ASCII | 94 |
| Katakana | 94 |
| Hiragana | 86 |
| Greek | 76 |
| CJK Symbols | 25 |
| Punctuation | 18 |

Total: `18,708`.

Three observations that matter more than the totals:

- **`CJK Unified` is a Unicode block, not a language.** Chinese, Japanese, and
  Korean all draw from it. No count decides which of them this artifact was
  trained for, and this document does not guess.
- **Hiragana and Katakana together are 180 scalars**, which is roughly the size
  of the kana syllabaries. Their presence means the output layer *can* spell
  kana. It is not evidence that Japanese text is read correctly, and no Japanese
  fixture exists here.
- **Emoji and symbols total 1,600 scalars**, nearly nine percent of the
  dictionary. If a scalar count implied support, this port would claim to read
  emoji. It does not.

## What is not supported

- Any artifact other than the pair above. A different `PP-OCR` version, a
  different size, or a different language pack is unverified here, and this
  port will happily load it and produce wrong answers, because the detector and
  recognizer expose the same tensor ABI and nothing but a declared SHA-256
  distinguishes them. See the README's model-verification section.
- Language *detection*. Nothing in this port inspects text to decide a language,
  and nothing selects a dictionary automatically. The caller names the
  dictionary file; that choice is the language decision.
- Script-specific normalization. Decoded text is the exact scalars the
  dictionary holds — no NFC, no NFKC, no case folding, no width folding. `U+3000`
  stays `U+3000` and never becomes `U+0020`.
- Right-to-left, vertical, and bidirectional layout. Reading order is the
  upstream top-to-bottom then left-to-right sort with a ten-pixel row tolerance,
  which is wrong for Arabic and Hebrew and for vertical CJK. The dictionary
  above contains no Arabic or Hebrew scalars, so the question does not arise for
  this artifact, but it will for any artifact that does.

## Adding a language

The bar is the evidence, not the dictionary. To add a mapping:

1. Provision the artifact pair and its dictionary as explicit local files, and
   record all three SHA-256 digests.
2. Commit an end-to-end fixture in that script with its recorded expectation,
   the way `classic-v1-e2e-unicode` does.
3. Have the `G1` gate reproduce it exactly, text and confidence.
4. Add a row above stating what was verified, not what the dictionary contains.

A dictionary census is useful for step 1 and proves nothing about steps 2–4.
