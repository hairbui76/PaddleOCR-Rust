// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded input acquisition: bytes, paths, and streams.
//!
//! Roadmap item `INPUT-001`. Three input shapes are supported and one is
//! rejected:
//!
//! - **bytes** — the caller already holds the encoded image; nothing to do here.
//! - **path** — an explicit local file, read under the encoded-input bound.
//! - **stream** — anything implementing [`std::io::Read`], read under the same
//!   bound.
//! - **URL** — *not supported*, and see below for why that is a decision rather
//!   than an omission.
//!
//! # The bound has to be enforced during the read
//!
//! `EncodedImage::new` rejects anything over `MAX_ENCODED_IMAGE_BYTES`, and that
//! is where the limit is documented. But rejecting a slice that already exists
//! is not the same as refusing to allocate it: `std::fs::read` on a ten-gigabyte
//! file allocates ten gigabytes and *then* the check fires, which honours the
//! limit's letter and defeats its purpose. Every reader here refuses at the
//! first byte past the bound and never allocates more than the bound plus one
//! chunk.
//!
//! This matters most for the shape where the size is not known in advance. A
//! file can be measured before reading; a stream cannot, and a stream that
//! claims one length and delivers another is exactly the hostile input this
//! crate is told to expect.
//!
//! # Why there is no URL input
//!
//! Accepting a URL makes this project an HTTP client, and a safe one needs a
//! policy for scheme and host allow-listing, redirect chains and the fact that a
//! redirect can cross into a private network, DNS rebinding, response size and
//! time limits, content-type validation that does not trust the server's own
//! label, and proxy handling. That is the SSRF surface, and none of it is
//! optional once the first URL is accepted.
//!
//! It is also redundant. A caller who wants to OCR a remote image can fetch it
//! with a tool built for fetching and pass the bytes, which keeps the network
//! policy where the caller can see and control it. `MOD-004` records the same
//! position for model downloads.

use std::io::Read;
use std::path::Path;

use crate::error::{Error, InputViolation, Result};
use crate::types::MAX_ENCODED_IMAGE_BYTES;

/// Bytes read per call while streaming, so a hostile stream cannot force one
/// enormous allocation before the bound is observed.
const CHUNK_BYTES: usize = 64 * 1024;

/// Reads an encoded image from a stream, refusing to exceed the input bound.
///
/// The reader is consumed until it ends or the bound is passed, whichever comes
/// first. Passing the bound is a typed `ResourceLimit`; it is not truncated,
/// because a truncated image would decode into something the caller did not
/// supply.
pub fn read_encoded_from<R: Read>(reader: R) -> Result<Vec<u8>> {
    read_bounded(reader, MAX_ENCODED_IMAGE_BYTES)
}

/// Reads an encoded image from an explicit local path under the input bound.
///
/// The file's declared length is checked first, which rejects an oversized file
/// without reading any of it. That check is an optimisation and not the
/// guarantee: the streaming read below still enforces the bound, because a file
/// can grow between the two, and because the metadata of a special file may not
/// describe how much it will produce.
pub fn read_encoded_file(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        operation: "open the encoded image",
        source,
    })?;
    let metadata = file.metadata().map_err(|source| Error::Io {
        operation: "inspect the encoded image",
        source,
    })?;
    if !metadata.is_file() {
        return Err(Error::InvalidInput {
            field: "input.path",
            violation: InputViolation::OutOfRange,
        });
    }
    if metadata.len() > MAX_ENCODED_IMAGE_BYTES as u64 {
        return Err(Error::ResourceLimit {
            resource: "image.encoded_bytes",
            limit: MAX_ENCODED_IMAGE_BYTES as u64,
            actual: metadata.len(),
        });
    }
    read_bounded(file, MAX_ENCODED_IMAGE_BYTES)
}

