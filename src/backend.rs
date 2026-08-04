// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Backend-neutral inference adapter contract.
//!
//! `docs/ADR_RT004_RUNTIME_SELECTION.md` selects ONNX Runtime through `ort` as
//! the initial backend. This module is deliberately the half of `RT-005` that
//! knows nothing about that choice: it owns model identity, the tensor
//! contract, resource budgets, and error mapping, and it exposes a trait that a
//! backend implements.
//!
//! Keeping the validation here rather than in the backend implementation is
//! what makes the ADR's reversal condition cheap. Replacing the backend means
//! writing one more [`InferenceBackend`] implementation; none of the checks in
//! this file move, and no public type changes.
//!
//! Nothing in this module loads a native library, opens a session, or performs
//! inference. Everything here is testable offline without a model.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{Error, ModelProblem, Result};

/// Maximum model artifact size accepted by identity verification.
const MAX_MODEL_BYTES: u64 = 512 * 1024 * 1024;

/// Bytes read per digest chunk while streaming an artifact.
const DIGEST_CHUNK_BYTES: usize = 1024 * 1024;

/// Length of a lowercase hexadecimal SHA-256 string.
const SHA256_HEX_LEN: usize = 64;

/// A required extent for one tensor axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AxisExtent {
    /// The axis must equal this exact value.
    Fixed(usize),
    /// The axis may vary but must lie inside this inclusive range.
    Bounded { minimum: usize, maximum: usize },
}

impl AxisExtent {
    fn accepts(self, value: usize) -> bool {
        match self {
            Self::Fixed(expected) => value == expected,
            Self::Bounded { minimum, maximum } => (minimum..=maximum).contains(&value),
        }
    }
}

/// The declared contract for one named model tensor.
///
/// A backend must never be trusted to report the tensor it was asked for. Every
/// field here is checked against what the backend actually returns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TensorContract {
    name: &'static str,
    axes: Vec<AxisExtent>,
}

