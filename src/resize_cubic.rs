// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cubic scaling, for the layout detector's non-aspect-preserving resize.
//!
//! Roadmap item `LAY-001`. `PP-DocLayout_plus-L` declares `interp: 2`, which
//! `docs/LAYOUT_CONTRACT.md` records maps to **bicubic** in PaddleX's own table
//! — not the linear interpolation every other resize in this project uses.
//!
//! # Why neither existing path is reusable
//!
//! - `src/resize.rs` reproduces OpenCV's `INTER_LINEAR` with its fixed-point
//!   weights. Different kernel entirely.
//! - `src/crop.rs` has cubic sampling, but it is a **projective** warp built for
//!   `get_rotate_crop_image`. It maps an arbitrary quadrilateral, not an axis
//!   aligned scale, and this needs the separable form `cv2.resize` uses.
//!
//! What is shared is the part worth sharing: the cubic weight construction and
//! the replicated border index, both already pinned against `72` captured
//! OpenCV cases.
//!
//! # The coordinate mapping
//!
//! `cv2.resize` maps a destination centre back to source space as
//! `src = (dst + 0.5) * scale - 0.5` with `scale = source / target`. The half
//! pixel on both sides is what makes a `2x` upscale land on `-0.25, 0.25, 0.75`
//! rather than on integers, and dropping it is the classic off-by-half that
//! produces an image which looks right and matches nothing.
//!
//! # A known residual divergence
//!
//! This reproduces all five committed cases byte for byte, and it does **not**
//! reproduce OpenCV exactly at page scale. A `297x421` to `800x800` resize
//! differs in `24` bytes out of `1,920,000` — roughly one pixel in eighty
//! thousand, each off by one.
//!
//! The cause is that OpenCV's 8-bit cubic path is **fixed point**: coefficients
//! are quantized to `i16` at `1 << 11` and accumulated in integers, the way
//! `src/resize.rs` already reproduces for the linear kernel. Two attempts at
//! that arithmetic here were **worse** than this float accumulator — `82,990`
//! and `73,910` mismatching bytes — which means the pass structure was being
//! guessed rather than read, and guessing is what this project's method exists
//! to avoid.
//!
//! So the float version stands, with the divergence measured and stated rather
//! than rounded away. It also stands as a warning about corpus size: the five
//! committed cases total about `2,600` pixels, and a one-in-eighty-thousand
//! defect cannot appear in `2,600` samples. Only a page-sized case found it.
//!
//! # Why nothing calls this yet
//!
//! `LAY-001` needs it and `LAY-001` has no implementation: the artifact is
//! provisioned and the contract frozen, but the model is not run. This operator
//! is verified against a capture on its own so that when the layout path is
//! built, its resize is not one of the things that could be wrong.
#![allow(dead_code)]

use crate::crop::{InterleavedImage, cubic_weights};
use crate::error::{Error, InputViolation, Result};
use crate::types::ImageDimensions;

/// Scales an image with OpenCV's `INTER_CUBIC`.
pub(crate) fn classic_cubic_resize(
    source: &InterleavedImage,
    target: ImageDimensions,
) -> Result<InterleavedImage> {
    let dimensions = source.dimensions();
    let (source_width, source_height) = (dimensions.width(), dimensions.height());
    let (target_width, target_height) = (target.width(), target.height());
    if source_width == 0 || source_height == 0 {
        return Err(Error::InvalidInput {
            field: "resize_cubic.source",
            violation: InputViolation::Empty,
        });
    }

    let channels = source.channels() as usize;
    let scale_x = f64::from(source_width) / f64::from(target_width);
    let scale_y = f64::from(source_height) / f64::from(target_height);

    // Per-axis taps are computed once per row and column rather than per pixel:
    // the mapping is separable, which is also why the crop's projective sampler
    // cannot be reused.
    let columns: Vec<[(u32, f32); 4]> = (0..target_width)
        .map(|x| axis_taps(f64::from(x), scale_x, source_width))
        .collect();
    let rows: Vec<[(u32, f32); 4]> = (0..target_height)
        .map(|y| axis_taps(f64::from(y), scale_y, source_height))
        .collect();

    let mut pixels: Vec<u8> = Vec::new();
    pixels
        .try_reserve_exact((target_width as usize) * (target_height as usize) * channels)
        .map_err(|_| Error::Backend {
            message: "cubic resize output allocation failed",
        })?;

    let source_pixels = source.pixels();
    for row_taps in &rows {
        for column_taps in &columns {
            for channel in 0..channels {
                let mut accumulated = 0.0_f32;
                for (sample_y, weight_y) in row_taps {
                    for (sample_x, weight_x) in column_taps {
                        let index = ((*sample_y as usize) * (source_width as usize)
                            + *sample_x as usize)
                            * channels
                            + channel;
                        accumulated += weight_x * weight_y * f32::from(source_pixels[index]);
                    }
                }
                pixels.push(saturate(accumulated));
            }
        }
    }

    InterleavedImage::new(target, source.channels(), pixels)
}

