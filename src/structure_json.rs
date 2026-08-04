// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Versioned JSON for the specialized modules' results.
//!
//! Roadmap item `SPECAPI-001`. `src/result_json.rs` freezes the classic OCR
//! document as `paddleocr-rust/ocr-result/v1`; this is the same discipline for
//! the modules that produce something other than lines of text.
//!
//! Hand-rolled for the reasons `result_json` records — no runtime
//! serialisation dependency, and byte-deterministic output because fields
//! appear in a fixed order with fixed numeric formatting.
//!
//! # A separate schema rather than a widened one
//!
//! A table result is not a page of text lines with extra fields: it has no
//! `lines` array and its coordinates describe a crop, not a page. Widening
//! `ocr-result/v1` to hold it would give consumers a document where half the
//! fields are `null` on any given input, and no way to tell "this producer does
//! not do tables" from "this page had none".
//!
//! # What these documents do not carry
//!
//! No model manifest block. The classic result carries one because
//! `MODEL-DEC-001` ties a text result to the artifacts that produced it; the
//! specialized modules have no equivalent manifest type yet, and inventing one
//! per module would freeze a shape before there is a second module to check it
//! against. The field is **absent rather than null**, which is the one case
//! `result_json`'s own reasoning allows: there is no version of these documents
//! that ever had it.
#![allow(dead_code)]

use crate::layout::LayoutRegion;
use crate::table_pipeline::Box as TableBox;

/// The frozen layout result schema version.
pub const LAYOUT_RESULT_SCHEMA_VERSION: &str = "paddleocr-rust/layout-result/v1";

/// The frozen detection-only result schema version.
///
/// Separate from `ocr-result/v1` because a detection has **no text**, and a
/// document with an always-empty `text` field would invite a consumer to read
/// "" as "nothing was recognized here" rather than "recognition did not run".
pub const DETECTION_RESULT_SCHEMA_VERSION: &str = "paddleocr-rust/detection-result/v1";

/// The frozen table result schema version.
pub const TABLE_RESULT_SCHEMA_VERSION: &str = "paddleocr-rust/table-result/v1";

/// Serialises detected layout regions as one versioned JSON document.
///
/// Boxes are in the source page's coordinates, which is what
/// `crate::layout::layout_regions` returns: the model divides by the supplied
/// scale factor itself.
#[must_use]
pub(crate) fn layout_to_json(
    regions: &[LayoutRegion],
    width: u32,
    height: u32,
    id: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"schema_version\":\"");
    out.push_str(LAYOUT_RESULT_SCHEMA_VERSION);
    out.push_str("\",\"input\":{\"id\":");
    push_id(&mut out, id);
    out.push_str(",\"width\":");
    out.push_str(&width.to_string());
    out.push_str(",\"height\":");
    out.push_str(&height.to_string());
    out.push_str("},\"regions\":[");
    for (index, region) in regions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"label\":");
        push_json_string(&mut out, region.label());
        out.push_str(",\"class\":");
        out.push_str(&region.class.to_string());
        out.push_str(",\"confidence\":");
        push_f32(&mut out, region.score);
        out.push_str(",\"box\":[");
        for (axis, value) in region.box_ltrb.iter().enumerate() {
            if axis > 0 {
                out.push(',');
            }
            push_f32(&mut out, *value);
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// What a table document needs, without depending on the `onnxruntime` feature.
///
/// `TableEngine` lives behind that feature; this schema does not, so that the
/// document shape is testable, reviewable, and frozen whether or not a runtime
/// is compiled in.
#[derive(Clone, Debug)]
pub(crate) struct TableDocument<'a> {
    /// `wired_table` or `wireless_table`.
    pub(crate) route: &'a str,
    /// The classifier's score for that route.
    pub(crate) route_score: f32,
    /// The assembled HTML.
    pub(crate) html: &'a str,
    /// Detected cell boxes, in the table crop's coordinates.
    pub(crate) cell_boxes: &'a [TableBox],
    /// Structure tokens, before assembly.
    pub(crate) tokens: &'a [String],
}

