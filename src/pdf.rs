// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! `PDF-001`: bounded PDF page rendering.
//!
//! Behind the `pdf` feature. The renderer is `hayro 0.4.0`, chosen by the user
//! decision in `docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md` after the measured
//! entry gate in `docs/PDF_ENTRY_GATE_EVIDENCE.md`.
//!
//! # Why this module exists rather than a thin wrapper
//!
//! The gate's part 4 measured one failure: a form XObject that draws itself
//! consumed a `2 GiB` limit and **aborted** the process, where the reference
//! renderer bounds the same file in `0.02` s. An abort is not a failure mode this
//! project's error contract can express, and this project's own policy is that
//! resource limits are "checked before allocation" — which a library that
//! recurses without a depth bound cannot honour from the inside.
//!
//! So the bound is **this port's**, and it is a pre-flight: before a page is
//! handed to the renderer, [`PdfDocument::render_page`] walks the page's XObject
//! reference graph with the same resolver the renderer will use, and refuses a
//! cycle or an over-deep nest as a typed error. That is the whole reason
//! `hayro-syntax` is a direct dependency and not merely a transitive one — a scan
//! over raw bytes would miss dictionaries hidden in object streams, and a
//! best-effort refusal is not a bound.
//!
//! # What is refused, and when
//!
//! Every check happens before the allocation it bounds:
//!
//! | Condition | Error | Checked |
//! |---|---|---|
//! | Empty input | [`Error::InvalidInput`] | before parsing |
//! | Document larger than the byte budget | [`Error::ResourceLimit`] | before parsing |
//! | Encrypted document | [`Error::Unsupported`] | at parse, before any page |
//! | Unparseable document | [`Error::InvalidInput`] | at parse |
//! | No pages | [`Error::InvalidInput`] | at parse |
//! | More pages than the page budget | [`Error::ResourceLimit`] | at parse |
//! | Page index past the end | [`Error::InvalidInput`] | before rendering |
//! | Page exceeding the pixel budget at the minimum scale | [`Error::ResourceLimit`] | before rendering |
//! | XObject reference cycle | [`Error::Unsupported`] | before rendering |
//! | XObject nesting past the depth budget | [`Error::ResourceLimit`] | before rendering |
//!
//! An encrypted document is refused rather than rendered even though the
//! candidate renderer also refuses it: the measurement showed today's behaviour,
//! not a guarantee, and rendering an encrypted document as though it were plain
//! would be worse than failing.
//!
//! # What is not claimed
//!
//! Pixel fidelity, except on the scan path. The gate measured the image-XObject
//! page reproducing the reference renderer **bit-identically** and `DCTDecode`
//! within `4` components of 255; vector and text pages agree closely but not
//! exactly, and no pixel-identity claim is made for them. See
//! `docs/PDF_ENTRY_GATE_EVIDENCE.md` section 2.
//!
//! # Not yet on a caller's path
//!
//! Nothing public reaches this module. Rendering a page is `PDF-001`'s job;
//! turning a document into ordered per-page results with typed per-page failures
//! is `MPAGE-001`'s, and that is the slice that exposes an API. Until then the
//! module allows dead code, the way `LAY-001` kept its producer `pub(crate)`
//! rather than exposing an operator it was not ready to make claims about.
#![allow(dead_code)]

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use hayro::{InterpreterSettings, RenderSettings};
use hayro_syntax::object::{Dict, Stream};
use hayro_syntax::page::Page;

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::pdf_render_plan::{
    DEFAULT_MAX_RENDER_PIXELS, DEFAULT_MIN_RENDER_SCALE, DEFAULT_RENDER_SCALE, PdfPageSize,
    plan_render_scale,
};
use crate::types::ImageDimensions;

