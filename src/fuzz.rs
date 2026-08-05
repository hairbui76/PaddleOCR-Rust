// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Byte-driven developer-only fuzz entry points for current pure kernels.
//!
//! This module is available only with the internal fuzzing feature. It is
//! intentionally not an OCR API, model loader, decoder, or normal runtime
//! dependency. The bounded driver is exposed so an external stdin-oriented
//! fuzzer can exercise private checked kernels without widening their public
//! production interfaces.

use crate::{
    crop::{InterleavedImage, classic_perspective_crop},
    ctc::{CtcScoreMatrix, classic_ctc_greedy_indices},
    db::{DetectorProbabilityMap, classic_db_binary_segmentation, classic_db_connected_components},
    geometry::{
        classic_detector_resize_plan, classic_order_clip_filter_quad,
        classic_perspective_crop_plan, classic_rescale_detector_quad, classic_sort_quadrilaterals,
        minimum_area_quad_candidate, polygon_area, polygon_perimeter, polygon_signed_area,
    },
    types::{
        EncodedImage, ImageDimensions, ImageTransform, ModelIdentity, ModelTask, Point, Polygon,
        Quadrilateral, Score,
    },
};

/// Maximum byte count consumed by one fuzz-target invocation.
pub const MAX_INPUT_BYTES: usize = 16 * 1024;

const MAX_DRIVER_SIDE: u32 = 32;
const MAX_CROP_SIDE: u32 = 16;
const MAX_FUZZ_POLYGON_VERTICES: usize = 10;

/// Exercises current bounded pure processing surfaces with one arbitrary byte input.
///
/// Malformed values are deliberately passed to checked constructors and are
/// expected to return typed errors. The driver itself performs no I/O,
/// allocation derived from an unbounded input length, model loading, decoding,
/// inference, or result serialization.
pub fn exercise(input: &[u8]) {
    let input = &input[..input.len().min(MAX_INPUT_BYTES)];
    let mut reader = ByteReader::new(input);

    exercise_public_validators(&mut reader, input);
    exercise_db_kernels(&mut reader);
    exercise_ctc_kernel(&mut reader);
    exercise_geometry_and_crop_kernels(&mut reader);
    exercise_structured_kernels(&mut reader);
    exercise_layout_order(&mut reader);
    exercise_structure_orchestration(&mut reader);
    exercise_parsers(input);
}

/// Drives the layout ordering object model end to end.
///
/// `xycut_enhanced_order` is pure arithmetic over caller-supplied blocks, so
/// the properties under test are structural: no panic on absurd coordinates
/// (the projection histogram inside refuses extents over its bound, and the
/// refusal must surface as the identity fallback rather than an allocation),
/// no index emitted twice — a duplicate would duplicate content in a
/// reconstructed document — and determinism, asserted by ordering a cloned
/// page and requiring the same order and the same `order_label`s.
fn exercise_layout_order(reader: &mut ByteReader<'_>) {
    const LABELS: [&str; 12] = [
        "text",
        "doc_title",
        "paragraph_title",
        "image",
        "table",
        "seal",
        "header",
        "footer",
        "footnote",
        "table_title",
        "reference",
        "abstract",
    ];
    let count = usize::from(reader.next_byte() % 8);
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let label = LABELS[usize::from(reader.next_byte()) % LABELS.len()];
        let mut block = crate::layout_order::OrderBlock::new(
            label,
            [
                i64::from(reader.next_u32() as i32),
                i64::from(reader.next_u32() as i32),
                i64::from(reader.next_u32() as i32),
                i64::from(reader.next_u32() as i32),
            ],
        );
        block.num_of_lines = u32::from(reader.next_byte());
        block.text_line_height = f64::from(reader.next_byte()) / 4.0;
        block.text_line_width = f64::from(reader.next_byte()) / 2.0;
        blocks.push(block);
    }
    let page_bbox = [
        0,
        0,
        i64::from(reader.next_u32() % 4_096) + 1,
        i64::from(reader.next_u32() % 4_096) + 1,
    ];
    let mut page = crate::layout_order::OrderPage::new(page_bbox, blocks);
    let mut replay = page.clone();

    let order = crate::layout_order::xycut_enhanced_order(&mut page);
    let mut seen = order.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), order.len(), "no block may be ordered twice");
    assert!(
        order.iter().all(|index| *index < page.blocks.len()),
        "every emitted index must be one that was supplied"
    );

    let again = crate::layout_order::xycut_enhanced_order(&mut replay);
    assert_eq!(order, again, "the ordering must be deterministic");
    for (block, replayed) in page.blocks.iter().zip(&replay.blocks) {
        assert_eq!(
            block.order_label, replayed.order_label,
            "order labels must be deterministic"
        );
    }

    // A second page with page-plausible coordinates. Arbitrary `i32`s almost
    // never form boxes that overlap or sit near each other, so the child
    // matchers, pre-cuts, and insert functions would go unexercised without a
    // corpus where blocks actually share a page.
    let count = usize::from(reader.next_byte() % 8);
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let label = LABELS[usize::from(reader.next_byte()) % LABELS.len()];
        let left = i64::from(reader.next_byte()) * 4;
        let top = i64::from(reader.next_byte()) * 4;
        let width = i64::from(reader.next_byte()) + 1;
        let height = i64::from(reader.next_byte()) + 1;
        let mut block =
            crate::layout_order::OrderBlock::new(label, [left, top, left + width, top + height]);
        block.num_of_lines = u32::from(reader.next_byte() % 24);
        block.text_line_height = f64::from(reader.next_byte() % 32) / 2.0 + 0.5;
        block.text_line_width = f64::from(reader.next_byte() % 64) + 1.0;
        blocks.push(block);
    }
    let mut page = crate::layout_order::OrderPage::new([0, 0, 1_100, 1_100], blocks);
    let mut replay = page.clone();
    let order = crate::layout_order::xycut_enhanced_order(&mut page);
    let mut seen = order.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), order.len(), "no block may be ordered twice");
    let again = crate::layout_order::xycut_enhanced_order(&mut replay);
    assert_eq!(order, again, "the ordering must be deterministic");
}

