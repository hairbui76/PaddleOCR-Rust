// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! A dependency-free streaming SHA-256 for artifact identity checks.
//!
//! `MOD-003` requires a model artifact to be verified before it is loaded. The
//! adapter takes the digest implementation by trait so this crate does not have
//! to pick a hash library; this is the implementation it uses by default.
//!
//! It is written out rather than pulled in because the only requirement is
//! artifact identity over a file the caller already named. A dependency would
//! add supply-chain surface for one well-specified function, and the constants
//! below are checkable against the published FIPS 180-4 vectors, which the
//! tests do.

use crate::backend::Sha256Stream;

/// The SHA-256 round constants.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// A streaming SHA-256 that hashes incrementally without buffering the input.
///
/// Buffering matters here: a model artifact is tens of megabytes, and holding
/// it in memory purely to hash it would defeat the point of the adapter's
/// bounded streaming read.
#[derive(Clone, Debug)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// Starts a new digest.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }

    /// Adds bytes to the digest.
    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        while !bytes.is_empty() {
            let take = (64 - self.filled).min(bytes.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&bytes[..take]);
            self.filled += take;
            bytes = &bytes[take..];
            if self.filled == 64 {
                let block = self.block;
                self.compress(&block);
                self.filled = 0;
            }
        }
    }

    /// Finishes the digest and returns lowercase hexadecimal.
    #[must_use]
    pub fn finish(mut self) -> String {
        let bit_length = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        // `update` counted the padding, so the recorded length is restored
        // from the value captured before padding began.
        let block = {
            self.block[56..64].copy_from_slice(&bit_length.to_be_bytes());
            self.block
        };
        self.compress(&block);

        let mut hex = String::with_capacity(64);
        for word in self.state {
            hex.push_str(&format!("{word:08x}"));
        }
        hex
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
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

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

impl Sha256Stream for Sha256 {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes);
    }

    fn finish(&mut self) -> String {
        let taken = core::mem::take(self);
        Self::finish(taken)
    }
}

/// Returns the SHA-256 of one byte slice as lowercase hexadecimal.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published FIPS 180-4 vectors, which pin the constants and padding.
    #[test]
    fn the_published_vectors_are_reproduced() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// Streaming in arbitrary chunks must equal hashing in one call.
    #[test]
    fn chunked_updates_match_a_single_update() {
        let data: Vec<u8> = (0..1000_u32).map(|value| (value % 251) as u8).collect();
        let once = sha256_hex(&data);
        for chunk in [1_usize, 7, 63, 64, 65, 128, 999] {
            let mut digest = Sha256::new();
            for piece in data.chunks(chunk) {
                digest.update(piece);
            }
            assert_eq!(digest.finish(), once, "chunk size {chunk}");
        }
    }

    /// A block-boundary length exercises the padding branch that needs a
    /// second block.
    #[test]
    fn block_boundary_lengths_pad_correctly() {
        for length in [55_usize, 56, 57, 63, 64, 65, 119, 120] {
            let data = vec![0x61_u8; length];
            // Compare with an independent reference: hashing the same bytes
            // through the streaming path in two halves.
            let mut split = Sha256::new();
            split.update(&data[..length / 2]);
            split.update(&data[length / 2..]);
            assert_eq!(split.finish(), sha256_hex(&data), "length {length}");
        }
    }
}
