# `VLM-DEC-001` evidence — what a VLM would actually require here

Roadmap item: `VLM-DEC-001`, resolving `D-010`
Recorded: 2026-08-05
Status: **evidence, not a decision.** `D-010` is a scope decision and belongs to
the user.

The third of three decision packets recorded the same way — after
`docs/DEPLOY_DEC_001_EVIDENCE.md` and `docs/TRAIN_DEC_001_EVIDENCE.md`. Nothing
here selects a runtime, an adapter, or a scope.

## 1. Upstream has three routes, not one

`paddlex/inference/models/doc_vlm/predictor.py` defines:

| Route | What it is |
|---|---|
| `DocVLMLocalPredictor` | Paddle dynamic graph, in process |
| `DocVLMTransformersPredictor` | Hugging Face `transformers` |
| `DocVLMGenAIClientPredictor` | **a remote service** |

That third one matters more than it looks. `D-010` is phrased as "local Rust VLM
runtime(s), remote server adapters" — and upstream itself treats the remote
adapter as a first-class route, not a fallback. A port could implement the
adapter and none of the local inference, which is a materially different and much
smaller project.

## 2. The artifacts are two orders larger than anything here

| Model | Size | Files |
|---|---|---|
| `PP-DocBee2-3B` | **`8.14 GB`** | 9 |
| `PP-Chart2Table` | `1.43 GB` | 10 |

Against `SLANeXt_wired` at `368 MB`, the largest artifact this project has
provisioned, and a `361 KB` binary.

Neither publishes an ONNX export — both ship `model_state.pdparams` — so
`docs/P8_ARTIFACT_AVAILABILITY.md` already blocks them under `MODEL-DEC-001`
regardless of what `D-010` decides. **A local route is blocked twice**, and the
second block is the one `D-010` does not control.

## 3. Determinism is not available, and upstream says so

The local route's generation parameters are `max_new_tokens`, `temperature`, and
`top_p`. Upstream's own code contains:

```python
"`temperature` is currently not supported by the local model and will be ignored."
```

Every result this project currently produces is reproducible: the same input
gives the same bytes, and `tests/documentation_links.rs` and the fuzz driver
assert determinism where it is claimed. A sampled decoder does not have that
property, and `RECON-001`'s and `SPECAPI-001`'s deterministic-output claims would
not extend to it.

That is not an argument against VLM. It is a statement that the compatibility
vocabulary this repository uses — *bit-identical*, *byte for byte*, *reproduced*
— has **no meaning** for a sampled generation, and `D-010` would need a
different one.

## 4. What a remote adapter would and would not cost

The adapter route needs no artifact, no tensor framework, and no GPU. It needs:

- **An HTTP client.** This project has `8` dependencies and no networking at all;
  `docs/THREAT_MODEL.md`'s trust boundaries assume no outbound connections, and
  `src/resolve.rs` treats "no network" as an *absent capability* rather than a
  flag.
- **A credential path.** Nothing in this repository handles secrets today, and
  `OBS-001` was built specifically so credentials cannot reach a log.
- **Document text leaving the machine.** Currently impossible by construction.

So the remote adapter is small in code and large in **policy**: it inverts two
properties this port currently gets for free.

## 5. What this does not decide

Whether to support VLM at all; local versus remote; which families; what a
result schema would look like when the output is generated rather than decoded.

No recommendation is offered. What the measurements say is that the three sub-
questions have very different answers: a **local** route is blocked on artifacts
independently of `D-010`, a **remote** route is cheap in code and expensive in
policy, and **either** would need a compatibility vocabulary this repository does
not currently have.