/// Drives the whole StructureV3 orchestration chain over one arbitrary page.
///
/// `standardized_data` → `assemble_layout_parsing` → `convert_markdown_page`
/// is now pure end to end (the two recognition sites are a trait, served here
/// by a deterministic stub), so a whole document can be driven from bytes.
/// The risks this targets are the ones no fixture can reach:
///
/// - **Termination.** The supplement-region loop consumes a block set until it
///   is empty and the region-growth loop iterates to a fixpoint. Both are
///   argued to terminate; a hang here is the argument being wrong.
/// - **Partitioning.** No layout block may land in two regions, and the
///   document may not grow blocks — either would duplicate content.
/// - **Parallel OCR arrays.** Re-recognition appends across five vectors, and
///   `dt_polys` is deliberately *not* one of them; assembly indexes them by
///   position, so a length that drifts the wrong way is an out-of-bounds.
/// - **UTF-8 boundaries.** The text-line machinery inspects the last and
///   second-to-last character of a line, so the corpus includes multi-byte
///   text: byte-indexing there would panic rather than mis-format.
fn exercise_structure_orchestration(reader: &mut ByteReader<'_>) {
    use crate::structure_standardize::{OcrData, TextRecognizer, standardized_data};

    /// Deterministic stand-in for the recognition model.
    struct StubRecognizer;

    impl TextRecognizer for StubRecognizer {
        fn recognize(&mut self, crop: [i64; 4]) -> (String, f64) {
            let height = crop[3].saturating_sub(crop[1]);
            let width = crop[2].saturating_sub(crop[0]);
            let score = f64::from((height.rem_euclid(97) + width.rem_euclid(31)) as u32) / 128.0;
            (format!("rec-{height}x{width}"), score)
        }
    }

    const LABELS: [&str; 12] = [
        "text",
        "doc_title",
        "paragraph_title",
        "table",
        "formula",
        "image",
        "seal",
        "header",
        "footer",
        "footnote",
        "reference",
        "abstract",
    ];
    // Multi-byte text belongs here: `format_line` and the paragraph-joining
    // rules read the last and second-to-last character of a line.
    const TEXTS: [&str; 8] = [
        "",
        "word ",
        "hyphen-",
        "第一行的中文内容",
        "1.2 Methods",
        "abstract",
        "  spaced  ",
        "ends,",
    ];

    let coordinate = |reader: &mut ByteReader<'_>| -> [f64; 4] {
        let left = f64::from(reader.next_byte()) * 4.0;
        let top = f64::from(reader.next_byte()) * 4.0;
        // Degenerate and inverted rectangles are deliberately reachable: a
        // zero-area block never matches a region and must still be consumed.
        let width = f64::from(reader.next_byte()) - 8.0;
        let height = f64::from(reader.next_byte()) - 8.0;
        [left, top, left + width, top + height]
    };

    let layout_count = usize::from(reader.next_byte() % 6);
    let mut layout_boxes = Vec::with_capacity(layout_count);
    for _ in 0..layout_count {
        layout_boxes.push(crate::structure_glue::GlueBlock {
            label: LABELS[usize::from(reader.next_byte()) % LABELS.len()].to_owned(),
            coordinate: coordinate(reader),
            score: f64::from(reader.next_byte()) / 255.0,
        });
    }

    let region_count = usize::from(reader.next_byte() % 3);
    let mut region_boxes = Vec::with_capacity(region_count);
    for _ in 0..region_count {
        region_boxes.push(crate::structure_glue::GlueBlock {
            label: "Region".to_owned(),
            coordinate: coordinate(reader),
            score: 1.0,
        });
    }

    let span_count = usize::from(reader.next_byte() % 8);
    let mut ocr = OcrData::default();
    for _ in 0..span_count {
        let bbox = coordinate(reader);
        let corners = [
            [bbox[0], bbox[1]],
            [bbox[2], bbox[1]],
            [bbox[2], bbox[3]],
            [bbox[0], bbox[3]],
        ];
        ocr.dt_polys.push(corners);
        ocr.rec_polys.push(corners);
        ocr.rec_boxes.push(bbox);
        ocr.rec_texts
            .push(TEXTS[usize::from(reader.next_byte()) % TEXTS.len()].to_owned());
        ocr.rec_scores.push(f64::from(reader.next_byte()) / 255.0);
        ocr.rec_labels.push("text".to_owned());
    }

    let threshold = f64::from(reader.next_byte()) / 255.0;
    let page_width = f64::from(reader.next_byte()) * 8.0 + 1.0;
    let page_height = f64::from(reader.next_byte()) * 8.0 + 1.0;

    let mut stub = StubRecognizer;
    let standardized = standardized_data(
        page_width,
        page_height,
        &layout_boxes,
        &region_boxes,
        ocr.clone(),
        &mut stub,
        threshold,
    );

    // No block may belong to two regions: a document that placed one block in
    // two regions would emit its content twice.
    let mut placed: Vec<usize> = standardized
        .region_to_block_map
        .values()
        .flatten()
        .copied()
        .collect();
    let total_placed = placed.len();
    placed.sort_unstable();
    placed.dedup();
    assert_eq!(
        placed.len(),
        total_placed,
        "no layout block may be placed in two regions"
    );
    assert!(
        placed
            .iter()
            .all(|index| *index < standardized.layout_boxes.len()),
        "every placed index must name a layout block"
    );

    // The five vectors assembly indexes by position must stay in step.
    // `dt_polys` is deliberately excluded: the no-text branch appends to the
    // others without it, so it may only fall behind.
    let spans = standardized.ocr.rec_texts.len();
    assert_eq!(standardized.ocr.rec_boxes.len(), spans, "rec_boxes length");
    assert_eq!(standardized.ocr.rec_polys.len(), spans, "rec_polys length");
    assert_eq!(
        standardized.ocr.rec_scores.len(),
        spans,
        "rec_scores length"
    );
    assert_eq!(
        standardized.ocr.rec_labels.len(),
        spans,
        "rec_labels length"
    );
    assert!(
        standardized.ocr.dt_polys.len() <= spans,
        "dt_polys may only fall behind the recognition arrays"
    );
    for indices in standardized.block_to_ocr_map.values() {
        assert!(
            indices.iter().all(|index| *index < spans),
            "every mapped span index must exist"
        );
    }

    let table_html: Vec<String> = (0..usize::from(reader.next_byte() % 3))
        .map(|index| format!("<table><tr><td>{index}</td></tr></table>"))
        .collect();
    let ignore = crate::structure_assembly::DEFAULT_MARKDOWN_IGNORE_LABELS;
    let assembled =
        crate::structure_assembly::assemble_layout_parsing(&standardized, &table_html, &ignore);

    // Assembly may drop a block whose ordering refused it, but it may never
    // invent one.
    assert!(
        assembled.len() <= standardized.layout_boxes.len(),
        "assembly must not grow the document"
    );
    let mut expected_order_index = 1_u32;
    for (position, block) in assembled.iter().enumerate() {
        assert_eq!(block.index, position, "block indices must be sequential");
        if let Some(order_index) = block.order_index {
            assert_eq!(
                order_index, expected_order_index,
                "reading numbers must be gap-free"
            );
            expected_order_index += 1;
        }
    }

    for pretty in [true, false] {
        let options = crate::markdown_v2::MarkdownOptions {
            pretty,
            use_table_recognition: !table_html.is_empty(),
            original_image_width: page_width as i64,
            markdown_ignore_labels: &ignore,
        };
        let page = crate::markdown_v2::convert_markdown_page(&assembled, &options);
        let again = crate::markdown_v2::convert_markdown_page(&assembled, &options);
        assert_eq!(page, again, "the Markdown page must be deterministic");
        let mut paths = page.image_paths.clone();
        let total_paths = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), total_paths, "image paths must be unique");
    }

    // The cross-page merge, over the fuzzed page repeated. Merging can only
    // ever drop blocks and move their text into an earlier block, so the block
    // count must fall and no block may be invented — a merge that grew the
    // document would mean text was duplicated rather than moved.
    let document = [assembled.clone(), assembled.clone()];
    let merged = crate::multipage::merge_text_across_page(&document);
    assert_eq!(merged.len(), 2, "merging must preserve the page count");
    let survivors: usize = merged.iter().map(Vec::len).sum();
    assert!(
        survivors <= assembled.len() * 2,
        "merging must not invent blocks"
    );
    assert_eq!(
        merged,
        crate::multipage::merge_text_across_page(&document),
        "the cross-page merge must be deterministic"
    );

    // The whole chain, replayed: same bytes in, same document out.
    let mut replay_stub = StubRecognizer;
    let replayed = standardized_data(
        page_width,
        page_height,
        &layout_boxes,
        &region_boxes,
        ocr,
        &mut replay_stub,
        threshold,
    );
    let replayed_blocks =
        crate::structure_assembly::assemble_layout_parsing(&replayed, &table_html, &ignore);
    assert_eq!(
        assembled, replayed_blocks,
        "the orchestration must be deterministic"
    );
}

