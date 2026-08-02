# PaddleX Baseline Resolution Record

Roadmap item: BASE-002
Status: Deferred from M2 by P0_DECISIONS.md; still unresolved for modern parity
Inspection date: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose

The modern PaddleOCR Python facade delegates predictor, pipeline, configuration,
device, and dependency behavior to PaddleX. This record identifies the exact
missing reference needed before any modern-facade compatibility claim is made.

This is an oracle/reference record only. PaddleX is not a Rust build, test,
runtime, or package dependency.

## Evidence from the pinned PaddleOCR checkout

The top-level package dependency is a range, not a pinned artifact:

| PaddleOCR dependency surface | Declared PaddleX requirement |
|---|---|
| Core package | paddlex[ocr-core] >=3.7.0,<3.8.0 |
| doc-parser extra | paddlex[ocr,genai-client] >=3.7.0,<3.8.0 |
| IE extra | paddlex[ie] >=3.7.0,<3.8.0 |
| Translation extra | paddlex[trans] >=3.7.0,<3.8.0 |
| all extra | paddlex[ocr,genai-client,ie,trans] >=3.7.0,<3.8.0 |

Source: PaddleOCR/pyproject.toml.

No poetry.lock, uv.lock, pdm.lock, constraints file, requirements lock, or
local PaddleX checkout was found in the inspected workspace. Therefore the
PaddleOCR commit alone does not reveal which 3.7.x PaddleX artifact a historical
or current installer resolved.

## Official tag candidates in the permitted range

Read-only tag discovery against the official PaddleX Git repository returned:

| Version tag | Commit | Reference |
|---|---|---|
| v3.7.0 | e0068ce0bfe75b2992e5b38d06a0393c70f887f7 | https://github.com/PaddlePaddle/PaddleX/tree/v3.7.0 |
| v3.7.1 | f802015bb9587e42d0abda1dcc645eb3171cdeb7 | https://github.com/PaddlePaddle/PaddleX/tree/v3.7.1 |
| v3.7.2 | ffb64904d23708863ff5b8da312a5cbd52a7f462 | https://github.com/PaddlePaddle/PaddleX/tree/v3.7.2 |

At inspection time, the official release/3.7 branch also resolved to
ffb64904d23708863ff5b8da312a5cbd52a7f462. This is evidence of the branch head,
not evidence that a particular PaddleOCR installation used v3.7.2.

## Modern facade interfaces that depend on PaddleX

| PaddleOCR source | PaddleX interface observed | Meaning for the Rust port |
|---|---|---|
| paddleocr/_models/base.py | paddlex.create_predictor | Modern standalone model facade delegates creation and inference |
| paddleocr/_pipelines/base.py | paddlex.create_pipeline | Modern pipeline facade delegates orchestration |
| paddleocr/_pipelines/base.py | paddlex.inference.load_pipeline_config | Pipeline names/config paths are resolved by PaddleX |
| paddleocr/_pipelines/base.py | paddlex.utils.config.AttrDict | Config merge/export behavior depends on PaddleX data semantics |
| paddleocr/_models/base.py and _pipelines/base.py | paddlex.utils.deps.DependencyError | Dependency failures are normalized by the facade |
| paddleocr/_common_args.py | paddlex.utils.device.get_default_device and parse_device | Device default/parse behavior is delegated |
| paddleocr/_pipelines/doc_understanding.py | paddlex.utils.pipeline_arguments.custom_type | Pipeline argument parsing behavior is delegated |
| paddleocr/__init__.py | paddlex.inference.utils.benchmark.benchmark | Public benchmark helper is delegated |

The Python facade advertises engines paddle, paddle_static, paddle_dynamic,
transformers, and onnxruntime. A Rust port must not mirror these names or
semantics unless a specific compatibility contract has been approved and tested.

## Consequence

The exact behavior of modern wrappers, particularly model lookup, automatic
downloads, default pipeline configurations, engine configuration, device
selection, and some error paths, cannot be inferred uniquely from the current
PaddleOCR checkout.

Classic source under ppocr/, the C++ deployment source, and the browser ONNX
implementation remain independent behavior references for native Rust work.
They do not resolve modern PaddleX pipeline semantics.

## Resolution options

| Option | Result | Evidence strength | Trade-off |
|---|---|---|---|
| Provide the original resolved environment | Pin its exact PaddleX wheel/version and source commit | Strongest historical compatibility evidence | Requires a lockfile, environment manifest, or installed-package record |
| Explicitly approve v3.7.2 as the oracle | Pin ffb64904d23708863ff5b8da312a5cbd52a7f462 | Reproducible forward baseline inside the declared range | Does not prove it was the resolver result for the current PaddleOCR checkout |
| Explicitly approve v3.7.0 or v3.7.1 | Pin the corresponding tag above | Reproducible baseline | Same historical limitation; potentially older behavior |
| Defer modern facade parity | Keep modern wrappers as unverified reference-only behavior | Honest and safe for early classic OCR porting | Delays P7-P11 wrapper/pipeline compatibility claims |

Recommended default for a newly chosen forward oracle is v3.7.2 because it is
the latest discovered release tag within the declared range and matches the
release/3.7 branch head at inspection time. This is not a decision and must not
be treated as one until explicitly approved.

## Scoped resolution and condition for modern-parity work

The M2 scope explicitly defers modern PaddleX-wrapper and pipeline parity. This
resolves `BASE-002` only for the classic M2 slice; it does not establish a
PaddleX baseline or permit modern compatibility claims.

Before any modern-wrapper or modern-pipeline parity work starts, one of the
following must be recorded in ROADMAP.md:

1. the exact PaddleX version/commit resolved by the intended reference
   environment; or
2. explicit approval to use one candidate tag as the reproducible forward
   oracle; or
3. explicit approval to defer modern PaddleX-wrapper parity from the scoped
   milestone.

When unblocked, capture the selected source/release in an isolated read-only
reference workflow. Do not add PaddleX as a dependency of PaddleOCR-Rust and do
not execute it inside ./PaddleOCR.
