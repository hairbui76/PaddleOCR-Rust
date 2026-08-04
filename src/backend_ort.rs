// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime implementation of the backend-neutral adapter.
//!
//! This is the only module that knows the backend selected by
//! `docs/ADR_RT004_RUNTIME_SELECTION.md`. It is compiled only under the
//! off-by-default `onnxruntime` feature, so the default build has no native
//! dependency and no `ort` in its dependency graph.
//!
//! Three properties are deliberate and load-bearing:
//!
//! - **Nothing is downloaded or bundled.** `ort` is configured with
//!   `load-dynamic`, so the ONNX Runtime library is opened at run time from an
//!   explicit caller-supplied path. No build script fetches or links a native
//!   library, and no test in this repository requires one.
//! - **Identity is verified before loading.** [`OrtBackend::load`] verifies the
//!   artifact digest through the backend-neutral [`ModelArtifact`] before a
//!   session is created, so a wrong or tampered file never reaches the runtime.
//! - **No backend type escapes.** Every `ort` error is mapped to this crate's
//!   [`Error`], and no `ort` type appears in a signature outside this module.
//!
//! Runtime initialisation is process-global, which is a property of `ort`
//! rather than a choice made here; [`initialize_runtime`] documents it rather
//! than hiding it.

use std::cell::RefCell;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, Sha256Stream};
use crate::error::{Error, ModelProblem, Result};

/// Initialises the process-global ONNX Runtime environment from an explicit path.
///
/// This must be called once per process before any [`OrtBackend::load`]. The
/// global scope is `ort`'s, not this crate's: the underlying environment and
/// dynamic loader are owned per process by the library.
pub(crate) fn initialize_runtime(library_path: &Path) -> Result<()> {
    let builder = ort::init_from(library_path).map_err(|_| Error::Backend {
        message: "cannot load the ONNX Runtime library from the supplied path",
    })?;
    // `commit` reports whether this call installed the environment; a `false`
    // result means another initialisation already owns the process-global
    // state, which is not an error for this adapter.
    let _installed: bool = builder.with_name("paddleocr-rust").commit();
    Ok(())
}

/// A validated ONNX Runtime session for one model contract.
///
/// The session is held behind a [`RefCell`] rather than a lock because `ort`
/// documents an explicitly non-concurrent same-session `Run` contract. Making
/// the type single-threaded states that limitation in the type system instead
/// of asserting a thread-safety property this project has not verified.
#[derive(Debug)]
pub(crate) struct OrtBackend {
    session: RefCell<Session>,
}

impl OrtBackend {
    /// Verifies the contract's artifact, then opens a session for it.
    ///
    /// The digest is streamed through the caller-supplied implementation before
    /// any runtime call, so a failed identity check costs one file read and
    /// never reaches ONNX Runtime.
    pub(crate) fn load(
        contract: &ModelContract,
        digest: &mut dyn Sha256Stream,
        intra_threads: usize,
        inter_threads: usize,
    ) -> Result<Self> {
        contract.artifact().verify(digest)?;

        let builder = Session::builder().map_err(|_| Error::Backend {
            message: "cannot create an ONNX Runtime session builder",
        })?;
        let builder = builder
            .with_intra_threads(intra_threads)
            .map_err(|_| Error::Backend {
                message: "cannot set the ONNX Runtime intra-op thread count",
            })?;
        let mut builder =
            builder
                .with_inter_threads(inter_threads)
                .map_err(|_| Error::Backend {
                    message: "cannot set the ONNX Runtime inter-op thread count",
                })?;
        let session = builder
            .commit_from_file(contract.artifact().path())
            .map_err(|_| Error::Model {
                problem: ModelProblem::Corrupt,
            })?;

        Ok(Self {
            session: RefCell::new(session),
        })
    }
}

impl InferenceBackend for OrtBackend {
    fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
        let tensor = Tensor::<f32>::from_array((input.shape().to_vec(), input.values().to_vec()))
            .map_err(|_| Error::Backend {
            message: "cannot build an ONNX Runtime input tensor",
        })?;

        let mut session = self.session.try_borrow_mut().map_err(|_| Error::Backend {
            message: "the ONNX Runtime session is already running",
        })?;
        let outputs = session
            .run(ort::inputs!["x" => tensor])
            .map_err(|_| Error::Backend {
                message: "the ONNX Runtime session failed to run",
            })?;

        if outputs.len() != 1 {
            return Err(Error::Model {
                problem: ModelProblem::TensorContract,
            });
        }
        // The name is read back from the runtime rather than assumed, so that
        // the backend-neutral contract check sees what actually came out.
        let name = outputs
            .keys()
            .next()
            .ok_or(Error::Model {
                problem: ModelProblem::TensorContract,
            })?
            .to_owned();
        let (shape, values) = outputs[name.as_str()]
            .try_extract_tensor::<f32>()
            .map_err(|_| Error::Model {
                problem: ModelProblem::TensorContract,
            })?;

