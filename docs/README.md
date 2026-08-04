# Project Documentation

This directory contains the supporting specifications, evidence packets,
contracts, and planning records for PaddleOCR-Rust. The repository root keeps
only the discovery and authority entrypoints: [README](../README.md),
[AGENTS](../AGENTS.md), [CLAUDE](../CLAUDE.md), and the canonical
[ROADMAP](../ROADMAP.md).

## Repository guides

- [Contribution guide](CONTRIBUTING.md)

## Planning and governance

- [P0 decisions](P0_DECISIONS.md)
- [Scope proposal](SCOPE_PROPOSAL.md) and [scope classification](SCOPE_CLASSIFICATION.md)
- [Inventory](INVENTORY.md) and [PaddleX baseline](PADDLEX_BASELINE.md)
- [First-slice evidence](FIRST_SLICE_EVIDENCE.md)
- [Identity and license evidence](IDENTITY_LICENSE_EVIDENCE.md)
- [Quality profile](QUALITY_PROFILE.md) and [risk register](RISK_REGISTER.md)
- [Gate G3 resource evidence](G3_RESOURCE_EVIDENCE.md) — the measured latency,
  memory, and binary figures behind the quality profile's budgets

## Contracts and architecture

- [Architecture](ARCHITECTURE.md)
- [Classic OCR contract](CLASSIC_OCR_CONTRACT.md) and [DB postprocessing specification](DB_POSTPROCESS_SPEC.md)
- [API contract](API_CONTRACT.md)
- [Compatibility ledger](COMPATIBILITY.md)
- [M2 contract coverage](M2_CONTRACT_COVERAGE.md)

## Models and runtime

- [Model candidates](MODEL_CANDIDATES.md)
- [Candidate provisioning ledger](CANDIDATE_PROVISIONING_LEDGER.md)
- [License review](LICENSE_REVIEW.md)
- [Local ONNX inspection](LOCAL_ONNX_CANDIDATE_INSPECTION.md)
- [Runtime rubric](RUNTIME_RUBRIC.md), [runtime proof plan](RUNTIME_PROOF_PLAN.md), [runtime scorecard](RUNTIME_SCORECARD.md), and the [RT-004 backend selection ADR](ADR_RT004_RUNTIME_SELECTION.md)
- [Tract evidence](RUNTIME_TRACT_EVIDENCE.md), [ORT evidence](RUNTIME_ORT_EVIDENCE.md), and [source-built ORT evidence](RUNTIME_ORT_SOURCE_EVIDENCE.md)

The reusable [parse-only ONNX ABI inspection](ONNX_ABI_INSPECTION.md), the
matching [parse-only static program ABI inspection](STATIC_ABI_INSPECTION.md),
and the aggregate-only recognizer dictionary replay described in
[Local ONNX inspection](LOCAL_ONNX_CANDIDATE_INSPECTION.md) record bounded
candidate facts without making a model or Python a normal build dependency.

## Inputs, fixtures, and oracle work

- [Image decoder decision](IMAGE_DECODER_DECISION.md), [image decoder evidence](IMAGE_DECODER_EVIDENCE.md), and [hybrid candidate source review](IMAGE_DECODER_SOURCE_REVIEW.md)
- [Bounded primitive fuzzing](FUZZING.md)
- [Fixture and tolerance plan](FIXTURE_AND_TOLERANCE_PLAN.md)
- [Oracle capture procedure](ORACLE_CAPTURE.md)
- [Crop oracle capture](CROP_ORACLE_CAPTURE.md)

All documents describe current evidence and gates. They do not imply a
supported OCR runtime, model, decoder, or public OCR API unless the relevant
compatibility row explicitly says so.
