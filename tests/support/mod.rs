pub fn assert_absolute_difference(actual: f32, expected: f32, tolerance: f32) {
    let difference = (actual - expected).abs();
    assert!(
        difference <= tolerance,
        "difference {difference} exceeded tolerance {tolerance}; actual={actual}, expected={expected}"
    );
}
