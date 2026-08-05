// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The PP-StructureV3 engine: provisioned models composed over the pure
//! orchestration chain.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase D — the engine.
//! This mirrors `pipeline_v2.predict` in the mode this port supports: formula,
//! seal, and chart recognition **off**, region detection **off** (no exported
//! artifact exists; upstream's flag substitutes an empty region list), table
//! recognition on when its models are supplied, and the general OCR pipeline
//! beneath everything. The stages compose exactly the pieces earlier slices
//! pinned:
//!
//! 1. decode (PNG/JPEG, BGR) and optional document preprocessing;
//! 2. layout detection — `PP-DocLayout_plus-L` through the pinned
//!    preprocessing in [`crate::layout`], **RGB** input, threshold `0.5`;
//! 3. full-page OCR through the classic pipeline; span labels are all `text`;
//! 4. `gather_imgs`: image-labelled crops become `imgs/…` paths, injected
//!    into the table OCR content exactly as upstream injects them;
//! 5. per-table recognition through [`crate::table_engine::TableEngine`];
//! 6. the pure chain: `standardized_data` (with the two model calls served by
//!    a crop recognizer over the real page), block assembly, the nested
//!    ordering, and the `result_v2` Markdown page.
//!
//! Coordinates in the result are in the **preprocessed** page's space, which
//! is also upstream's contract: `predict` never maps back to the source image.
#![allow(dead_code)]

use crate::api::{Artifacts, Dictionary, OcrEngine, OcrOptions};
use crate::backend::{BackendTensor, InferenceBackend};
use crate::crop::InterleavedImage;
use crate::error::{Error, Result};
use crate::layout::{LAYOUT_INPUT_SIDE, LAYOUT_THRESHOLD, layout_input, layout_scale_factor};
use crate::markdown_v2::{MarkdownOptions, convert_markdown_page};
use crate::structure_assembly::{
    AssembledBlock, DEFAULT_MARKDOWN_IGNORE_LABELS, assemble_layout_parsing,
};
use crate::structure_glue::GlueBlock;
use crate::structure_standardize::{OcrData, TextRecognizer, standardized_data};
use crate::table_engine::{TableArtifacts, TableEngine, TableImage};

/// The artifacts a [`StructureEngine`] loads.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct StructureArtifacts<'a> {
    /// The classic OCR artifacts: runtime library, detector, recognizer, and
    /// any optional preprocessing models.
    pub ocr: Artifacts<'a>,
    /// `PP-DocLayout_plus-L`.
    pub layout: &'a str,
    /// The table trio; `None` runs with table recognition off, and table
    /// blocks render as image references.
    pub table: Option<TableArtifacts<'a>>,
}

impl<'a> StructureArtifacts<'a> {
    /// Names the OCR artifacts and the layout model; no table recognition.
    #[must_use]
    pub const fn new(ocr: Artifacts<'a>, layout: &'a str) -> Self {
        Self {
            ocr,
            layout,
            table: None,
        }
    }

    /// Adds the table trio, turning table recognition on.
    #[must_use]
    pub const fn with_table(mut self, table: TableArtifacts<'a>) -> Self {
        self.table = Some(table);
        self
    }
}

/// How a page is parsed.
#[derive(Clone, Debug)]
pub struct StructureOptions {
    /// The OCR thresholds and document preprocessing switches.
    pub ocr: OcrOptions,
    /// The recognition score a re-recognized crop must reach, upstream's
    /// `text_rec_score_thresh` (default `0.0`).
    pub text_rec_score_thresh: f64,
    /// Render the Markdown with upstream's `pretty` HTML (default) or plain.
    pub pretty: bool,
}

impl StructureOptions {
    /// Upstream's defaults over the given OCR options.
    #[must_use]
    pub fn new(ocr: OcrOptions) -> Self {
        Self {
            ocr,
            text_rec_score_thresh: 0.0,
            pretty: true,
        }
    }
}

/// One block of the parsed page, in reading order.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StructureBlock {
    /// The final label, after ordering relabels.
    pub label: String,
    /// `[x1, y1, x2, y2]` in the preprocessed page's pixels.
    pub bbox: [i64; 4],
    /// Table HTML or assembled text.
    pub content: String,
    /// Position in reading order.
    pub index: usize,
    /// One-based reading number over the visualized labels.
    pub order_index: Option<u32>,
    /// The `imgs/…` path for visual blocks.
    pub image_path: Option<String>,
}

