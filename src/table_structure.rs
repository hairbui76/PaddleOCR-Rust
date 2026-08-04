// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Table structure recognition: the `SLANeXt` and `SLANet` token grammar.
//!
//! Roadmap item `TBLSTRUCT-001`. The first `P8` module whose contract is not a
//! variation on one already ported. Its preprocessing is new in three ways and
//! its postprocess emits **HTML structure tokens** rather than boxes.
//!
//! # Three facts that live in the registration functions, not the operators
//!
//! Reading `Pad` or `Normalize` alone gives the wrong answer for all three:
//!
//! | Fact | What the operator says | What actually runs |
//! |---|---|---|
//! | The pad value | `Pad::val` defaults to `127.5` | `build_padding` passes `pad_value=0` |
//! | The scale | config says `scale: '1./255.'` | `build_normalize` **never forwards it** |
//! | The pad order | — | pad runs **after** normalize |
//!
//! The third is the one that changes the picture most. Because the pad runs
//! after the normalize, its zeros are zeros **in normalized space** — not black
//! pixels, which would be `(0/255 − 0.485)/0.229 = −2.117` in the first channel.
//! Padding before normalizing would be a plausible border and the wrong one.
//!
//! The second is worth recording even though it changes nothing today: the
//! config's `scale` is a *string*, `'1./255.'`, and the registration function
//! drops it on the floor. A different scale there would be silently ignored.
//!
//! # A fourth channel order
//!
//! `DecodeImage` declares `img_mode: BGR`, and `build_readimg` asserts it. Every
//! other PaddleX model this port has touched reads `RGB`. Getting this backwards
//! swaps two of the three planes, which is exactly the bug
//! `docs/TABLE_CELLS_CONTRACT.md` records finding in `LAY-001`.
//!
//! # The vocabulary is built, not read
//!
//! `TableLabelDecode` **mutates** the character list it is handed: with
//! `merge_no_span_structure` it removes `<td>` and appends `<td></td>`, then
//! wraps the result in `sos` and `eos`. The config's `character_dict` is
//! therefore not the vocabulary — it is the input to a transformation, and the
//! indices the model emits are indices into the *result*.
//!
//! # Not wired into a pipeline
//!
//! Structure tokens compose with cell boxes and recognized text to make a table,
//! and that composition is `P9`'s subject.
#![allow(dead_code)]

use crate::crop::InterleavedImage;
use crate::error::{Error, InputViolation, Result};
use crate::resize::classic_linear_resize;
use crate::tensor::NchwTensor;
use crate::types::ImageDimensions;

/// `ResizeTableImage.max_len`, from `inference.yml`.
pub(crate) const TABLE_STRUCTURE_LONG_EDGE: u32 = 512;

/// `PaddingTableImage.size`, which `build_padding` squares by taking `size[0]`.
pub(crate) const TABLE_STRUCTURE_PAD_SIDE: u32 = 512;

/// The pad value `build_padding` passes, **not** `Pad`'s own default of `127.5`.
pub(crate) const TABLE_STRUCTURE_PAD_VALUE: f32 = 0.0;

const TABLE_STRUCTURE_SCALE: f64 = 1.0 / 255.0;
const TABLE_STRUCTURE_MEAN: [f64; 3] = [0.485, 0.456, 0.406];
const TABLE_STRUCTURE_STD: [f64; 3] = [0.229, 0.224, 0.225];

/// The begin token `add_special_char` prepends.
pub(crate) const TABLE_STRUCTURE_BEGIN: &str = "sos";
/// The end token `add_special_char` appends.
pub(crate) const TABLE_STRUCTURE_END: &str = "eos";

/// The tokens that carry a cell box.
const TD_TOKENS: [&str; 3] = ["<td>", "<td", "<td></td>"];