/// The document byte budget, reusing the decode envelope's precedent.
pub(crate) const DEFAULT_MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
/// The page-count budget.
///
/// Not a specification value: upstream has no page cap, and an unbounded one
/// makes a page count a memory claim. `4096` is far past any document this port
/// is meant for and far below anything that matters.
pub(crate) const DEFAULT_MAX_PAGES: u32 = 4096;
/// The XObject nesting budget.
///
/// A legitimate document nests forms a handful of levels; `16` is generous. The
/// budget exists so that a deep-but-acyclic nest is refused by a number rather
/// than by exhausting memory, which is the same failure the cycle check prevents
/// by structure.
pub(crate) const DEFAULT_MAX_XOBJECT_DEPTH: u32 = 16;
/// How many XObject nodes the pre-flight will visit before refusing.
///
/// The walk is itself work a hostile document controls, so it is bounded too.
pub(crate) const DEFAULT_MAX_XOBJECT_NODES: u32 = 4096;

/// The bounds a document is read under.
///
/// Every field is a refusal threshold, not a hint: exceeding one produces a
/// typed error before the memory it bounds is allocated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PdfLimits {
    /// Largest document accepted, in bytes.
    pub(crate) max_document_bytes: u64,
    /// Largest page count accepted.
    pub(crate) max_pages: u32,
    /// Largest rendered page accepted, in pixels.
    pub(crate) max_page_pixels: u64,
    /// Requested render scale, before the pixel budget reduces it.
    pub(crate) requested_scale: f64,
    /// Smallest scale the planner may fall back to.
    pub(crate) min_scale: f64,
    /// Deepest XObject nesting accepted.
    pub(crate) max_xobject_depth: u32,
    /// Most XObject nodes the pre-flight will visit.
    pub(crate) max_xobject_nodes: u32,
}

impl Default for PdfLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: DEFAULT_MAX_DOCUMENT_BYTES,
            max_pages: DEFAULT_MAX_PAGES,
            max_page_pixels: DEFAULT_MAX_RENDER_PIXELS,
            requested_scale: DEFAULT_RENDER_SCALE,
            min_scale: DEFAULT_MIN_RENDER_SCALE,
            max_xobject_depth: DEFAULT_MAX_XOBJECT_DEPTH,
            max_xobject_nodes: DEFAULT_MAX_XOBJECT_NODES,
        }
    }
}

/// One page's rendered raster, and the scale it was rendered at.
#[derive(Clone, Debug)]
pub(crate) struct RenderedPage {
    /// The page's pixels in this project's classic interleaved BGR convention.
    pub(crate) image: InterleavedImage,
    /// The scale actually used, which the pixel budget may have reduced below
    /// the requested one.
    pub(crate) scale: f64,
    /// Zero-based page index within the document.
    pub(crate) index: u32,
}

/// An opened PDF, bounded at open time.
pub(crate) struct PdfDocument {
    pdf: hayro_syntax::Pdf,
    limits: PdfLimits,
    pages: u32,
}

/// Hand-written because the renderer's document type has no `Debug`, and because
/// the document's own bytes are the last thing a log should carry: the page count
/// and the bounds are the whole useful summary.
impl fmt::Debug for PdfDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PdfDocument")
            .field("pages", &self.pages)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// Opens a document, refusing before it parses whatever it can.
///
/// The bytes are taken by value because the renderer holds them for the
/// document's lifetime; copying them to hand over would double the peak.
pub(crate) fn open(bytes: Vec<u8>, limits: PdfLimits) -> Result<PdfDocument> {
    if bytes.is_empty() {
        return Err(Error::InvalidInput {
            field: "pdf.document",
            violation: InputViolation::Empty,
        });
    }
    let length = bytes.len() as u64;
    if length > limits.max_document_bytes {
        return Err(Error::ResourceLimit {
            resource: "pdf.document_bytes",
            limit: limits.max_document_bytes,
            actual: length,
        });
    }

    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
    let pdf = match hayro_syntax::Pdf::new(data) {
        Ok(pdf) => pdf,
        // Encryption is a capability this port does not support, not a malformed
        // file: the distinction is what lets a caller tell "give me the password"
        // apart from "this is not a PDF".
        Err(hayro_syntax::LoadPdfError::Decryption(_)) => {
            return Err(Error::Unsupported {
                capability: "pdf.encrypted",
            });
        }
        Err(hayro_syntax::LoadPdfError::Invalid) => {
            return Err(Error::InvalidInput {
                field: "pdf.document",
                violation: InputViolation::Malformed,
            });
        }
    };

    let count = pdf.pages().len();
    if count == 0 {
        return Err(Error::InvalidInput {
            field: "pdf.pages",
            violation: InputViolation::Empty,
        });
    }
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    if count > limits.max_pages {
        return Err(Error::ResourceLimit {
            resource: "pdf.pages",
            limit: u64::from(limits.max_pages),
            actual: u64::from(count),
        });
    }

    Ok(PdfDocument {
        pdf,
        limits,
        pages: count,
    })
}

