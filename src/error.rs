// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Structured errors for public PaddleOCR-Rust foundations.

use std::error::Error as StdError;
use std::fmt;

/// Result type used by PaddleOCR-Rust public foundations.
pub type Result<T> = std::result::Result<T, Error>;

/// A category of invalid user-supplied value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputViolation {
    /// A required value was empty or zero.
    Empty,
    /// A floating-point value was NaN or infinite.
    NonFinite,
    /// A value fell outside its supported range.
    OutOfRange,
    /// A geometry value had zero area or otherwise could not represent a shape.
    DegenerateGeometry,
    /// An identifier contained unsupported or control characters.
    InvalidIdentifier,
    /// Encoded data was truncated, corrupt, or violated its container format.
    Malformed,
}

impl fmt::Display for InputViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "must not be empty or zero",
            Self::NonFinite => "must be finite",
            Self::OutOfRange => "is outside the supported range",
            Self::DegenerateGeometry => "does not describe non-degenerate geometry",
            Self::InvalidIdentifier => "contains an invalid identifier value",
            Self::Malformed => "is truncated, corrupt, or violates its container format",
        };
        formatter.write_str(message)
    }
}

/// A category of model-artifact failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelProblem {
    /// The required artifact was not supplied.
    Missing,
    /// The supplied artifact was malformed or failed integrity checks.
    Corrupt,
    /// The supplied artifact does not match its declared identity.
    IdentityMismatch,
    /// The artifact's input/output tensors do not satisfy the required contract.
    TensorContract,
}

impl fmt::Display for ModelProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Missing => "model artifact is missing",
            Self::Corrupt => "model artifact is corrupt",
            Self::IdentityMismatch => "model artifact identity does not match the manifest",
            Self::TensorContract => "model artifact tensor contract is invalid",
        };
        formatter.write_str(message)
    }
}

/// An error reported by the public Rust surface.
#[derive(Debug)]
pub enum Error {
    /// A user-provided value did not satisfy its documented contract.
    InvalidInput {
        /// The stable field or value name that failed validation.
        field: &'static str,
        /// The reason validation failed.
        violation: InputViolation,
    },
    /// A documented resource limit would be exceeded.
    ResourceLimit {
        /// The stable resource name being bounded.
        resource: &'static str,
        /// The inclusive maximum permitted value.
        limit: u64,
        /// The observed value that exceeded the limit.
        actual: u64,
    },
    /// A model artifact was absent, corrupt, incompatible, or had an invalid tensor ABI.
    Model {
        /// The stable category of model failure.
        problem: ModelProblem,
    },
    /// An inference backend failed without a usable model result.
    Backend {
        /// A stable implementation-controlled failure description.
        message: &'static str,
    },
    /// An I/O operation failed.
    Io {
        /// The operation that failed, without embedding a user-controlled path.
        operation: &'static str,
        /// The underlying operating-system error.
        source: std::io::Error,
    },
    /// The requested feature is intentionally unsupported for the current scope.
    Unsupported {
        /// The stable name of the unsupported capability.
        capability: &'static str,
    },
    /// Work was cancelled before a result was produced.
    Cancelled,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, violation } => {
                write!(formatter, "invalid input {field}: {violation}")
            }
            Self::ResourceLimit {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "resource limit exceeded for {resource}: actual {actual}, limit {limit}"
            ),
            Self::Model { problem } => write!(formatter, "model error: {problem}"),
            Self::Backend { message } => write!(formatter, "backend error: {message}"),
            Self::Io { operation, source } => {
                write!(formatter, "I/O error during {operation}: {source}")
            }
            Self::Unsupported { capability } => {
                write!(formatter, "unsupported capability: {capability}")
            }
            Self::Cancelled => formatter.write_str("operation cancelled"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