        let shape = shape
            .iter()
            .map(|axis| usize::try_from(*axis))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::Model {
                problem: ModelProblem::TensorContract,
            })?;
        let output = BackendTensor::new(shape, values.to_vec())?;
        Ok((name, output))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};

    struct RecordingDigest {
        bytes_seen: usize,
        result: String,
    }

    impl Sha256Stream for RecordingDigest {
        fn update(&mut self, bytes: &[u8]) {
            self.bytes_seen += bytes.len();
        }
        fn finish(&mut self) -> String {
            self.result.clone()
        }
    }

    fn contract_for(path: &Path, expected_sha256: &str) -> ModelContract {
        let artifact = match ModelArtifact::new(path, expected_sha256) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid artifact declaration, got {error}"),
        };
        let axes = vec![AxisExtent::Fixed(1), AxisExtent::Fixed(1)];
        let input = match TensorContract::new("x", axes.clone()) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output = match TensorContract::new("fetch_name_0", axes) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid output contract, got {error}"),
        };
        let budget = match RunBudget::new(16, 16, 1) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    /// Builds a process-unique temporary path for a test artifact.
    ///
    /// A fixed name in the shared temporary directory is predictable, and the
    /// directory is world-writable: another user can pre-create that path as a
    /// symlink, and `fs::write` would follow it. Including the process id and a
    /// counter removes both the collision and the symlink target.
    fn unique_temp_path(stem: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "paddleocr-rust-{stem}-{}-{unique}",
            std::process::id()
        ))
    }

    /// A wrong artifact must fail before ONNX Runtime is involved at all.
    ///
    /// This test runs offline with no native library present, which is exactly
    /// the property being asserted: identity verification precedes loading.
    #[test]
    fn a_digest_mismatch_fails_before_any_runtime_call() {
        let path = unique_temp_path("ort-identity-test");
        if std::fs::write(&path, b"not a real model").is_err() {
            return;
        }
        let contract = contract_for(&path, &"a".repeat(64));
        let mut digest = RecordingDigest {
            bytes_seen: 0,
            result: "b".repeat(64),
        };

        let outcome = OrtBackend::load(&contract, &mut digest, 1, 1);
        let _ = std::fs::remove_file(&path);

        assert!(
            matches!(
                outcome,
                Err(Error::Model {
                    problem: ModelProblem::IdentityMismatch
                })
            ),
            "a mismatched digest must be reported before a session is created"
        );
        assert_eq!(
            digest.bytes_seen, 16,
            "the artifact must be streamed exactly once"
        );
    }

    /// A missing artifact must not reach the runtime or read anything.
    #[test]
    fn a_missing_artifact_fails_before_any_runtime_call() {
        let contract = contract_for(
            Path::new("/nonexistent/paddleocr-rust-ort-missing.onnx"),
            &"a".repeat(64),
        );
        let mut digest = RecordingDigest {
            bytes_seen: 0,
            result: "a".repeat(64),
        };
        assert!(matches!(
            OrtBackend::load(&contract, &mut digest, 1, 1),
            Err(Error::Io {
                operation: "inspect model artifact",
                ..
            })
        ));
        assert_eq!(digest.bytes_seen, 0, "no bytes may be read");
    }

    /// An invalid library path must produce a mapped error, never a panic.
    ///
    /// This is ignored by default because `ort` initialisation is
    /// process-global: running it would affect every other test in the same
    /// process. Run it alone with `--ignored` when checking loader behaviour.
    #[test]
    #[ignore = "ort initialisation is process-global"]
    fn an_invalid_library_path_is_mapped_to_a_backend_error() {
        assert!(matches!(
            initialize_runtime(Path::new("/nonexistent/libonnxruntime.so")),
            Err(Error::Backend { .. })
        ));
    }

    /// Optional developer check: run the real detector through the adapter.
    ///
    /// Ignored by default because it needs an explicitly provisioned ONNX
    /// Runtime library and model that this repository never ships. It exists so
    /// that the adapter is not merely compiled: when run, it reproduces the
    /// exact FNV-1a fingerprint that the separate external C probe recorded for
    /// the same shape, which cross-checks this Rust path against the C API path
    /// through the same native library.
    ///
    /// ```sh
    /// PADDLEOCR_RUST_ORT_DYLIB=<libonnxruntime.so.1.28.0> \
    /// PADDLEOCR_RUST_DETECTOR_ONNX=<detector inference.onnx> \
    ///   cargo test --features onnxruntime --lib -- --ignored --exact \
    ///   backend_ort::tests::the_adapter_reproduces_the_recorded_detector_fingerprint
    /// ```
    #[test]
    #[ignore = "needs an explicitly provisioned ONNX Runtime library and model"]
    fn the_adapter_reproduces_the_recorded_detector_fingerprint() {
        let library = match std::env::var("PADDLEOCR_RUST_ORT_DYLIB") {
            Ok(value) => value,
            Err(_) => panic!("set PADDLEOCR_RUST_ORT_DYLIB"),
        };
        let model = match std::env::var("PADDLEOCR_RUST_DETECTOR_ONNX") {
            Ok(value) => value,
            Err(_) => panic!("set PADDLEOCR_RUST_DETECTOR_ONNX"),
        };

        if let Err(error) = initialize_runtime(Path::new(&library)) {
            panic!("cannot initialise the runtime: {error}");
        }

        let artifact = match ModelArtifact::new(
            &model,
            "eb13b44b25bb36f89528b68720af8a61d9cf381176107f465db1757b65d086e1",
        ) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid artifact, got {error}"),
        };
        let bounded = AxisExtent::Bounded {
            minimum: 32,
            maximum: 960,
        };
        let input_contract = match TensorContract::new(
            "x",
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(3), bounded, bounded],
        ) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output_contract = match TensorContract::new(
            "fetch_name_0",
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(1), bounded, bounded],
        ) {
            Ok(contract) => contract,
            Err(error) => panic!("expected a valid output contract, got {error}"),
        };
        let budget = match RunBudget::new(4_000_000, 1_000_000, 8) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        let contract = ModelContract::new(artifact, input_contract, output_contract, budget);

        let mut digest = crate::backend_ort::tests::StreamingSha256::default();
        let backend = match OrtBackend::load(&contract, &mut digest, 1, 1) {
            Ok(backend) => backend,
            Err(error) => panic!("cannot load the detector: {error}"),
        };

        // The declared lcg-v1 input, identical to the external probe.
        let mut state: u32 = 0x6d2b_79f5;
        let mut values = Vec::with_capacity(3 * 32 * 32);
        for _ in 0..(3 * 32 * 32) {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            values.push((f64::from((state >> 16) & 0xFFFF) / 32768.0 - 1.0) as f32);
        }
        let input = match BackendTensor::new(vec![1, 3, 32, 32], values) {
            Ok(tensor) => tensor,
            Err(error) => panic!("expected a valid input, got {error}"),
        };

        let output = match crate::backend::run_validated(&backend, &contract, &input) {
            Ok(output) => output,
            Err(error) => panic!("adapter run failed: {error}"),
        };
        assert_eq!(output.shape(), [1, 1, 32, 32]);

        let mut fingerprint: u64 = 0xcbf2_9ce4_8422_2325;
        for value in output.values() {
            let bits = value.to_bits();
            for byte in 0..4 {
                fingerprint ^= u64::from((bits >> (8 * byte)) & 0xFF);
                fingerprint = fingerprint.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        assert_eq!(
            format!("{fingerprint:016x}"),
            "44e397a3ba709d99",
            "the adapter must reproduce the fingerprint recorded by the external C probe"
        );
    }

    /// A minimal streaming SHA-256 for the optional developer check.
    #[derive(Default)]
    pub(super) struct StreamingSha256 {
        state: Vec<u8>,
    }

    impl Sha256Stream for StreamingSha256 {
        fn update(&mut self, bytes: &[u8]) {
            self.state.extend_from_slice(bytes);
        }
        fn finish(&mut self) -> String {
            sha256_hex(&self.state)
        }
    }

    /// Exposes the test SHA-256 to the sibling gate-G1 module.
    pub(crate) fn sha256_hex_for_tests(data: &[u8]) -> String {
        sha256_hex(data)
    }

    /// A compact SHA-256 used only by the optional developer check.
    fn sha256_hex(data: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut message = data.to_vec();
        let bit_length = (data.len() as u64) * 8;
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&bit_length.to_be_bytes());

        for chunk in message.chunks(64) {
            let mut w = [0_u32; 64];
            for (index, word) in chunk.chunks(4).enumerate() {
                w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for index in 16..64 {
                let s0 = w[index - 15].rotate_right(7)
                    ^ w[index - 15].rotate_right(18)
                    ^ (w[index - 15] >> 3);
                let s1 = w[index - 2].rotate_right(17)
                    ^ w[index - 2].rotate_right(19)
                    ^ (w[index - 2] >> 10);
                w[index] = w[index - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[index - 7])
                    .wrapping_add(s1);
            }
            let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
                (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
            for index in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[index])
                    .wrapping_add(w[index]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }
            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }
        h.iter().map(|word| format!("{word:08x}")).collect()
    }
}