impl PdfDocument {
    /// How many pages the document has, in document order.
    #[must_use]
    pub(crate) fn page_count(&self) -> u32 {
        self.pages
    }

    /// The bounds this document was opened under.
    #[must_use]
    pub(crate) fn limits(&self) -> PdfLimits {
        self.limits
    }

    /// The page size in PDF points, before any scaling.
    pub(crate) fn page_size(&self, index: u32) -> Result<PdfPageSize> {
        let page = self.page(index)?;
        let (width, height) = page.render_dimensions();
        Ok(PdfPageSize {
            width: f64::from(width),
            height: f64::from(height),
        })
    }

    /// Renders one page into the classic interleaved BGR convention.
    ///
    /// The scale is planned by [`plan_render_scale`], so a page too large for the
    /// pixel budget is rendered smaller rather than refused — and refused only
    /// when it exceeds the budget even at the minimum scale, which is upstream's
    /// own behaviour. The XObject pre-flight runs before the renderer is called.
    pub(crate) fn render_page(&self, index: u32) -> Result<RenderedPage> {
        let page = self.page(index)?;
        let size = self.page_size(index)?;
        let scale = plan_render_scale(
            size,
            self.limits.requested_scale,
            self.limits.min_scale,
            self.limits.max_page_pixels,
        )?;

        // The port-owned bound, before the renderer allocates anything.
        self.check_xobject_graph(page)?;

        let settings = RenderSettings {
            #[allow(clippy::cast_possible_truncation)]
            x_scale: scale as f32,
            #[allow(clippy::cast_possible_truncation)]
            y_scale: scale as f32,
            width: None,
            height: None,
        };
        let pixmap = hayro::render(page, &InterpreterSettings::default(), &settings);
        let width = u32::from(pixmap.width());
        let height = u32::from(pixmap.height());
        let dimensions = ImageDimensions::new(width, height)?;
        let image = InterleavedImage::new(dimensions, 3, premultiplied_rgba_to_bgr(&pixmap))?;
        Ok(RenderedPage {
            image,
            scale,
            index,
        })
    }