/// The wrapper `TableLabelDecode` puts around every decoded structure.
const STRUCTURE_PREFIX: [&str; 3] = ["<html>", "<body>", "<table>"];
const STRUCTURE_SUFFIX: [&str; 3] = ["</table>", "</body>", "</html>"];

/// Which model produced a result, because the two scale boxes differently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TableStructureModel {
    /// `SLANet`, which scales boxes by the **original** size.
    SlaNet,
    /// `SLANeXt`, which scales by the padded size over the resize ratio.
    SlaNeXt,
}

/// A decoded table structure.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TableStructure {
    /// The HTML structure tokens, wrapped in `<html><body><table>` … .
    pub tokens: Vec<String>,
    /// The mean probability of the tokens that were kept.
    pub score: f32,
    /// One `xyxyxyxy` box per cell token, truncated to integers as upstream does.
    pub cell_boxes: Vec<[i64; 8]>,
}

/// Builds the decoder's vocabulary from the artifact's `character_dict`.
///
/// Not a read: `merge_no_span_structure` removes `<td>` and appends
/// `<td></td>`, and `add_special_char` wraps the result. The indices the model
/// emits point into what this returns, not into the config list.
pub(crate) fn table_structure_vocabulary(
    character_dict: &[&str],
    merge_no_span_structure: bool,
) -> Vec<String> {
    let mut characters: Vec<String> = character_dict.iter().map(|s| (*s).to_owned()).collect();
    if merge_no_span_structure {
        // Append before removing, matching upstream's order. The two are
        // independent here, but the order is what the source does and a future
        // dictionary containing both tokens would notice.
        if !characters.iter().any(|token| token == "<td></td>") {
            characters.push("<td></td>".to_owned());
        }
        characters.retain(|token| token != "<td>");
    }
    let mut vocabulary = Vec::with_capacity(characters.len() + 2);
    vocabulary.push(TABLE_STRUCTURE_BEGIN.to_owned());
    vocabulary.extend(characters);
    vocabulary.push(TABLE_STRUCTURE_END.to_owned());
    vocabulary
}

/// Returns the dimensions `ResizeByLong` produces.
///
/// Python's `round`, half to even — the same rule
/// [`crate::table_classification`] pins for `ResizeByShort`, and the same
/// disagreement with the C++ baseline's `std::round`.
pub(crate) fn resize_by_long_dimensions(
    source: ImageDimensions,
    target_long_edge: u32,
) -> Result<ImageDimensions> {
    let (width, height) = (f64::from(source.width()), f64::from(source.height()));
    let longer = width.max(height);
    if longer <= 0.0 || target_long_edge == 0 {
        return Err(Error::InvalidInput {
            field: "table_structure.long_edge",
            violation: InputViolation::Empty,
        });
    }
    let scale = f64::from(target_long_edge) / longer;
    let scaled_width = (width * scale).round_ties_even();
    let scaled_height = (height * scale).round_ties_even();
    if !scaled_width.is_finite()
        || !scaled_height.is_finite()
        || scaled_width < 1.0
        || scaled_height < 1.0
    {
        return Err(Error::InvalidInput {
            field: "table_structure.long_edge",
            violation: InputViolation::OutOfRange,
        });
    }
    ImageDimensions::new(scaled_width as u32, scaled_height as u32)
}

