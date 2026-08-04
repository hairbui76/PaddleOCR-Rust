// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Recognition batching policy: aspect sort, batch width, per-crop resize.
//!
//! `docs/CLASSIC_OCR_CONTRACT.md` freezes this for M2, from
//! `tools/infer/predict_rec.py` and `ppocr/data/imaug/rec_img_aug.py`:
//!
//! - crops are sorted by `width / height` before batching, and the original
//!   order is restored afterwards;
//! - the base shape is `[3, 48, 320]`;
//! - for each batch, `max_wh_ratio` starts at `320 / 48` and grows to the
//!   largest crop ratio in that batch;
//! - each crop resizes to height `48` with width `ceil(48 * crop_ratio)`,
//!   capped by the batch width, and is right-padded with zeros.
//!
//! Two details are easy to lose and are pinned by tests. The batch width floor
//! is `320`, so a batch of narrow crops still pads to `320` rather than
//! shrinking. And the per-crop width uses `ceil`, not rounding, so a crop whose
//! exact width is fractional always grows.
//!
//! This module computes the plan only. It performs no resize and holds no
//! pixels; `src/resize.rs` and `src/tensor.rs` do that work.

use crate::error::{Error, InputViolation, Result};

/// The frozen recognition input height.
pub(crate) const RECOGNITION_HEIGHT: u32 = 48;

/// The frozen base recognition width.
pub(crate) const RECOGNITION_BASE_WIDTH: u32 = 320;

/// Maximum crops accepted in one batch plan.
const MAX_BATCH: usize = 256;

/// One crop's place in the batch: its original index and resized width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BatchedCrop {
    /// Index of this crop in the caller's original order.
    pub(crate) original_index: usize,
    /// Width this crop resizes to before right-padding.
    pub(crate) resized_width: u32,
}

/// A batch plan: the shared padded width and the crops in sorted order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BatchPlan {
    /// Padded width shared by every crop in the batch.
    pub(crate) batch_width: u32,
    /// Crops in aspect-sorted order, each carrying its original index.
    pub(crate) crops: Vec<BatchedCrop>,
}

