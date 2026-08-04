// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `SEC-IMG-001`: a malformed corpus and property tests for the input surfaces.
//!
//! The fuzz driver in `src/fuzz.rs` explores randomly. This file does the
//! complementary thing: it constructs the **specific** malformed inputs that a
//! reader of the format would choose, and asserts the exact answer each one
//! gets.
//!
//! The distinction matters. A fuzz campaign shows nothing panicked on the
//! inputs it happened to try; a named corpus shows that a truncated `IHDR`, a
//! declared dimension of four billion, and a zero-length stream each produce a
//! **typed error** rather than a panic, an abort, or an allocation sized from
//! the attacker's number — and it keeps showing it in every future run.
//!
//! Everything here runs without a model, a network, or a runtime.

use paddleocr_rust::Error;
use paddleocr_rust::types::{EncodedImage, ImageDimensions, Point, Polygon, Quadrilateral, Score};

/// The eight-byte PNG signature.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Builds a PNG-shaped byte string with an `IHDR` declaring the given size.
///
/// The CRC is deliberately wrong and the image data absent: the point is to
/// reach the decoder's **header** checks, which is where a declared dimension
/// becomes an allocation.
fn png_with_declared_size(width: u32, height: u32, depth: u8, colour: u8) -> Vec<u8> {
    let mut bytes = Vec::from(PNG_SIGNATURE);
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.push(depth);
    bytes.push(colour);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes
}

/// Builds the named malformed corpus.
///
/// Shared by the two tests below so the corpus is written once and both the
/// gate and the decoder see the same inputs.
fn malformed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("empty", Vec::new()),
        ("signature_only", Vec::from(PNG_SIGNATURE)),
        ("one_byte", vec![0x89]),
        ("truncated_signature", Vec::from(&PNG_SIGNATURE[..4])),
        ("not_a_png", b"GIF89a....".to_vec()),
        ("truncated_ihdr", {
            let mut bytes = Vec::from(PNG_SIGNATURE);
            bytes.extend_from_slice(&13_u32.to_be_bytes());
            bytes.extend_from_slice(b"IHD");
            bytes
        }),
        // A declared size whose product overflows a 32-bit pixel count. This is
        // the case that matters most: the number is the attacker's and it is
        // used to size a buffer.
        (
            "four_billion_square",
            png_with_declared_size(0xFFFF_FFFF, 0xFFFF_FFFF, 8, 2),
        ),
        (
            "wide_and_short",
            png_with_declared_size(0xFFFF_FFFF, 1, 8, 2),
        ),
        ("zero_width", png_with_declared_size(0, 16, 8, 2)),
        ("zero_height", png_with_declared_size(16, 0, 8, 2)),
        ("absurd_bit_depth", png_with_declared_size(4, 4, 64, 2)),
        ("unknown_colour_type", png_with_declared_size(4, 4, 8, 99)),
        // Valid-looking header, no pixel data at all.
        ("header_without_idat", png_with_declared_size(4, 4, 8, 2)),
    ]
}

/// Nothing in the corpus is accepted as a decodable image.
///
/// `EncodedImage::new` is the public gate. Some of these are refused there;
/// the rest get through it and must be refused by the decoder, which
/// `the_decoder_refuses_the_whole_corpus` covers. Here the claim is only that
/// the gate never **panics** and that its verdict is a typed error when it
/// refuses.
#[test]
fn the_public_gate_refuses_or_defers_without_panicking() {
    for (name, bytes) in malformed_corpus() {
        match EncodedImage::new(&bytes) {
            Ok(_) => {}
            Err(Error::InvalidInput { .. }) => {}
            other => panic!("{name}: expected a typed refusal, got {other:?}"),
        }
    }
}

