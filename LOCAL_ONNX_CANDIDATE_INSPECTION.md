# Local ONNX Candidate Inspection

Roadmap items: MOD-001, LIC-001  
Status: User-authorized external ONNX candidates were inventoried and parsed;
neither candidate is accepted, supported, converted, redistributed, or bundled  
Inspection date: 2026-08-02  
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Scope and boundary

The project user explicitly authorized a one-time Hugging Face CLI download of
the two revision-pinned ONNX candidates into a user-chosen directory outside
this repository and the read-only PaddleOCR checkout. The complete external
record, including its local root, is retained beside those files as
provisioning-evidence.json. It is intentionally not a project resolver,
environment-variable, cache, CLI option, or distribution contract.

The download used hf 1.26.0. Every expected package-root file was verified as
a regular non-symlink file. Hugging Face local metadata recorded the requested
immutable revision for each downloaded file. No file was copied into this
repository, test fixture, Cargo package, or build output.

This inspection uses onnx 1.22.0 only as an isolated developer-side protobuf
parser. It performed onnx.checker.check_model with full_check disabled and
never loaded ONNX Runtime or executed model inference.

## File inventory and hash verification

| Candidate key | Repository and revision | File | Bytes | SHA-256 | Result |
|---|---|---|---:|---|---|
| m2-onnx-det-v6-medium | PaddlePaddle/PP-OCRv6_medium_det_onnx at 61323801669c338b7891481ec7bac61ce31b576a | .gitattributes | 1,519 | 11ad7efa24975ee4b0c3c3a38ed18737f0658a5f75a0a96787b576a78a023361 | Recorded locally |
| m2-onnx-det-v6-medium | Same | README.md | 16,089 | 3046e3aab0194a2291bb3941c93b980c2b3a938a24a5be88354968f6d6187ac8 | Recorded locally |
| m2-onnx-det-v6-medium | Same | inference.json | 312,150 | 0f1a7ec35da36173529c7a60238b7f7919e3831929c3f700ad90ad4896adecd5 | Matches candidate metadata |
| m2-onnx-det-v6-medium | Same | inference.onnx | 62,032,837 | eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1 | Matches candidate metadata |
| m2-onnx-det-v6-medium | Same | inference.yml | 886 | 7298d5ead546584af2504d03355f881ac7a7bc0eb1e282d3e159277c1d0af871 | Matches candidate metadata |
| m2-onnx-rec-v6-medium | PaddlePaddle/PP-OCRv6_medium_rec_onnx at 50c7eacafc52fa7bcf4194e8cd08e46f8558504b | .gitattributes | 1,519 | 11ad7efa24975ee4b0c3c3a38ed18737f0658a5f75a0a96787b576a78a023361 | Recorded locally |
| m2-onnx-rec-v6-medium | Same | README.md | 16,587 | ebce8d28436623ecab4952e24935aed86b3f8ecaf8f8736b92d5544f60fae1e9 | Recorded locally |
| m2-onnx-rec-v6-medium | Same | inference.json | 221,814 | 0b2e25e990bd072f1bf77d59d67d508bce6c4bd44af6624e0fb27d6da2cd00e8 | Matches candidate metadata |
| m2-onnx-rec-v6-medium | Same | inference.onnx | 76,554,979 | 9c09abf0957f7968c7586464b7397b84ad2387a0497a351af40e9acc71b673ba | Matches candidate metadata |
| m2-onnx-rec-v6-medium | Same | inference.yml | 150,580 | 991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129 | Matches candidate metadata |

The locally present inference.json files match the previously recorded static
candidate metadata hashes. That is file-identity evidence only; it does not
make either ONNX package graph-, numerical-, license-, or runtime-equivalent
to a static package.

## Parse-only graph inspection

| Role | ONNX opset | Input | Output | Nodes / initializers | External tensor data |
|---|---:|---|---|---:|---|
| Detector | 14 | x: FLOAT [dynamic, 3, dynamic, dynamic] | fetch_name_0: FLOAT [dynamic, 1, dynamic, dynamic] | 276 / 226 | None |
| Recognizer | 11 | x: FLOAT [dynamic, 3, 48, dynamic] | fetch_name_0: FLOAT [dynamic, dynamic, 18,710] | 508 / 254 | None |

The detector uses Add, Concat, Conv, ConvTranspose, Div, Erf, HardSigmoid,
MaxPool, Mul, ReduceMean, Relu, Resize, and Sigmoid. The recognizer uses Add,
AveragePool, BatchNormalization, Concat, Conv, Div, Erf, HardSigmoid,
Identity, MatMul, MaxPool, Mul, Pow, ReduceMean, Relu, Reshape, Shape,
Sigmoid, Slice, Softmax, Sqrt, Squeeze, Sub, Transpose, and Unsqueeze.

These are graph facts for runtime qualification. They do not select a runtime
or demonstrate that any particular backend supports the graphs correctly.

## Local package license observation

Both README.md files start with the model-card field license: apache-2.0 and
display a badge pointing to ./LICENSE. Neither downloaded package contains a
top-level LICENSE file. This confirms the previously recorded evidence gap; it
does not resolve the applicable terms for weights, embedded dictionary data, or
redistribution.

## Remaining gates

1. Review durable revision-specific terms and publisher/rightsholder evidence
   for the selected representation under LIC-001.
2. Verify the recognizer dictionary's exact 18,710-class CTC ABI, including
   blank and space handling.
3. Run bounded runtime candidate proofs and raw tensor comparisons before
   selecting a backend under RT-002 through RT-004.
4. Resolve the artifact lifecycle and local-path policy under MODEL-DEC-001
   and MOD-002 through MOD-004.
5. Obtain legal offline input fixtures and differential results before making
   a detector, recognizer, pipeline, API, CLI, model, or compatibility claim.
