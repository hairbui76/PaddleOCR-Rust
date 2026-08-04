// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The classic detector path: decoded image to source-space text boxes.
//!
//! This composes the already verified steps in the frozen order from
//! `docs/CLASSIC_OCR_CONTRACT.md`:
//!
//! 1. plan the resize with the `960 / max` policy and multiple-of-32 rounding;
//! 2. resize the decoded BGR image with `INTER_LINEAR`;
//! 3. normalize into an `NCHW` `f32` tensor;
//! 4. run the backend through the validated adapter;
//! 5. postprocess the probability map into boxes rescaled to the **original**
//!    image dimensions.
//!
//! The rescale target is deliberately the original size, not the resized one:
//! the resize ratio is not exactly recoverable from the padded multiple-of-32
//! dimensions, so the source extent is carried through rather than inferred.
//!
//! The backend is injected, so this whole path is exercised offline against a
//! fake backend. That is what makes the wiring testable without a model.

use crate::backend::{BackendTensor, InferenceBackend, ModelContract, run_validated};
use crate::crop::InterleavedImage;
use crate::detector_boxes::{DetectedBox, classic_db_boxes};
use crate::error::{Error, InputViolation, Result};
use crate::geometry::classic_detector_resize_plan;
use crate::resize::classic_linear_resize;
use crate::tensor::classic_detector_input;
use crate::types::ImageDimensions;

/// Runs the detector over one decoded BGR image.
pub(crate) fn detect_boxes(
    backend: &dyn InferenceBackend,
    contract: &ModelContract,
    image: &InterleavedImage,
    box_threshold: f64,
    unclip_ratio: f64,
) -> Result<Vec<DetectedBox>> {
    let source = image.dimensions();
    let plan = classic_detector_resize_plan(source);
    let resized = classic_linear_resize(image, plan.resized())?;

    let tensor = classic_detector_input(&resized)?;
    let shape = tensor.shape();
    let input = BackendTensor::new(shape.to_vec(), tensor.values().to_vec())?;
    let output = run_validated(backend, contract, &input)?;

    // The detector emits one map per image with a single channel.
    let output_shape = output.shape();
    if output_shape.len() != 4 || output_shape[0] != 1 || output_shape[1] != 1 {
        return Err(Error::InvalidInput {
            field: "detector.output_shape",
            violation: InputViolation::OutOfRange,
        });
    }
    let map_height = u32::try_from(output_shape[2]).map_err(|_| Error::InvalidInput {
        field: "detector.output_shape",
        violation: InputViolation::OutOfRange,
    })?;
    let map_width = u32::try_from(output_shape[3]).map_err(|_| Error::InvalidInput {
        field: "detector.output_shape",
        violation: InputViolation::OutOfRange,
    })?;

    classic_db_boxes(
        output.values(),
        map_width,
        map_height,
        box_threshold,
        unclip_ratio,
        source.width(),
        source.height(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::{AxisExtent, ModelArtifact, RunBudget, TensorContract};

    /// A backend that returns a fixed probability map, so the wiring is
    /// exercised without a model or a native library.
    struct FakeDetector {
        map: Vec<f32>,
        width: usize,
        height: usize,
    }

    impl InferenceBackend for FakeDetector {
        fn run(&self, input: &BackendTensor) -> Result<(String, BackendTensor)> {
            // The fake asserts the input arrived in NCHW with three channels,
            // which is the contract this path is responsible for producing.
            let shape = input.shape();
            assert_eq!(shape.len(), 4, "detector input must be NCHW");
            assert_eq!(shape[0], 1, "batch must be one");
            assert_eq!(shape[1], 3, "three interleaved channels");
            let tensor = BackendTensor::new(vec![1, 1, self.height, self.width], self.map.clone())?;
            Ok(("fetch_name_0".to_owned(), tensor))
        }
    }

    fn contract(map_width: usize, map_height: usize) -> ModelContract {
        let artifact = match ModelArtifact::new("/nonexistent/det.onnx", "0".repeat(64)) {
            Ok(artifact) => artifact,
            Err(error) => panic!("expected a valid artifact, got {error}"),
        };
        let free = AxisExtent::Bounded {
            minimum: 1,
            maximum: 4096,
        };
        let input = match TensorContract::new(
            "x",
            vec![AxisExtent::Fixed(1), AxisExtent::Fixed(3), free, free],
        ) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid input contract, got {error}"),
        };
        let output = match TensorContract::new(
            "fetch_name_0",
            vec![
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(1),
                AxisExtent::Fixed(map_height),
                AxisExtent::Fixed(map_width),
            ],
        ) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid output contract, got {error}"),
        };
        let budget = match RunBudget::new(40_000_000, 40_000_000, 1) {
            Ok(budget) => budget,
            Err(error) => panic!("expected a valid budget, got {error}"),
        };
        ModelContract::new(artifact, input, output, budget)
    }

    fn image(width: u32, height: u32) -> InterleavedImage {
        let dimensions = match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        };
        let pixels = vec![128_u8; (width * height * 3) as usize];
        match InterleavedImage::new(dimensions, 3, pixels) {
            Ok(value) => value,
            Err(error) => panic!("expected a valid image, got {error}"),
        }
    }

    #[test]
    fn a_detected_region_is_rescaled_to_the_original_dimensions() {
        let source = image(64, 48);
        let plan = classic_detector_resize_plan(source.dimensions());
        let resized = plan.resized();
        let (map_width, map_height) = (resized.width() as usize, resized.height() as usize);

        // One solid block in the middle of the map.
        let mut map = vec![0.0_f32; map_width * map_height];
        let (x0, x1) = (map_width / 4, map_width * 3 / 4);
        let (y0, y1) = (map_height / 4, map_height * 3 / 4);
        for y in y0..y1 {
            for x in x0..x1 {
                map[y * map_width + x] = 0.9;
            }
        }

        let backend = FakeDetector {
            map,
            width: map_width,
            height: map_height,
        };
        let boxes = match detect_boxes(
            &backend,
            &contract(map_width, map_height),
            &source,
            0.5,
            1.5,
        ) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected detected boxes, got {error}"),
        };

        assert_eq!(boxes.len(), 1, "one solid region must yield one box");
        for (x, y) in &boxes[0].corners {
            assert!(
                (0..=64).contains(x) && (0..=48).contains(y),
                "corner ({x}, {y}) must lie in the original 64x48 extent"
            );
        }
        assert!(boxes[0].score > 0.8, "a solid 0.9 region must score high");
    }

    #[test]
    fn an_empty_map_detects_nothing() {
        let source = image(64, 48);
        let plan = classic_detector_resize_plan(source.dimensions());
        let resized = plan.resized();
        let (map_width, map_height) = (resized.width() as usize, resized.height() as usize);
        let backend = FakeDetector {
            map: vec![0.0_f32; map_width * map_height],
            width: map_width,
            height: map_height,
        };
        let boxes = match detect_boxes(
            &backend,
            &contract(map_width, map_height),
            &source,
            0.5,
            1.5,
        ) {
            Ok(boxes) => boxes,
            Err(error) => panic!("expected an empty result, got {error}"),
        };
        assert!(boxes.is_empty());
    }
}