    fn page(&self, index: u32) -> Result<&Page<'_>> {
        if index >= self.pages {
            return Err(Error::InvalidInput {
                field: "pdf.page_index",
                violation: InputViolation::OutOfRange,
            });
        }
        match self.pdf.pages().get(index as usize) {
            Some(page) => Ok(page),
            // Unreachable: `pages` was taken from the same list.
            None => Err(Error::InvalidInput {
                field: "pdf.page_index",
                violation: InputViolation::OutOfRange,
            }),
        }
    }

    /// Refuses a page whose form XObjects reference each other in a cycle, or
    /// nest deeper than the budget.
    ///
    /// Depth-first over the reference graph, carrying the current **path** rather
    /// than a global visited set: a form legitimately drawn twice from different
    /// parents is not a cycle, and a global set would call it one. The node count
    /// is bounded separately, because the walk is work a hostile document
    /// controls.
    fn check_xobject_graph(&self, page: &Page<'_>) -> Result<()> {
        let resources = page.resources();
        let mut path = Vec::new();
        let mut visited = 0_u32;
        self.walk_xobjects(&resources.x_objects, resources, 0, &mut path, &mut visited)
    }

    fn walk_xobjects(
        &self,
        x_objects: &Dict<'_>,
        resources: &hayro_syntax::page::Resources<'_>,
        depth: u32,
        path: &mut Vec<hayro_syntax::object::ObjRef>,
        visited: &mut u32,
    ) -> Result<()> {
        if depth > self.limits.max_xobject_depth {
            return Err(Error::ResourceLimit {
                resource: "pdf.xobject_depth",
                limit: u64::from(self.limits.max_xobject_depth),
                actual: u64::from(depth),
            });
        }

        for key in x_objects.keys() {
            *visited += 1;
            if *visited > self.limits.max_xobject_nodes {
                return Err(Error::ResourceLimit {
                    resource: "pdf.xobject_nodes",
                    limit: u64::from(self.limits.max_xobject_nodes),
                    actual: u64::from(*visited),
                });
            }

            // Only a reference can close a cycle; an inline dictionary cannot
            // name itself.
            let Some(reference) = x_objects.get_ref(key.deref()) else {
                continue;
            };
            if path.contains(&reference) {
                return Err(Error::Unsupported {
                    capability: "pdf.recursive_xobject",
                });
            }
            let Some(stream) = resources.resolve_ref::<Stream<'_>>(reference) else {
                continue;
            };
            // Image XObjects hold no resources, so only forms can recurse.
            let nested = stream
                .dict()
                .get::<Dict<'_>>(b"Resources".as_slice())
                .and_then(|nested| nested.get::<Dict<'_>>(b"XObject".as_slice()));
            let Some(nested) = nested else {
                continue;
            };
            path.push(reference);
            let outcome = self.walk_xobjects(&nested, resources, depth + 1, path, visited);
            path.pop();
            outcome?;
        }
        Ok(())
    }
}

