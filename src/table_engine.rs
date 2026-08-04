// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Running the three table models in order against real artifacts.
//!
//! Roadmap item `TABLEPIPE-001`, orchestration. The behaviour was frozen and
//! matched in [`crate::table_pipeline`]; this is the plumbing that was named as
//! what remained.
//!
//! # Why this needed a backend change rather than a wrapper
//!
//! Every model this project had loaded until now takes **one** input named `x`
//! and emits **one** output. The table models do not:
//!
//! | Model | Inputs | Outputs |
//! |---|---|---|
//! | `PP-LCNet_x1_0_table_cls` | `x` | `1` |
//! | `RT-DETR-L_*_table_cell_det` | `image`, `im_shape`, `scale_factor` | `1` |
//! | `SLANeXt_wired` | `x` | **`2`** — boxes and token probabilities |
//!
//! So `InferenceBackend` grew `run_named`, which takes named inputs and returns
//! every named output. It **defaults to a refusal** rather than to a
//! single-input call: a backend that has not implemented it says so instead of
//! quietly running the wrong graph.
//!
//! # What this engine does not do
//!
//! It does not run OCR. Text boxes and strings come in as arguments, because
//! `OcrEngine` already produces them and duplicating its artifact handling here
//! would mean two ways to load a detector. Composing the two is a caller's job
//! until `STRUCT-001` decides what a full-page structured result contains.
//!
//! # One engine per thread
//!
//! Sessions live behind `RefCell`, so this type is `!Sync` by construction —
//! the same property `CONC_001_EVIDENCE.md` records for `OcrEngine`, enforced by
//! the compiler rather than by documentation.
#![allow(dead_code)]

use crate::backend::{
    AxisExtent, BackendTensor, InferenceBackend, ModelArtifact, ModelContract, TensorContract,
};
use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, ModelProblem, Result};
use crate::table_cells::{TABLE_CELL_INPUT_SIDE, table_cell_input, table_cell_scale_factor};
use crate::table_classification::{TABLE_CLS_LABELS, table_classification_input};
use crate::table_pipeline::{
    Box as TableBox, TABLE_CELL_DETECTION_THRESHOLD, TABLE_CELL_NMS_THRESHOLD, TableRoute,
    align_row_flags, crop_ocr_boxes_to_table, match_cells_to_ocr, row_start_indices,
    sort_cell_boxes, suppress_overlapping_cells, table_html, table_route,
};
use crate::table_structure::{
    TABLE_STRUCTURE_PAD_SIDE, TableStructureModel, decode_table_structure, table_structure_input,
    table_structure_vocabulary,
};

/// The artifacts a [`TableEngine`] loads.
///
/// Paths only; digests are optional and follow `Artifacts`' convention of being
/// checked when supplied and recorded as unchecked when not.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct TableArtifacts<'a> {
    /// The ONNX Runtime shared library.
    pub library: &'a str,
    /// `PP-LCNet_x1_0_table_cls`.
    pub classifier: &'a str,
    /// `RT-DETR-L_wired_table_cell_det` or its wireless twin.
    pub cell_detector: &'a str,
    /// `SLANeXt_wired` or its wireless twin.
    pub structure: &'a str,
    /// Which pair the cell detector and structure model belong to.
    pub route: TableRoute,
}

impl<'a> TableArtifacts<'a> {
    /// Names the three artifacts and the route they serve.
    #[must_use]
    pub const fn new(
        library: &'a str,
        classifier: &'a str,
        cell_detector: &'a str,
        structure: &'a str,
        route: TableRoute,
    ) -> Self {
        Self {
            library,
            classifier,
            cell_detector,
            structure,
            route,
        }
    }
}

/// One recognized table.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TableResult {
    /// Which route the classifier selected.
    pub route: TableRoute,
    /// The classifier's score for that route.
    pub route_score: f32,
    /// The assembled HTML.
    pub html: String,
    /// The detected cell boxes, in the table image's coordinates.
    pub cell_boxes: Vec<TableBox>,
    /// The structure tokens, before assembly.
    pub tokens: Vec<String>,
}

/// An interleaved three-channel image handed to a [`TableEngine`].
///
/// A newtype rather than a re-export: `InterleavedImage` is internal, and
/// widening the public surface to it would expose every other module's use of
/// it too. The channel order is the caller's to state, because the three models
/// disagree about it — see [`TableEngine::recognize_table`].
#[derive(Clone, Debug)]
pub struct TableImage {
    inner: InterleavedImage,
}