/// One parsed page.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct StructureResult {
    /// The ordered blocks.
    pub blocks: Vec<StructureBlock>,
    /// The page's Markdown, in the requested variant.
    pub markdown: String,
    /// Every image path the page references: the Markdown's own, then the
    /// document-level crops, first-seen order, deduplicated.
    pub image_paths: Vec<String>,
    /// `(first_block_starts_fresh, last_block_ends)` for cross-page merging.
    pub continuation_flags: (bool, bool),
    /// The preprocessed page's width and height in pixels.
    pub page_size: (u32, u32),
    /// The blocks as assembly produced them, kept for the JSON writer.
    assembled: Vec<AssembledBlock>,
}

impl StructureResult {
    /// Serialises the ordered blocks as `paddleocr-rust/parsing-result/v1`.
    ///
    /// Byte-deterministic, and the Markdown is deliberately not a field: it is
    /// a second representation of the same blocks, and a document carrying
    /// both would invite a consumer to treat them as independent.
    #[must_use]
    pub fn to_json(&self, id: Option<&str>) -> String {
        crate::structure_json::parsing_to_json(
            &self.assembled,
            self.page_size.0,
            self.page_size.1,
            id,
        )
    }
}

/// Loaded structure models, usable from one thread.
pub struct StructureEngine {
    ocr: OcrEngine,
    layout: crate::backend_ort::OrtBackend,
    table: Option<TableEngine>,
}

impl std::fmt::Debug for StructureEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StructureEngine")
            .field("table", &self.table.is_some())
            .finish_non_exhaustive()
    }
}

/// `BLOCK_LABEL_MAP["image_labels"]`, the crops `gather_imgs` collects.
fn is_gathered_image_label(label: &str) -> bool {
    matches!(label, "image" | "figure" | "seal")
}

/// Swaps a three-channel image between `BGR` and `RGB`.
fn swap_channels(image: &InterleavedImage) -> Result<InterleavedImage> {
    let mut pixels = image.pixels().to_vec();
    for pixel in pixels.chunks_exact_mut(3) {
        pixel.swap(0, 2);
    }
    InterleavedImage::new(image.dimensions(), image.channels(), pixels)
}

/// Crops `[x1, y1, x2, y2]` out of a page, clamped to its bounds.
///
/// Returns `None` for an empty rectangle — upstream would hand the model a
/// zero-sized array and crash; refusing quietly is this port's bounded answer.
fn crop_page(page: &InterleavedImage, rect: [i64; 4]) -> Option<InterleavedImage> {
    let width = i64::from(page.dimensions().width());
    let height = i64::from(page.dimensions().height());
    let x1 = rect[0].clamp(0, width);
    let x2 = rect[2].clamp(0, width);
    let y1 = rect[1].clamp(0, height);
    let y2 = rect[3].clamp(0, height);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    let channels = usize::from(page.channels());
    let row_stride = width as usize * channels;
    let mut pixels = Vec::with_capacity(((x2 - x1) * (y2 - y1)) as usize * channels);
    for row in y1..y2 {
        let start = row as usize * row_stride + x1 as usize * channels;
        let end = row as usize * row_stride + x2 as usize * channels;
        pixels.extend_from_slice(&page.pixels()[start..end]);
    }
    let dimensions = crate::types::ImageDimensions::new((x2 - x1) as u32, (y2 - y1) as u32).ok()?;
    InterleavedImage::new(dimensions, page.channels(), pixels).ok()
}

/// The real `text_rec_model`: crops the page and runs the recognizer.
///
/// The trait cannot return an error, so the first failure is stored and
/// re-raised by the engine after `standardized_data` returns; the failing
/// call reports an impossible score so no branch acts on it.
struct CropRecognizer<'a> {
    page: &'a InterleavedImage,
    ocr: &'a OcrEngine,
    options: &'a OcrOptions,
    error: Option<Error>,
}

impl CropRecognizer<'_> {
    fn try_recognize(&self, rect: [i64; 4]) -> Result<Option<(String, f64)>> {
        let Some(crop) = crop_page(self.page, rect) else {
            return Ok(None);
        };
        let (backend, contract, dictionary) = self.ocr.recognizer_parts();
        let schedule = self.options.control.begin();
        let lines =
            crate::recognizer::recognize(backend, contract, dictionary, &[&crop], &schedule)?;
        Ok(lines.into_iter().next().map(|line| (line.text, line.score)))
    }
}

impl TextRecognizer for CropRecognizer<'_> {
    fn recognize(&mut self, crop: [i64; 4]) -> (String, f64) {
        match self.try_recognize(crop) {
            Ok(Some(result)) => result,
            Ok(None) => (String::new(), f64::NEG_INFINITY),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                (String::new(), f64::NEG_INFINITY)
            }
        }
    }
}

