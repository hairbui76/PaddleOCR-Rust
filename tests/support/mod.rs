#[derive(Debug)]
pub struct FixtureMetadata {
    pub id: &'static str,
    pub upstream_baseline: &'static str,
    pub license: &'static str,
    pub tolerance: f32,
}

pub fn assert_absolute_difference(actual: f32, expected: f32, tolerance: f32) {
    let difference = (actual - expected).abs();
    assert!(
        difference <= tolerance,
        "difference {difference} exceeded tolerance {tolerance}; actual={actual}, expected={expected}"
    );
}