/// Premultiplied RGBA8 to interleaved BGR8.
///
/// The renderer fills the page white before interpreting it, so alpha is `255`
/// everywhere in practice. The un-premultiply is still applied, because "in
/// practice" is not a guarantee and a transparent pixel would otherwise come out
/// darker than it is.
fn premultiplied_rgba_to_bgr(pixmap: &hayro::Pixmap) -> Vec<u8> {
    let source = pixmap.data_as_u8_slice();
    let mut out = Vec::with_capacity(source.len() / 4 * 3);
    for pixel in source.chunks_exact(4) {
        let (red, green, blue, alpha) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        let unpremultiply = |value: u8| -> u8 {
            match alpha {
                0 => 0,
                255 => value,
                alpha => {
                    let scaled = u32::from(value) * 255 + u32::from(alpha) / 2;
                    (scaled / u32::from(alpha)).min(255) as u8
                }
            }
        };
        out.push(unpremultiply(blue));
        out.push(unpremultiply(green));
        out.push(unpremultiply(red));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIDELITY: &str = "tests/fixtures/classic-v1-pdf-entry-gate/fidelity";
    const MALFORMED: &str = "tests/fixtures/classic-v1-pdf-entry-gate/malformed";

    fn corpus(directory: &str, name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(directory)
            .join(name);
        match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => panic!("{name}: {error}"),
        }
    }

    /// Every fidelity document opens and renders to a non-empty BGR raster.
    #[test]
    fn the_fidelity_corpus_renders() {
        for name in [
            "vector.pdf",
            "scanned_flate.pdf",
            "scanned_jpeg.pdf",
            "form_xobject.pdf",
            "cid_font.pdf",
            "shading.pdf",
            "standard_font.pdf",
        ] {
            let document = match open(corpus(FIDELITY, name), PdfLimits::default()) {
                Ok(document) => document,
                Err(error) => panic!("{name}: {error}"),
            };
            assert_eq!(document.page_count(), 1, "{name}: page count");
            let page = match document.render_page(0) {
                Ok(page) => page,
                Err(error) => panic!("{name}: {error}"),
            };
            assert_eq!(page.index, 0, "{name}: index");
            assert!(page.scale > 0.0, "{name}: scale");
            let dimensions = page.image.dimensions();
            let (width, height) = (dimensions.width(), dimensions.height());
            assert!(width > 0 && height > 0, "{name}: dimensions");
            assert_eq!(
                page.image.pixels().len(),
                width as usize * height as usize * 3,
                "{name}: BGR byte count"
            );
        }
    }

    /// The scan path renders at the dimensions the gate measured.
    ///
    /// `120x160` points at the default scale of `2.0` is `240x320`, which is what
    /// the fidelity measurement compared bit-identically against poppler. If this
    /// changes, the recorded measurement no longer describes this code.
    #[test]
    fn the_scan_path_renders_at_the_measured_dimensions() {
        let document = match open(corpus(FIDELITY, "scanned_flate.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        let page = match document.render_page(0) {
            Ok(page) => page,
            Err(error) => panic!("render: {error}"),
        };
        assert_eq!(
            (
                page.image.dimensions().width(),
                page.image.dimensions().height()
            ),
            (240, 320)
        );
        assert_eq!(page.scale, 2.0);
    }

    /// The whole malformed corpus answers with a value or a typed error.
    ///
    /// This is the test the entry gate's part 4 was measured for. The renderer
    /// alone **aborts** on `deep_nesting`; the assertion here is that this port
    /// does not, and that every other case is answered rather than survived by
    /// luck. A panic or an abort fails the test by killing it.
    #[test]
    fn the_malformed_corpus_is_answered_rather_than_survived() {
        let expected: [(&str, Option<&str>); 15] = [
            ("truncated_header.pdf", Some("invalid input")),
            ("truncated_body.pdf", Some("invalid input")),
            ("missing_root.pdf", Some("invalid input")),
            ("no_xref.pdf", None),
            ("bad_xref_offsets.pdf", None),
            ("huge_declared_length.pdf", None),
            ("huge_image_dimensions.pdf", None),
            ("negative_dimensions.pdf", None),
            ("zero_pages.pdf", Some("invalid input")),
            ("circular_pages.pdf", Some("invalid input")),
            ("encrypted_stub.pdf", Some("unsupported")),
            ("javascript_openaction.pdf", None),
            ("embedded_file.pdf", None),
            ("deep_nesting.pdf", Some("unsupported")),
            // Page 0 of this one renders; the sweep only ever asks about page 0.
            ("mixed_pages.pdf", None),
        ];

        for (name, refusal) in expected {
            let outcome = open(corpus(MALFORMED, name), PdfLimits::default())
                .and_then(|document| document.render_page(0).map(|_| ()));
            match (outcome, refusal) {
                (Ok(()), None) => {}
                (Err(error), Some(prefix)) => {
                    let rendered = error.to_string();
                    assert!(
                        rendered.starts_with(prefix),
                        "{name}: expected an error starting {prefix:?}, got {rendered:?}"
                    );
                }
                (Ok(()), Some(prefix)) => {
                    panic!("{name}: rendered, but should have been refused with {prefix:?}")
                }
                (Err(error), None) => panic!("{name}: refused unexpectedly: {error}"),
            }
        }
    }

    /// The recursive form XObject is refused as a capability, not survived.
    ///
    /// Named separately from the corpus sweep because it is the one case the
    /// measurement showed the renderer cannot survive, and therefore the one that
    /// justifies the pre-flight existing at all.
    #[test]
    fn a_recursive_form_xobject_is_refused_before_rendering() {
        let document = match open(corpus(MALFORMED, "deep_nesting.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        match document.render_page(0) {
            Err(Error::Unsupported { capability }) => {
                assert_eq!(capability, "pdf.recursive_xobject");
            }
            Err(other) => panic!("wrong error: {other}"),
            Ok(_) => panic!("a self-referential form XObject was rendered"),
        }
    }

    /// An acyclic page passes the pre-flight it shares with the cyclic one.
    ///
    /// Without this, a pre-flight that refused every form XObject would pass the
    /// test above.
    #[test]
    fn a_form_xobject_that_is_not_recursive_is_not_refused() {
        let document = match open(corpus(FIDELITY, "form_xobject.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        assert!(document.render_page(0).is_ok());
    }

    /// Empty, oversized, and out-of-range inputs are refused by their own bound.
    #[test]
    fn the_documented_bounds_are_the_enforced_bounds() {
        assert!(matches!(
            open(Vec::new(), PdfLimits::default()),
            Err(Error::InvalidInput {
                field: "pdf.document",
                violation: InputViolation::Empty
            })
        ));

        let limits = PdfLimits {
            max_document_bytes: 16,
            ..PdfLimits::default()
        };
        match open(corpus(FIDELITY, "vector.pdf"), limits) {
            Err(Error::ResourceLimit {
                resource,
                limit,
                actual,
            }) => {
                assert_eq!(resource, "pdf.document_bytes");
                assert_eq!(limit, 16);
                assert!(actual > 16);
            }
            other => panic!("expected a byte-budget refusal, got {other:?}"),
        }

        let limits = PdfLimits {
            max_pages: 0,
            ..PdfLimits::default()
        };
        assert!(matches!(
            open(corpus(FIDELITY, "vector.pdf"), limits),
            Err(Error::ResourceLimit {
                resource: "pdf.pages",
                ..
            })
        ));

        let document = match open(corpus(FIDELITY, "vector.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        assert!(matches!(
            document.render_page(1),
            Err(Error::InvalidInput {
                field: "pdf.page_index",
                violation: InputViolation::OutOfRange
            })
        ));
    }

    /// A page too large for the pixel budget is scaled down, then refused.
    ///
    /// Both halves matter: the planner exists so an oversized page still yields a
    /// result, and the refusal exists so an absurd budget does not silently
    /// produce a one-pixel page.
    #[test]
    fn the_pixel_budget_scales_down_before_it_refuses() {
        let bytes = corpus(FIDELITY, "vector.pdf");
        let limits = PdfLimits {
            max_page_pixels: 10_000,
            ..PdfLimits::default()
        };
        let document = match open(bytes.clone(), limits) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        let page = match document.render_page(0) {
            Ok(page) => page,
            Err(error) => panic!("render: {error}"),
        };
        assert!(page.scale < DEFAULT_RENDER_SCALE, "scale was not reduced");
        assert!(
            u64::from(page.image.dimensions().width())
                * u64::from(page.image.dimensions().height())
                <= 10_000,
            "the reduced page still exceeds the budget"
        );

        let limits = PdfLimits {
            max_page_pixels: 1,
            min_scale: 0.5,
            ..PdfLimits::default()
        };
        let document = match open(bytes, limits) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        assert!(matches!(
            document.render_page(0),
            Err(Error::ResourceLimit { .. })
        ));
    }

    /// A two-page document where one page renders and the other cannot.
    ///
    /// This is the per-page failure policy at the level it can be tested without
    /// a model: the outcome is per page, so page one must come back and page two
    /// must come back as a typed error. An all-or-nothing implementation would
    /// lose page one, and one that stopped at the first failure would never
    /// report page two.
    #[test]
    fn a_failing_page_does_not_take_the_document_with_it() {
        let document = match open(corpus(MALFORMED, "mixed_pages.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        assert_eq!(document.page_count(), 2);

        let first = document.render_page(0);
        assert!(first.is_ok(), "the good page did not render: {first:?}");

        match document.render_page(1) {
            Err(Error::Unsupported { capability }) => {
                assert_eq!(capability, "pdf.recursive_xobject");
            }
            Err(other) => panic!("wrong error for the bad page: {other}"),
            Ok(_) => panic!("the recursive page rendered"),
        }

        // And the order does not matter: the bad page does not poison the good
        // one, which a shared cache keyed carelessly could have caused.
        assert!(document.render_page(0).is_ok());
    }

    /// Rendering the same page twice gives the same bytes.
    #[test]
    fn rendering_is_deterministic() {
        let document = match open(corpus(FIDELITY, "cid_font.pdf"), PdfLimits::default()) {
            Ok(document) => document,
            Err(error) => panic!("open: {error}"),
        };
        let first = match document.render_page(0) {
            Ok(page) => page,
            Err(error) => panic!("render: {error}"),
        };
        let second = match document.render_page(0) {
            Ok(page) => page,
            Err(error) => panic!("render: {error}"),
        };
        assert_eq!(first.image.pixels(), second.image.pixels());
    }
}
