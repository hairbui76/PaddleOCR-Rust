// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! OCR spans to block content: the `TextLine` machinery of StructureV3.
//!
//! Roadmap item `STRUCT-001`, the orchestration slice, phase B. This ports
//! `LayoutBlock.update_text_content` and everything beneath it —
//! `group_boxes_into_lines`, `format_line`, and the character classifiers —
//! for the scope this port ships: every span is `text`, because formula
//! recognition is an artifact-blocked sub-pipeline, and the one branch that
//! calls a recognition model (`formula`-labelled spans being re-split) is
//! therefore unreachable and deliberately absent.
//!
//! Quirks preserved from upstream, pinned in
//! `tests/fixtures/classic-v1-text-lines/`:
//!
//! - a **single-line block never sets `seg_end`** (upstream's last-line
//!   branch is an `elif`), so it stays at its `-inf` initial value;
//! - `num_of_lines` keeps its constructed value `1` when no span arrives;
//! - the per-line block width handed to `format_line` is the running maximum
//!   of the widths **seen so far**, not the final maximum;
//! - the vertical tall-line filter counts lines with height strictly below
//!   `1.1x` the minimum but keeps lines at or below it;
//! - `is_numeric` is Python's `\d` on one character — Unicode digits — which
//!   this port approximates with `char::is_numeric`; the difference is
//!   categories `Nl`/`No`, which no recorded case reaches.
#![allow(dead_code)]

/// A span's box in `[x1, y1, x2, y2]` page coordinates.
pub type SpanBox = [f64; 4];

/// `LINE_SETTINGS["line_height_iou_threshold"]`.
const LINE_HEIGHT_IOU_THRESHOLD: f64 = 0.6;

/// Reading direction of a block or line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextDirection {
    /// Left to right, lines stacked downward.
    Horizontal,
    /// Top to bottom, lines stacked right to left.
    Vertical,
}

/// One OCR span: a box and its recognized text.
#[derive(Clone, Debug)]
pub struct OcrSpan {
    /// The span's bounding box.
    pub bbox: SpanBox,
    /// The recognized text.
    pub text: String,
}

/// What `update_text_content` derives for a block.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockText {
    /// The assembled content.
    pub content: String,
    /// The number of text lines, including empty ones.
    pub num_of_lines: u32,
    /// The block's derived reading direction.
    pub direction: TextDirection,
    /// First line's first span start, `None` when no line set it.
    pub seg_start: Option<f64>,
    /// Last line's last span end, `None` when no line set it — including the
    /// single-line case, exactly as upstream leaves it.
    pub seg_end: Option<f64>,
    /// Mean line height, `0` with no lines.
    pub text_line_height: f64,
    /// Mean line width, `0` with no lines.
    pub text_line_width: f64,
}

/// `is_english_letter`: ASCII letters only.
fn is_english_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// `is_numeric`: Python `\d` on one character.
fn is_numeric(c: char) -> bool {
    c.is_numeric()
}

/// `is_non_breaking_punctuation`, the exact upstream set.
fn is_non_breaking_punctuation(c: char) -> bool {
    matches!(
        c,
        ',' | '，' | '、' | ';' | '；' | ':' | '：' | '-' | '\'' | '"' | '“'
    )
}

/// One grouped text line.
#[derive(Clone, Debug)]
struct Line {
    spans: Vec<OcrSpan>,
    direction: TextDirection,
    region_box: SpanBox,
    need_new_line: bool,
}

impl Line {
    fn new(span: OcrSpan, direction: TextDirection) -> Self {
        let region_box = span.bbox;
        Self {
            spans: vec![span],
            direction,
            region_box,
            need_new_line: false,
        }
    }

    fn add_span(&mut self, span: OcrSpan) {
        self.spans.push(span);
        let mut region = self.spans[0].bbox;
        for span in &self.spans {
            region[0] = region[0].min(span.bbox[0]);
            region[1] = region[1].min(span.bbox[1]);
            region[2] = region[2].max(span.bbox[2]);
            region[3] = region[3].max(span.bbox[3]);
        }
        self.region_box = region;
    }

    /// The extent across the reading direction — for a vertical line this is
    /// the x extent, exactly as upstream's property swaps the axes.
    fn height(&self) -> f64 {
        match self.direction {
            TextDirection::Horizontal => (self.region_box[3] - self.region_box[1]).abs(),
            TextDirection::Vertical => (self.region_box[2] - self.region_box[0]).abs(),
        }
    }