/// Serialises one recognized table as a versioned JSON document.
///
/// The tokens are carried alongside the HTML rather than only inside it,
/// because they are what the model actually produced: the HTML is an assembly
/// step over them and over the OCR text, and a consumer checking a structure
/// prediction should not have to parse HTML to see it.
#[must_use]
pub(crate) fn table_to_json(
    document: &TableDocument<'_>,
    width: u32,
    height: u32,
    id: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"schema_version\":\"");
    out.push_str(TABLE_RESULT_SCHEMA_VERSION);
    out.push_str("\",\"input\":{\"id\":");
    push_id(&mut out, id);
    out.push_str(",\"width\":");
    out.push_str(&width.to_string());
    out.push_str(",\"height\":");
    out.push_str(&height.to_string());
    out.push_str("},\"route\":{\"label\":");
    push_json_string(&mut out, document.route);
    out.push_str(",\"confidence\":");
    push_f32(&mut out, document.route_score);
    out.push_str("},\"html\":");
    push_json_string(&mut out, document.html);
    out.push_str(",\"structure_tokens\":[");
    for (index, token) in document.tokens.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, token);
    }
    out.push_str("],\"cells\":[");
    for (index, cell) in document.cell_boxes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        for (axis, value) in cell.iter().enumerate() {
            if axis > 0 {
                out.push(',');
            }
            push_f64(&mut out, *value);
        }
        out.push(']');
    }
    out.push_str("]}");
    out
}

/// Serialises detected regions as one versioned JSON document.
///
/// `scores` are the **detector's**, and the field is named for that. A
/// consumer comparing them against `ocr-result/v1`'s `confidence` would be
/// comparing two different numbers.
#[must_use]
pub(crate) fn detection_to_json(
    regions: &[([(f32, f32); 4], f64)],
    width: u32,
    height: u32,
    id: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"schema_version\":\"");
    out.push_str(DETECTION_RESULT_SCHEMA_VERSION);
    out.push_str("\",\"input\":{\"id\":");
    push_id(&mut out, id);
    out.push_str(",\"width\":");
    out.push_str(&width.to_string());
    out.push_str(",\"height\":");
    out.push_str(&height.to_string());
    out.push_str("},\"regions\":[");
    for (index, (corners, score)) in regions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"quad\":[");
        for (corner, (x, y)) in corners.iter().enumerate() {
            if corner > 0 {
                out.push(',');
            }
            // Whole pixels, matching `result_json`'s quad formatting so the two
            // documents place the same box at the same coordinates.
            out.push_str(&format!("[{x:.0},{y:.0}]"));
        }
        out.push_str("],\"detector_score\":");
        push_f64(&mut out, *score);
        out.push('}');
    }
    out.push_str("]}");
    out
}

fn push_id(out: &mut String, id: Option<&str>) {
    match id {
        Some(id) => push_json_string(out, id),
        None => out.push_str("null"),
    }
}

/// Ten decimals, the precision `result_json` uses for confidences.
fn push_f32(out: &mut String, value: f32) {
    out.push_str(&format!("{:.10}", f64::from(value)));
}

fn push_f64(out: &mut String, value: f64) {
    out.push_str(&format!("{value:.10}"));
}