impl TensorContract {
    /// Builds a contract for one `float32` tensor.
    pub(crate) fn new(name: &'static str, axes: Vec<AxisExtent>) -> Result<Self> {
        if name.is_empty() || axes.is_empty() {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        Ok(Self { name, axes })
    }

    /// Returns the declared tensor name.
    pub(crate) const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the declared rank.
    pub(crate) fn rank(&self) -> usize {
        self.axes.len()
    }

    /// Validates one observed tensor name and shape against this contract.
    pub(crate) fn validate(&self, name: &str, shape: &[usize]) -> Result<()> {
        if name != self.name {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        if shape.len() != self.axes.len() {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        for (axis, extent) in shape.iter().zip(&self.axes) {
            if !extent.accepts(*axis) {
                return Err(Error::Model {
                    problem: ModelProblem::TensorContract,
                });
            }
        }
        Ok(())
    }
}

/// A model artifact identified by an explicit local path and expected digest.
///
/// Resolution is explicit by construction: there is no search path, no cache
/// directory, no environment variable, and no download.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ModelArtifact {
    path: PathBuf,
    expected_sha256: String,
}

impl ModelArtifact {
    /// Declares an artifact without touching the filesystem.
    pub(crate) fn new(
        path: impl Into<PathBuf>,
        expected_sha256: impl Into<String>,
    ) -> Result<Self> {
        let expected_sha256 = expected_sha256.into();
        let valid = expected_sha256.len() == SHA256_HEX_LEN
            && expected_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase() && byte <= b'f');
        if !valid {
            return Err(Error::Model {
                problem: ModelProblem::IdentityMismatch,
            });
        }
        Ok(Self {
            path: path.into(),
            expected_sha256,
        })
    }

    /// Returns the explicit local path.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the expected lowercase hexadecimal digest.
    pub(crate) fn expected_sha256(&self) -> &str {
        &self.expected_sha256
    }

    /// Streams the artifact and reports whether its digest matches.
    ///
    /// A backend must call this before loading the file. The size bound applies
    /// before any read, so a wrong path cannot cause unbounded work.
    pub(crate) fn verify(&self, digest: &mut dyn Sha256Stream) -> Result<()> {
        let metadata = std::fs::symlink_metadata(&self.path).map_err(|source| Error::Io {
            operation: "inspect model artifact",
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(Error::Model {
                problem: ModelProblem::Corrupt,
            });
        }
        if metadata.len() == 0 || metadata.len() > MAX_MODEL_BYTES {
            return Err(Error::ResourceLimit {
                resource: "model.bytes",
                limit: MAX_MODEL_BYTES,
                actual: metadata.len(),
            });
        }

        let mut file = File::open(&self.path).map_err(|source| Error::Io {
            operation: "open model artifact",
            source,
        })?;
        let mut buffer = vec![0_u8; DIGEST_CHUNK_BYTES];
        let mut seen = 0_u64;
        loop {
            let read = file.read(&mut buffer).map_err(|source| Error::Io {
                operation: "read model artifact",
                source,
            })?;
            if read == 0 {
                break;
            }
            seen += read as u64;
            if seen > metadata.len() {
                return Err(Error::Model {
                    problem: ModelProblem::Corrupt,
                });
            }
            digest.update(&buffer[..read]);
        }
        if seen != metadata.len() {
            return Err(Error::Model {
                problem: ModelProblem::Corrupt,
            });
        }
        if digest.finish() != self.expected_sha256 {
            return Err(Error::Model {
                problem: ModelProblem::IdentityMismatch,
            });
        }
        Ok(())
    }
}

/// A streaming SHA-256 implementation supplied by the caller.
///
/// The adapter does not choose a hash crate: the digest implementation is
/// injected so that this module stays dependency-free and testable.
pub(crate) trait Sha256Stream {
    /// Adds bytes to the digest.
    fn update(&mut self, bytes: &[u8]);
    /// Consumes the accumulated digest as lowercase hexadecimal.
    fn finish(&mut self) -> String;
}

/// Resource bounds applied before any backend call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RunBudget {
    max_input_elements: u64,
    max_output_elements: u64,
    max_batch: usize,
}

impl RunBudget {
    /// Builds a budget with strictly positive bounds.
    pub(crate) fn new(
        max_input_elements: u64,
        max_output_elements: u64,
        max_batch: usize,
    ) -> Result<Self> {
        if max_input_elements == 0 || max_output_elements == 0 || max_batch == 0 {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        Ok(Self {
            max_input_elements,
            max_output_elements,
            max_batch,
        })
    }

    /// Rejects an input shape that exceeds the declared bounds.
    pub(crate) fn admit_input(&self, shape: &[usize]) -> Result<u64> {
        let elements = element_count(shape)?;
        if elements > self.max_input_elements {
            return Err(Error::ResourceLimit {
                resource: "backend.input_elements",
                limit: self.max_input_elements,
                actual: elements,
            });
        }
        let batch = shape.first().copied().unwrap_or(1);
        if batch > self.max_batch {
            return Err(Error::ResourceLimit {
                resource: "backend.batch",
                limit: self.max_batch as u64,
                actual: batch as u64,
            });
        }
        Ok(elements)
    }

    /// Rejects an output shape that exceeds the declared bounds.
    pub(crate) fn admit_output(&self, shape: &[usize]) -> Result<u64> {
        let elements = element_count(shape)?;
        if elements > self.max_output_elements {
            return Err(Error::ResourceLimit {
                resource: "backend.output_elements",
                limit: self.max_output_elements,
                actual: elements,
            });
        }
        Ok(elements)
    }
}

/// Returns the element count of a shape, rejecting zero and overflow.
fn element_count(shape: &[usize]) -> Result<u64> {
    if shape.is_empty() || shape.contains(&0) {
        return Err(Error::Model {
            problem: ModelProblem::TensorContract,
        });
    }
    let mut elements = 1_u64;
    for axis in shape {
        let axis = u64::try_from(*axis).map_err(|_| Error::Model {
            problem: ModelProblem::TensorContract,
        })?;
        elements = elements.checked_mul(axis).ok_or(Error::Model {
            problem: ModelProblem::TensorContract,
        })?;
    }
    Ok(elements)
}

/// One validated `float32` tensor crossing the adapter boundary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BackendTensor {
    shape: Vec<usize>,
    values: Vec<f32>,
}

impl BackendTensor {
    /// Builds a tensor whose value count matches its shape exactly.
    pub(crate) fn new(shape: Vec<usize>, values: Vec<f32>) -> Result<Self> {
        let elements = element_count(&shape)?;
        if elements != values.len() as u64 {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        Ok(Self { shape, values })
    }

    /// Returns the tensor shape.
    pub(crate) fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Returns the tensor values.
    pub(crate) fn values(&self) -> &[f32] {
        &self.values
    }
}

/// The full contract one model must satisfy at the adapter boundary.
#[derive(Clone, Debug)]
pub(crate) struct ModelContract {
    artifact: ModelArtifact,
    input: TensorContract,
    output: TensorContract,
    budget: RunBudget,
}

impl ModelContract {
    /// Assembles a model contract.
    pub(crate) const fn new(
        artifact: ModelArtifact,
        input: TensorContract,
        output: TensorContract,
        budget: RunBudget,
    ) -> Self {
        Self {
            artifact,
            input,
            output,
            budget,
        }
    }