/// Drives the structured-document kernels: reading order, Markdown, and table
/// composition.
///
/// These take **caller-supplied numbers and strings** rather than a decoded
/// image, which makes them a different shape of risk: `reading_order` allocates
/// a histogram sized from box coordinates, and the table matcher indexes text
/// by positions the caller chose. Both must answer with a typed error rather
/// than a panic or an allocation proportional to a declared coordinate.
///
/// The JSON writers are driven too, and their output is compared against a
/// second call: `SPECAPI-001` claims determinism, and a claim that is only in a
/// document is not a claim this project makes.
fn exercise_structured_kernels(reader: &mut ByteReader<'_>) {
    // Reading order, over boxes built from arbitrary bytes. The coordinates are
    // deliberately allowed to be absurd: refusing them is the behaviour under
    // test, not an obstacle to it.
    let count = usize::from(reader.next_byte() % 8);
    let mut boxes = Vec::with_capacity(count);
    for _ in 0..count {
        boxes.push([
            i64::from(reader.next_u32() as i32),
            i64::from(reader.next_u32() as i32),
            i64::from(reader.next_u32() as i32),
            i64::from(reader.next_u32() as i32),
        ]);
    }
    let indices: Vec<usize> = (0..boxes.len()).collect();
    let mut order = Vec::new();
    if crate::reading_order::recursive_yx_cut(&boxes, &indices, &mut order).is_ok() {
        // A **subset without duplicates**, not a permutation. Boxes whose edges
        // are inverted are dropped from the ordering -- by upstream as well as
        // here -- so requiring a permutation would assert something neither
        // implementation provides. Duplication is still forbidden: an index
        // emitted twice would duplicate content in a reconstructed document.
        let mut seen = order.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), order.len(), "no index may be emitted twice");
        assert!(
            order.iter().all(|index| *index < boxes.len()),
            "every emitted index must be one that was supplied"
        );
    }
    let mut mirrored = Vec::new();
    let _ = crate::reading_order::recursive_xy_cut(&boxes, &indices, &mut mirrored);

    // Table composition, over the same arbitrary boxes reinterpreted as floats.
    let cells: Vec<crate::table_pipeline::Box> = boxes
        .iter()
        .map(|entry| {
            [
                entry[0] as f64,
                entry[1] as f64,
                entry[2] as f64,
                entry[3] as f64,
            ]
        })
        .collect();
    let (sorted, flags) = crate::table_pipeline::sort_cell_boxes(&cells);
    assert_eq!(sorted.len(), cells.len(), "the sort must not lose a cell");
    if let Ok(matched) = crate::table_pipeline::match_cells_to_ocr(&sorted, &cells, &flags) {
        let texts: Vec<String> = (0..cells.len()).map(|index| index.to_string()).collect();
        let tokens: Vec<String> = [
            "<html>",
            "<body>",
            "<table>",
            "<td></td>",
            "</table>",
            "</body>",
            "</html>",
        ]
        .iter()
        .map(|token| (*token).to_owned())
        .collect();
        let _ = crate::table_pipeline::table_html(&matched, &texts, &tokens, &flags);
    }
    let _ = crate::table_pipeline::suppress_overlapping_cells(
        &cells,
        &vec![0.5_f32; cells.len()],
        f64::from(reader.next_byte()) / 255.0,
    );

    // The Markdown formatters, over arbitrary text. They cannot fail, so what
    // is under test is that they neither panic on a UTF-8 boundary nor loop.
    let text = String::from_utf8_lossy(&[
        reader.next_byte(),
        reader.next_byte(),
        reader.next_byte(),
        reader.next_byte(),
        reader.next_byte(),
        reader.next_byte(),
    ])
    .into_owned();
    let _ = crate::markdown::format_title(&text);
    let _ = crate::markdown::format_paragraph_title(&text, Some(usize::from(reader.next_byte())));
    let _ = crate::markdown::normalize_newlines(&text);
    let _ = crate::markdown::simplify_table(&text);
    let _ = crate::markdown::format_first_line(&text, &["abstract"], "## ", "", " ");

    // The multipage concatenation, over arbitrary page text and flags. The
    // interesting input is the *join*: the two facing characters decide the
    // separator, so a lossy UTF-8 tail is exactly the case that could index a
    // byte inside a character.
    let pages: Vec<crate::markdown_v2::MarkdownPage> = (0..3)
        .map(|_| crate::markdown_v2::MarkdownPage {
            markdown: String::from_utf8_lossy(&[
                reader.next_byte(),
                reader.next_byte(),
                reader.next_byte(),
            ])
            .into_owned(),
            image_paths: Vec::new(),
            continuation_flags: (
                reader.next_byte().is_multiple_of(2),
                reader.next_byte().is_multiple_of(2),
            ),
        })
        .collect();
    let joined = crate::multipage::concatenate_markdown_pages(&pages);
    assert_eq!(
        joined,
        crate::multipage::concatenate_markdown_pages(&pages),
        "the page concatenation must be deterministic"
    );

    // Determinism, asserted rather than documented.
    let regions: Vec<crate::layout::LayoutRegion> = Vec::new();
    let first = crate::structure_json::layout_to_json(&regions, 1, 1, Some(&text));
    let second = crate::structure_json::layout_to_json(&regions, 1, 1, Some(&text));
    assert_eq!(first, second, "the layout writer must be deterministic");
}