impl StructureEngine {
    /// Loads every artifact and opens its session.
    pub fn load(artifacts: &StructureArtifacts<'_>, dictionary: &Dictionary) -> Result<Self> {
        use crate::backend::{AxisExtent, ModelArtifact, ModelContract, RunBudget, TensorContract};
        use crate::backend_ort::OrtBackend;

        let ocr = OcrEngine::load(&artifacts.ocr, dictionary)?;

        // The layout input is `1x3x800x800` — 1,920,000 elements.
        let budget = RunBudget::new(4_194_304, 4_194_304, 1)?;
        let layout_contract = ModelContract::new(
            ModelArtifact::new(artifacts.layout, crate::api::UNCHECKED_DIGEST)?,
            TensorContract::new("image", vec![AxisExtent::Fixed(1)])?,
            TensorContract::new("fetch_name_0", vec![AxisExtent::Fixed(1)])?,
            budget,
        );
        let mut digest = crate::api::UncheckedDigest;
        let layout = OrtBackend::load(&layout_contract, &mut digest, 1, 1)?;

        let table = match &artifacts.table {
            Some(table) => Some(TableEngine::load(table)?),
            None => None,
        };

        Ok(Self { ocr, layout, table })
    }

    /// Detects layout regions on an RGB page.
    fn detect_layout(&self, page_rgb: &InterleavedImage) -> Result<Vec<GlueBlock>> {
        let tensor = layout_input(page_rgb)?;
        let image = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
        let side = LAYOUT_INPUT_SIDE as f32;
        let shape = BackendTensor::new(vec![1, 2], vec![side, side])?;
        let factor = layout_scale_factor(page_rgb.dimensions());
        let scale = BackendTensor::new(vec![1, 2], factor.to_vec())?;

        let outputs = self.layout.run_named(&[
            ("image", &image),
            ("im_shape", &shape),
            ("scale_factor", &scale),
        ])?;
        let detections = outputs
            .iter()
            .find(|(_, tensor)| tensor.shape().len() == 2 && tensor.shape()[1] == 6)
            .ok_or(Error::Model {
                problem: crate::error::ModelProblem::TensorContract,
            })?;
        let regions = crate::layout::layout_regions(
            detections.1.shape(),
            detections.1.values(),
            LAYOUT_THRESHOLD,
        )?;
        Ok(regions
            .iter()
            .map(|region| GlueBlock {
                label: region.label().to_owned(),
                coordinate: region.box_ltrb.map(f64::from),
                score: f64::from(region.score),
            })
            .collect())
    }

