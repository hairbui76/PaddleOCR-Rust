// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded PNG decoding into the classic BGR input convention.
//!
//! The M2 input scope is frozen by [`docs/IMAGE_DECODER_DECISION.md`]. It is
//! deliberately PNG-only: every committed end-to-end fixture input is a PNG,
//! and no evaluated pure-Rust JPEG decoder reproduces the committed OpenCV
//! JPEG oracle. JPEG remains a separate, gated roadmap item.
//!
//! The output mirrors `cv2.imdecode(..., IMREAD_COLOR)` at the pinned upstream
//! baseline: three interleaved `uint8` BGR channels, row-major, top-left
//! origin, grayscale replicated across all three channels, alpha discarded
//! rather than composited, palettes applied, and 16-bit samples truncated to
//! their high byte.
//!
//! [`docs/IMAGE_DECODER_DECISION.md`]: ../docs/IMAGE_DECODER_DECISION.md

use std::io::Cursor;

use png::{BitDepth, ColorType, DecodingError, Transformations};

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::types::{EncodedImage, ImageDimensions};

/// The fixed eight-byte PNG content signature.
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Bytes required before the declared image header can be read.
///
/// Eight signature bytes, then the four-byte `IHDR` length, the four-byte
/// chunk type, and the first ten bytes of `IHDR` data: width, height, bit
/// depth, and colour type.
const PNG_HEADER_PREFIX_BYTES: usize = 26;

/// The exact `IHDR` payload length required by the PNG specification.
const PNG_IHDR_DATA_BYTES: u32 = 13;

/// Interleaved channel count of every decoded classic image.
const BGR_CHANNELS: u8 = 3;

/// Maximum total decode allocation for one image, in bytes.
///
/// A pixel-count limit alone does not bound decode memory, because the decoded
/// sample buffer can be up to eight bytes per pixel while the BGR result is
/// always three. This envelope covers both buffers and is checked before either
/// is allocated, so a large declared image fails with a typed error instead of
/// aborting the process on a second unaccounted allocation.
pub(crate) const MAX_DECODE_ALLOCATION_BYTES: u64 = 256 * 1024 * 1024;

/// Decodes one bounded encoded image into interleaved BGR bytes.
///
/// Format selection uses the content signature only. A filename, extension, or
/// caller-supplied hint never selects a decoder.
pub(crate) fn decode_classic_bgr(encoded: EncodedImage<'_>) -> Result<InterleavedImage> {
    let bytes = encoded.bytes();
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(Error::Unsupported {
            capability: "image format",
        });
    }

    let header = declared_png_header(bytes)?;
    let dimensions = header.dimensions;
    let bgr_bytes = bgr_byte_len(dimensions)?;
    // Reject an image whose declared header alone cannot fit the decode
    // envelope, before the decoder is constructed and before any pixel buffer
    // exists. The exact size is rechecked below once the decoder reports it.
    let worst_case = dimensions
        .pixels()
        .saturating_mul(u64::from(header.worst_case_sample_bytes_per_pixel))
        .saturating_add(bgr_bytes as u64);
    if worst_case > MAX_DECODE_ALLOCATION_BYTES {
        return Err(Error::ResourceLimit {
            resource: "image.decode_allocation",
            limit: MAX_DECODE_ALLOCATION_BYTES,
            actual: worst_case,
        });
    }

    // The decoder requires `BufRead + Seek`; a cursor over the already bounded
    // borrowed slice satisfies both without copying the encoded bytes.
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // `EXPAND` normalizes palettes, sub-byte grayscale, and `tRNS` into plain
    // samples. `STRIP_16` is deliberately not requested: this module performs
    // the 16-bit reduction itself so the recorded OpenCV convention stays
    // explicit and reviewable.
    decoder.set_transformations(Transformations::EXPAND);
    decoder.set_limits(png::Limits {
        bytes: decode_allocation_limit(),
    });

    let mut reader = decoder.read_info().map_err(map_png_error)?;
    let info = reader.info();
    if info.width != dimensions.width() || info.height != dimensions.height() {
        return Err(Error::InvalidInput {
            field: "image.png.header",
            violation: InputViolation::Malformed,
        });
    }

    let sample_bytes = match reader.output_buffer_size() {
        Some(size) => size,
        None => {
            return Err(Error::ResourceLimit {
                resource: "image.decode_allocation",
                limit: MAX_DECODE_ALLOCATION_BYTES,
                actual: u64::MAX,
            });
        }
    };
    let total = (sample_bytes as u64).saturating_add(bgr_bytes as u64);
    if total > MAX_DECODE_ALLOCATION_BYTES {
        return Err(Error::ResourceLimit {
            resource: "image.decode_allocation",
            limit: MAX_DECODE_ALLOCATION_BYTES,
            actual: total,
        });
    }

    let mut samples = fallible_zeroed_vec(sample_bytes, "image.decode_allocation")?;
    let frame = reader.next_frame(&mut samples).map_err(map_png_error)?;
    if frame.width != dimensions.width() || frame.height != dimensions.height() {
        return Err(Error::InvalidInput {
            field: "image.png.frame",
            violation: InputViolation::Malformed,
        });
    }

    let bgr = convert_to_bgr(
        &samples,
        frame.line_size,
        dimensions,
        frame.color_type,
        frame.bit_depth,
        bgr_bytes,
    )?;

    InterleavedImage::new(dimensions, BGR_CHANNELS, bgr)
}

