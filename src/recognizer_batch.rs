// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Recognition batching policy: aspect sort, batch width, per-crop resize.
//!
//! `docs/CLASSIC_OCR_CONTRACT.md` freezes this for M2, from
//! `tools/infer/predict_rec.py` and `ppocr/data/imaug/rec_img_aug.py`:
//!
//! - crops are sorted by `width / height` before batching, and the original
//!   order is restored afterwards;
//! - the sorted crops are then split into consecutive batches of `6`, which is
//!   the frozen `rec_batch_num`;
//! - the base shape is `[3, 48, 320]`;
//! - for each batch, `max_wh_ratio` starts at `320 / 48` and grows to the
//!   largest crop ratio **in that batch**;
//! - each crop resizes to height `48` with width `ceil(48 * crop_ratio)`,
//!   capped by that batch's width, and is right-padded with zeros.
//!
//! The batch split is not a performance detail. `max_wh_ratio` is computed per
//! batch, so the padded width — and therefore every crop's resized width and
//! the amount of zero padding the model sees — depends on which crops share a
//! batch. Planning all crops as one batch gives a different tensor than
//! upstream for any input with more than six crops, which is most real pages.
//!
//! Two further details are easy to lose and are pinned by tests. The batch
//! width floor is `320`, so a batch of narrow crops still pads to `320` rather
//! than shrinking. And the per-crop width uses `ceil`, not rounding, so a crop
//! whose exact width is fractional always grows.
//!
//! This module computes the plan only. It performs no resize and holds no
//! pixels; `src/resize.rs` and `src/tensor.rs` do that work.

use crate::error::{Error, InputViolation, Result};

/// The frozen recognition input height.
pub(crate) const RECOGNITION_HEIGHT: u32 = 48;

/// The frozen base recognition width.
pub(crate) const RECOGNITION_BASE_WIDTH: u32 = 320;

/// The frozen `rec_batch_num`: how many sorted crops share one batch.
pub(crate) const RECOGNITION_BATCH_SIZE: usize = 6;

/// Maximum crops accepted in one planning call.
///
/// This is the `docs/QUALITY_PROFILE.md` work-unit budget — reject a request
/// requiring more than `1,000` detected text regions — and it deliberately
/// matches the detector's `MAX_CANDIDATES`, so a page the detector accepts is a
/// page the recognizer can plan.
const MAX_CROPS: usize = 1_000;

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