    /// Parses one encoded page image into ordered blocks and Markdown.
    pub fn parse_image(&self, image: &[u8], options: &StructureOptions) -> Result<StructureResult> {
        let encoded = crate::types::EncodedImage::new(image)?;
        let decoded = crate::image::decode_classic_bgr(encoded)?;
        let preprocessed = self.ocr.preprocess_document(decoded, &options.ocr)?;
        let page_bgr = preprocessed.image();
        let page_rgb = swap_channels(page_bgr)?;
        let dimensions = page_bgr.dimensions();
        let (page_width, page_height) = (dimensions.width(), dimensions.height());

        // Layout detection.
        let layout_blocks = self.detect_layout(&page_rgb)?;

        // Full-page OCR; every span is labelled text, as upstream stamps it.
        let lines = crate::pipeline::run_classic_ocr(
            &self.ocr.classic_models(),
            page_bgr,
            crate::pipeline::ClassicThresholds {
                box_threshold: options.ocr.box_threshold,
                unclip_ratio: options.ocr.unclip_ratio,
                drop_score: options.ocr.drop_score,
                orientation_threshold: options.ocr.orientation_threshold,
            },
            &options.ocr.control.begin(),
        )?;
        let mut ocr = OcrData::default();
        for line in &lines {
            let corners = line.quadrilateral.points();
            let poly = corners.map(|point| [f64::from(point.x()), f64::from(point.y())]);
            let bbox = [
                poly.iter().fold(f64::INFINITY, |low, p| low.min(p[0])),
                poly.iter().fold(f64::INFINITY, |low, p| low.min(p[1])),
                poly.iter()
                    .fold(f64::NEG_INFINITY, |high, p| high.max(p[0])),
                poly.iter()
                    .fold(f64::NEG_INFINITY, |high, p| high.max(p[1])),
            ];
            ocr.dt_polys.push(poly);
            ocr.rec_polys.push(poly);
            ocr.rec_boxes.push(bbox);
            ocr.rec_texts.push(line.text.clone());
            ocr.rec_scores.push(line.score);
            ocr.rec_labels.push("text".to_owned());
        }

        // `gather_imgs`: image-labelled crops with clamped integer bounds.
        let mut imgs_in_doc: Vec<(String, [i64; 4], f64)> = Vec::new();
        for block in &layout_blocks {
            if !is_gathered_image_label(&block.label) {
                continue;
            }
            let [x1, y1, x2, y2] = block.coordinate.map(|value| value.trunc() as i64);
            let x1 = x1.clamp(0, i64::from(page_width));
            let x2 = x2.clamp(0, i64::from(page_width));
            let y1 = y1.clamp(0, i64::from(page_height));
            let y2 = y2.clamp(0, i64::from(page_height));
            if x2 <= x1 || y2 <= y1 {
                continue;
            }
            // `construct_img_path` truncates the raw coordinate, not the clamp.
            let [px1, py1, px2, py2] = block.coordinate.map(|value| value.trunc() as i64);
            let path = format!(
                "imgs/img_in_{}_box_{px1}_{py1}_{px2}_{py2}.jpg",
                block.label
            );
            imgs_in_doc.push((path, [x1, y1, x2, y2], block.score));
        }

        // Table recognition, over the OCR content plus the injected image
        // spans, exactly as `predict` builds `table_contents_for_img`.
        let mut table_html: Vec<String> = Vec::new();
        if let Some(engine) = &self.table {
            let mut content_boxes: Vec<[f64; 4]> = ocr.rec_boxes.clone();
            let mut content_texts: Vec<String> = ocr.rec_texts.clone();
            for (path, rect, _) in &imgs_in_doc {
                content_boxes.push(rect.map(|v| v as f64));
                content_texts.push(format!(
                    "<div style=\"text-align: center;\"><img src=\"{path}\" alt=\"Image\" /></div>"
                ));
            }
            for block in &layout_blocks {
                if block.label != "table" {
                    continue;
                }
                let rect = block.coordinate.map(|value| value.trunc() as i64);
                let Some(crop_bgr) = crop_page(page_bgr, rect) else {
                    continue;
                };
                let crop_rgb = swap_channels(&crop_bgr)?;
                let crop_dimensions = crop_bgr.dimensions();
                let to_table_image = |image: InterleavedImage| -> Result<TableImage> {
                    TableImage::new(
                        crop_dimensions.width(),
                        crop_dimensions.height(),
                        image.pixels().to_vec(),
                    )
                };
                let table = engine.recognize_table(
                    &to_table_image(crop_rgb)?,
                    &to_table_image(crop_bgr)?,
                    block.coordinate.map(f64::from),
                    &content_boxes,
                    &content_texts,
                )?;
                table_html.push(table.html);
            }
        }

        // The pure chain: reconcile, assemble, order, number, render.
        let mut recognizer = CropRecognizer {
            page: page_bgr,
            ocr: &self.ocr,
            options: &options.ocr,
            error: None,
        };
        let standardized = standardized_data(
            f64::from(page_width),
            f64::from(page_height),
            &layout_blocks,
            &[],
            ocr,
            &mut recognizer,
            options.text_rec_score_thresh,
        );
        if let Some(error) = recognizer.error {
            return Err(error);
        }
        let assembled =
            assemble_layout_parsing(&standardized, &table_html, &DEFAULT_MARKDOWN_IGNORE_LABELS);
        let markdown = convert_markdown_page(
            &assembled,
            &MarkdownOptions {
                pretty: options.pretty,
                use_table_recognition: self.table.is_some(),
                original_image_width: i64::from(page_width),
                markdown_ignore_labels: &DEFAULT_MARKDOWN_IGNORE_LABELS,
            },
        );

        // `_to_markdown` appends `imgs_in_doc` to the collected images.
        let mut image_paths = markdown.image_paths;
        for (path, _, _) in &imgs_in_doc {
            if !image_paths.contains(path) {
                image_paths.push(path.clone());
            }
        }

        let blocks = assembled
            .iter()
            .map(|block: &AssembledBlock| StructureBlock {
                label: block.label.clone(),
                bbox: block.bbox,
                content: block.content.clone(),
                index: block.index,
                order_index: block.order_index,
                image_path: block.image_path.clone(),
            })
            .collect();

        Ok(StructureResult {
            blocks,
            markdown: markdown.markdown,
            image_paths,
            continuation_flags: markdown.continuation_flags,
            page_size: (page_width, page_height),
            assembled,
        })
    }
}