    fn width(&self) -> f64 {
        match self.direction {
            TextDirection::Horizontal => (self.region_box[2] - self.region_box[0]).abs(),
            TextDirection::Vertical => (self.region_box[3] - self.region_box[1]).abs(),
        }
    }
}

/// Projection overlap on one axis over the smaller extent.
fn projection_overlap_small(a: SpanBox, b: SpanBox, axis_start: usize) -> f64 {
    let axis_end = axis_start + 2;
    let overlap = a[axis_end].min(b[axis_end]) - a[axis_start].max(b[axis_start]);
    if overlap <= 0.0 {
        return 0.0;
    }
    let reference = (a[axis_end] - a[axis_start]).min(b[axis_end] - b[axis_start]);
    if reference <= 0.0 {
        0.0
    } else {
        overlap / reference
    }
}

/// `calculate_text_line_direction` with the default ratio `1.5`.
fn text_line_direction(boxes: &[SpanBox]) -> TextDirection {
    let horizontal = boxes
        .iter()
        .filter(|b| (b[2] - b[0]) * 1.5 >= b[3] - b[1])
        .count();
    if horizontal as f64 >= boxes.len() as f64 * 0.5 {
        TextDirection::Horizontal
    } else {
        TextDirection::Vertical
    }
}

/// `group_boxes_into_lines`, minus the formula-only paths.
fn group_boxes_into_lines(spans: &[OcrSpan]) -> (Vec<Line>, TextDirection) {
    let boxes: Vec<SpanBox> = spans.iter().map(|s| s.bbox).collect();
    let direction = text_line_direction(&boxes);

    if spans.is_empty() {
        return (Vec::new(), direction);
    }

    let mut sorted: Vec<OcrSpan> = spans.to_vec();
    let match_axis = match direction {
        TextDirection::Vertical => {
            sorted.sort_by(|a, b| b.bbox[0].total_cmp(&a.bbox[0]));
            0
        }
        TextDirection::Horizontal => {
            sorted.sort_by(|a, b| a.bbox[1].total_cmp(&b.bbox[1]));
            1
        }
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut iterator = sorted.into_iter();
    let mut current = match iterator.next() {
        Some(span) => Line::new(span, direction),
        None => return (Vec::new(), direction),
    };
    for span in iterator {
        let ratio = projection_overlap_small(current.region_box, span.bbox, match_axis);
        if ratio >= LINE_HEIGHT_IOU_THRESHOLD {
            current.add_span(span);
        } else {
            lines.push(current);
            current = Line::new(span, direction);
        }
    }
    lines.push(current);

    if direction == TextDirection::Vertical && !lines.is_empty() {
        let heights: Vec<f64> = lines.iter().map(Line::height).collect();
        let min_height = heights.iter().copied().fold(f64::INFINITY, f64::min);
        let max_height = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if max_height > min_height * 2.0 {
            let threshold = min_height * 1.1;
            let normal = heights.iter().filter(|h| **h < threshold).count();
            if (normal as f64) < lines.len() as f64 * 0.4 {
                let kept: Vec<Line> = lines
                    .into_iter()
                    .filter(|line| line.height() <= threshold)
                    .collect();
                return (kept, direction);
            }
        }
    }

    (lines, direction)
}

/// `format_line`, minus the formula spacing rules its labels never trigger.
///
/// Sorts the line's spans in place (half-pixel reading-order cells), then
/// joins them with upstream's English-letter spacing, hyphen strip, and
/// end-of-line geometry, setting `need_new_line` as a side effect.
fn format_line(
    line: &mut Line,
    block_text_width: f64,
    block_start: f64,
    block_stop: f64,
    line_gap_limit: f64,
) -> String {
    match line.direction {
        TextDirection::Horizontal => {
            line.spans.sort_by(|a, b| {
                (a.bbox[0] / 2.0)
                    .floor()
                    .total_cmp(&(b.bbox[0] / 2.0).floor())
                    .then(a.bbox[1].total_cmp(&b.bbox[1]))
            });
        }
        TextDirection::Vertical => {
            line.spans.sort_by(|a, b| {
                (a.bbox[1] / 2.0)
                    .floor()
                    .total_cmp(&(b.bbox[1] / 2.0).floor())
                    .then((-a.bbox[3]).total_cmp(&-b.bbox[3]))
            });
        }
    }
    let first_span_box = line.spans[0].bbox;
    let last_span_box = line.spans[line.spans.len() - 1].bbox;

    let mut line_text = String::new();
    for span in &line.spans {
        line_text.push_str(&span.text);
        if !span.text.is_empty() && line_text.chars().last().is_some_and(is_english_letter) {
            line_text.push(' ');
        }
    }

    let text_stop_index = match line.direction {
        TextDirection::Horizontal => 2,
        TextDirection::Vertical => 3,
    };

    if line_text.ends_with(' ') {
        line_text.pop();
    }
    if line_text.is_empty() {
        return String::new();
    }
    let last_char = match line_text.chars().last() {
        Some(c) => c,
        None => return String::new(),
    };

    if (!is_english_letter(last_char)
        && !is_non_breaking_punctuation(last_char)
        && !is_numeric(last_char))
        || block_stop - last_span_box[text_stop_index] > block_text_width * 0.3
    {
        let past_gap = match line.direction {
            TextDirection::Horizontal => {
                block_stop - last_span_box[text_stop_index] > line_gap_limit
            }
            TextDirection::Vertical => {
                block_stop - last_span_box[text_stop_index] > line_gap_limit
                    || first_span_box[1] - block_start > line_gap_limit
            }
        };
        if past_gap {
            line.need_new_line = true;
        }
    }

    if line_text.ends_with('-') {
        line_text.pop();
        return line_text;
    }

    if is_english_letter(last_char) || line_text.ends_with('$') {
        line_text.push(' ');
    }
    if (!is_english_letter(last_char) && !is_numeric(last_char))
        || line.direction == TextDirection::Vertical
    {
        if block_stop - last_span_box[text_stop_index] > block_text_width * 0.3
            && !is_non_breaking_punctuation(last_char)
        {
            line_text.push('\n');
            line.need_new_line = true;
        }
    } else if block_stop - last_span_box[text_stop_index] > (block_stop - block_start) * 0.5 {
        line_text.push('\n');
        line.need_new_line = true;
    }

    line_text
}

/// `LayoutBlock.update_text_content` for text-only spans.
///
/// `label` selects the delimiter (`doc_title` joins with a space, `content`
/// with a newline, everything else concatenates with the paragraph rules) and
/// the `reference` coordinate override.
#[must_use]
pub fn update_text_content(label: &str, bbox: SpanBox, spans: &[OcrSpan]) -> BlockText {
    let mut result = BlockText {
        content: String::new(),
        num_of_lines: 1,
        direction: TextDirection::Horizontal,
        seg_start: None,
        seg_end: None,
        text_line_height: 0.0,
        text_line_width: 0.0,
    };
    if spans.is_empty() {
        // Upstream returns before touching stats; the constructed defaults
        // (num_of_lines 1, text line sizes 1) survive, except the means this
        // struct reports which upstream also leaves at their constructed
        // values. The fixture pins the observable fields.
        result.text_line_height = 1.0;
        result.text_line_width = 1.0;
        return result;
    }

    let (mut lines, direction) = group_boxes_into_lines(spans);
    result.direction = direction;
    if lines.is_empty() {
        result.text_line_height = 0.0;
        result.text_line_width = 0.0;
        result.content = String::new();
        return result;
    }
    result.text_line_height = lines.iter().map(Line::height).sum::<f64>() / lines.len() as f64;
    result.text_line_width = lines.iter().map(Line::width).sum::<f64>() / lines.len() as f64;

    let (coord_start_idx, coord_end_idx) = match direction {
        TextDirection::Horizontal => (0, 2),
        TextDirection::Vertical => (1, 3),
    };
    let (block_start, block_stop) = if label == "reference" {
        let start = spans
            .iter()
            .map(|s| s.bbox[coord_start_idx])
            .fold(f64::INFINITY, f64::min);
        let stop = spans
            .iter()
            .map(|s| s.bbox[coord_end_idx])
            .fold(f64::NEG_INFINITY, f64::max);
        (start, stop)
    } else {
        (bbox[coord_start_idx], bbox[coord_end_idx])
    };

    let mut text_lines: Vec<String> = Vec::new();
    let mut running_max_width = f64::NEG_INFINITY;
    let mut need_new_line_num = 0_usize;
    let line_count = lines.len();
    for (line_idx, line) in lines.iter_mut().enumerate() {
        running_max_width = running_max_width.max(line.width());
        let gap_limit = line.height() * 1.5;
        let line_text = format_line(line, running_max_width, block_start, block_stop, gap_limit);
        if line.need_new_line {
            need_new_line_num += 1;
        }
        if line_idx == 0 {
            result.seg_start = Some(line.spans[0].bbox[0]);
        } else if line_idx == line_count - 1 {
            result.seg_end = Some(line.spans[line.spans.len() - 1].bbox[2]);
        }
        text_lines.push(line_text);
    }

    let delimiter = match label {
        "doc_title" => Some(" "),
        "content" => Some("\n"),
        _ => None,
    };
    let content = match delimiter {
        Some(delimiter) => text_lines.join(delimiter),
        None => {
            let mut content = String::new();
            let mut pre_line_end = false;
            let mut last_char: Option<char> = None;
            for (idx, line_text) in text_lines.iter().enumerate() {
                if line_text.is_empty() {
                    continue;
                }
                let mut line_text = line_text.clone();
                let line = &lines[idx];
                if pre_line_end {
                    let start_gap = line.region_box[coord_start_idx] - block_start;
                    let letterish =
                        last_char.is_some_and(|c| is_english_letter(c) || is_numeric(c));
                    if ((start_gap > line.height() * 1.5 && !letterish)
                        || start_gap > (block_stop - block_start) * 0.4)
                        && !content.ends_with('\n')
                    {
                        line_text.insert(0, '\n');
                    }
                }
                content.push_str(&line_text);

                let chars: Vec<char> = line_text.chars().collect();
                last_char = if chars.len() > 2 && line_text.ends_with(' ') {
                    chars.get(chars.len() - 2).copied()
                } else {
                    chars.last().copied()
                };
                let blocked_last = last_char.is_some_and(|c| {
                    is_english_letter(c) || is_non_breaking_punctuation(c) || is_numeric(c)
                });
                if (!line_text.is_empty()
                    && !line_text.ends_with('\n')
                    && !blocked_last
                    && need_new_line_num as f64 > text_lines.len() as f64 * 0.5)
                    || need_new_line_num as f64 > text_lines.len() as f64 * 0.6
                {
                    content.push('\n');
                }
                pre_line_end =
                    block_stop - line.region_box[coord_end_idx] > (block_stop - block_start) * 0.3;
            }
            content
        }
    };

    result.content = content;
    result.num_of_lines = text_lines.len() as u32;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-text-lines/expected.json");

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

    #[test]
    fn the_captured_block_texts_are_reproduced() {
        let fixture = fixture();
        let cases = items(&fixture["cases"], "cases");
        assert_eq!(cases.len(), 14);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");
            let bbox_values = items(&case["bbox"], "bbox");
            let bbox: SpanBox = [
                bbox_values[0].as_f64().unwrap_or(0.0),
                bbox_values[1].as_f64().unwrap_or(0.0),
                bbox_values[2].as_f64().unwrap_or(0.0),
                bbox_values[3].as_f64().unwrap_or(0.0),
            ];
            let spans: Vec<OcrSpan> = items(&case["spans"], "spans")
                .iter()
                .map(|span| {
                    let span = items(span, "span");
                    let b = items(&span[0], "span box");
                    OcrSpan {
                        bbox: [
                            b[0].as_f64().unwrap_or(0.0),
                            b[1].as_f64().unwrap_or(0.0),
                            b[2].as_f64().unwrap_or(0.0),
                            b[3].as_f64().unwrap_or(0.0),
                        ],
                        text: span[1].as_str().unwrap_or("").to_owned(),
                    }
                })
                .collect();
            let label = case["label"].as_str().unwrap_or("text");

            let actual = update_text_content(label, bbox, &spans);
            assert_eq!(
                actual.content,
                case["content"].as_str().unwrap_or(""),
                "{name}: content"
            );
            assert_eq!(
                u64::from(actual.num_of_lines),
                case["num_of_lines"].as_u64().unwrap_or(0),
                "{name}: lines"
            );
            let expected_direction = case["direction"].as_str().unwrap_or("");
            let actual_direction = match actual.direction {
                TextDirection::Horizontal => "horizontal",
                TextDirection::Vertical => "vertical",
            };
            assert_eq!(actual_direction, expected_direction, "{name}: direction");
            for (field, actual_value) in [
                ("seg_start_coordinate", actual.seg_start),
                ("seg_end_coordinate", actual.seg_end),
            ] {
                match (actual_value, case[field].as_f64()) {
                    (Some(a), Some(e)) => {
                        assert!((a - e).abs() < 1e-9, "{name}: {field} {a} vs {e}")
                    }
                    (None, None) => {}
                    (a, e) => panic!("{name}: {field} presence mismatch {a:?} vs {e:?}"),
                }
            }
            if !spans.is_empty() {
                for (field, actual_value) in [
                    ("text_line_height", actual.text_line_height),
                    ("text_line_width", actual.text_line_width),
                ] {
                    let expected = case[field].as_f64().unwrap_or(f64::NAN);
                    assert!(
                        (actual_value - expected).abs() < 1e-9,
                        "{name}: {field} {actual_value} vs {expected}"
                    );
                }
            }
        }
    }
}