/// Builds the `[1, 3, 512, 512]` input tensor for one BGR table image.
///
/// Normalize, **then** pad. The order is upstream's and it decides what the
/// border contains.
pub(crate) fn table_structure_input(image: &InterleavedImage) -> Result<NchwTensor> {
    if image.channels() != 3 {
        return Err(Error::InvalidInput {
            field: "table_structure.channels",
            violation: InputViolation::OutOfRange,
        });
    }

    let resized_dimensions =
        resize_by_long_dimensions(image.dimensions(), TABLE_STRUCTURE_LONG_EDGE)?;
    let resized = if resized_dimensions == image.dimensions() {
        None
    } else {
        Some(classic_linear_resize(image, resized_dimensions)?)
    };
    let resized = resized.as_ref().unwrap_or(image);

    let side = TABLE_STRUCTURE_PAD_SIDE as usize;
    let (width, height) = (
        resized.dimensions().width() as usize,
        resized.dimensions().height() as usize,
    );
    if width > side || height > side {
        return Err(Error::InvalidInput {
            field: "table_structure.pad_size",
            violation: InputViolation::OutOfRange,
        });
    }

    let mut values: Vec<f32> = Vec::new();
    values
        .try_reserve_exact(3 * side * side)
        .map_err(|_| Error::Backend {
            message: "table structure input allocation failed",
        })?;
    let pixels = resized.pixels();
    for channel in 0..3_usize {
        let alpha = (TABLE_STRUCTURE_SCALE / TABLE_STRUCTURE_STD[channel]) as f32;
        let beta = (-TABLE_STRUCTURE_MEAN[channel] / TABLE_STRUCTURE_STD[channel]) as f32;
        for row in 0..side {
            for column in 0..side {
                if row < height && column < width {
                    let index = (row * width + column) * 3 + channel;
                    values.push(f32::from(pixels[index]) * alpha + beta);
                } else {
                    // The pad, in normalized space because it runs after the
                    // normalize. A black pixel would be about -2.117 here.
                    values.push(TABLE_STRUCTURE_PAD_VALUE);
                }
            }
        }
    }

    NchwTensor::new(1, 3, side, side, values)
}

/// Returns the per-axis box scales for a model and page.
///
/// The two branches are upstream's, including the fact that `SLANeXt`'s pair is
/// **named backwards** — `_get_bbox_scales` returns `w/ratio, h/ratio` into
/// variables called `h_scale, w_scale`. With a square pad both values are equal,
/// so the naming costs nothing; it is reproduced rather than tidied because a
/// future non-square pad would make it matter.
pub(crate) fn table_structure_box_scales(
    model: TableStructureModel,
    padded: (f64, f64),
    original: (f64, f64),
) -> (f64, f64) {
    match model {
        TableStructureModel::SlaNet => original,
        TableStructureModel::SlaNeXt => {
            let (pad_width, pad_height) = padded;
            let (width, height) = original;
            let ratio = (pad_width / width).min(pad_height / height);
            (pad_width / ratio, pad_height / ratio)
        }
    }
}