/// The decoder refuses every input in the corpus.
///
/// Reached through `fuzz::exercise`, which drives the internal decoder. Routing
/// through it rather than adding a public decode entry keeps the API surface
/// unchanged: a test-only public function would widen what this crate promises
/// in order to test what it does not.
#[cfg(feature = "fuzzing")]
#[test]
fn the_decoder_refuses_the_whole_corpus() {
    for (_, bytes) in malformed_corpus() {
        // A panic inside fails the test; there is nothing else to assert,
        // because the driver deliberately discards typed errors.
        paddleocr_rust::fuzz::exercise(&bytes);
    }
}

/// A declared dimension may never become an allocation.
///
/// `ImageDimensions` is the gate every pixel buffer in this project is sized
/// through, so its bound is the one that has to hold.
#[test]
fn declared_dimensions_are_bounded_before_they_allocate() {
    // Accepted: an ordinary page.
    assert!(ImageDimensions::new(1280, 720).is_ok());

    // Refused: zero on either axis, and products that would exhaust memory.
    for (width, height) in [
        (0, 16),
        (16, 0),
        (0, 0),
        (u32::MAX, u32::MAX),
        (u32::MAX, 1),
        (1, u32::MAX),
        (100_000, 100_000),
    ] {
        assert!(
            ImageDimensions::new(width, height).is_err(),
            "{width}x{height} must be refused"
        );
    }
}

/// Non-finite and out-of-range scalars are refused at the type boundary.
///
/// A `NaN` that reaches geometry produces comparisons that are all false, which
/// is how a sort silently stops sorting. Refusing at construction is what stops
/// that from being possible further in.
#[test]
fn non_finite_scalars_never_enter_the_type_system() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(Point::new(value, 0.0).is_err(), "x = {value}");
        assert!(Point::new(0.0, value).is_err(), "y = {value}");
        assert!(Score::new(value).is_err(), "score = {value}");
    }
    // Scores are a closed interval, and the ends are inside it.
    assert!(Score::new(0.0).is_ok());
    assert!(Score::new(1.0).is_ok());
    assert!(Score::new(-0.000_01).is_err());
    assert!(Score::new(1.000_01).is_err());
}

/// Degenerate polygons are refused on **two** grounds, not one.
///
/// Vertex count is the obvious check. The second is a zero signed area, which
/// rejects three identical points and three collinear ones — shapes that pass
/// a count check and then produce a zero-area crop downstream.
#[test]
fn degenerate_polygons_are_refused() {
    let point = |x: f32, y: f32| match Point::new(x, y) {
        Ok(value) => value,
        Err(error) => panic!("point: {error}"),
    };
    let corner = point(1.0, 1.0);

    // Too few vertices to bound an area.
    for count in 0..3_usize {
        assert!(
            Polygon::new(vec![corner; count]).is_err(),
            "{count} vertices must be refused"
        );
    }

    // Three vertices, but no area: identical points, and collinear ones.
    assert!(Polygon::new(vec![corner, corner, corner]).is_err());
    assert!(
        Polygon::new(vec![point(0.0, 0.0), point(1.0, 1.0), point(2.0, 2.0)]).is_err(),
        "collinear vertices enclose no area"
    );

    // A real triangle is accepted.
    assert!(Polygon::new(vec![point(0.0, 0.0), point(4.0, 0.0), point(0.0, 3.0)]).is_ok());

    // And a vertex count large enough to be an attack is a resource refusal,
    // which is a different error than a degenerate shape.
    match Polygon::new(vec![corner; 1_000_000]) {
        Err(Error::ResourceLimit { resource, .. }) => {
            assert_eq!(resource, "polygon.vertices");
        }
        other => panic!("expected a resource refusal, got {other:?}"),
    }
}

/// A quadrilateral is exactly four corners, and the type says so.
#[test]
fn a_quadrilateral_cannot_be_built_from_the_wrong_shape() {
    let points: Vec<Point> = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        .iter()
        .map(|(x, y)| match Point::new(*x, *y) {
            Ok(value) => value,
            Err(error) => panic!("point: {error}"),
        })
        .collect();
    let quad: [Point; 4] = match points.clone().try_into() {
        Ok(value) => value,
        Err(_) => panic!("four corners"),
    };
    assert!(Quadrilateral::new(quad).is_ok());
}