impl TableImage {
    /// Builds an image from interleaved three-channel pixels.
    ///
    /// `pixels` must be exactly `width * height * 3` bytes.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        let dimensions = crate::types::ImageDimensions::new(width, height)?;
        Ok(Self {
            inner: InterleavedImage::new(dimensions, 3, pixels)?,
        })
    }

    /// The image's dimensions.
    #[must_use]
    pub fn dimensions(&self) -> crate::types::ImageDimensions {
        self.inner.dimensions()
    }

    fn inner(&self) -> &InterleavedImage {
        &self.inner
    }
}

impl TableResult {
    /// Serialises this result as `paddleocr-rust/table-result/v1`.
    ///
    /// Byte-deterministic, for the reason `src/result_json.rs` records: fields
    /// appear in a fixed order with fixed numeric formatting, so two runs over
    /// the same input produce the same bytes.
    ///
    /// `width` and `height` describe the **table crop**, not the page. The cell
    /// boxes are in that crop's coordinates, and saying so in the document is
    /// what keeps a consumer from placing them on the wrong image.
    #[must_use]
    pub fn to_json(&self, width: u32, height: u32, id: Option<&str>) -> String {
        let route = match self.route {
            TableRoute::Wired => TABLE_CLS_LABELS[0],
            TableRoute::Wireless => TABLE_CLS_LABELS[1],
        };
        crate::structure_json::table_to_json(
            &crate::structure_json::TableDocument {
                route,
                route_score: self.route_score,
                html: &self.html,
                cell_boxes: &self.cell_boxes,
                tokens: &self.tokens,
            },
            width,
            height,
            id,
        )
    }
}

/// Three loaded table models, usable from one thread.
///
/// `Debug` prints the route and the loaded vocabulary size and nothing else:
/// the sessions hold native handles that have no useful textual form, and a
/// derived implementation would leak paths into logs.
pub struct TableEngine {
    classifier: crate::backend_ort::OrtBackend,
    classifier_contract: ModelContract,
    cells: crate::backend_ort::OrtBackend,
    structure: crate::backend_ort::OrtBackend,
    route: TableRoute,
    vocabulary: Vec<String>,
}

/// The structure model's `character_dict`, from `SLANeXt_wired/inference.yml`.
///
/// Committed rather than read from the artifact for the reason `LANG-001`
/// records about dictionaries: a contract this port claims to implement has to
/// be in this repository, not only in a file a user supplies.
pub(crate) const TABLE_STRUCTURE_CHARACTER_DICT: [&str; 48] = [
    "<thead>",
    "</thead>",
    "<tbody>",
    "</tbody>",
    "<tr>",
    "</tr>",
    "<td>",
    "<td",
    ">",
    "</td>",
    " colspan=\"2\"",
    " colspan=\"3\"",
    " colspan=\"4\"",
    " colspan=\"5\"",
    " colspan=\"6\"",
    " colspan=\"7\"",
    " colspan=\"8\"",
    " colspan=\"9\"",
    " colspan=\"10\"",
    " colspan=\"11\"",
    " colspan=\"12\"",
    " colspan=\"13\"",
    " colspan=\"14\"",
    " colspan=\"15\"",
    " colspan=\"16\"",
    " colspan=\"17\"",
    " colspan=\"18\"",
    " colspan=\"19\"",
    " colspan=\"20\"",
    " rowspan=\"2\"",
    " rowspan=\"3\"",
    " rowspan=\"4\"",
    " rowspan=\"5\"",
    " rowspan=\"6\"",
    " rowspan=\"7\"",
    " rowspan=\"8\"",
    " rowspan=\"9\"",
    " rowspan=\"10\"",
    " rowspan=\"11\"",
    " rowspan=\"12\"",
    " rowspan=\"13\"",
    " rowspan=\"14\"",
    " rowspan=\"15\"",
    " rowspan=\"16\"",
    " rowspan=\"17\"",
    " rowspan=\"18\"",
    " rowspan=\"19\"",
    " rowspan=\"20\"",
];

impl std::fmt::Debug for TableEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TableEngine")
            .field("route", &self.route)
            .field("vocabulary", &self.vocabulary.len())
            .finish_non_exhaustive()
    }
}