/// Drives the parsers that consume caller-supplied bytes directly.
///
/// These are the surfaces where the input is *the attacker's document* rather
/// than a derived tensor: an encoded image, a manifest, and a stream. Every one
/// is bounded before it allocates, and every one must answer with a typed error
/// rather than a panic, an abort, or an allocation proportional to a declared
/// field.
fn exercise_parsers(input: &[u8]) {
    // The PNG decoder. Its resource envelope is enforced from the declared
    // header, so a hostile header is the interesting case rather than a hostile
    // pixel stream.
    if let Ok(encoded) = EncodedImage::new(input) {
        let _ = crate::image::decode_classic_bgr(encoded);
    }

    // The manifest parser. Arbitrary bytes are rarely valid UTF-8, so the
    // lossy conversion is what keeps this reaching the parser at all rather
    // than stopping at the encoding check.
    let text = String::from_utf8_lossy(input);
    let _ = crate::manifest::ModelManifest::parse(&text);

    // The bounded stream reader, driven by a reader that yields the input in
    // awkward pieces rather than one slice.
    let _ = crate::input::read_encoded_from(Dribble {
        remaining: input,
        step: 1 + usize::from(input.first().copied().unwrap_or(0)),
    });

    // The dictionary parser, which decides every scalar a recognizer can emit.
    let _ = crate::dictionary::CtcDictionary::new(
        text.lines().map(str::to_owned).collect(),
        input.len().is_multiple_of(2),
    );
}

