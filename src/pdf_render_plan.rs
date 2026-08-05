// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The PDF render-scale planner, frozen from PaddleX `3.7.2`.
//!
//! Roadmap item `PDF-001`, its first executable slice. The renderer itself —
//! pdfium via `pypdfium2` — stays behind the five-part entry gate in
//! `docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md`. This module is the arithmetic
//! **above** the renderer: given a page size in PDF points and a pixel budget,
//! upstream's `get_pdf_render_scale_within_pixel_limit` either keeps the
//! requested scale, refuses the page when even the minimum scale exceeds the
//! budget, or **bisects thirty-two iterations** between the minimum scale and
//! an analytic upper bound and returns the lower edge of the final bracket.
//! Whatever renderer eventually satisfies the gate must be driven at this
//! scale, so it is pinned now, by execution, in
//! `tests/fixtures/classic-v1-pdf-scale/`.
//!
//! Upstream facts recorded with the capture rather than assumed:
//!
//! - defaults: requested scale `2.0` (`PADDLE_PDX_PDF_RENDER_SCALE`), minimum
//!   `0.1`, budget `178,956,970` pixels — PIL's decompression-bomb threshold;
//! - the reader quirk: `PDFReaderBackend.__init__` defaults `max_pixels=None`,
//!   which **bypasses the budget entirely** unless a caller passes one — this
//!   port keeps the budget mandatory instead, because an unbounded default is
//!   exactly what its own module exists to prevent;
//! - dimensions are `ceil(points * scale)` per axis, and a scale is kept when
//!   its pixel count is `<=` the budget, so a page exactly at the budget is
//!   not bisected.
#![allow(dead_code)]

use crate::error::{Error, InputViolation, Result};

/// Upstream's default requested render scale.
pub const DEFAULT_RENDER_SCALE: f64 = 2.0;
/// Upstream's default minimum render scale.
pub const DEFAULT_MIN_RENDER_SCALE: f64 = 0.1;
/// Upstream's default pixel budget, PIL's decompression-bomb threshold.
pub const DEFAULT_MAX_RENDER_PIXELS: u64 = 178_956_970;
/// The fixed bisection depth.
const BISECTION_ITERATIONS: u32 = 32;

/// A page size in PDF points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPageSize {
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
}

/// The rendered extent a page size produces at one scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderEstimate {
    /// Pixel width, `ceil(width * scale)`.
    pub width: u64,
    /// Pixel height, `ceil(height * scale)`.
    pub height: u64,
    /// `width * height`.
    pub pixels: u64,
}

/// `estimate_pdf_render_pixels`: per-axis ceiling, then the product.
///
/// The caller has already validated the page size and scale as positive and
/// finite, so the ceilings are representable.
#[must_use]
pub fn estimate_render_pixels(page: PdfPageSize, scale: f64) -> RenderEstimate {
    let width = (page.width * scale).ceil() as u64;
    let height = (page.height * scale).ceil() as u64;
    RenderEstimate {
        width,
        height,
        pixels: width * height,
    }
}