/// Reads at most `limit` bytes, erroring rather than truncating past it.
fn read_bounded<R: Read>(mut reader: R, limit: usize) -> Result<Vec<u8>> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = vec![0_u8; CHUNK_BYTES];
    loop {
        let read = reader.read(&mut chunk).map_err(|source| Error::Io {
            operation: "read the encoded image",
            source,
        })?;
        if read == 0 {
            break;
        }
        // Checked before extending, so the allocation never passes the bound.
        if buffer.len() + read > limit {
            return Err(Error::ResourceLimit {
                resource: "image.encoded_bytes",
                limit: limit as u64,
                actual: (buffer.len() + read) as u64,
            });
        }
        buffer.try_reserve(read).map_err(|_| Error::Backend {
            message: "encoded image buffer allocation failed",
        })?;
        buffer.extend_from_slice(&chunk[..read]);
    }
    if buffer.is_empty() {
        return Err(Error::InvalidInput {
            field: "image.encoded_bytes",
            violation: InputViolation::Empty,
        });
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that yields a fixed byte forever, so the bound is the only
    /// thing that can stop it.
    struct Endless;

    impl Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(0x41);
            Ok(buffer.len())
        }
    }

    /// A reader that returns one byte per call, exercising the accumulation
    /// path rather than a single large read.
    struct Dribble(usize);

    impl Read for Dribble {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.0 == 0 || buffer.is_empty() {
                return Ok(0);
            }
            self.0 -= 1;
            buffer[0] = 0x42;
            Ok(1)
        }
    }

    #[test]
    fn a_stream_within_the_bound_is_read_whole() {
        let data = vec![7_u8; 1_000];
        let read = match read_encoded_from(data.as_slice()) {
            Ok(bytes) => bytes,
            Err(error) => panic!("expected bytes, got {error}"),
        };
        assert_eq!(read, data);
    }

    #[test]
    fn a_byte_at_a_time_stream_accumulates_correctly() {
        let read = match read_bounded(Dribble(300), 1_000) {
            Ok(bytes) => bytes,
            Err(error) => panic!("expected bytes, got {error}"),
        };
        assert_eq!(read.len(), 300);
        assert!(read.iter().all(|byte| *byte == 0x42));
    }

    /// The bound is inclusive: exactly the limit is accepted.
    #[test]
    fn exactly_the_bound_is_accepted() {
        let data = vec![1_u8; 128];
        assert!(read_bounded(data.as_slice(), 128).is_ok());
        assert!(matches!(
            read_bounded(data.as_slice(), 127),
            Err(Error::ResourceLimit { .. })
        ));
    }

    /// An endless stream stops at the bound instead of exhausting memory.
    ///
    /// This is the case a file-size check cannot cover, and it is why the bound
    /// is enforced during the read rather than after it.
    #[test]
    fn an_endless_stream_is_stopped_by_the_bound() {
        match read_bounded(Endless, 4 * CHUNK_BYTES) {
            Err(Error::ResourceLimit {
                resource, limit, ..
            }) => {
                assert_eq!(resource, "image.encoded_bytes");
                assert_eq!(limit, (4 * CHUNK_BYTES) as u64);
            }
            other => panic!("expected a resource limit, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_stream_is_rejected_rather_than_returning_nothing() {
        assert!(matches!(
            read_encoded_from([].as_slice()),
            Err(Error::InvalidInput {
                field: "image.encoded_bytes",
                violation: InputViolation::Empty,
            })
        ));
    }

    #[test]
    fn a_missing_path_is_a_typed_io_error() {
        assert!(matches!(
            read_encoded_file("/nonexistent/definitely/not/here.png"),
            Err(Error::Io { .. })
        ));
    }

    #[test]
    fn a_directory_is_not_an_image() {
        let outcome = read_encoded_file(env!("CARGO_MANIFEST_DIR"));
        assert!(
            matches!(
                outcome,
                Err(Error::InvalidInput {
                    field: "input.path",
                    ..
                }) | Err(Error::Io { .. })
            ),
            "expected a rejection, got {outcome:?}"
        );
    }

    #[test]
    fn a_committed_fixture_reads_from_its_path() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/classic-v1-e2e-reading-order/input.png"
        );
        let bytes = match read_encoded_file(path) {
            Ok(bytes) => bytes,
            Err(error) => panic!("expected bytes, got {error}"),
        };
        assert_eq!(bytes.len(), 8_988);
        // The same bytes the crate embeds, so path reading and byte input
        // cannot drift apart.
        assert_eq!(
            bytes.as_slice(),
            include_bytes!("../tests/fixtures/classic-v1-e2e-reading-order/input.png").as_slice()
        );
    }
}