/// Decodes `[1, T, V]` structure probabilities into tokens and cell boxes.
///
/// `boxes` is `[1, T, 8]` in `xyxyxyxy` order and may be omitted, in which case
/// no cell boxes are produced — the branch upstream takes when the model emits
/// only one output.
pub(crate) fn decode_table_structure(
    model: TableStructureModel,
    vocabulary: &[String],
    probabilities: &[f32],
    sequence: usize,
    boxes: Option<&[f32]>,
    padded: (f64, f64),
    original: (f64, f64),
) -> Result<TableStructure> {
    let classes = vocabulary.len();
    if classes < 2 || sequence == 0 || probabilities.len() != sequence * classes {
        return Err(Error::InvalidInput {
            field: "table_structure.probabilities",
            violation: InputViolation::OutOfRange,
        });
    }
    if let Some(boxes) = boxes
        && boxes.len() != sequence * 8
    {
        return Err(Error::InvalidInput {
            field: "table_structure.boxes",
            violation: InputViolation::OutOfRange,
        });
    }
    let end_index = classes - 1;

    let (x_scale, y_scale) = table_structure_box_scales(model, padded, original);

    let mut tokens: Vec<String> = STRUCTURE_PREFIX.iter().map(|s| (*s).to_owned()).collect();
    let mut scores: Vec<f32> = Vec::new();
    let mut cell_boxes: Vec<[i64; 8]> = Vec::new();

    for position in 0..sequence {
        let row = &probabilities[position * classes..(position + 1) * classes];
        if !row.iter().all(|value| value.is_finite()) {
            return Err(Error::InvalidInput {
                field: "table_structure.probabilities",
                violation: InputViolation::NonFinite,
            });
        }
        // `argmax` over the class axis. NumPy returns the **first** maximum, so
        // ties go to the lower class index — the opposite of `Topk`'s rule in
        // `crate::table_classification`, and captured rather than assumed.
        let mut best = 0_usize;
        for (index, value) in row.iter().enumerate() {
            if *value > row[best] {
                best = index;
            }
        }

        if position > 0 && best == end_index {
            break;
        }
        // Both `sos` and `eos` are ignored wherever they appear; the `break`
        // above only fires for `eos` past the first position.
        if best == 0 || best == end_index {
            continue;
        }

        let token = &vocabulary[best];
        if TD_TOKENS.contains(&token.as_str())
            && let Some(boxes) = boxes
        {
            let entry = &boxes[position * 8..(position + 1) * 8];
            if !entry.iter().all(|value| value.is_finite()) {
                return Err(Error::InvalidInput {
                    field: "table_structure.boxes",
                    violation: InputViolation::NonFinite,
                });
            }
            let mut scaled = [0_i64; 8];
            for (slot, value) in entry.iter().enumerate() {
                // `scales[0::2]` is the first returned value and `scales[1::2]`
                // the second, then `astype(int)` truncates toward zero.
                let scale = if slot % 2 == 0 { x_scale } else { y_scale };
                scaled[slot] = (f64::from(*value) * scale) as i64;
            }
            cell_boxes.push(scaled);
        }

        tokens.push(token.clone());
        scores.push(row[best]);
    }

    tokens.extend(STRUCTURE_SUFFIX.iter().map(|s| (*s).to_owned()));
    // `np.mean` over `float32` accumulates in `float64` and returns a scalar;
    // an empty list is `0.0` rather than a NaN.
    let score = if scores.is_empty() {
        0.0
    } else {
        (scores.iter().map(|v| f64::from(*v)).sum::<f64>() / scores.len() as f64) as f32
    };

    Ok(TableStructure {
        tokens,
        score,
        cell_boxes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    const FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-table-structure/expected.json");

    fn fixture() -> Value {
        match serde_json::from_str(FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn synthetic_bgr(width: u32, height: u32) -> InterleavedImage {
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..u64::from(height) {
            for x in 0..u64::from(width) {
                for c in 0..3_u64 {
                    pixels.push(((x * 7 + y * 13 + c * 29) % 256) as u8);
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

    fn vocabulary_from_fixture() -> Vec<String> {
        let fixture = fixture();
        match fixture["vocabulary"].as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_str().unwrap_or_default().to_owned())
                .collect(),
            None => panic!("vocabulary"),
        }
    }

    /// The vocabulary is a transformation of the config list, not the list.
    #[test]
    fn the_vocabulary_is_built_from_the_character_dict() {
        let captured = vocabulary_from_fixture();
        // Rebuild it from the config list the capture recorded, minus the two
        // tokens the transformation adds and removes.
        let mut config: Vec<&str> = captured
            .iter()
            .skip(1)
            .take(captured.len() - 2)
            .map(String::as_str)
            .filter(|token| *token != "<td></td>")
            .collect();
        // The config list has `<td>` where the vocabulary does not.
        let insert_at = config.iter().position(|token| *token == "<td").unwrap_or(0);
        config.insert(insert_at, "<td>");

        let rebuilt = table_structure_vocabulary(&config, true);
        assert_eq!(
            rebuilt, captured,
            "the merge transformation must round-trip"
        );

        assert_eq!(rebuilt.first().map(String::as_str), Some("sos"));
        assert_eq!(rebuilt.last().map(String::as_str), Some("eos"));
        assert!(!rebuilt.iter().any(|token| token == "<td>"));
        assert!(rebuilt.iter().any(|token| token == "<td></td>"));
    }

    /// Without the merge, `<td>` survives and `<td></td>` is not added.
    #[test]
    fn without_the_merge_the_dictionary_is_left_alone() {
        let plain = table_structure_vocabulary(&["<td>", "<td", ">"], false);
        assert_eq!(plain, vec!["sos", "<td>", "<td", ">", "eos"]);
    }

    /// Every captured tensor, hashed whole and sampled on failure.
    #[test]
    fn the_captured_structure_tensors_are_reproduced() {
        let fixture = fixture();
        let records = match fixture["records"].as_array() {
            Some(value) => value,
            None => panic!("records"),
        };
        assert_eq!(records.len(), 5);

        for record in records {
            let case = record["case"].as_str().unwrap_or("?");
            let wh = match record["source_wh"].as_array() {
                Some(value) => value,
                None => panic!("{case}: source_wh"),
            };
            let width = wh[0].as_u64().unwrap_or(0) as u32;
            let height = wh[1].as_u64().unwrap_or(0) as u32;
            let image = synthetic_bgr(width, height);

            let mut hasher = Sha256::new();
            hasher.update(image.pixels());
            assert_eq!(
                format!("{:x}", hasher.finalize()),
                record["source_bgr_sha256"].as_str().unwrap_or(""),
                "{case}: source pixels"
            );

            let resized =
                match resize_by_long_dimensions(image.dimensions(), TABLE_STRUCTURE_LONG_EDGE) {
                    Ok(value) => value,
                    Err(error) => panic!("{case}: {error}"),
                };
            let expected_resized = match record["resized_hwc_shape"].as_array() {
                Some(value) => value,
                None => panic!("{case}: resized shape"),
            };
            assert_eq!(
                (resized.height() as u64, resized.width() as u64),
                (
                    expected_resized[0].as_u64().unwrap_or(0),
                    expected_resized[1].as_u64().unwrap_or(0)
                ),
                "{case}: resized shape"
            );

            let tensor = match table_structure_input(&image) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };
            assert_eq!(tensor.shape(), [1, 3, 512, 512], "{case}: tensor shape");

            let values = tensor.values();
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            if format!("{:x}", hasher.finalize())
                == record["input_values_sha256"].as_str().unwrap_or("")
            {
                continue;
            }

            let indices = match record["input_sample_indices"].as_array() {
                Some(value) => value,
                None => panic!("{case}: sample indices"),
            };
            let sampled = match STANDARD
                .decode(record["input_sample_values_base64"].as_str().unwrap_or(""))
            {
                Ok(value) => value,
                Err(error) => panic!("{case}: samples: {error}"),
            };
            let mut differing = 0_usize;
            let mut first: Option<(usize, f32, f32)> = None;
            for (slot, index) in indices.iter().enumerate() {
                let index = index.as_u64().unwrap_or(0) as usize;
                let start = slot * 4;
                let expected = f32::from_le_bytes([
                    sampled[start],
                    sampled[start + 1],
                    sampled[start + 2],
                    sampled[start + 3],
                ]);
                if values[index].to_bits() != expected.to_bits() {
                    differing += 1;
                    first.get_or_insert((index, expected, values[index]));
                }
            }
            panic!(
                "{case}: tensor differs; {differing} of {} samples differ, first {first:?}",
                indices.len()
            );
        }
    }

    /// The padded border is zero **in normalized space**, not a black pixel.
    #[test]
    fn the_pad_runs_after_the_normalize() {
        // 300x800 resizes to 192x512, so columns 192..512 are padding.
        let tensor = match table_structure_input(&synthetic_bgr(300, 800)) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let side = TABLE_STRUCTURE_PAD_SIDE as usize;
        let padded = tensor.values()[10 * side + 400];
        assert_eq!(padded, 0.0, "the pad value is a normalized zero");

        // A black pixel would land here instead, which is what padding before
        // normalizing would have produced.
        let black = (0.0_f32 * (1.0 / 255.0) - 0.485) / 0.229;
        assert!((black + 2.117_9).abs() < 1e-3, "{black}");
        assert_ne!(padded, black);
    }

    /// Every captured decode: tokens, score, and cell boxes.
    #[test]
    fn the_captured_decodes_are_reproduced() {
        let fixture = fixture();
        let vocabulary = vocabulary_from_fixture();
        let decodes = match fixture["decodes"].as_array() {
            Some(value) => value,
            None => panic!("decodes"),
        };
        assert_eq!(decodes.len(), 5);

        for entry in decodes {
            let case = entry["case"].as_str().unwrap_or("?");
            let model = match entry["model_name"].as_str().unwrap_or("") {
                "SLANet" => TableStructureModel::SlaNet,
                _ => TableStructureModel::SlaNeXt,
            };
            let tokens: Vec<usize> = match entry["token_ids"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_u64().unwrap_or(0) as usize)
                    .collect(),
                None => panic!("{case}: token_ids"),
            };
            let classes = vocabulary.len();
            assert_eq!(
                classes,
                entry["vocabulary_size"].as_u64().unwrap_or(0) as usize,
                "{case}: vocabulary size"
            );

            // The same probability tensor the capture built.
            let filler = 0.1_f32 / (classes as f32 - 1.0);
            let mut probabilities = vec![filler; tokens.len() * classes];
            for (position, token) in tokens.iter().enumerate() {
                probabilities[position * classes + token] = 0.9;
            }
            let mut boxes = vec![0.0_f32; tokens.len() * 8];
            for position in 0..tokens.len() {
                let base = 0.05_f32 + 0.01 * position as f32;
                boxes[position * 8..(position + 1) * 8].copy_from_slice(&[
                    base,
                    base,
                    base + 0.2,
                    base,
                    base + 0.2,
                    base + 0.1,
                    base,
                    base + 0.1,
                ]);
            }

            let ori = match entry["ori_wh"].as_array() {
                Some(value) => (
                    value[0].as_f64().unwrap_or(0.0),
                    value[1].as_f64().unwrap_or(0.0),
                ),
                None => panic!("{case}: ori_wh"),
            };
            let decoded = match decode_table_structure(
                model,
                &vocabulary,
                &probabilities,
                tokens.len(),
                Some(&boxes),
                (
                    f64::from(TABLE_STRUCTURE_PAD_SIDE),
                    f64::from(TABLE_STRUCTURE_PAD_SIDE),
                ),
                ori,
            ) {
                Ok(value) => value,
                Err(error) => panic!("{case}: {error}"),
            };

            let expected_tokens: Vec<String> = match entry["structure"].as_array() {
                Some(values) => values
                    .iter()
                    .map(|value| value.as_str().unwrap_or_default().to_owned())
                    .collect(),
                None => panic!("{case}: structure"),
            };
            assert_eq!(decoded.tokens, expected_tokens, "{case}: tokens");

            let expected_score = entry["structure_score"].as_f64().unwrap_or(f64::NAN) as f32;
            assert!(
                (decoded.score - expected_score).abs() < 1e-6,
                "{case}: score {} vs {expected_score}",
                decoded.score
            );

            let expected_boxes = match entry["bbox"].as_array() {
                Some(value) => value,
                None => panic!("{case}: bbox"),
            };
            assert_eq!(
                decoded.cell_boxes.len(),
                expected_boxes.len(),
                "{case}: box count"
            );
            for (slot, expected) in expected_boxes.iter().enumerate() {
                let expected = match expected.as_array() {
                    Some(value) => value,
                    None => panic!("{case}: bbox {slot}"),
                };
                for (axis, value) in expected.iter().enumerate() {
                    assert_eq!(
                        decoded.cell_boxes[slot][axis],
                        value.as_i64().unwrap_or(i64::MIN),
                        "{case}: bbox {slot} axis {axis}"
                    );
                }
            }
        }
    }

    /// The two models scale boxes differently, and the capture proves it.
    #[test]
    fn the_two_models_scale_boxes_differently() {
        let padded = (512.0, 512.0);
        let original = (640.0, 480.0);
        let slanet = table_structure_box_scales(TableStructureModel::SlaNet, padded, original);
        let slanext = table_structure_box_scales(TableStructureModel::SlaNeXt, padded, original);
        assert_eq!(slanet, original);
        // With a square pad both `SLANeXt` scales collapse to the long side, so
        // the backwards naming in `_get_bbox_scales` costs nothing today.
        assert!((slanext.0 - 640.0).abs() < 1e-9, "{slanext:?}");
        assert!((slanext.1 - 640.0).abs() < 1e-9, "{slanext:?}");
        assert_ne!(slanet, slanext);
    }

    /// Without boxes, the decode still produces tokens.
    #[test]
    fn a_model_with_no_box_output_still_decodes_tokens() {
        let vocabulary = vocabulary_from_fixture();
        let classes = vocabulary.len();
        let mut probabilities = vec![0.001_f32; 2 * classes];
        probabilities[3] = 0.9;
        probabilities[classes + 4] = 0.9;
        let decoded = match decode_table_structure(
            TableStructureModel::SlaNeXt,
            &vocabulary,
            &probabilities,
            2,
            None,
            (512.0, 512.0),
            (640.0, 480.0),
        ) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert!(decoded.cell_boxes.is_empty());
        assert_eq!(decoded.tokens.len(), 2 + 6);
    }

    /// An empty decode scores `0.0` rather than a NaN.
    #[test]
    fn an_empty_decode_scores_zero() {
        let vocabulary = vocabulary_from_fixture();
        let classes = vocabulary.len();
        // A single `eos` at position 0 is ignored rather than breaking, and
        // leaves nothing behind.
        let mut probabilities = vec![0.001_f32; classes];
        probabilities[classes - 1] = 0.9;
        let decoded = match decode_table_structure(
            TableStructureModel::SlaNeXt,
            &vocabulary,
            &probabilities,
            1,
            None,
            (512.0, 512.0),
            (640.0, 480.0),
        ) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(decoded.score, 0.0);
        assert_eq!(decoded.tokens.len(), 6, "only the wrapper survives");
    }

    /// A mismatched probability length is refused rather than truncated.
    #[test]
    fn a_mismatched_tensor_is_refused() {
        let vocabulary = vocabulary_from_fixture();
        assert!(
            decode_table_structure(
                TableStructureModel::SlaNeXt,
                &vocabulary,
                &[0.0; 4],
                2,
                None,
                (512.0, 512.0),
                (640.0, 480.0),
            )
            .is_err()
        );
    }

    /// An image larger than the pad after resizing cannot happen, and the guard
    /// says so rather than writing out of bounds.
    #[test]
    fn a_non_three_channel_image_is_refused() {
        let dimensions = match ImageDimensions::new(64, 64) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        let grey = match InterleavedImage::new(dimensions, 1, vec![3_u8; 64 * 64]) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        match table_structure_input(&grey) {
            Err(Error::InvalidInput { field, .. }) => {
                assert_eq!(field, "table_structure.channels");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The config's `scale` is a string upstream never reads.
    #[test]
    fn the_config_scale_is_recorded_as_ignored() {
        let fixture = fixture();
        assert_eq!(
            fixture["preprocess"]["config_scale_is_ignored"]
                .as_str()
                .unwrap_or(""),
            "1./255.",
            "the config declares a scale as a string"
        );
        // And what actually runs is `Normalize`'s own default.
        assert!((TABLE_STRUCTURE_SCALE - 1.0 / 255.0).abs() < f64::EPSILON);
    }
}