/// `get_pdf_render_scale_within_pixel_limit`, bit for bit.
///
/// Returns the scale to hand the renderer. Errors:
///
/// - [`Error::InvalidInput`] where upstream raises `ValueError`: a
///   non-positive or non-finite page dimension, requested scale, minimum
///   scale, or budget;
/// - [`Error::ResourceLimit`] where upstream raises `PDFRenderSizeError`: the
///   page exceeds the budget even at the minimum scale. The reported `actual`
///   is the pixel count at the **minimum** scale, which is the figure
///   upstream reports.
pub fn plan_render_scale(
    page: PdfPageSize,
    requested_scale: f64,
    min_scale: f64,
    max_pixels: u64,
) -> Result<f64> {
    // NaN fails `is_finite`, so each check rejects non-positive, infinite,
    // and NaN values alike.
    let positive_finite = |value: f64| value.is_finite() && value > 0.0;
    if !positive_finite(page.width) || !positive_finite(page.height) {
        return Err(Error::InvalidInput {
            field: "pdf.page_size",
            violation: InputViolation::OutOfRange,
        });
    }
    if !positive_finite(requested_scale) {
        return Err(Error::InvalidInput {
            field: "pdf.requested_scale",
            violation: InputViolation::OutOfRange,
        });
    }
    if !positive_finite(min_scale) {
        return Err(Error::InvalidInput {
            field: "pdf.min_scale",
            violation: InputViolation::OutOfRange,
        });
    }
    if max_pixels == 0 {
        return Err(Error::InvalidInput {
            field: "pdf.max_pixels",
            violation: InputViolation::Empty,
        });
    }

    let requested = estimate_render_pixels(page, requested_scale);
    if requested.pixels <= max_pixels {
        return Ok(requested_scale);
    }

    let minimum = estimate_render_pixels(page, min_scale);
    if minimum.pixels > max_pixels {
        return Err(Error::ResourceLimit {
            resource: "pdf.render_pixels",
            limit: max_pixels,
            actual: minimum.pixels,
        });
    }

    // The bracket: upstream seeds the upper edge analytically so the fixed
    // depth lands tight, and returns the LOWER edge, which is the largest
    // probed scale known to fit.
    let mut upper = requested_scale.min((max_pixels as f64 / (page.width * page.height)).sqrt());
    let mut lower = min_scale;
    for _ in 0..BISECTION_ITERATIONS {
        let scale = (lower + upper) / 2.0;
        if estimate_render_pixels(page, scale).pixels <= max_pixels {
            lower = scale;
        } else {
            upper = scale;
        }
    }
    Ok(lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-pdf-scale/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn items<'a>(value: &'a Value, what: &str) -> &'a [Value] {
        match value.as_array() {
            Some(values) => values,
            None => panic!("fixture field {what} is not an array"),
        }
    }

    /// The exact f64 recorded as hex bits; a decimal round-trip is not an
    /// oracle.
    fn from_bits(value: &Value) -> f64 {
        let text = match value.as_str() {
            Some(text) => text,
            None => panic!("bits"),
        };
        let mut bits = 0_u64;
        for c in text.chars() {
            bits = bits << 4
                | match c.to_digit(16) {
                    Some(digit) => u64::from(digit),
                    None => panic!("hex digit"),
                };
        }
        f64::from_bits(bits)
    }

    #[test]
    fn the_captured_scales_are_reproduced_bit_for_bit() {
        let fixture = fixture();
        let cases = items(&fixture["cases"], "cases");
        assert_eq!(cases.len(), 11);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let sizes = items(&case["page_size_bits"], "size bits");
            let page = PdfPageSize {
                width: from_bits(&sizes[0]),
                height: from_bits(&sizes[1]),
            };
            let requested = case["requested_scale"].as_f64().unwrap_or(f64::NAN);
            let minimum = case["min_scale"].as_f64().unwrap_or(f64::NAN);
            let budget = case["max_pixels"].as_u64().unwrap_or(0);
            let outcome = &case["outcome"];

            match plan_render_scale(page, requested, minimum, budget) {
                Ok(scale) => {
                    let expected = from_bits(&outcome["scale_bits"]);
                    assert_eq!(
                        scale.to_bits(),
                        expected.to_bits(),
                        "{name}: scale {scale} vs {expected}"
                    );
                    let estimate = estimate_render_pixels(page, scale);
                    assert_eq!(
                        estimate.width,
                        outcome["width"].as_u64().unwrap_or(0),
                        "{name}: width"
                    );
                    assert_eq!(
                        estimate.height,
                        outcome["height"].as_u64().unwrap_or(0),
                        "{name}: height"
                    );
                    assert_eq!(
                        estimate.pixels,
                        outcome["pixels"].as_u64().unwrap_or(0),
                        "{name}: pixels"
                    );
                }
                Err(Error::ResourceLimit { actual, .. }) => {
                    assert_eq!(
                        outcome["error"].as_str(),
                        Some("render_size"),
                        "{name}: unexpected refusal"
                    );
                    assert_eq!(
                        actual,
                        outcome["pixel_count"].as_u64().unwrap_or(0),
                        "{name}: refused pixel count"
                    );
                }
                Err(other) => panic!("{name}: unexpected error {other}"),
            }
        }
    }

    #[test]
    fn the_captured_estimates_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["estimates"], "estimates") {
            let name = case["case"].as_str().unwrap_or("?");
            let sizes = items(&case["page_size"], "size");
            let page = PdfPageSize {
                width: sizes[0].as_f64().unwrap_or(f64::NAN),
                height: sizes[1].as_f64().unwrap_or(f64::NAN),
            };
            let estimate = estimate_render_pixels(page, case["scale"].as_f64().unwrap_or(f64::NAN));
            assert_eq!(
                estimate.width,
                case["width"].as_u64().unwrap_or(0),
                "{name}: width"
            );
            assert_eq!(
                estimate.height,
                case["height"].as_u64().unwrap_or(0),
                "{name}: height"
            );
            assert_eq!(
                estimate.pixels,
                case["pixels"].as_u64().unwrap_or(0),
                "{name}: pixels"
            );
        }
    }

    /// Every invalid input upstream refuses with `ValueError` is refused here
    /// with a typed error, including the non-finite cases upstream cannot
    /// express.
    #[test]
    fn invalid_inputs_are_refused() {
        let page = PdfPageSize {
            width: 595.0,
            height: 842.0,
        };
        let cases: [(PdfPageSize, f64, f64, u64); 8] = [
            (
                PdfPageSize {
                    width: 0.0,
                    height: 842.0,
                },
                2.0,
                0.1,
                1_000_000,
            ),
            (
                PdfPageSize {
                    width: 595.0,
                    height: -1.0,
                },
                2.0,
                0.1,
                1_000_000,
            ),
            (page, 0.0, 0.1, 1_000_000),
            (page, 2.0, 0.0, 1_000_000),
            (page, 2.0, 0.1, 0),
            (
                PdfPageSize {
                    width: f64::NAN,
                    height: 842.0,
                },
                2.0,
                0.1,
                1_000_000,
            ),
            (page, f64::INFINITY, 0.1, 1_000_000),
            (page, 2.0, f64::NAN, 1_000_000),
        ];
        for (index, (size, requested, minimum, budget)) in cases.into_iter().enumerate() {
            assert!(
                matches!(
                    plan_render_scale(size, requested, minimum, budget),
                    Err(Error::InvalidInput { .. })
                ),
                "invalid case {index} must be refused"
            );
        }
    }

    /// The planner's result always fits the budget it was given, and the
    /// defaults match the captured upstream flags.
    #[test]
    fn planned_scales_fit_their_budget() {
        let fixture = fixture();
        let defaults = &fixture["defaults"];
        assert_eq!(
            defaults["requested_scale"].as_f64(),
            Some(DEFAULT_RENDER_SCALE)
        );
        assert_eq!(
            defaults["min_scale"].as_f64(),
            Some(DEFAULT_MIN_RENDER_SCALE)
        );
        assert_eq!(
            defaults["max_pixels"].as_u64(),
            Some(DEFAULT_MAX_RENDER_PIXELS)
        );

        for case in items(&fixture["cases"], "cases") {
            let sizes = items(&case["page_size_bits"], "size bits");
            let page = PdfPageSize {
                width: from_bits(&sizes[0]),
                height: from_bits(&sizes[1]),
            };
            let budget = case["max_pixels"].as_u64().unwrap_or(0);
            if let Ok(scale) = plan_render_scale(
                page,
                case["requested_scale"].as_f64().unwrap_or(f64::NAN),
                case["min_scale"].as_f64().unwrap_or(f64::NAN),
                budget,
            ) {
                assert!(
                    estimate_render_pixels(page, scale).pixels <= budget,
                    "a planned scale must fit its budget"
                );
            }
        }
    }
}