/// Builds the batch plan for one group of crops.
///
/// `sizes` are the crop dimensions in caller order. The returned crops are
/// sorted by aspect ratio; `original_index` is what restores the caller's order
/// after decoding.
pub(crate) fn plan_batch(sizes: &[(u32, u32)]) -> Result<BatchPlan> {
    if sizes.is_empty() {
        return Err(Error::InvalidInput {
            field: "recognizer.batch",
            violation: InputViolation::Empty,
        });
    }
    if sizes.len() > MAX_BATCH {
        return Err(Error::ResourceLimit {
            resource: "recognizer.batch",
            limit: MAX_BATCH as u64,
            actual: sizes.len() as u64,
        });
    }
    if sizes
        .iter()
        .any(|(width, height)| *width == 0 || *height == 0)
    {
        return Err(Error::InvalidInput {
            field: "recognizer.crop",
            violation: InputViolation::Empty,
        });
    }

    let mut ordered: Vec<(usize, f64)> = sizes
        .iter()
        .enumerate()
        .map(|(index, (width, height))| (index, f64::from(*width) / f64::from(*height)))
        .collect();
    // A stable sort keeps equal-ratio crops in their original relative order,
    // which is what makes the later restore deterministic.
    ordered.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // The batch ratio starts at the base shape's own ratio and only grows.
    let base_ratio = f64::from(RECOGNITION_BASE_WIDTH) / f64::from(RECOGNITION_HEIGHT);
    let max_ratio = ordered
        .iter()
        .fold(base_ratio, |widest, (_, ratio)| widest.max(*ratio));
    let batch_width = scaled_width(max_ratio)?;

    let crops = ordered
        .into_iter()
        .map(|(original_index, ratio)| {
            Ok(BatchedCrop {
                original_index,
                resized_width: scaled_width(ratio)?.min(batch_width),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(BatchPlan { batch_width, crops })
}

/// Returns `ceil(height * ratio)` as the upstream `math.ceil` does.
fn scaled_width(ratio: f64) -> Result<u32> {
    let scaled = (f64::from(RECOGNITION_HEIGHT) * ratio).ceil();
    if !scaled.is_finite() || scaled < 1.0 || scaled > f64::from(u32::MAX) {
        return Err(Error::InvalidInput {
            field: "recognizer.crop",
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(scaled as u32)
}

/// Restores caller order from decoded results in batch order.
pub(crate) fn restore_order<T>(plan: &BatchPlan, decoded: Vec<T>) -> Result<Vec<Option<T>>> {
    if decoded.len() != plan.crops.len() {
        return Err(Error::InvalidInput {
            field: "recognizer.batch",
            violation: InputViolation::OutOfRange,
        });
    }
    let mut restored: Vec<Option<T>> = (0..decoded.len()).map(|_| None).collect();
    for (crop, value) in plan.crops.iter().zip(decoded) {
        if crop.original_index >= restored.len() {
            return Err(Error::InvalidInput {
                field: "recognizer.batch",
                violation: InputViolation::OutOfRange,
            });
        }
        restored[crop.original_index] = Some(value);
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(sizes: &[(u32, u32)]) -> BatchPlan {
        match plan_batch(sizes) {
            Ok(plan) => plan,
            Err(error) => panic!("expected a batch plan, got {error}"),
        }
    }

    #[test]
    fn narrow_crops_still_pad_to_the_base_width() {
        // Every crop is far narrower than the base ratio, so the batch width
        // must stay at 320 rather than shrinking to fit.
        let plan = plan(&[(20, 48), (30, 48)]);
        assert_eq!(plan.batch_width, RECOGNITION_BASE_WIDTH);
        assert_eq!(plan.crops[0].resized_width, 20);
        assert_eq!(plan.crops[1].resized_width, 30);
    }

    #[test]
    fn a_wide_crop_raises_the_batch_width_for_everyone() {
        // 96/48 = 2.0 -> 96 wide; 960/48 = 20.0 -> 960 wide and it sets the
        // batch width.
        let plan = plan(&[(96, 48), (960, 48)]);
        assert_eq!(plan.batch_width, 960);
        assert_eq!(plan.crops[0].resized_width, 96);
        assert_eq!(plan.crops[1].resized_width, 960);
    }

    #[test]
    fn crops_are_sorted_by_aspect_ratio_and_carry_their_origin() {
        let plan = plan(&[(480, 48), (48, 48), (240, 48)]);
        let order: Vec<usize> = plan.crops.iter().map(|c| c.original_index).collect();
        assert_eq!(order, [1, 2, 0], "narrowest first");
    }

    #[test]
    fn the_width_uses_ceil_so_a_fractional_crop_grows() {
        // 50/48 = 1.041666..., times 48 is 50.0 exactly, so this checks the
        // neighbouring case: 49/48 * 48 = 49.0, and a truly fractional ratio.
        let plan = plan(&[(49, 47)]);
        // 49/47 = 1.04255..., * 48 = 50.042..., ceil -> 51.
        assert_eq!(plan.crops[0].resized_width, 51);
    }

    #[test]
    fn a_resized_width_never_exceeds_the_batch_width() {
        let plan = plan(&[(1000, 48), (100, 48)]);
        for crop in &plan.crops {
            assert!(crop.resized_width <= plan.batch_width);
        }
    }

    #[test]
    fn order_is_restored_from_batch_order() {
        let plan = plan(&[(480, 48), (48, 48), (240, 48)]);
        // Decoded in batch order: narrowest first.
        let decoded = vec!["narrow", "middle", "wide"];
        let restored = match restore_order(&plan, decoded) {
            Ok(values) => values,
            Err(error) => panic!("expected restored order, got {error}"),
        };
        assert_eq!(
            restored,
            [Some("wide"), Some("narrow"), Some("middle")],
            "each result must return to its original slot"
        );
    }

    #[test]
    fn equal_ratios_keep_their_original_relative_order() {
        let plan = plan(&[(96, 48), (192, 96), (48, 48)]);
        // The first two have ratio 2.0; the third has 1.0 and sorts first.
        let order: Vec<usize> = plan.crops.iter().map(|c| c.original_index).collect();
        assert_eq!(order, [2, 0, 1], "the stable sort preserves 0 before 1");
    }

    #[test]
    fn invalid_batches_are_rejected() {
        assert!(matches!(
            plan_batch(&[]),
            Err(Error::InvalidInput {
                field: "recognizer.batch",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            plan_batch(&[(0, 48)]),
            Err(Error::InvalidInput {
                field: "recognizer.crop",
                ..
            })
        ));
        let oversized = vec![(48_u32, 48_u32); MAX_BATCH + 1];
        assert!(matches!(
            plan_batch(&oversized),
            Err(Error::ResourceLimit {
                resource: "recognizer.batch",
                ..
            })
        ));
        let plan = plan(&[(48, 48)]);
        assert!(matches!(
            restore_order(&plan, vec!["a", "b"]),
            Err(Error::InvalidInput {
                field: "recognizer.batch",
                ..
            })
        ));
    }
}