/// Declared `IHDR` facts needed before any decoder allocation.
#[derive(Clone, Copy, Debug)]
struct PngHeader {
    dimensions: ImageDimensions,
    worst_case_sample_bytes_per_pixel: u8,
}

/// Reads the declared `IHDR` header before any decoder allocation.
///
/// Doing this directly is what makes the project's side-length and pixel-count
/// limits observable as typed resource errors rather than as a decoder-internal
/// failure, and it happens before the PNG decoder sees the input at all.
fn declared_png_header(bytes: &[u8]) -> Result<PngHeader> {
    let malformed = || Error::InvalidInput {
        field: "image.png.header",
        violation: InputViolation::Malformed,
    };
    let prefix = match bytes.get(..PNG_HEADER_PREFIX_BYTES) {
        Some(prefix) => prefix,
        None => return Err(malformed()),
    };

    let mut length = [0_u8; 4];
    length.copy_from_slice(&prefix[8..12]);
    if u32::from_be_bytes(length) != PNG_IHDR_DATA_BYTES {
        return Err(malformed());
    }
    if &prefix[12..16] != b"IHDR" {
        return Err(malformed());
    }

    let mut width = [0_u8; 4];
    width.copy_from_slice(&prefix[16..20]);
    let mut height = [0_u8; 4];
    height.copy_from_slice(&prefix[20..24]);
    let dimensions = ImageDimensions::new(u32::from_be_bytes(width), u32::from_be_bytes(height))?;

    let bit_depth = prefix[24];
    let color_type = prefix[25];
    Ok(PngHeader {
        dimensions,
        worst_case_sample_bytes_per_pixel: worst_case_sample_bytes(color_type, bit_depth)?,
    })
}

/// Returns an upper bound on decoded sample bytes per pixel after `EXPAND`.
///
/// A `tRNS` chunk can add one alpha sample, and a palette always expands to
/// eight-bit colour, so each colour type is bounded by its widest expanded
/// form. This is intentionally an upper bound: it is used only to reject an
/// oversized image before the decoder runs.
fn worst_case_sample_bytes(color_type: u8, bit_depth: u8) -> Result<u8> {
    let malformed = || Error::InvalidInput {
        field: "image.png.header",
        violation: InputViolation::Malformed,
    };
    let sample_bytes: u8 = match bit_depth {
        1 | 2 | 4 | 8 => 1,
        16 => 2,
        _ => return Err(malformed()),
    };
    let samples: u8 = match color_type {
        // Grayscale and truecolor may gain one alpha sample through `tRNS`.
        0 => 2,
        2 => 4,
        // A palette always expands to eight-bit RGB or RGBA.
        3 => return Ok(4),
        4 => 2,
        6 => 4,
        _ => return Err(malformed()),
    };
    Ok(samples * sample_bytes)
}