/// A reader that hands out the input in small, irregular chunks.
struct Dribble<'a> {
    remaining: &'a [u8],
    step: usize,
}

impl std::io::Read for Dribble<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let take = self.step.min(buffer.len()).min(self.remaining.len());
        buffer[..take].copy_from_slice(&self.remaining[..take]);
        self.remaining = &self.remaining[take..];
        Ok(take)
    }
}

fn exercise_public_validators(reader: &mut ByteReader<'_>, input: &[u8]) {
    let _ = EncodedImage::new(input);
    let _ = ImageDimensions::new(reader.next_u32(), reader.next_u32());
    let _ = Point::new(reader.next_f32(), reader.next_f32());
    let _ = Score::new(reader.next_f32());

    let mut identity_component = String::new();
    for _ in 0..16 {
        identity_component.push(char::from(reader.next_byte()));
    }
    let task = if reader.next_byte() & 1 == 0 {
        ModelTask::TextDetection
    } else {
        ModelTask::TextRecognition
    };
    let _ = ModelIdentity::new(task, identity_component, "fuzz");

    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    if let Ok(transform) = ImageTransform::new(
        dimensions,
        dimensions,
        reader.next_f32(),
        reader.next_f32(),
        reader.next_f32(),
        reader.next_f32(),
    ) && let Ok(point) = Point::new(reader.next_f32(), reader.next_f32())
    {
        let _ = transform.forward(point);
        let _ = transform.inverse(point);
    }
}

fn exercise_db_kernels(reader: &mut ByteReader<'_>) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    let value_count = dimensions.pixels() as usize;
    let values = (0..value_count)
        .map(|_| score_value(reader))
        .collect::<Vec<_>>();
    let wrong_length = values.len().saturating_sub(1);
    let _ = DetectorProbabilityMap::new(dimensions, &values[..wrong_length]);

    if let Ok(map) = DetectorProbabilityMap::new(dimensions, &values)
        && let Ok(bitmap) = classic_db_binary_segmentation(map)
    {
        let _ = classic_db_connected_components(&bitmap);
    }
}

fn exercise_ctc_kernel(reader: &mut ByteReader<'_>) {
    let _ = CtcScoreMatrix::new(reader.next_u32(), reader.next_u32(), &[]);

    let time_steps = u32::from(reader.next_byte() % 33);
    let class_count = u32::from(reader.next_byte() % 32) + 1;
    let value_count = time_steps as usize * class_count as usize;
    let values = (0..value_count)
        .map(|_| score_value(reader))
        .collect::<Vec<_>>();
    let wrong_length = values.len().saturating_sub(1);
    let _ = CtcScoreMatrix::new(time_steps, class_count, &values[..wrong_length]);

    if let Ok(matrix) = CtcScoreMatrix::new(time_steps, class_count, &values) {
        let _ = classic_ctc_greedy_indices(matrix);
    }
}

fn exercise_geometry_and_crop_kernels(reader: &mut ByteReader<'_>) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_DRIVER_SIDE) else {
        return;
    };
    let _ = classic_detector_resize_plan(dimensions);

    let arbitrary_points =
        core::array::from_fn(|_| Point::new(reader.next_f32(), reader.next_f32()));
    if let [Ok(first), Ok(second), Ok(third), Ok(fourth)] = arbitrary_points {
        exercise_arbitrary_quadrilateral([first, second, third, fourth], dimensions, reader);
    }

    let Some(quadrilateral) = bounded_quadrilateral(reader) else {
        return;
    };
    let points = quadrilateral.points();
    let _ = classic_order_clip_filter_quad(points, dimensions);
    let _ = classic_rescale_detector_quad(points, dimensions, dimensions);

    let mut reading_order = [quadrilateral, quadrilateral];
    classic_sort_quadrilaterals(&mut reading_order);

    if let Ok(polygon) = Polygon::new(points.to_vec()) {
        exercise_polygon_measurements(&polygon);
    }
    exercise_polygon_kernels(reader);

    if let Ok(plan) = classic_perspective_crop_plan(quadrilateral) {
        let _ = plan.map_source_to_warp(points[0]);
        let _ = plan.map_warp_to_source(points[0]);
        let _ =
            plan.map_warp_coordinates_to_source(f64::from(points[0].x()), f64::from(points[0].y()));
    }

    exercise_crop_kernel(reader, quadrilateral);
}