    /// Returns the declared artifact.
    pub(crate) const fn artifact(&self) -> &ModelArtifact {
        &self.artifact
    }

    /// Returns the declared input contract.
    pub(crate) const fn input(&self) -> &TensorContract {
        &self.input
    }

    /// Returns the declared output contract.
    pub(crate) const fn output(&self) -> &TensorContract {
        &self.output
    }

    /// Validates an input tensor before it reaches a backend.
    pub(crate) fn admit_input(&self, tensor: &BackendTensor) -> Result<()> {
        self.input.validate(self.input.name(), tensor.shape())?;
        self.budget.admit_input(tensor.shape())?;
        Ok(())
    }

    /// Validates what a backend actually returned.
    ///
    /// The observed name is checked even though this crate supplied it, because
    /// a backend that returns a different output than the one requested is a
    /// contract error rather than a silent success.
    pub(crate) fn admit_output(&self, name: &str, tensor: &BackendTensor) -> Result<()> {
        self.output.validate(name, tensor.shape())?;
        self.budget.admit_output(tensor.shape())?;
        if tensor.values().iter().any(|value| !value.is_finite()) {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        Ok(())
    }
}

/// A validated inference backend.
///
/// Implementations must not surface a backend-specific type, error, or handle
/// through this trait. `run` receives an already validated input and its output
/// is validated again by [`run_validated`] before any caller sees it.
pub(crate) trait InferenceBackend {
    /// Runs one already validated input and returns the named output.
    fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)>;
}

/// Runs one inference with contract and budget checks on both sides.
pub(crate) fn run_validated(
    backend: &dyn InferenceBackend,
    contract: &ModelContract,
    input: &BackendTensor,
) -> Result<BackendTensor> {
    contract.admit_input(input)?;
    let (name, output) = backend.run(input)?;
    contract.admit_output(&name, &output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    /// A fake backend: no native library, no model, no inference.
    struct FakeBackend {
        name: String,
        response: RefCell<Option<BackendTensor>>,
        error: Option<Error>,
    }

    impl FakeBackend {
        fn returning(name: &str, tensor: BackendTensor) -> Self {
            Self {
                name: name.to_owned(),
                response: RefCell::new(Some(tensor)),
                error: None,
            }
        }

        fn failing() -> Self {
            Self {
                name: String::new(),
                response: RefCell::new(None),
                error: Some(Error::Backend {
                    message: "fake backend failure",
                }),
            }
        }
    }

    impl InferenceBackend for FakeBackend {
        fn run(&self, _input: &BackendTensor) -> Result<(String, BackendTensor)> {
            if self.error.is_some() {
                return Err(Error::Backend {
                    message: "fake backend failure",
                });
            }
            match self.response.borrow_mut().take() {
                Some(tensor) => Ok((self.name.clone(), tensor)),
                None => Err(Error::Backend {
                    message: "fake backend exhausted",
                }),
            }
        }
    }

    fn detector_contract() -> ModelContract {
        let artifact = match ModelArtifact::new("/nonexistent/inference.onnx", "0".repeat(64)) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid artifact declaration, got {error}"),
        };
        let input = match TensorContract::new(
            "x",
            vec![
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(3),
                AxisExtent::Bounded {
                    minimum: 32,
                    maximum: 960,
                },
                AxisExtent::Bounded {
                    minimum: 32,
                    maximum: 960,
                },
            ],
        ) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output = match TensorContract::new(
            "fetch_name_0",
            vec![
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(1),
                AxisExtent::Bounded {
                    minimum: 32,
                    maximum: 960,
                },
                AxisExtent::Bounded {
                    minimum: 32,
                    maximum: 960,
                },
            ],
        ) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid output contract, got {error}"),
        };
        let budget = match RunBudget::new(4_000_000, 1_000_000, 8) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn tensor(shape: Vec<usize>, fill: f32) -> BackendTensor {
        let count: usize = shape.iter().product();
        match BackendTensor::new(shape, vec![fill; count]) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a valid tensor, got {error}"),
        }
    }