/// Returns the exact BGR output length for already checked dimensions.
fn bgr_byte_len(dimensions: ImageDimensions) -> Result<usize> {
    let bytes = dimensions
        .pixels()
        .checked_mul(u64::from(BGR_CHANNELS))
        .ok_or(Error::ResourceLimit {
            resource: "image.decode_allocation",
            limit: MAX_DECODE_ALLOCATION_BYTES,
            actual: u64::MAX,
        })?;
    usize::try_from(bytes).map_err(|_| Error::ResourceLimit {
        resource: "image.decode_allocation",
        limit: MAX_DECODE_ALLOCATION_BYTES,
        actual: bytes,
    })
}

/// Returns the decode envelope as a `usize` bound for the PNG decoder.
fn decode_allocation_limit() -> usize {
    usize::try_from(MAX_DECODE_ALLOCATION_BYTES).unwrap_or(usize::MAX)
}

/// Allocates one zeroed buffer, reporting allocation failure as a typed error.
fn fallible_zeroed_vec(len: usize, resource: &'static str) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    if buffer.try_reserve_exact(len).is_err() {
        return Err(Error::ResourceLimit {
            resource,
            limit: MAX_DECODE_ALLOCATION_BYTES,
            actual: len as u64,
        });
    }
    buffer.resize(len, 0);
    Ok(buffer)
}

/// Converts expanded PNG samples into interleaved BGR bytes.
///
/// `EXPAND` guarantees that palettes are already resolved, so an indexed colour
/// type here is a decoder contract violation rather than an input problem.
fn convert_to_bgr(
    samples: &[u8],
    source_stride: usize,
    dimensions: ImageDimensions,
    color_type: ColorType,
    bit_depth: BitDepth,
    bgr_bytes: usize,
) -> Result<Vec<u8>> {
    let unsupported = || Error::Unsupported {
        capability: "png sample format",
    };
    let channels = match color_type {
        ColorType::Grayscale => 1_usize,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => return Err(unsupported()),
    };
    let sample_bytes = match bit_depth {
        BitDepth::Eight => 1_usize,
        BitDepth::Sixteen => 2,
        // `EXPAND` promotes every sub-byte depth to eight bits.
        BitDepth::One | BitDepth::Two | BitDepth::Four => return Err(unsupported()),
    };
    let pixel_bytes = channels * sample_bytes;

    let width = dimensions.width() as usize;
    let height = dimensions.height() as usize;
    let required_stride = match width.checked_mul(pixel_bytes) {
        Some(stride) => stride,
        None => return Err(unsupported()),
    };
    if source_stride < required_stride {
        return Err(Error::InvalidInput {
            field: "image.png.frame",
            violation: InputViolation::Malformed,
        });
    }

    let mut bgr = fallible_zeroed_vec(bgr_bytes, "image.decode_allocation")?;
    let mut output = 0_usize;
    for y in 0..height {
        let row_start = match y.checked_mul(source_stride) {
            Some(start) => start,
            None => return Err(unsupported()),
        };
        let row = match samples.get(row_start..row_start + required_stride) {
            Some(row) => row,
            None => {
                return Err(Error::InvalidInput {
                    field: "image.png.frame",
                    violation: InputViolation::Malformed,
                });
            }
        };
        for x in 0..width {
            let pixel = &row[x * pixel_bytes..x * pixel_bytes + pixel_bytes];
            // A 16-bit sample is stored big-endian, and `IMREAD_COLOR` keeps
            // only its high byte. Rounding instead of truncating reproduces a
            // different value for the committed 16-bit oracle case.
            let component = |index: usize| pixel[index * sample_bytes];
            let (red, green, blue) = match color_type {
                ColorType::Grayscale | ColorType::GrayscaleAlpha => {
                    let value = component(0);
                    (value, value, value)
                }
                ColorType::Rgb | ColorType::Rgba => (component(0), component(1), component(2)),
                ColorType::Indexed => return Err(unsupported()),
            };
            bgr[output] = blue;
            bgr[output + 1] = green;
            bgr[output + 2] = red;
            output += BGR_CHANNELS as usize;
        }
    }

    Ok(bgr)
}

