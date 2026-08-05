# Project Documentation

This directory contains the supporting specifications, evidence packets,
contracts, and planning records for PaddleOCR-Rust. The repository root keeps
only the discovery and authority entrypoints: [README](../README.md),
[AGENTS](../AGENTS.md), [CLAUDE](../CLAUDE.md), and the canonical
[ROADMAP](../ROADMAP.md).

## Repository guides

- [Contribution guide](CONTRIBUTING.md)

## Planning and governance

- [Remaining roadmap and next-agent handoff](REMAINING_ROADMAP.md) — what is
  delivered, and the four classes of blocker that are left; a summary of
  [ROADMAP](../ROADMAP.md) section 10, never a substitute for it
- [P0 decisions](P0_DECISIONS.md)
- [Scope proposal](SCOPE_PROPOSAL.md) and [scope classification](SCOPE_CLASSIFICATION.md)
- [Inventory](INVENTORY.md) and [PaddleX baseline](PADDLEX_BASELINE.md)
- [First-slice evidence](FIRST_SLICE_EVIDENCE.md)
- [Identity and license evidence](IDENTITY_LICENSE_EVIDENCE.md)
- [Quality profile](QUALITY_PROFILE.md) and [risk register](RISK_REGISTER.md)
- [SUPPLY-001 supply chain policy](SUPPLY_001_POLICY.md) — the generated SBOM,
  the drift check, and what is deliberately not signed
- [THREAT-001 threat model](THREAT_MODEL.md) — the two trust boundaries, and
  the surfaces that deliberately do not exist
- [ROB-001 robustness evidence](ROB_001_EVIDENCE.md) — the fuzz campaign and
  the clause-by-clause verification
- [CONC-001 concurrency evidence](CONC_001_EVIDENCE.md) — what is enforced by
  the compiler, what is measured, and what this project deliberately does not do
- [LIC-002 licensing audit](LIC_002_AUDIT.md) — every asset resolved, and the
  two gaps it found
- [SAFE-001 unsafe and native boundary audit](SAFE_001_AUDIT.md) — what the
  memory-safety argument actually rests on
- [STABLE-001 API stability review](STABLE_001_API_REVIEW.md) — the semver
  policy, and the growth mechanism the API did not have
- [CLOSE-001 inventory closeout](CLOSE_001_AUDIT.md) — every inventory row
  classified: verified, intentional difference, deferred, or out of scope
- [RC-001 release candidate evidence](RC_001_RELEASE_EVIDENCE.md) — hashes,
  commands, verdicts, and ten known limitations
- [CLEAN-001 clean checkout and package evidence](CLEAN_001_EVIDENCE.md) — what
  a release must be built from, and why
- [Gate P6 evidence](GATE_P6_EVIDENCE.md) — the M2 milestone gate, clause by
  clause, against what was actually run
- [PERF-001 benchmark record](PERF_001_BENCHMARK.md) — startup, throughput, and
  the verdict against every predeclared budget
- [Gate G3 resource evidence](G3_RESOURCE_EVIDENCE.md) — the measured latency,
  memory, and binary figures behind the quality profile's budgets
- [PDF and office input decision](ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md) — why
  office is rejected, why PDF waits, and what a renderer must prove first
- [PDF rendering contract](PDF_RENDER_CONTRACT.md) — the scale planner,
  implemented and matched bit for bit; the measured pdfium figures a future
  renderer must be compared against
- [Model artifact policy](ADR_MODEL_DEC_001_ARTIFACT_POLICY.md) — conversion,
  distribution, cache, integrity, provenance, and licensing decisions
- [Layout contract](LAYOUT_CONTRACT.md) — frozen from the PaddleX baseline, and
  what the artifact config does not say
- [Table classification contract](TABLE_CLASSIFICATION_CONTRACT.md) — two
  operators the two pinned baselines spell the same and compute differently
- [Table cell contract](TABLE_CELLS_CONTRACT.md) — a second oracle on a shared
  code path, and the two bugs it found in layout
- [Table structure contract](TABLE_STRUCTURE_CONTRACT.md) — three facts that
  live in the registration functions, and a vocabulary that is built not read
- [Reconstruction contract](RECONSTRUCTION_CONTRACT.md) — the per-label
  Markdown formatters, and a newline that is added twice
- [Reading order contract](READING_ORDER_CONTRACT.md) — the four XY-cut
  primitives, and why the two cut orders are different reading orders
- [VLM-DEC-001 evidence](VLM_DEC_001_EVIDENCE.md) — three routes with very
  different answers, and a determinism vocabulary that does not apply
- [TRAIN-DEC-001 evidence](TRAIN_DEC_001_EVIDENCE.md) — 121 architecture
  components for full coverage, 4 for the minimal one
- [DEPLOY-DEC-001 evidence](DEPLOY_DEC_001_EVIDENCE.md) — what each deployment
  target measurably costs, and which are not equally distant
- [Config reconciliation](CONFIG_RECONCILIATION.md) — all 139 upstream configs
  classified per file, and why only two are Verified
- [Evaluation metric contract](METRIC_CONTRACT.md) — upstream's detection and
  recognition metrics, and the greedy matcher a reimplementation gets wrong
- [IMG-003 delta measurement](IMG_003_DELTA_MEASUREMENT.md) — what a component
  delta of 36 does to recognized text, and what it does not establish
- [Specialized module API](SPECIALIZED_API.md) — what the table pipeline
  exposes, the two result schemas, and why layout is not exposed
- [Table pipeline contract](TABLE_PIPELINE_CONTRACT.md) — the matcher's score
  is not IoU, and two branches nothing can reach
- [P8 artifact availability](P8_ARTIFACT_AVAILABILITY.md) — the four modules
  with no published ONNX export, and what would unblock them
- [P8 baseline finding](P8_BASELINE_FINDING.md) — why the specialized modules
  cannot be frozen from the pinned checkout
- [Unwarping contract](UNWARPING_CONTRACT.md) — why unwarping has no inverse,
  and what that costs a caller
- [Orientation contract](ORIENTATION_CONTRACT.md) — the frozen text-line
  classifier behaviour, and why document orientation cannot be specified yet
- [User guide](USER_GUIDE.md) — building, provisioning, running, output schema,
  limits, and known differences from upstream
- [Language and script support](LANGUAGE_SUPPORT.md) — the one verified
  artifact/dictionary mapping, and what the dictionary contains but does not
  make supported
- Preprocessing oracle: `tests/fixtures/classic-v1-preprocess-input`
  holds captured upstream detector input tensors; regenerate with
  `tools/capture_preprocess_oracle.py`

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
