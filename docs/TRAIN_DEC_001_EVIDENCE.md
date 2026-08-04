# `TRAIN-DEC-001` evidence — how large the training surface actually is

Roadmap item: `TRAIN-DEC-001`, resolving `D-011`
Recorded: 2026-08-05
Status: **evidence, not a decision.** `D-011` is a scope decision and belongs to
the user.

The row asks for a native-Rust tensor, autograd, distributed, and checkpoint
strategy selected "by operator coverage". Coverage of *what* is the question
this document answers, by counting the pinned checkout rather than estimating.

## 1. The full surface

Measured across all `139` configs in the pinned checkout:

| Component | Distinct implementations |
|---|---|
| Backbones | **36** |
| Losses | **33** |
| Heads | **31** |
| Necks | **16** |
| Learning-rate schedules | **8** |
| Transforms | **5** |
| Optimizers | **4** |

`121` distinct architecture components. The most common of each:

- **Backbone**: `MobileNetV3` (34), `MobileNetV1Enhance` (18), `ResNet_vd` (15),
  `PPLCNetV3` (14)
- **Loss**: `MultiLoss` (32), `CTCLoss` (25), `DBLoss` (20), `CombinedLoss` (14)
- **Head**: `MultiHead` (40), `DBHead` (32), `CTCHead` (29)
- **Neck**: `SequenceEncoder` (32), `DBFPN` (15), `RSEFPN` (9)
- **Optimizer**: `Adam` (107), `AdamW` (25), `Momentum` (4), `Adadelta` (3)
- **Schedule**: `Cosine` (82), `Piecewise` (16), `Linear` (6)

Full coverage means a Rust autograd framework plus **121** components plus every
one of their gradients, verified numerically against Paddle. That is a project
several times the size of the one this repository has built so far, and it is
the number the decision should be made against.

## 2. The minimal surface is two orders smaller

Training only the two configs `docs/CONFIG_RECONCILIATION.md` marks `Verified` —
`PP-OCRv6_medium_det` and `PP-OCRv6_medium_rec` — needs:

| Component | Count | Which |
|---|---|---|
| Backbone | 1 | `PPLCNetV4` |
| Head | 2 | `DBHead`, `MultiHead` |
| Neck | 1 | `RepLKPAN` |
| Loss | 2 | `DBLoss`, `MultiLoss` |
| Optimizer | 1 | `Adam` |
| Schedule | 1 | `Cosine` |
| Dataset transforms | 17 | `MakeShrinkMap`, `MakeBorderMap`, `IaaAugment`, `CopyPaste`, `RandomPerspective`, … |

**Four** architecture components against `121`. That is the actual shape of the
decision: not "port training" versus "do not", but *which* of those two numbers
the project is signing up for.

The `17` dataset transforms are worth naming separately. They are not autograd —
they are augmentation and label encoding, and several (`MakeShrinkMap`,
`MakeBorderMap`) are the exact geometric operations this port already implements
for **inference** postprocessing, run in the opposite direction.

## 3. What this project already has that would carry over

Not nothing, and the overlap is not accidental:

| Already implemented | Training use |
|---|---|
| Unclipping and shrinking (`src/unclip.rs`) | `MakeShrinkMap`, `MakeBorderMap` |
| Perspective transforms (`src/crop.rs`) | `RandomPerspective` |
| Linear and cubic resize | `RecResizeImg`, `DetResizeForTest` |
| Normalization conventions (five of them) | `NormalizeImage` |
| CTC decoding (`src/ctc.rs`) | the forward half of `CTCLoss` |
| The evaluation metrics (`src/metrics.rs`) | validation |

The **gradients** are what is missing, and they are the hard part: an operator
that agrees with Paddle on a forward pass can disagree on a backward one, and
nothing in this repository currently tests a gradient.

## 4. Three constraints already recorded that bear on this

**`unsafe_code = "forbid"`** at the crate level (`Cargo.toml`,
`docs/SAFE_001_AUDIT.md`). Most Rust tensor frameworks use `unsafe` for their
kernels; adopting one either relaxes that or confines it behind a boundary this
project would have to audit.

**Two dependencies at default features.** `docs/DEPLOY_DEC_001_EVIDENCE.md`
records `8` crates and a `361 KB` binary. A tensor framework is a dependency tree
larger than everything here combined.

**Every claim is checked against a capture.** That method is what has caught this
project's recorded bugs. Applying it to training means numerical gradient
comparison against Paddle for every operator — the same discipline, at `121`
times the scale, or at `4`.

## 5. What this does not decide

Whether to port training at all; which framework; whether to target the full
surface or the minimal one; whether checkpoints interoperate with Paddle's.

No recommendation is offered. `D-011` trades a very large amount of work against
a capability this port has never claimed, and the size of that trade is now
measured rather than guessed — which is all this document is for.