    #[test]
    fn a_conforming_backend_result_is_returned() {
        let contract = detector_contract();
        let backend = FakeBackend::returning("fetch_name_0", tensor(vec![1, 1, 32, 32], 0.5));
        let output = match run_validated(&backend, &contract, &tensor(vec![1, 3, 32, 32], 0.25)) {
            Ok(output) => output,
            Err(error) => panic!("expected a validated output, got {error}"),
        };
        assert_eq!(output.shape(), [1, 1, 32, 32]);
        assert_eq!(output.values().len(), 1024);
    }

    #[test]
    fn an_unexpected_output_name_is_a_contract_error() {
        let contract = detector_contract();
        let backend = FakeBackend::returning("some_other_output", tensor(vec![1, 1, 32, 32], 0.5));
        assert!(matches!(
            run_validated(&backend, &contract, &tensor(vec![1, 3, 32, 32], 0.25)),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn an_unexpected_output_shape_is_a_contract_error() {
        let contract = detector_contract();
        // A backend that silently returns three channels instead of one.
        let backend = FakeBackend::returning("fetch_name_0", tensor(vec![1, 3, 32, 32], 0.5));
        assert!(matches!(
            run_validated(&backend, &contract, &tensor(vec![1, 3, 32, 32], 0.25)),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn a_non_finite_output_value_is_a_contract_error() {
        let contract = detector_contract();
        let mut values = vec![0.5_f32; 1024];
        values[512] = f32::NAN;
        let poisoned = match BackendTensor::new(vec![1, 1, 32, 32], values) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a constructible tensor, got {error}"),
        };
        let backend = FakeBackend::returning("fetch_name_0", poisoned);
        assert!(matches!(
            run_validated(&backend, &contract, &tensor(vec![1, 3, 32, 32], 0.25)),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn an_input_outside_the_contract_never_reaches_the_backend() {
        let contract = detector_contract();
        // The fake backend is exhausted, so reaching it would surface a
        // distinguishable "exhausted" error instead of the contract error.
        let backend = FakeBackend {
            name: "fetch_name_0".to_owned(),
            response: RefCell::new(None),
            error: None,
        };
        // Height below the contract minimum.
        assert!(matches!(
            run_validated(&backend, &contract, &tensor(vec![1, 3, 16, 32], 0.25)),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
        // Wrong rank.
        assert!(matches!(
            run_validated(&backend, &contract, &tensor(vec![1, 3, 32], 0.25)),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn budgets_reject_oversized_work_before_the_backend_runs() {
        let budget = match RunBudget::new(1_000, 1_000, 4) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        assert!(matches!(
            budget.admit_input(&[1, 3, 64, 64]),
            Err(Error::ResourceLimit {
                resource: "backend.input_elements",
                ..
            })
        ));
        assert!(matches!(
            budget.admit_input(&[8, 1, 1, 1]),
            Err(Error::ResourceLimit {
                resource: "backend.batch",
                ..
            })
        ));
        assert!(matches!(
            budget.admit_output(&[1, 1, 64, 64]),
            Err(Error::ResourceLimit {
                resource: "backend.output_elements",
                ..
            })
        ));
        assert!(matches!(
            RunBudget::new(0, 1, 1),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
    }

    #[test]
    fn a_backend_failure_is_mapped_without_leaking_a_backend_type() {
        let contract = detector_contract();
        let backend = FakeBackend::failing();
        match run_validated(&backend, &contract, &tensor(vec![1, 3, 32, 32], 0.25)) {
            Err(Error::Backend { message }) => assert_eq!(message, "fake backend failure"),
            other => panic!("expected a mapped backend error, got {other:?}"),
        }
    }

    #[test]
    fn element_counts_reject_zero_axes_and_overflow() {
        assert!(matches!(
            element_count(&[]),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
        assert!(matches!(
            element_count(&[1, 0, 3]),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
        assert!(matches!(
            element_count(&[usize::MAX, usize::MAX]),
            Err(Error::Model {
                problem: ModelProblem::TensorContract
            })
        ));
        assert_eq!(
            match element_count(&[2, 3, 4]) {
                Ok(value) => value,
                Err(error) => panic!("expected a counted shape, got {error}"),
            },
            24
        );
    }

    #[test]
    fn artifact_declarations_require_a_lowercase_hexadecimal_digest() {
        assert!(ModelArtifact::new("/model.onnx", "a".repeat(64)).is_ok());
        for invalid in [
            "A".repeat(64),
            "z".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
            String::new(),
        ] {
            assert!(matches!(
                ModelArtifact::new("/model.onnx", invalid),
                Err(Error::Model {
                    problem: ModelProblem::IdentityMismatch
                })
            ));
        }
    }

    struct FakeDigest {
        seen: Vec<u8>,
        result: String,
    }

    impl Sha256Stream for FakeDigest {
        fn update(&mut self, bytes: &[u8]) {
            self.seen.extend_from_slice(bytes);
        }
        fn finish(&mut self) -> String {
            self.result.clone()
        }
    }

    #[test]
    fn artifact_verification_rejects_a_missing_path_without_reading() {
        let artifact = match ModelArtifact::new("/nonexistent/model.onnx", "a".repeat(64)) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid declaration, got {error}"),
        };
        let mut digest = FakeDigest {
            seen: Vec::new(),
            result: "a".repeat(64),
        };
        assert!(matches!(
            artifact.verify(&mut digest),
            Err(Error::Io {
                operation: "inspect model artifact",
                ..
            })
        ));
        assert!(digest.seen.is_empty(), "no bytes may be read");
    }

    #[test]
    fn artifact_verification_reports_a_digest_mismatch() {
        let path = std::env::temp_dir().join("paddleocr-rust-backend-artifact-test.bin");
        if std::fs::write(&path, b"self-authored bytes").is_err() {
            return;
        }
        let artifact = match ModelArtifact::new(&path, "b".repeat(64)) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid declaration, got {error}"),
        };
        let mut digest = FakeDigest {
            seen: Vec::new(),
            result: "c".repeat(64),
        };
        let outcome = artifact.verify(&mut digest);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(
            outcome,
            Err(Error::Model {
                problem: ModelProblem::IdentityMismatch
            })
        ));
        assert_eq!(digest.seen, b"self-authored bytes");
    }
}
