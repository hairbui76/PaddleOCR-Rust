// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The versioned JSON result document.
//!
//! `docs/API_CONTRACT.md` freezes the shape as
//! `paddleocr-rust/ocr-result/v1`. The writer is hand-rolled rather than
//! derived for two reasons: the crate needs no runtime serialisation
//! dependency, and the output stays byte-deterministic because fields appear
//! in a fixed order with fixed numeric formatting.
//!
//! Text is escaped per JSON rules but never otherwise transformed. The exact
//! Unicode scalars the recognizer produced are what appear in the document,
//! which is the same exact-scalar rule the dictionary enforces.

use crate::api::TextLine;

/// The frozen result schema version.
pub const RESULT_SCHEMA_VERSION: &str = "paddleocr-rust/ocr-result/v1";

/// Serialises recognized lines as one versioned JSON document.
///
/// `id` identifies which input the document describes. It is `None` for a
/// single anonymous input and `Some` when the caller has several, which is what
/// makes a JSONL stream of these documents self-describing: without it, two
/// lines of output would be indistinguishable except by position.
#[must_use]
pub fn result_to_json(lines: &[TextLine], width: u32, height: u32, id: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("{\"schema_version\":\"");
    out.push_str(RESULT_SCHEMA_VERSION);
    out.push_str("\",\"input\":{\"id\":");
    match id {
        Some(id) => push_json_string(&mut out, id),
        None => out.push_str("null"),
    }
    out.push_str(",\"page_index\":null,\"width\":");
    out.push_str(&width.to_string());
    out.push_str(",\"height\":");
    out.push_str(&height.to_string());
    out.push_str("},\"lines\":[");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"quad\":[");
        for (corner, point) in line.quadrilateral.points().iter().enumerate() {
            if corner > 0 {
                out.push(',');
            }
            out.push('[');
            out.push_str(&format!("{:.0}", point.x()));
            out.push(',');
            out.push_str(&format!("{:.0}", point.y()));
            out.push(']');
        }
        out.push_str("],\"text\":");
        push_json_string(&mut out, &line.text);
        out.push_str(",\"confidence\":");
        out.push_str(&format!("{:.10}", line.score));
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// Appends one JSON string literal with the minimal required escaping.
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

    use crate::types::{Point, Quadrilateral};

    fn line(text: &str, score: f64) -> TextLine {
        let raw = [(1.0, 2.0), (3.0, 2.0), (3.0, 4.0), (1.0, 4.0)];
        let mut points = Vec::with_capacity(4);
        for (x, y) in raw {
            points.push(match Point::new(x, y) {
                Ok(value) => value,
                Err(error) => panic!("corner: {error}"),
            });
        }
        let corners = [points[0], points[1], points[2], points[3]];
        TextLine {
            quadrilateral: match Quadrilateral::new(corners) {
                Ok(value) => value,
                Err(error) => panic!("quadrilateral: {error}"),
            },
            text: text.to_owned(),
            score,
        }
    }

    #[test]
    fn the_result_shape_is_stable_and_deterministic() {
        let json = result_to_json(&[line("hi", 0.5)], 800, 320, None);
        let expected = concat!(
            "{\"schema_version\":\"paddleocr-rust/ocr-result/v1\",",
            "\"input\":{\"id\":null,\"page_index\":null,\"width\":800,\"height\":320},",
            "\"lines\":[{\"quad\":[[1,2],[3,2],[3,4],[1,4]],",
            "\"text\":\"hi\",\"confidence\":0.5000000000}]}"
        );
        assert_eq!(json, expected);
        // Serialising the same input twice must be byte-identical.
        assert_eq!(json, result_to_json(&[line("hi", 0.5)], 800, 320, None));
    }

    /// A named input carries its identifier, escaped like any other text.
    ///
    /// Without this a JSONL stream would rely on line position alone to say
    /// which document belongs to which input.
    #[test]
    fn a_named_input_records_its_identifier() {
        let json = result_to_json(&[], 4, 4, Some("pages/a\"b.png"));
        assert!(json.contains("\"id\":\"pages/a\\\"b.png\""), "{json}");
    }

    #[test]
    fn an_empty_result_is_still_well_formed() {
        let json = result_to_json(&[], 3, 2, None);
        assert!(json.contains("\"lines\":[]"), "{json}");
    }

    #[test]
    fn text_is_escaped_but_scalars_are_not_transformed() {
        let control = char::from_u32(1).unwrap_or('?');
        let text = format!("a\"b\\c{control}\u{4f60}");
        let json = result_to_json(&[line(&text, 0.25)], 10, 10, None);
        // The quote and backslash are escaped, the control becomes a \u
        // sequence, and the CJK scalar passes through unchanged.
        assert!(json.contains("\\\"b\\\\c\\u0001\u{4f60}"), "{json}");
    }
}