fn exercise_arbitrary_quadrilateral(
    points: [Point; 4],
    dimensions: ImageDimensions,
    reader: &mut ByteReader<'_>,
) {
    let Ok(quadrilateral) = Quadrilateral::new(points) else {
        return;
    };
    let points = quadrilateral.points();

    let _ = classic_order_clip_filter_quad(points, dimensions);
    let _ = classic_rescale_detector_quad(points, dimensions, dimensions);

    let mut reading_order = [quadrilateral, quadrilateral];
    classic_sort_quadrilaterals(&mut reading_order);

    let Ok(plan) = classic_perspective_crop_plan(quadrilateral) else {
        return;
    };
    for point in points {
        let _ = plan.map_source_to_warp(point);
        let _ = plan.map_warp_to_source(point);
    }
    let _ = plan
        .map_warp_coordinates_to_source(f64::from(reader.next_f32()), f64::from(reader.next_f32()));
    let _ = plan.map_warp_pixel_to_source_for_sampling(reader.next_u32(), reader.next_u32());
}

fn exercise_polygon_kernels(reader: &mut ByteReader<'_>) {
    // Keep every candidate bounded independently of the fuzzer input length.
    // These shapes deliberately cover construction rejection as well as the
    // convex-hull/minimum-area path on non-convex and repeated vertices.
    let short_count = usize::from(reader.next_byte() % 3);
    let Some(short_points) = bounded_polygon_points(reader, short_count) else {
        return;
    };
    let _ = Polygon::new(short_points);

    let collinear_count = 3 + usize::from(reader.next_byte() % 8);
    let Some(collinear_points) = horizontal_polygon_points(reader, collinear_count) else {
        return;
    };
    exercise_polygon_candidate(collinear_points);

    let Some(concave_points) = concave_polygon_points(reader) else {
        return;
    };
    exercise_polygon_candidate(concave_points.clone());

    let mut repeated_points = concave_points;
    repeated_points[2] = repeated_points[1];
    exercise_polygon_candidate(repeated_points);

    let arbitrary_count = 3 + usize::from(reader.next_byte() % 8);
    let Some(arbitrary_points) = bounded_polygon_points(reader, arbitrary_count) else {
        return;
    };
    exercise_polygon_candidate(arbitrary_points);
}

fn exercise_polygon_candidate(points: Vec<Point>) {
    if let Ok(polygon) = Polygon::new(points) {
        exercise_polygon_measurements(&polygon);
    }
}

fn exercise_polygon_measurements(polygon: &Polygon) {
    let _ = polygon_signed_area(polygon);
    let _ = polygon_area(polygon);
    let _ = polygon_perimeter(polygon);
    let _ = minimum_area_quad_candidate(polygon);
}

fn bounded_polygon_points(reader: &mut ByteReader<'_>, count: usize) -> Option<Vec<Point>> {
    debug_assert!(count <= MAX_FUZZ_POLYGON_VERTICES);
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        points.push(bounded_polygon_point(reader)?);
    }
    Some(points)
}

fn horizontal_polygon_points(reader: &mut ByteReader<'_>, count: usize) -> Option<Vec<Point>> {
    debug_assert!((3..=MAX_FUZZ_POLYGON_VERTICES).contains(&count));
    let left = bounded_polygon_coordinate(reader);
    let y = bounded_polygon_coordinate(reader);
    let mut points = Vec::with_capacity(count);
    for offset in 0..count {
        points.push(Point::new(left + offset as f32, y).ok()?);
    }
    Some(points)
}

fn concave_polygon_points(reader: &mut ByteReader<'_>) -> Option<Vec<Point>> {
    let left = bounded_polygon_coordinate(reader);
    let top = bounded_polygon_coordinate(reader);
    let width = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let height = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let right = left + width;
    let bottom = top + height;
    let notch_x = left + width * 0.5;
    let notch_y = top + height * 0.45;

    [
        Point::new(left, top).ok(),
        Point::new(right, top).ok(),
        Point::new(right, bottom).ok(),
        Point::new(notch_x, notch_y).ok(),
        Point::new(left, bottom).ok(),
    ]
    .into_iter()
    .collect()
}

fn bounded_polygon_point(reader: &mut ByteReader<'_>) -> Option<Point> {
    Point::new(
        bounded_polygon_coordinate(reader),
        bounded_polygon_coordinate(reader),
    )
    .ok()
}

fn bounded_polygon_coordinate(reader: &mut ByteReader<'_>) -> f32 {
    f32::from(reader.next_byte()) / 8.0 - 16.0
}