impl TableEngine {
    /// Loads the three artifacts and opens a session for each.
    pub fn load(artifacts: &TableArtifacts<'_>) -> Result<Self> {
        use crate::backend_ort::{OrtBackend, initialize_runtime};

        initialize_runtime(std::path::Path::new(artifacts.library))?;

        // Generous but finite: the largest of these tensors is the structure
        // model's `512x512x3` input at `786,432` elements.
        let budget = crate::backend::RunBudget::new(4_194_304, 4_194_304, 1)?;
        let classifier_contract = ModelContract::new(
            ModelArtifact::new(artifacts.classifier, crate::api::UNCHECKED_DIGEST)?,
            TensorContract::new(
                "x",
                vec![
                    AxisExtent::Fixed(1),
                    AxisExtent::Fixed(3),
                    AxisExtent::Fixed(224),
                    AxisExtent::Fixed(224),
                ],
            )?,
            TensorContract::new(
                "fetch_name_0",
                vec![
                    AxisExtent::Fixed(1),
                    AxisExtent::Fixed(TABLE_CLS_LABELS.len()),
                ],
            )?,
            budget,
        );

        let mut digest = crate::api::UncheckedDigest;
        let classifier = OrtBackend::load(&classifier_contract, &mut digest, 1, 1)?;

        // The cell detector and the structure model carry a contract only so
        // that `OrtBackend::load` has an artifact to verify. Their shapes are
        // checked at the call site instead: both have input or output arities
        // the single-tensor contract type cannot describe, and widening that
        // type to fit them would weaken it for the models it does describe.
        //
        // The cell detector emits `fetch_name_0` at `[300N, 6]` and a second
        // output this port does not read; the structure model emits
        // `[N, T, 8]` boxes and `[N, T, 50]` probabilities. Both are told apart
        // by shape rather than by name or order, because the export guarantees
        // neither.
        let cells_contract = ModelContract::new(
            ModelArtifact::new(artifacts.cell_detector, crate::api::UNCHECKED_DIGEST)?,
            TensorContract::new("image", vec![AxisExtent::Fixed(1)])?,
            TensorContract::new("fetch_name_0", vec![AxisExtent::Fixed(1)])?,
            budget,
        );
        let cells = OrtBackend::load(&cells_contract, &mut digest, 1, 1)?;

        let structure_contract = ModelContract::new(
            ModelArtifact::new(artifacts.structure, crate::api::UNCHECKED_DIGEST)?,
            TensorContract::new("x", vec![AxisExtent::Fixed(1)])?,
            TensorContract::new("fetch_name_0", vec![AxisExtent::Fixed(1)])?,
            budget,
        );
        let structure = OrtBackend::load(&structure_contract, &mut digest, 1, 1)?;

        Ok(Self {
            classifier,
            classifier_contract,
            cells,
            structure,
            route: artifacts.route,
            vocabulary: table_structure_vocabulary(&TABLE_STRUCTURE_CHARACTER_DICT, true),
        })
    }

    /// Classifies a table image as wired or wireless.
    ///
    /// The classifier reads `RGB`; the caller supplies it in that order.
    pub fn classify(&self, table_rgb: &TableImage) -> Result<(TableRoute, f32)> {
        let tensor = table_classification_input(table_rgb.inner())?;
        let input = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
        let scores = crate::backend::run_validated(
            &self.classifier as &dyn InferenceBackend,
            &self.classifier_contract,
            &input,
        )?;
        let ranked = crate::table_classification::rank_table_scores(scores.values());
        let (index, score) = *ranked.first().ok_or(Error::Model {
            problem: ModelProblem::TensorContract,
        })?;
        let label = TABLE_CLS_LABELS.get(index).ok_or(Error::Model {
            problem: ModelProblem::TensorContract,
        })?;
        table_route(label)
            .map(|route| (route, score))
            .ok_or(Error::Model {
                problem: ModelProblem::TensorContract,
            })
    }