/// Builds the batch plans for one group of crops.
///
/// `sizes` are the crop dimensions in caller order. Crops are sorted by aspect
/// ratio across the whole group, then split into consecutive batches of
/// [`RECOGNITION_BATCH_SIZE`]; each returned plan carries its own padded width.
/// `original_index` is what restores the caller's order after decoding.
pub(crate) fn plan_batches(sizes: &[(u32, u32)]) -> Result<Vec<BatchPlan>> {
    if sizes.is_empty() {
        return Err(Error::InvalidInput {
            field: "recognizer.batch",
            violation: InputViolation::Empty,
        });
    }
    if sizes.len() > MAX_CROPS {
        return Err(Error::ResourceLimit {
            resource: "recognizer.crops",
            limit: MAX_CROPS as u64,
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
    let mut plans = Vec::with_capacity(ordered.len().div_ceil(RECOGNITION_BATCH_SIZE));
    for chunk in ordered.chunks(RECOGNITION_BATCH_SIZE) {
        let max_ratio = chunk
            .iter()
            .fold(base_ratio, |widest, (_, ratio)| widest.max(*ratio));
        let batch_width = scaled_width(max_ratio)?;
        let crops = chunk
            .iter()
            .map(|(original_index, ratio)| {
                Ok(BatchedCrop {
                    original_index: *original_index,
                    resized_width: scaled_width(*ratio)?.min(batch_width),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        plans.push(BatchPlan { batch_width, crops });
    }
    Ok(plans)
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

/// Restores caller order from decoded results in concatenated batch order.
///
/// `decoded` must hold every batch's results in plan order, first batch first.
pub(crate) fn restore_order<T>(plans: &[BatchPlan], decoded: Vec<T>) -> Result<Vec<Option<T>>> {
    let total: usize = plans.iter().map(|plan| plan.crops.len()).sum();
    if decoded.len() != total {
        return Err(Error::InvalidInput {
            field: "recognizer.batch",
            violation: InputViolation::OutOfRange,
        });
    }
    let mut restored: Vec<Option<T>> = (0..total).map(|_| None).collect();
    let flattened = plans.iter().flat_map(|plan| plan.crops.iter());
    for (crop, value) in flattened.zip(decoded) {
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

    fn plans(sizes: &[(u32, u32)]) -> Vec<BatchPlan> {
        match plan_batches(sizes) {
            Ok(plans) => plans,
            Err(error) => panic!("expected batch plans, got {error}"),
        }
    }

    /// A group at or under `rec_batch_num` is one batch, so the pre-split
    /// behaviour is unchanged for small pages.
    fn single(sizes: &[(u32, u32)]) -> BatchPlan {
        let mut plans = plans(sizes);
        assert_eq!(plans.len(), 1, "expected exactly one batch");
        match plans.pop() {
            Some(plan) => plan,
            None => panic!("expected one batch"),
        }
    }

    #[test]
    fn narrow_crops_still_pad_to_the_base_width() {
        // Every crop is far narrower than the base ratio, so the batch width
        // must stay at 320 rather than shrinking to fit.
        let plan = single(&[(20, 48), (30, 48)]);
        assert_eq!(plan.batch_width, RECOGNITION_BASE_WIDTH);
        assert_eq!(plan.crops[0].resized_width, 20);
        assert_eq!(plan.crops[1].resized_width, 30);
    }

    #[test]
    fn a_wide_crop_raises_the_batch_width_for_everyone() {
        // 96/48 = 2.0 -> 96 wide; 960/48 = 20.0 -> 960 wide and it sets the
        // batch width.
        let plan = single(&[(96, 48), (960, 48)]);
        assert_eq!(plan.batch_width, 960);
        assert_eq!(plan.crops[0].resized_width, 96);
        assert_eq!(plan.crops[1].resized_width, 960);
    }

    #[test]
    fn crops_are_sorted_by_aspect_ratio_and_carry_their_origin() {
        let plan = single(&[(480, 48), (48, 48), (240, 48)]);
        let order: Vec<usize> = plan.crops.iter().map(|c| c.original_index).collect();
        assert_eq!(order, [1, 2, 0], "narrowest first");
    }

    #[test]
    fn the_width_uses_ceil_so_a_fractional_crop_grows() {
        // 50/48 = 1.041666..., times 48 is 50.0 exactly, so this checks the
        // neighbouring case: 49/48 * 48 = 49.0, and a truly fractional ratio.
        let plan = single(&[(49, 47)]);
        // 49/47 = 1.04255..., * 48 = 50.042..., ceil -> 51.
        assert_eq!(plan.crops[0].resized_width, 51);
    }

    #[test]
    fn a_resized_width_never_exceeds_its_batch_width() {
        for plan in plans(&[(1000, 48), (100, 48)]) {
            for crop in &plan.crops {
                assert!(crop.resized_width <= plan.batch_width);
            }
        }
    }

    #[test]
    fn order_is_restored_from_batch_order() {
        let plans = plans(&[(480, 48), (48, 48), (240, 48)]);
        // Decoded in batch order: narrowest first.
        let decoded = vec!["narrow", "middle", "wide"];
        let restored = match restore_order(&plans, decoded) {
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
        let plan = single(&[(96, 48), (192, 96), (48, 48)]);
        // The first two have ratio 2.0; the third has 1.0 and sorts first.
        let order: Vec<usize> = plan.crops.iter().map(|c| c.original_index).collect();
        assert_eq!(order, [2, 0, 1], "the stable sort preserves 0 before 1");
    }

    /// The sorted order is split into consecutive groups of `rec_batch_num`.
    #[test]
    fn the_sorted_order_splits_into_batches_of_six() {
        let sizes: Vec<(u32, u32)> = (0..14).map(|index| (48 + index * 8, 48)).collect();
        let plans = plans(&sizes);
        let lengths: Vec<usize> = plans.iter().map(|plan| plan.crops.len()).collect();
        assert_eq!(lengths, [6, 6, 2], "six, six, then the remainder");

        // Concatenated batch order must still be the global aspect-sorted order.
        let flattened: Vec<usize> = plans
            .iter()
            .flat_map(|plan| plan.crops.iter())
            .map(|crop| crop.original_index)
            .collect();
        assert_eq!(flattened, (0..14).collect::<Vec<_>>());
    }

    /// A single very wide crop widens only its own batch.
    ///
    /// This is the whole point of splitting: with one global batch every narrow
    /// crop on the page would be padded to the widest crop's width, which is
    /// not the tensor upstream builds and not the tensor the model was
    /// calibrated on.
    #[test]
    fn a_wide_crop_does_not_widen_the_batches_it_is_not_in() {
        let mut sizes = vec![(48_u32, 48_u32); 6];
        sizes.push((4800, 48));
        let plans = plans(&sizes);
        assert_eq!(plans.len(), 2);
        assert_eq!(
            plans[0].batch_width, RECOGNITION_BASE_WIDTH,
            "the six square crops keep the base width"
        );
        assert_eq!(
            plans[1].batch_width, 4800,
            "the wide crop sets only its own batch width"
        );
    }

    /// Order is restored across batch boundaries, not just within a batch.
    #[test]
    fn order_is_restored_across_several_batches() {
        // Caller order is widest first, so batch order reverses it entirely.
        let sizes: Vec<(u32, u32)> = (0..13).rev().map(|index| (48 + index * 8, 48)).collect();
        let plans = plans(&sizes);
        assert_eq!(plans.len(), 3);
        let decoded: Vec<usize> = (0..13).collect();
        let restored = match restore_order(&plans, decoded) {
            Ok(values) => values,
            Err(error) => panic!("expected restored order, got {error}"),
        };
        // Caller index 0 is the widest crop, so it decoded last.
        assert_eq!(restored[0], Some(12));
        assert_eq!(restored[12], Some(0));
        assert!(restored.iter().all(Option::is_some), "no slot left empty");
    }

    #[test]
    fn invalid_batches_are_rejected() {
        assert!(matches!(
            plan_batches(&[]),
            Err(Error::InvalidInput {
                field: "recognizer.batch",
                violation: InputViolation::Empty,
            })
        ));
        assert!(matches!(
            plan_batches(&[(0, 48)]),
            Err(Error::InvalidInput {
                field: "recognizer.crop",
                ..
            })
        ));
        assert!(matches!(
            plan_batches(&[(48, 0)]),
            Err(Error::InvalidInput {
                field: "recognizer.crop",
                ..
            })
        ));
        let oversized = vec![(48_u32, 48_u32); MAX_CROPS + 1];
        assert!(matches!(
            plan_batches(&oversized),
            Err(Error::ResourceLimit {
                resource: "recognizer.crops",
                ..
            })
        ));
        // The work-unit budget is a limit, not a target: exactly the maximum is
        // accepted.
        assert!(plan_batches(&vec![(48_u32, 48_u32); MAX_CROPS]).is_ok());

        let plans = plans(&[(48, 48)]);
        assert!(matches!(
            restore_order(&plans, vec!["a", "b"]),
            Err(Error::InvalidInput {
                field: "recognizer.batch",
                ..
            })
        ));
    }
}