/// Returns the four source taps for one destination coordinate.
fn axis_taps(destination: f64, scale: f64, length: u32) -> [(u32, f32); 4] {
    // `(dst + 0.5) * scale - 0.5`, OpenCV's centre-aligned mapping.
    let mapped = (destination + 0.5) * scale - 0.5;
    let base = mapped.floor();
    let alpha = (mapped - base) as f32;
    let weights = cubic_weights(alpha);
    core::array::from_fn(|index| {
        let coordinate = base + index as f64 - 1.0;
        (clamped_index(coordinate, length), weights[index])
    })
}

/// Clamps a sample coordinate into the source, replicating the border.
fn clamped_index(coordinate: f64, length: u32) -> u32 {
    if coordinate <= 0.0 {
        0
    } else if coordinate >= f64::from(length - 1) {
        length - 1
    } else {
        coordinate as u32
    }
}

/// Rounds and saturates to `uint8`, as OpenCV's cubic accumulator does.
fn saturate(value: f32) -> u8 {
    crate::detector_boxes::round_half_to_even(value).clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-cubic-resize/expected.json");

    fn synthetic(index: usize, width: u32, height: u32) -> InterleavedImage {
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                for channel in 0..3_usize {
                    pixels.push(((x * 7 + y * 13 + channel * 29 + index * 31) % 256) as u8);
                }
            }
        }
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("dimensions: {error}"),
        };
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("image: {error}"),
        }
    }

    /// Every captured `cv2.resize` with `INTER_CUBIC` is reproduced byte for
    /// byte, across upscale, downscale, and both at once.
    #[test]
    fn every_captured_cubic_resize_is_reproduced() {
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture json: {error}"),
        };
        let records = match document["records"].as_array() {
            Some(records) => records,
            None => panic!("fixture must hold records"),
        };
        assert_eq!(records.len(), 5);

        for record in records {
            let case = record["case"].as_str().unwrap_or_default();
            let index: usize = case
                .trim_start_matches("cubic-")
                .parse()
                .unwrap_or_default();
            let pair = |key: &str| -> (u32, u32) {
                match record[key].as_array() {
                    Some(values) => (
                        values[0].as_u64().unwrap_or_default() as u32,
                        values[1].as_u64().unwrap_or_default() as u32,
                    ),
                    None => panic!("{case}: no {key}"),
                }
            };
            let (width, height) = pair("source_wh");
            let (target_width, target_height) = pair("target_wh");

            let source = synthetic(index, width, height);
            let target = match ImageDimensions::new(target_width, target_height) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };
            let resized = match classic_cubic_resize(&source, target) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };

            let expected =
                match STANDARD.decode(record["output_bgr_base64"].as_str().unwrap_or_default()) {
                    Ok(bytes) => bytes,
                    Err(error) => panic!("{case}: base64 {error}"),
                };
            assert_eq!(resized.pixels().len(), expected.len(), "{case}: byte count");
            for (position, (actual, want)) in resized.pixels().iter().zip(&expected).enumerate() {
                assert_eq!(
                    actual,
                    want,
                    "{case}: byte {position} of {}",
                    expected.len()
                );
            }
        }
    }

    /// The half-pixel in the mapping is load-bearing.
    ///
    /// Without it a `2x` upscale would sample at integers and reproduce the
    /// source exactly in every other column, which looks plausible and matches
    /// nothing.
    #[test]
    fn the_mapping_is_centre_aligned() {
        // 4 -> 8 has scale 0.5; destination 0 maps to (0 + 0.5) * 0.5 - 0.5,
        // which is -0.25 rather than 0.
        let taps = axis_taps(0.0, 0.5, 4);
        assert_eq!(taps[1].0, 0, "the second tap is the clamped base");
        // A destination exactly at a source centre has all its weight on one tap.
        let centre = axis_taps(1.0, 1.0, 4);
        assert!(
            (centre[1].1 - 1.0).abs() < 1e-6,
            "an integer mapping puts full weight on the sample: {centre:?}"
        );
    }

    #[test]
    fn a_border_sample_replicates_rather_than_wrapping() {
        assert_eq!(clamped_index(-5.0, 4), 0);
        assert_eq!(clamped_index(0.0, 4), 0);
        assert_eq!(clamped_index(2.0, 4), 2);
        assert_eq!(clamped_index(3.0, 4), 3);
        assert_eq!(clamped_index(99.0, 4), 3);
    }
}