    /// Detects table cells and suppresses overlaps.
    ///
    /// Uses the **pipeline's** `0.3` threshold, not the artifact's `0.5`.
    pub fn detect_cells(&self, table_rgb: &TableImage) -> Result<Vec<TableBox>> {
        let tensor = table_cell_input(table_rgb.inner())?;
        let image = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
        let side = f32::from(
            u16::try_from(TABLE_CELL_INPUT_SIDE).map_err(|_| Error::Backend {
                message: "the cell detector input side does not fit a u16",
            })?,
        );
        let shape = BackendTensor::new(vec![1, 2], vec![side, side])?;
        let factor = table_cell_scale_factor(table_rgb.dimensions());
        let scale = BackendTensor::new(vec![1, 2], factor.to_vec())?;

        let outputs = self.cells.run_named(&[
            ("image", &image),
            ("im_shape", &shape),
            ("scale_factor", &scale),
        ])?;
        let detections = outputs
            .iter()
            .find(|(_, tensor)| tensor.shape().len() == 2 && tensor.shape()[1] == 6)
            .ok_or(Error::Model {
                problem: ModelProblem::TensorContract,
            })?;

        let cells = crate::table_cells::table_cells(
            detections.1.shape(),
            detections.1.values(),
            TABLE_CELL_DETECTION_THRESHOLD,
        )?;
        let boxes: Vec<TableBox> = cells
            .iter()
            .map(|cell| {
                [
                    f64::from(cell.box_ltrb[0]),
                    f64::from(cell.box_ltrb[1]),
                    f64::from(cell.box_ltrb[2]),
                    f64::from(cell.box_ltrb[3]),
                ]
            })
            .collect();
        let scores: Vec<f32> = cells.iter().map(|cell| cell.score).collect();
        let (kept, _) = suppress_overlapping_cells(&boxes, &scores, TABLE_CELL_NMS_THRESHOLD);
        Ok(kept)
    }

    /// Recognizes the table's structure tokens.
    ///
    /// The structure model reads **`BGR`**, unlike the other two.
    pub fn recognize_structure(&self, table_bgr: &TableImage) -> Result<Vec<String>> {
        let tensor = table_structure_input(table_bgr.inner())?;
        let input = BackendTensor::new(tensor.shape().to_vec(), tensor.values().to_vec())?;
        let outputs = self.structure.run_named(&[("x", &input)])?;

        // Two outputs: `[1, T, 8]` boxes and `[1, T, V]` probabilities. They are
        // told apart by their last axis against the vocabulary size rather than
        // by name or order, because neither is guaranteed by the export.
        let classes = self.vocabulary.len();
        let probabilities = outputs
            .iter()
            .find(|(_, tensor)| tensor.shape().len() == 3 && tensor.shape()[2] == classes)
            .ok_or(Error::Model {
                problem: ModelProblem::TensorContract,
            })?;
        let boxes = outputs
            .iter()
            .find(|(_, tensor)| tensor.shape().len() == 3 && tensor.shape()[2] == 8);

        let sequence = probabilities.1.shape()[1];
        let model = match self.route {
            TableRoute::Wired | TableRoute::Wireless => TableStructureModel::SlaNeXt,
        };
        let dimensions = table_bgr.dimensions();
        let decoded = decode_table_structure(
            model,
            &self.vocabulary,
            probabilities.1.values(),
            sequence,
            boxes.map(|(_, tensor)| tensor.values()),
            (
                f64::from(TABLE_STRUCTURE_PAD_SIDE),
                f64::from(TABLE_STRUCTURE_PAD_SIDE),
            ),
            (
                f64::from(dimensions.width()),
                f64::from(dimensions.height()),
            ),
        )?;
        Ok(decoded.tokens)
    }

