# `IMG-003` entry gate — what a component delta of `36` does downstream

Roadmap item: `IMG-003`
Recorded: 2026-08-05
Status: **the gate is satisfied**; the decision it unblocks is the user's

`docs/IMAGE_DECODER_EVIDENCE.md` records that every evaluated pure-Rust JPEG
decoder differs from the committed OpenCV oracle by up to **`36`** in one
component on the committed `classic-v1-image-inputs` corpus. `IMG-003` makes
measuring the **consequence** of that number the precondition for deciding
anything:

> Only after that measurement may this item accept a decoder tolerance, adopt
> the byte-exact `libjpeg-turbo-rs` route subject to a targeted
> unchecked-indexing and raw-pointer review, or record JPEG as unsupported.

This document is that measurement.

## 1. Method

The probe does **not** use a JPEG decoder. It perturbs an already-decoded page
and re-encodes it losslessly, which isolates the question *"what does a delta of
`d` do?"* from *"which decoder produces it"*.

Two perturbation shapes, because they answer different questions:

| Shape | What it answers |
|---|---|
| **Uniform** `+d` on every component | The worst case a bounded delta permits |
| **Scattered** `±d`, deterministic per component | Something closer to decoder noise |

The perturbed page goes through the **whole pipeline** — the detector's resize
and normalization, DB postprocessing, the perspective crop, and CTC decoding —
so the number being measured is the one that matters: the effect on output, not
on pixels.

Probe: `tests/end_to_end.rs`, `jpeg_delta_gate`, opt-in behind the four model
environment variables.

## 2. Result

Against the committed `classic-v1-benchmark-page` with the M2 artifact pair:

```
baseline: 6 lines ["Hello", "World", "Rust", "Rust", "OCR", "你好"]

uniform +1         lines  6   same_text true   worst_corner_px 1
uniform +4         lines  6   same_text true   worst_corner_px 1
uniform +16        lines  6   same_text true   worst_corner_px 1
uniform +36        lines  6   same_text true   worst_corner_px 1
scattered +/-36    lines  6   same_text true   worst_corner_px 4
```

**Recognized text is identical in every case**, including scattered `±36`. The
line count is identical. Box corners move by at most `1` pixel under a uniform
shift and at most `4` pixels under scattered noise at the full delta.

## 3. What this does and does not establish

It establishes that a component delta of `36` is **not automatically fatal**: on
this page, with these artifacts, it changed no character and moved no corner more
than `4` pixels.

It does **not** establish that JPEG is safe to accept, and three limits are why.

**One page.** The benchmark page is six short lines at high contrast. A dense
scan with small text, low contrast, or thin strokes is a different regime, and
the detector's threshold is exactly where a few components can flip a pixel from
inside a box to outside it. A single page is a data point, not a
characterization.

**The perturbation is not a decoder difference.** Real JPEG decoder disagreements
are **spatially correlated** — they follow the `8x8` block structure and cluster
at edges, which is where text is. Scattered noise averages out under a resize
in a way block-structured error does not. This probe bounds the *magnitude*
faithfully and does not reproduce the *shape*.

**No accuracy claim.** "Same text" here means the two runs agreed with each
other, not that either was correct.

## 4. The decision this unblocks

`IMG-003` names three outcomes, and this measurement does not choose between
them — that is a scope decision, and it belongs to the user.

1. **Accept a decoder tolerance.** The measurement supports this being *possible*
   and does not support it being *established*. Doing it responsibly would mean
   extending the probe to a dense-text corpus and to a block-structured
   perturbation before any tolerance is written down.
2. **Adopt the byte-exact `libjpeg-turbo-rs` route.** This removes the tolerance
   question entirely and replaces it with an `unsafe` question: the roadmap
   already requires a targeted unchecked-indexing and raw-pointer review, and
   `docs/SAFE_001_AUDIT.md` records that this project currently forbids
   `unsafe_code` at the crate level.
3. **Record JPEG as unsupported.** The cheapest honest answer, and the one
   consistent with how office formats were closed in `DOCIO-DEC-001`.

No recommendation is offered, because the choice trades this port's
byte-exactness story against input coverage, and that trade is not a technical
question.

Progressive JPEG, CMYK JPEG, and EXIF orientations `1`–`8` remain part of
`IMG-003` under every option, and none of them is touched by this measurement.