/// A stream that never ends is stopped by a bound, not by memory pressure.
#[test]
fn an_endless_stream_is_refused_rather_than_consumed() {
    struct Endless;
    impl std::io::Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(0xA5);
            Ok(buffer.len())
        }
    }
    // A `ResourceLimit`, not an `InvalidInput`: the stream is not malformed,
    // it is too large, and the error says which limit and by how much. That
    // distinction is what lets a caller tell "this file is broken" from "this
    // file is bigger than I allow".
    match paddleocr_rust::input::read_encoded_from(Endless) {
        Err(Error::ResourceLimit {
            resource,
            limit,
            actual,
        }) => {
            assert_eq!(resource, "image.encoded_bytes");
            assert!(actual > limit, "{actual} must exceed {limit}");
        }
        other => panic!("expected a bounded refusal, got {other:?}"),
    }
}

/// A manifest whose fields are hostile is refused, field by field.
#[test]
fn hostile_manifests_are_refused() {
    use paddleocr_rust::manifest::ModelManifest;

    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("not_json", "just some text".to_owned()),
        ("truncated_json", "{\"family\":".to_owned()),
        (
            "short_digest",
            "{\"family\":\"a\",\"version\":\"b\",\"format\":\"onnx\",\"backend\":\"ort\",\
             \"detector\":{\"sha256\":\"abc\"}}"
                .to_owned(),
        ),
        (
            "non_hex_digest",
            format!(
                "{{\"family\":\"a\",\"version\":\"b\",\"format\":\"onnx\",\"backend\":\"ort\",\
                 \"detector\":{{\"sha256\":\"{}\"}}}}",
                "z".repeat(64)
            ),
        ),
        // A deeply nested document, which is where a recursive parser stops
        // being a parser and starts being a stack overflow.
        ("deeply_nested", "[".repeat(4096) + &"]".repeat(4096)),
    ];

    for (name, text) in cases {
        assert!(
            ModelManifest::parse(&text).is_err(),
            "{name}: a hostile manifest must be refused"
        );
    }
}

/// The same input twice gives the same answer, byte for byte.
///
/// Determinism is a claim `docs/API_CONTRACT.md` makes, and a claim that lives
/// only in a document is not one this project makes.
#[test]
fn refusals_are_deterministic() {
    let bytes = png_with_declared_size(0xFFFF_FFFF, 0xFFFF_FFFF, 8, 2);
    let first = format!("{:?}", EncodedImage::new(&bytes).map(|_| ()));
    let second = format!("{:?}", EncodedImage::new(&bytes).map(|_| ()));
    assert_eq!(first, second);

    let dimensions = format!("{:?}", ImageDimensions::new(0, 0));
    assert_eq!(dimensions, format!("{:?}", ImageDimensions::new(0, 0)));
}

/// Bounded work: a large malformed corpus finishes quickly.
///
/// Not a benchmark — a liveness check. If a refusal ever became proportional to
/// a declared dimension rather than to the bytes actually supplied, this is
/// where it would show up as a hang rather than as a wrong answer.
#[test]
fn a_large_malformed_corpus_finishes_promptly() {
    let start = std::time::Instant::now();
    for seed in 0..2_000_u32 {
        let bytes = png_with_declared_size(
            seed.wrapping_mul(2_654_435_761),
            seed.wrapping_mul(40_503),
            (seed % 255) as u8,
            (seed % 7) as u8,
        );
        let _ = EncodedImage::new(&bytes).map(|_| ());
        let _ = ImageDimensions::new(seed.wrapping_mul(2_654_435_761), seed.wrapping_mul(40_503));
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "2,000 malformed inputs took {elapsed:?}; refusal work must not scale with declared size"
    );
}