    /// Runs all three models and assembles the HTML.
    ///
    /// `table_rgb` and `table_bgr` are the same crop in the two channel orders
    /// the models disagree about. Taking both rather than converting internally
    /// keeps the conversion where a caller can see it — the swap is what
    /// `docs/TABLE_CELLS_CONTRACT.md` records going wrong once already.
    ///
    /// `ocr_boxes` are in the **page's** coordinates and `table_box` says where
    /// the crop came from; boxes not fully inside it are dropped, as upstream
    /// drops them.
    pub fn recognize_table(
        &self,
        table_rgb: &TableImage,
        table_bgr: &TableImage,
        table_box: TableBox,
        ocr_boxes: &[TableBox],
        ocr_texts: &[String],
    ) -> Result<TableResult> {
        if ocr_boxes.len() != ocr_texts.len() {
            return Err(Error::InvalidInput {
                field: "table_engine.ocr",
                violation: InputViolation::OutOfRange,
            });
        }
        if table_rgb.dimensions() != table_bgr.dimensions() {
            return Err(Error::InvalidInput {
                field: "table_engine.crop_dimensions",
                violation: InputViolation::OutOfRange,
            });
        }

        let (route, route_score) = self.classify(table_rgb)?;
        if route != self.route {
            // The loaded pair does not serve the route the classifier chose.
            // Refusing beats running the wrong structure model against a table
            // it was not trained for.
            return Err(Error::InvalidInput {
                field: "table_engine.route",
                violation: InputViolation::OutOfRange,
            });
        }

        let cells = self.detect_cells(table_rgb)?;
        let tokens = self.recognize_structure(table_bgr)?;

        let cropped = crop_ocr_boxes_to_table(ocr_boxes, table_box);
        // `crop_ocr_boxes_to_table` drops boxes, so the texts must be filtered
        // by the same predicate rather than indexed by position.
        let texts: Vec<String> = ocr_boxes
            .iter()
            .zip(ocr_texts)
            .filter(|(entry, _)| {
                entry[0] >= table_box[0]
                    && entry[1] >= table_box[1]
                    && entry[2] <= table_box[2]
                    && entry[3] <= table_box[3]
            })
            .map(|(_, text)| text.clone())
            .collect();

        let (sorted, flags) = sort_cell_boxes(&cells);
        let starts = row_start_indices(&tokens);
        let mut aligned = align_row_flags(&flags, &starts);
        aligned.push(sorted.len());
        let mut starts_full = starts;
        starts_full.push(sorted.len());

        let matched = match_cells_to_ocr(&sorted, &cropped, &aligned)?;
        let html = table_html(&matched, &texts, &tokens, &starts_full)?;

        Ok(TableResult {
            route,
            route_score,
            html,
            cell_boxes: sorted,
            tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed dictionary must be what the artifact declares.
    ///
    /// Checked against the fixture the structure capture recorded, so a
    /// transcription slip in the 47-entry list fails here rather than emitting
    /// wrong tokens at run time.
    #[test]
    fn the_committed_dictionary_matches_the_captured_vocabulary() {
        const FIXTURE: &str =
            include_str!("../tests/fixtures/classic-v1-table-structure/expected.json");
        let document: serde_json::Value = match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        };
        let captured: Vec<String> = match document["vocabulary"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_str().unwrap_or_default().to_owned())
                .collect(),
            None => panic!("vocabulary"),
        };
        let built = table_structure_vocabulary(&TABLE_STRUCTURE_CHARACTER_DICT, true);
        assert_eq!(
            built, captured,
            "the committed character_dict must round-trip"
        );
    }

    /// A result serialises to the frozen schema, deterministically.
    #[test]
    fn a_table_result_serialises_to_the_frozen_schema() {
        let result = TableResult {
            route: TableRoute::Wired,
            route_score: 0.95067,
            html: "<html><body><table></table></body></html>".to_owned(),
            cell_boxes: vec![[0.0, 0.0, 50.0, 20.0]],
            tokens: vec!["<html>".to_owned()],
        };
        let json = result.to_json(480, 320, Some("t0"));
        assert!(
            json.starts_with("{\"schema_version\":\"paddleocr-rust/table-result/v1\""),
            "{json}"
        );
        assert!(json.contains("\"label\":\"wired_table\""), "{json}");
        assert_eq!(json, result.to_json(480, 320, Some("t0")));

        let wireless = TableResult {
            route: TableRoute::Wireless,
            ..result
        };
        assert!(
            wireless
                .to_json(1, 1, None)
                .contains("\"label\":\"wireless_table\""),
        );
    }

    /// The dictionary is the artifact's length, not a truncation of it.
    #[test]
    fn the_dictionary_has_the_artifacts_entry_count() {
        assert_eq!(TABLE_STRUCTURE_CHARACTER_DICT.len(), 48);
        assert_eq!(TABLE_STRUCTURE_CHARACTER_DICT[6], "<td>");
        assert_eq!(TABLE_STRUCTURE_CHARACTER_DICT[7], "<td");
    }

    /// One engine per thread, enforced by the compiler.
    ///
    /// The same probe `api::concurrency_position` uses: the inherent constant
    /// wins over the trait default only when `T: Sync`, which is the stable way
    /// to observe an auto trait without a compile failure.
    ///
    /// An earlier version of this test implemented `NotSync` for `T: ?Sized`
    /// rather than for the probe, which made the constant unconditionally
    /// `false` and the assertion vacuous. Clippy caught it. The second
    /// assertion below exists so it cannot happen again: a probe that cannot
    /// see `Sync` at all fails there.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn the_engine_is_not_shareable_between_threads() {
        struct Probe<T>(core::marker::PhantomData<T>);

        trait NotSync {
            const IS_SYNC: bool = false;
        }

        impl<T> NotSync for Probe<T> {}

        impl<T: Sync> Probe<T> {
            const IS_SYNC: bool = true;
        }

        assert!(
            !<Probe<TableEngine>>::IS_SYNC,
            "TableEngine became Sync; one thread must own one set of sessions"
        );
        assert!(<Probe<u32>>::IS_SYNC, "the probe is not detecting Sync");
    }
}