/// The same minimal escaping `result_json` applies, for the same reason: the
/// exact Unicode scalars a model produced are what appear in the document.
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions() -> Vec<LayoutRegion> {
        vec![
            LayoutRegion {
                class: 2,
                score: 0.875,
                box_ltrb: [10.0, 20.0, 110.0, 60.0],
            },
            LayoutRegion {
                class: 8,
                score: 0.5,
                box_ltrb: [0.0, 100.0, 200.0, 300.0],
            },
        ]
    }

    #[test]
    fn the_layout_document_shape_is_stable_and_deterministic() {
        let json = layout_to_json(&regions(), 1280, 720, Some("page-1"));
        assert_eq!(
            json,
            "{\"schema_version\":\"paddleocr-rust/layout-result/v1\",\
             \"input\":{\"id\":\"page-1\",\"width\":1280,\"height\":720},\
             \"regions\":[\
             {\"label\":\"text\",\"class\":2,\"confidence\":0.8750000000,\
             \"box\":[10.0000000000,20.0000000000,110.0000000000,60.0000000000]},\
             {\"label\":\"table\",\"class\":8,\"confidence\":0.5000000000,\
             \"box\":[0.0000000000,100.0000000000,200.0000000000,300.0000000000]}]}"
        );
        // Determinism: the same input twice is the same bytes.
        assert_eq!(json, layout_to_json(&regions(), 1280, 720, Some("page-1")));
    }

    #[test]
    fn an_absent_id_is_null_rather_than_omitted() {
        let json = layout_to_json(&[], 4, 5, None);
        assert!(json.contains("\"id\":null"), "{json}");
        assert!(json.ends_with("\"regions\":[]}"), "{json}");
    }

    #[test]
    fn the_table_document_shape_is_stable_and_deterministic() {
        let tokens = vec!["<html>".to_owned(), "<td></td>".to_owned()];
        let cells = [[0.0, 0.0, 50.0, 20.0]];
        let document = TableDocument {
            route: "wired_table",
            route_score: 0.95067,
            html: "<html><body><table></table></body></html>",
            cell_boxes: &cells,
            tokens: &tokens,
        };
        let json = table_to_json(&document, 480, 320, Some("table-0"));
        assert!(json.starts_with(
            "{\"schema_version\":\"paddleocr-rust/table-result/v1\",\
             \"input\":{\"id\":\"table-0\",\"width\":480,\"height\":320},\
             \"route\":{\"label\":\"wired_table\",\"confidence\":0.9506"
        ));
        assert!(json.contains("\"structure_tokens\":[\"<html>\",\"<td></td>\"]"));
        assert!(
            json.ends_with("\"cells\":[[0.0000000000,0.0000000000,50.0000000000,20.0000000000]]}")
        );
        assert_eq!(json, table_to_json(&document, 480, 320, Some("table-0")));
    }

    /// HTML is escaped as a JSON string, not transformed.
    #[test]
    fn html_is_escaped_but_not_rewritten() {
        let tokens: Vec<String> = Vec::new();
        let document = TableDocument {
            route: "wireless_table",
            route_score: 0.5,
            html: "<td>a \"quoted\"\ttab</td>",
            cell_boxes: &[],
            tokens: &tokens,
        };
        let json = table_to_json(&document, 1, 1, None);
        assert!(
            json.contains("\"html\":\"<td>a \\\"quoted\\\"\\ttab</td>\""),
            "{json}"
        );
    }

    #[test]
    fn the_detection_document_shape_is_stable_and_deterministic() {
        let regions = [(
            [(1.0_f32, 2.0_f32), (3.0, 2.0), (3.0, 4.0), (1.0, 4.0)],
            0.875_f64,
        )];
        let json = detection_to_json(&regions, 640, 480, Some("page"));
        assert_eq!(
            json,
            "{\"schema_version\":\"paddleocr-rust/detection-result/v1\",\
             \"input\":{\"id\":\"page\",\"width\":640,\"height\":480},\
             \"regions\":[{\"quad\":[[1,2],[3,2],[3,4],[1,4]],\
             \"detector_score\":0.8750000000}]}"
        );
        assert_eq!(json, detection_to_json(&regions, 640, 480, Some("page")));
    }

    /// The score field is named for whose score it is.
    #[test]
    fn the_detection_document_names_the_score_it_carries() {
        let json = detection_to_json(&[], 1, 1, None);
        assert!(!json.contains("confidence"), "{json}");
        let with_region = detection_to_json(&[([(0.0_f32, 0.0_f32); 4], 0.5_f64)], 1, 1, None);
        assert!(with_region.contains("\"detector_score\""), "{with_region}");
    }

    /// The two schemas are distinct names, not versions of one.
    #[test]
    fn the_two_schemas_are_separate() {
        assert_ne!(LAYOUT_RESULT_SCHEMA_VERSION, TABLE_RESULT_SCHEMA_VERSION);
        assert!(LAYOUT_RESULT_SCHEMA_VERSION.ends_with("/v1"));
        assert!(TABLE_RESULT_SCHEMA_VERSION.ends_with("/v1"));
        // And neither is the classic one, which a consumer keys on.
        assert_ne!(
            LAYOUT_RESULT_SCHEMA_VERSION,
            crate::result_json::RESULT_SCHEMA_VERSION
        );
        assert_ne!(
            TABLE_RESULT_SCHEMA_VERSION,
            crate::result_json::RESULT_SCHEMA_VERSION
        );
        assert_ne!(
            DETECTION_RESULT_SCHEMA_VERSION,
            crate::result_json::RESULT_SCHEMA_VERSION
        );
        assert_ne!(DETECTION_RESULT_SCHEMA_VERSION, TABLE_RESULT_SCHEMA_VERSION);
    }
}