fn exercise_crop_kernel(reader: &mut ByteReader<'_>, quadrilateral: Quadrilateral) {
    let Some(dimensions) = bounded_dimensions(reader, MAX_CROP_SIDE) else {
        return;
    };
    let channels = reader.next_byte() % 4 + 1;
    let byte_count = dimensions.pixels() as usize * usize::from(channels);
    let pixels = (0..byte_count)
        .map(|_| reader.next_byte())
        .collect::<Vec<_>>();

    let Some(source) = InterleavedImage::new(dimensions, channels, pixels).ok() else {
        return;
    };
    if let Ok(plan) = classic_perspective_crop_plan(quadrilateral) {
        let _ = classic_perspective_crop(&source, plan);
    }
}

fn bounded_dimensions(reader: &mut ByteReader<'_>, maximum_side: u32) -> Option<ImageDimensions> {
    let width = u32::from(reader.next_byte()) % maximum_side + 1;
    let height = u32::from(reader.next_byte()) % maximum_side + 1;
    ImageDimensions::new(width, height).ok()
}

fn bounded_quadrilateral(reader: &mut ByteReader<'_>) -> Option<Quadrilateral> {
    let left = f32::from(reader.next_byte()) / 8.0 - 16.0;
    let top = f32::from(reader.next_byte()) / 8.0 - 16.0;
    let width = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let height = f32::from(reader.next_byte() % 64) / 8.0 + 1.0;
    let points = if reader.next_byte() & 1 == 0 {
        [
            Point::new(left, top),
            Point::new(left + width, top),
            Point::new(left + width, top + height),
            Point::new(left, top + height),
        ]
    } else {
        let top_inset = width * (f32::from(reader.next_byte() % 48) / 128.0);
        let top_shift = width * (f32::from(reader.next_byte()) / 256.0 - 0.5);
        [
            Point::new(left + top_shift + top_inset, top),
            Point::new(left + top_shift + width - top_inset, top),
            Point::new(left + width, top + height),
            Point::new(left, top + height),
        ]
    };
    let [Ok(first), Ok(second), Ok(third), Ok(fourth)] = points else {
        return None;
    };
    Quadrilateral::new([first, second, third, fourth]).ok()
}