/// Maps a PNG decoder failure onto this project's typed error categories.
fn map_png_error(error: DecodingError) -> Error {
    match error {
        DecodingError::LimitsExceeded => Error::ResourceLimit {
            resource: "image.decode_allocation",
            limit: MAX_DECODE_ALLOCATION_BYTES,
            actual: u64::MAX,
        },
        // A truncated stream surfaces as an I/O error from the byte reader; it
        // is a malformed input here, not an operating-system failure.
        DecodingError::IoError(_) | DecodingError::Format(_) | DecodingError::Parameter(_) => {
            Error::InvalidInput {
                field: "image.png",
                violation: InputViolation::Malformed,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;

    const CAPTURED_IMAGE_INPUT_ORACLE: &str =
        include_str!("../tests/fixtures/classic-v1-image-inputs/capture.json");

    fn capture() -> Value {
        match serde_json::from_str(CAPTURED_IMAGE_INPUT_ORACLE) {
            Ok(value) => value,
            Err(error) => panic!("image-input capture is not valid JSON: {error}"),
        }
    }

    fn array<'a>(value: &'a Value, field: &str) -> &'a [Value] {
        match value.get(field).and_then(Value::as_array) {
            Some(values) => values,
            None => panic!("image-input capture field {field:?} must be an array"),
        }
    }

    fn string<'a>(value: &'a Value, field: &str) -> &'a str {
        match value.get(field).and_then(Value::as_str) {
            Some(value) => value,
            None => panic!("image-input capture field {field:?} must be a string"),
        }
    }

    fn decoded_base64(value: &Value, field: &str) -> Vec<u8> {
        let payload = match value.get(field) {
            Some(payload) => payload,
            None => panic!("image-input capture is missing {field:?}"),
        };
        match STANDARD.decode(string(payload, "base64")) {
            Ok(bytes) => bytes,
            Err(error) => panic!("image-input capture {field:?} base64 is invalid: {error}"),
        }
    }

    fn png_signature_prefixed(bytes: &[u8]) -> bool {
        bytes.starts_with(&PNG_SIGNATURE)
    }

    /// Decodes every recorded PNG case and compares it with the OpenCV oracle.
    ///
    /// The ten recorded JPEG cases must be reported as unsupported: M2 does not
    /// decode JPEG, and silently returning a near-miss image would hide the
    /// recorded component differences behind an unmeasured tolerance.
    #[test]
    fn classic_decode_matches_every_captured_opencv_png_case() {
        let capture = capture();
        let cases = array(&capture, "cases");
        assert_eq!(cases.len(), 15, "captured image-input case count");

        let mut png_cases = 0_usize;
        let mut jpeg_cases = 0_usize;
        for case in cases {
            let fixture_id = string(case, "fixture_id");
            let format = string(case, "format");
            let encoded = decoded_base64(case, "encoded_image");
            let input = match EncodedImage::new(&encoded) {
                Ok(input) => input,
                Err(error) => panic!("{fixture_id} encoded input was rejected: {error}"),
            };

            if format == "jpeg" {
                jpeg_cases += 1;
                assert!(
                    !png_signature_prefixed(&encoded),
                    "{fixture_id} must not carry a PNG signature"
                );
                match decode_classic_bgr(input) {
                    Err(Error::Unsupported { capability }) => {
                        assert_eq!(capability, "image format", "{fixture_id} capability");
                    }
                    other => panic!("{fixture_id} must be unsupported in M2, got {other:?}"),
                }
                continue;
            }

            assert_eq!(format, "png", "{fixture_id} unexpected recorded format");
            png_cases += 1;
            let expected = decoded_base64(case, "opencv_imread_color");
            let oracle = match case.get("opencv_imread_color") {
                Some(value) => value,
                None => panic!("{fixture_id} is missing its OpenCV record"),
            };
            assert_eq!(
                string(oracle, "channel_order"),
                "BGR",
                "{fixture_id} oracle channel order"
            );
            assert_eq!(
                string(oracle, "dtype"),
                "uint8",
                "{fixture_id} oracle dtype"
            );
            let shape = array(oracle, "shape");
            assert_eq!(shape.len(), 3, "{fixture_id} oracle shape rank");

            let decoded = match decode_classic_bgr(input) {
                Ok(decoded) => decoded,
                Err(error) => panic!("{fixture_id} failed to decode: {error}"),
            };
            assert_eq!(decoded.channels(), BGR_CHANNELS, "{fixture_id} channels");
            assert_eq!(
                u64::from(decoded.dimensions().height()),
                shape[0].as_u64().unwrap_or_default(),
                "{fixture_id} height"
            );
            assert_eq!(
                u64::from(decoded.dimensions().width()),
                shape[1].as_u64().unwrap_or_default(),
                "{fixture_id} width"
            );
            assert_eq!(decoded.pixels(), expected, "{fixture_id} BGR bytes");
        }

        assert_eq!(png_cases, 5, "recorded PNG case count");
        assert_eq!(jpeg_cases, 10, "recorded JPEG case count");
    }

    /// Applies each recorded negative case to the real decoder entry point.
    #[test]
    fn classic_decode_reports_every_captured_negative_outcome() {
        let capture = capture();
        let negatives = array(&capture, "negative_cases");
        assert_eq!(negatives.len(), 5, "captured negative case count");

        for case in negatives {
            let fixture_id = string(case, "fixture_id");
            let outcome = string(case, "required_outcome");
            let encoded = decoded_base64(case, "encoded_input");
            let input = EncodedImage::new(&encoded);

            match outcome {
                "invalid_input_empty" => match input {
                    Err(Error::InvalidInput { field, violation }) => {
                        assert_eq!(field, "image.bytes", "{fixture_id} field");
                        assert_eq!(violation, InputViolation::Empty, "{fixture_id} violation");
                    }
                    other => panic!("{fixture_id} must be rejected as empty, got {other:?}"),
                },
                "unsupported_format" => {
                    let input = must_accept(input, fixture_id);
                    match decode_classic_bgr(input) {
                        Err(Error::Unsupported { capability }) => {
                            assert_eq!(capability, "image format", "{fixture_id} capability");
                        }
                        other => panic!("{fixture_id} must be unsupported, got {other:?}"),
                    }
                }
                "malformed_input" => {
                    let input = must_accept(input, fixture_id);
                    match decode_classic_bgr(input) {
                        Err(Error::InvalidInput { violation, .. }) => {
                            assert_eq!(
                                violation,
                                InputViolation::Malformed,
                                "{fixture_id} violation"
                            );
                        }
                        other => panic!("{fixture_id} must be malformed, got {other:?}"),
                    }
                }
                "resource_limit_before_project_pixel_allocation" => {
                    let input = must_accept(input, fixture_id);
                    match decode_classic_bgr(input) {
                        Err(Error::ResourceLimit {
                            resource,
                            limit,
                            actual,
                        }) => {
                            assert_eq!(resource, "image.width_pixels", "{fixture_id} resource");
                            assert_eq!(limit, 16_384, "{fixture_id} limit");
                            assert_eq!(actual, 16_385, "{fixture_id} actual");
                        }
                        other => panic!("{fixture_id} must hit a resource limit, got {other:?}"),
                    }
                }
                "content_detection_ignores_filename_hint" => {
                    // The recorded filename hint is never passed to the
                    // decoder; only the content signature selects the path.
                    assert_eq!(
                        string(case, "filename_hint"),
                        "self-authored-input.jpg",
                        "{fixture_id} recorded filename hint"
                    );
                    let input = must_accept(input, fixture_id);
                    let decoded = match decode_classic_bgr(input) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            panic!("{fixture_id} must decode by content, got {error}")
                        }
                    };
                    assert_eq!(decoded.dimensions().width(), 3, "{fixture_id} width");
                    assert_eq!(decoded.dimensions().height(), 2, "{fixture_id} height");
                }
                other => panic!("{fixture_id} has an unhandled recorded outcome {other:?}"),
            }
        }
    }

    fn must_accept<'a>(input: Result<EncodedImage<'a>>, fixture_id: &str) -> EncodedImage<'a> {
        match input {
            Ok(input) => input,
            Err(error) => panic!("{fixture_id} encoded input was rejected: {error}"),
        }
    }

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::from(PNG_SIGNATURE);
        bytes.extend(PNG_IHDR_DATA_BYTES.to_be_bytes());
        bytes.extend(b"IHDR");
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 2, 0, 0, 0]);
        bytes
    }

    #[test]
    fn declared_dimensions_reject_malformed_and_oversized_headers() {
        assert!(matches!(
            declared_png_header(&PNG_SIGNATURE),
            Err(Error::InvalidInput {
                violation: InputViolation::Malformed,
                ..
            })
        ));

        let mut wrong_length = png_header(3, 2);
        wrong_length[8..12].copy_from_slice(&12_u32.to_be_bytes());
        assert!(matches!(
            declared_png_header(&wrong_length),
            Err(Error::InvalidInput {
                violation: InputViolation::Malformed,
                ..
            })
        ));

        let mut wrong_type = png_header(3, 2);
        wrong_type[12..16].copy_from_slice(b"IDAT");
        assert!(matches!(
            declared_png_header(&wrong_type),
            Err(Error::InvalidInput {
                violation: InputViolation::Malformed,
                ..
            })
        ));

        assert!(matches!(
            declared_png_header(&png_header(0, 2)),
            Err(Error::InvalidInput {
                violation: InputViolation::Empty,
                ..
            })
        ));
        assert!(matches!(
            declared_png_header(&png_header(16_385, 1)),
            Err(Error::ResourceLimit {
                resource: "image.width_pixels",
                ..
            })
        ));
        assert!(matches!(
            declared_png_header(&png_header(1, 16_385)),
            Err(Error::ResourceLimit {
                resource: "image.height_pixels",
                ..
            })
        ));
        assert!(matches!(
            declared_png_header(&png_header(16_384, 16_384)),
            Err(Error::ResourceLimit {
                resource: "image.total_pixels",
                ..
            })
        ));

        let accepted = match declared_png_header(&png_header(3, 2)) {
            Ok(header) => header,
            Err(error) => panic!("expected accepted header, got {error}"),
        };
        assert_eq!(accepted.dimensions.width(), 3);
        assert_eq!(accepted.dimensions.height(), 2);
        assert_eq!(accepted.worst_case_sample_bytes_per_pixel, 4);
    }

    #[test]
    fn worst_case_sample_bytes_bound_every_declared_png_form() {
        // Grayscale, truecolor, palette, grayscale+alpha, and truecolor+alpha,
        // each at the bit depths the format permits for that colour type.
        for (color_type, bit_depth, expected) in [
            (0_u8, 8_u8, 2_u8),
            (0, 16, 4),
            (2, 8, 4),
            (2, 16, 8),
            (3, 1, 4),
            (3, 8, 4),
            (4, 8, 2),
            (4, 16, 4),
            (6, 8, 4),
            (6, 16, 8),
        ] {
            assert_eq!(
                match worst_case_sample_bytes(color_type, bit_depth) {
                    Ok(value) => value,
                    Err(error) => panic!("colour type {color_type} depth {bit_depth}: {error}"),
                },
                expected,
                "colour type {color_type} depth {bit_depth}"
            );
        }
        for (color_type, bit_depth) in [(1_u8, 8_u8), (5, 8), (7, 8), (0, 3), (0, 32)] {
            assert!(
                matches!(
                    worst_case_sample_bytes(color_type, bit_depth),
                    Err(Error::InvalidInput {
                        violation: InputViolation::Malformed,
                        ..
                    })
                ),
                "colour type {color_type} depth {bit_depth} must be rejected"
            );
        }
    }

    /// A header inside the pixel limit can still exceed the decode envelope.
    ///
    /// This is the case a pixel-count limit alone does not cover: a very large
    /// 16-bit RGBA image needs eleven bytes per pixel across both buffers.
    #[test]
    fn decode_envelope_rejects_a_large_declared_sixteen_bit_image() {
        let mut bytes = png_header(6_000, 6_000);
        // Colour type 6 (RGBA) at bit depth 16 is eight source bytes per pixel.
        let header_len = bytes.len();
        bytes[header_len - 5..header_len].copy_from_slice(&[16, 6, 0, 0, 0]);
        let input = match EncodedImage::new(&bytes) {
            Ok(input) => input,
            Err(error) => panic!("expected accepted encoded bytes, got {error}"),
        };
        match decode_classic_bgr(input) {
            Err(Error::ResourceLimit {
                resource, limit, ..
            }) => {
                assert_eq!(resource, "image.decode_allocation");
                assert_eq!(limit, MAX_DECODE_ALLOCATION_BYTES);
            }
            other => panic!("expected a decode envelope error, got {other:?}"),
        }
    }

    /// The `G3` benchmark page is a page-sized decode, an order of magnitude
    /// larger than any other committed input. It is the only fixture that
    /// exercises the decoder at the size the latency and memory budgets are
    /// stated against, so its decoded bytes are pinned by digest rather than
    /// held in the repository as a 2.7 MB expectation file.
    #[test]
    fn the_benchmark_page_decodes_to_the_recorded_bgr_digest() {
        let bytes = include_bytes!("../tests/fixtures/classic-v1-benchmark-page/input.png");
        let input = match EncodedImage::new(bytes) {
            Ok(input) => input,
            Err(error) => panic!("benchmark page rejected as encoded input: {error}"),
        };
        let decoded = match decode_classic_bgr(input) {
            Ok(decoded) => decoded,
            Err(error) => panic!("benchmark page failed to decode: {error}"),
        };
        assert_eq!(decoded.dimensions().width(), 1280);
        assert_eq!(decoded.dimensions().height(), 720);
        assert_eq!(decoded.channels(), BGR_CHANNELS);
        assert_eq!(
            crate::digest::sha256_hex(decoded.pixels()),
            "dbc63016931458f402b90230c961aaec121fa48fe09ec75ed16804978cc6a382",
            "benchmark page decoded BGR bytes changed"
        );
    }

    /// The `IMG-003` dense-corpus pages, pinned the same way as the benchmark
    /// page: by the digest of their decoded BGR bytes. The pages are grayscale
    /// PNGs, so this also exercises the gray-replication path at page size.
    #[test]
    fn the_jpeg_delta_corpus_decodes_to_the_recorded_bgr_digests() {
        let pages: [(&str, &[u8], u32, u32, &str); 3] = [
            (
                "dense-small",
                include_bytes!("../tests/fixtures/classic-v1-jpeg-delta-corpus/dense-small.png"),
                640,
                632,
                "d452d0de9ba4d36829529f6efbbd4b68651ca65a0471ebb582f052de4560b352",
            ),
            (
                "low-contrast",
                include_bytes!("../tests/fixtures/classic-v1-jpeg-delta-corpus/low-contrast.png"),
                640,
                566,
                "f400d792466b471dc8ed6c3e51618bec994e75261d7225a1be69da5aced8a18d",
            ),
            (
                "thin-strokes",
                include_bytes!("../tests/fixtures/classic-v1-jpeg-delta-corpus/thin-strokes.png"),
                640,
                632,
                "2df3f71b0886c07961ed8f50e5191d785b5d5d50fad2088c618369bb57ae9f71",
            ),
        ];
        let mut actual = Vec::new();
        for (name, bytes, width, height, _) in pages {
            let input = match EncodedImage::new(bytes) {
                Ok(input) => input,
                Err(error) => panic!("{name} rejected as encoded input: {error}"),
            };
            let decoded = match decode_classic_bgr(input) {
                Ok(decoded) => decoded,
                Err(error) => panic!("{name} failed to decode: {error}"),
            };
            assert_eq!(decoded.dimensions().width(), width, "{name}: width");
            assert_eq!(decoded.dimensions().height(), height, "{name}: height");
            assert_eq!(decoded.channels(), BGR_CHANNELS, "{name}: channels");
            actual.push(crate::digest::sha256_hex(decoded.pixels()));
        }
        let expected: Vec<String> = pages.iter().map(|page| page.4.to_owned()).collect();
        assert_eq!(actual, expected, "corpus decoded BGR bytes changed");
    }

    #[test]
    fn non_png_signatures_are_reported_as_unsupported() {
        for bytes in [
            b"\xff\xd8\xff\xe0not-a-png".as_slice(),
            b"GIF89a-not-a-png".as_slice(),
            b"\x89PNGbroken-signature".as_slice(),
        ] {
            let input = match EncodedImage::new(bytes) {
                Ok(input) => input,
                Err(error) => panic!("expected accepted encoded bytes, got {error}"),
            };
            assert!(matches!(
                decode_classic_bgr(input),
                Err(Error::Unsupported {
                    capability: "image format"
                })
            ));
        }
    }
}
