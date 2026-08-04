// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

pub fn assert_absolute_difference(actual: f32, expected: f32, tolerance: f32) {
    let difference = (actual - expected).abs();
    assert!(
        difference <= tolerance,
        "difference {difference} exceeded tolerance {tolerance}; actual={actual}, expected={expected}"
    );
}