fn score_value(reader: &mut ByteReader<'_>) -> f32 {
    match reader.next_byte() & 0x0f {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => f32::NEG_INFINITY,
        _ => (f32::from(reader.next_byte()) - 128.0) / 32.0,
    }
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn next_byte(&mut self) -> u8 {
        if self.bytes.is_empty() {
            return 0;
        }
        let value = self.bytes[self.offset % self.bytes.len()];
        self.offset = self.offset.wrapping_add(1);
        value
    }

    fn next_u32(&mut self) -> u32 {
        u32::from_le_bytes(core::array::from_fn(|_| self.next_byte()))
    }

    fn next_f32(&mut self) -> f32 {
        f32::from_bits(self.next_u32())
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteReader, MAX_INPUT_BYTES, exercise, exercise_arbitrary_quadrilateral};
    use crate::types::{ImageDimensions, Point, Quadrilateral};

    const GENERATED_STRESS_CASES: usize = 4_096;
    const MUTATION_CAMPAIGN_CASES: usize = 2_048;
    const MUTATION_OPERATIONS_PER_CASE: usize = 8;
    const MUTATION_SEEDS: &[&[u8]] = &[
        b"",
        b"\x00",
        b"\xff",
        b"\x00\x01\x7f\x80\xfe\xff",
        b"\x00\x00\x00\x00\x01\x00\x00\x00\xff\xff\x7f\x7f\x00\x00\x80\x7f",
        b"PaddleOCR-Rust bounded primitive mutation seed",
    ];
    const BYTE_BOUNDARIES: [u8; 8] = [0, 1, 2, 0x7f, 0x80, 0xfe, 0xff, 0x55];
    const FLOAT_BOUNDARIES: [u32; 8] = [
        0x0000_0000,
        0x0000_0001,
        0x3f80_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7fc0_0000,
        0xff80_0000,
        0xffff_ffff,
    ];

    #[test]
    fn byte_driven_fuzz_driver_handles_bounded_seed_corpus() {
        exercise(&[]);
        for seed in 0_u8..=127 {
            let mut input = [0_u8; 97];
            for (index, value) in input.iter_mut().enumerate() {
                *value = seed
                    .wrapping_mul(37)
                    .wrapping_add(index as u8)
                    .rotate_left((index % 8) as u32);
            }
            exercise(&input);
        }
        exercise(&vec![0xA5; MAX_INPUT_BYTES + 1]);
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_generated_stress_corpus() {
        for case_index in 0..GENERATED_STRESS_CASES {
            let length = match case_index {
                0 => 0,
                1 => MAX_INPUT_BYTES,
                _ => (case_index * 193) % (MAX_INPUT_BYTES - 1) + 1,
            };
            let mut state = 0x9E37_79B9_u32 ^ case_index as u32;
            let mut input = Vec::with_capacity(length);
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                input.push((state >> 24) as u8);
            }
            exercise(&input);
        }
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_deterministic_mutation_campaign() {
        for case_index in 0..MUTATION_CAMPAIGN_CASES {
            let seed = MUTATION_SEEDS[case_index % MUTATION_SEEDS.len()];
            let mut input = seed.to_vec();
            let mut state = 0xD1B5_4A35_u32 ^ case_index as u32;

            for operation_index in 0..MUTATION_OPERATIONS_PER_CASE {
                mutate_input(&mut input, &mut state, (case_index + operation_index) % 7);
                assert!(
                    input.len() <= MAX_INPUT_BYTES,
                    "mutation case {case_index} exceeded its input bound"
                );
            }

            exercise(&input);
        }
    }

    #[test]
    fn byte_driven_fuzz_driver_handles_bounded_polygon_variants() {
        for selector in 0_u8..=u8::MAX {
            let mut input = [0_u8; 97];
            for (index, value) in input.iter_mut().enumerate() {
                *value = selector
                    .wrapping_add((index as u8).wrapping_mul(29))
                    .rotate_left((index % 8) as u32);
            }
            exercise(&input);
        }
    }

    #[test]
    fn arbitrary_quadrilateral_route_handles_small_and_large_finite_coordinates() {
        let dimensions = must_dimensions(32, 32);
        let reader_bytes = [
            0x00, 0x00, 0x80, 0x7f, // Positive infinity: checked mapping error.
            0x00, 0x00, 0xc0, 0x7f, // Quiet NaN: checked mapping error.
            0xff, 0xff, 0x7f, 0x7f, // Largest finite positive f32.
            0xff, 0xff, 0x7f, 0xff, // Largest finite negative f32.
        ];

        for side in [1.0e-7_f32, 1.0_f32, 1_024.0_f32, f32::MAX / 2.0] {
            let quadrilateral = must_quadrilateral([
                must_point(-side, -side),
                must_point(side, -side),
                must_point(side, side),
                must_point(-side, side),
            ]);
            let mut reader = ByteReader::new(&reader_bytes);
            exercise_arbitrary_quadrilateral(quadrilateral.points(), dimensions, &mut reader);
        }
    }

    fn must_dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(dimensions) => dimensions,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        }
    }

    fn must_point(x: f32, y: f32) -> Point {
        match Point::new(x, y) {
            Ok(point) => point,
            Err(error) => panic!("expected finite point, got {error}"),
        }
    }

    fn must_quadrilateral(points: [Point; 4]) -> Quadrilateral {
        match Quadrilateral::new(points) {
            Ok(quadrilateral) => quadrilateral,
            Err(error) => panic!("expected valid quadrilateral, got {error}"),
        }
    }

    fn mutate_input(input: &mut Vec<u8>, state: &mut u32, mutation_kind: usize) {
        match mutation_kind {
            0 => flip_one_bit(input, state),
            1 => overwrite_one_byte(input, state),
            2 => insert_one_byte(input, state),
            3 => remove_one_byte(input, state),
            4 => duplicate_one_byte(input, state),
            5 => inject_float_boundary(input, state),
            6 => truncate_input(input, state),
            _ => unreachable!("mutation kind is reduced modulo seven"),
        }
    }

    fn flip_one_bit(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            input.push(1_u8 << (next_word(state) % 8));
            return;
        }
        let index = next_index(state, input.len());
        input[index] ^= 1_u8 << (next_word(state) % 8);
    }

    fn overwrite_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        let value = BYTE_BOUNDARIES[next_index(state, BYTE_BOUNDARIES.len())];
        if input.is_empty() {
            input.push(value);
            return;
        }
        let index = next_index(state, input.len());
        input[index] = value;
    }

    fn insert_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.len() == MAX_INPUT_BYTES {
            return;
        }
        let index = next_index(state, input.len() + 1);
        let value = BYTE_BOUNDARIES[next_index(state, BYTE_BOUNDARIES.len())];
        input.insert(index, value);
    }

    fn remove_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            return;
        }
        let index = next_index(state, input.len());
        input.remove(index);
    }

    fn duplicate_one_byte(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() || input.len() == MAX_INPUT_BYTES {
            return;
        }
        let source = next_index(state, input.len());
        let destination = next_index(state, input.len() + 1);
        input.insert(destination, input[source]);
    }

    fn inject_float_boundary(input: &mut Vec<u8>, state: &mut u32) {
        let bits = FLOAT_BOUNDARIES[next_index(state, FLOAT_BOUNDARIES.len())];
        write_bounded_bytes(input, state, &bits.to_le_bytes());
    }

    fn truncate_input(input: &mut Vec<u8>, state: &mut u32) {
        if input.is_empty() {
            return;
        }
        input.truncate(next_index(state, input.len() + 1));
    }

    fn write_bounded_bytes(input: &mut Vec<u8>, state: &mut u32, bytes: &[u8]) {
        let start = next_index(state, input.len() + 1);
        for (offset, byte) in bytes.iter().copied().enumerate() {
            let index = start + offset;
            if index < input.len() {
                input[index] = byte;
            } else if input.len() < MAX_INPUT_BYTES {
                input.push(byte);
            } else {
                let replacement = next_index(state, input.len());
                input[replacement] = byte;
            }
        }
    }

    fn next_index(state: &mut u32, length: usize) -> usize {
        debug_assert!(length > 0);
        next_word(state) as usize % length
    }

    fn next_word(state: &mut u32) -> u32 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *state
    }
}
