# Initial Risk Register

Roadmap item: RISK-001
Status: Approved P0 register; risks remain open until their named gates close
Baseline: PaddleOCR commit 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Register

| ID | Risk | Initial level | Preventive control | Closure evidence / roadmap owner |
|---|---|---|---|---|
| `RISK-001` | Model weights, dictionaries, fonts, fixtures, or conversion output may lack redistributable terms. | High | No assets are bundled or downloaded automatically; retain provenance per artifact. | `LIC-001` is Done only for the exact pinned M2 ONNX pair; `LIC-002` and manifest review must still establish final terms/notices for every selected release artifact. |
| `RISK-002` | The selected v6 medium artifact format, tensor ABI, or supported operators may not be viable in a safe Rust runtime. | High | Do not select a runtime by preference; qualify serious candidates against captured raw tensors. | `MOD-001`, `RT-001`–`RT-004` record accepted/rejected evidence. |
| `RISK-003` | PaddleX delegates modern wrapper/pipeline behavior not recoverable from the pinned PaddleOCR checkout alone. | High | Defer modern facade claims from M2; maintain exact PaddleX baseline record. | A chosen resolver/oracle and later wrapper contracts. |
| `RISK-004` | Image decoding, dimension arithmetic, perspective transforms, or crop batching may panic, overflow, exhaust memory, or hang on hostile input. | High | Predeclare strict byte/pixel/region bounds and checked geometry; avoid `unsafe` by default. | `IMG-*`, `GEO-*`, `SEC-IMG-001`, and fuzz/property tests. |
| `RISK-005` | Detector polygons, reading order, crop orientation, CTC decoding, or numerical scores may drift from source behavior. | High | Freeze contracts/tolerances before implementation and compare legal oracle captures. | `CTR-*`, `FIX-*`, `TOL-001`, component and E2E tests. |
| `RISK-006` | Unicode/dictionary behavior can be corrupted by invalid UTF-8, normalization assumptions, special tokens, or language overclaims. | High | Treat dictionary/tokenizer as an exact artifact ABI and test non-ASCII fixtures. | `MOD-001`, `REC-*`, `LANG-001`, and fixture evidence. |
| `RISK-007` | Rust/native dependencies can introduce licensing, CVE, ABI, thread-safety, or `unsafe` risks. | High | Minimize dependencies; evaluate licenses/native boundaries before selection. | `RT-001`, `IMG-DEC-001`, `SAFE-001`, `SUPPLY-001`. |
| `RISK-008` | Performance claims can be distorted by model format, CPU features, thread counts, cache state, or unrecorded benchmark setup. | Medium | Use the named reference configuration and predeclared budgets. | `PERF-001`, `PERF-002`, `PLAT-001` reports. |
| `RISK-009` | A local upstream symlink can accidentally become a build/test/runtime dependency or be modified by recursive tools. | High | Keep the symlink read-only; normal gates must pass without it. | `CI-001`, clean-clone validation, repeated upstream status checks. |
| `RISK-010` | Branding or package metadata may imply an official PaddlePaddle release. | Medium | Use independent-port wording and no logos/endorsement claims. | `NOTICE`, public documentation review, `DEC-001`. |
| `RISK-011` | The broad M7 target can be silently narrowed after M2 delivers a useful slice. | High | Keep all inventory rows classified as Later rather than excluded and audit at P14. | `SCOPE-002`, `CLOSE-001`, `USER-GATE-001`. |

## Escalation rules

- Any high risk with evidence of an unsatisfied license, unsafe unbounded input
  path, unsupportable selected operator, fabricated result, or unqualified
  platform blocks the affected milestone.
- A mitigation is not closure until a test, review record, or reproducible
  measurement proves it.
- A later decision may add risks but may not delete an open risk without its
  recorded closure evidence.
