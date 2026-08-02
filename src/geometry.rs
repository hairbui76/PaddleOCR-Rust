// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Private checked geometry planning for the selected classic OCR contract.

use core::cmp::Ordering;

use crate::{
    error::{Error, InputViolation, Result},
    types::{ImageDimensions, Point, Polygon, Quadrilateral},
};

const CLASSIC_DETECTOR_LIMIT_SIDE_LENGTH: u32 = 960;
const CLASSIC_DETECTOR_MAX_SIDE_LENGTH: u32 = 4_000;
const CLASSIC_DETECTOR_RESIZE_STRIDE: u32 = 32;
const CLASSIC_READING_ORDER_ROW_TOLERANCE: f32 = 10.0;

/// A checked geometry-only plan for one classic perspective crop.
///
/// The plan contains no pixel buffer and does not perform interpolation or
/// border handling. `CROP-001` owns those image operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClassicPerspectiveCropPlan {
    source: Quadrilateral,
    source_to_warp: ProjectiveTransform,
    warp_to_source: ProjectiveTransform,
    sampling_warp_to_source: SamplingProjectiveTransform,
    warp_width: u32,
    warp_height: u32,
    rotates_counter_clockwise: bool,
}

impl ClassicPerspectiveCropPlan {
    /// Returns the source quadrilateral in classic top-left-first order.
    #[must_use]
    pub(crate) const fn source(self) -> Quadrilateral {
        self.source
    }

    /// Returns the width passed to the perspective warp before any rotation.
    #[must_use]
    pub(crate) const fn warp_width(self) -> u32 {
        self.warp_width
    }

    /// Returns the height passed to the perspective warp before any rotation.
    #[must_use]
    pub(crate) const fn warp_height(self) -> u32 {
        self.warp_height
    }

    /// Returns whether the classic post-warp counter-clockwise rotation applies.
    #[must_use]
    pub(crate) const fn rotates_counter_clockwise(self) -> bool {
        self.rotates_counter_clockwise
    }

    /// Returns the final crop width after the planned rotation.
    #[must_use]
    pub(crate) const fn output_width(self) -> u32 {
        if self.rotates_counter_clockwise {
            self.warp_height
        } else {
            self.warp_width
        }
    }

    /// Returns the final crop height after the planned rotation.
    #[must_use]
    pub(crate) const fn output_height(self) -> u32 {
        if self.rotates_counter_clockwise {
            self.warp_width
        } else {
            self.warp_height
        }
    }

    /// Maps a source point into pre-rotation warp coordinates.
    pub(crate) fn map_source_to_warp(self, point: Point) -> Result<Point> {
        self.source_to_warp.map(point)
    }

    /// Maps a pre-rotation warp point back into source coordinates.
    pub(crate) fn map_warp_to_source(self, point: Point) -> Result<Point> {
        self.warp_to_source.map(point)
    }

    /// Maps finite pre-rotation warp coordinates back into source coordinates.
    ///
    /// This geometry helper preserves the projective calculation in `f64` for
    /// mapping evidence. The crop sampler instead uses the distinct private
    /// OpenCV-style `f32` sampling transform below. Neither helper may become
    /// a public crop-coordinate API before `CROP-001` has complete
    /// inverse-mapping evidence.
    pub(crate) fn map_warp_coordinates_to_source(self, x: f64, y: f64) -> Result<(f64, f64)> {
        self.warp_to_source.map_coordinates(x, y)
    }

    /// Maps one integral pre-rotation output pixel through the private classic
    /// OpenCV-style sampling transform.
    ///
    /// The classic pixel path creates a source-to-warp matrix, inverts it, then
    /// evaluates the inverted coefficients as `f32` row terms before the
    /// final projective division. Keep this distinct from the geometry-only
    /// `f64` mapping above: it is an interpolation implementation detail, not
    /// a public coordinate contract.
    pub(crate) fn map_warp_pixel_to_source_for_sampling(
        self,
        x: u32,
        y: u32,
    ) -> Result<(f64, f64)> {
        self.sampling_warp_to_source.map_pixel(x, y)
    }
}

/// A finite source-to-destination projective transform.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ProjectiveTransform {
    coefficients: [f64; 9],
}

impl ProjectiveTransform {
    fn inverse(self) -> Result<Self> {
        let [a00, a01, a02, a10, a11, a12, a20, a21, a22] = self.coefficients;
        let determinant = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
            + a02 * (a10 * a21 - a11 * a20);
        if !determinant.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.matrix",
                violation: InputViolation::NonFinite,
            });
        }
        if determinant == 0.0 {
            return Err(Error::InvalidInput {
                field: "perspective.matrix",
                violation: InputViolation::DegenerateGeometry,
            });
        }

        let reciprocal_determinant = 1.0 / determinant;
        let coefficients = [
            (a11 * a22 - a12 * a21) * reciprocal_determinant,
            (a02 * a21 - a01 * a22) * reciprocal_determinant,
            (a01 * a12 - a02 * a11) * reciprocal_determinant,
            (a12 * a20 - a10 * a22) * reciprocal_determinant,
            (a00 * a22 - a02 * a20) * reciprocal_determinant,
            (a02 * a10 - a00 * a12) * reciprocal_determinant,
            (a10 * a21 - a11 * a20) * reciprocal_determinant,
            (a01 * a20 - a00 * a21) * reciprocal_determinant,
            (a00 * a11 - a01 * a10) * reciprocal_determinant,
        ];
        if coefficients.iter().any(|value| !value.is_finite()) {
            return Err(Error::InvalidInput {
                field: "perspective.matrix",
                violation: InputViolation::NonFinite,
            });
        }
        Ok(Self { coefficients })
    }

    fn map(self, point: Point) -> Result<Point> {
        let x = f64::from(point.x());
        let y = f64::from(point.y());
        let (mapped_x, mapped_y) = self.map_coordinates(x, y)?;
        projective_point(mapped_x, mapped_y)
    }

    fn map_coordinates(self, x: f64, y: f64) -> Result<(f64, f64)> {
        if !x.is_finite() || !y.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.input",
                violation: InputViolation::NonFinite,
            });
        }
        let denominator =
            self.coefficients[6] * x + self.coefficients[7] * y + self.coefficients[8];
        if !denominator.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.denominator",
                violation: InputViolation::NonFinite,
            });
        }
        if denominator == 0.0 {
            return Err(Error::InvalidInput {
                field: "perspective.denominator",
                violation: InputViolation::DegenerateGeometry,
            });
        }

        let mapped_x = (self.coefficients[0] * x + self.coefficients[1] * y + self.coefficients[2])
            / denominator;
        let mapped_y = (self.coefficients[3] * x + self.coefficients[4] * y + self.coefficients[5])
            / denominator;
        if !mapped_x.is_finite() || !mapped_y.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.output",
                violation: InputViolation::NonFinite,
            });
        }
        Ok((mapped_x, mapped_y))
    }
}

/// A private `f32` transform matching the selected classic warp sampler's
/// coefficient and row-evaluation precision boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SamplingProjectiveTransform {
    coefficients: [f32; 9],
}

impl SamplingProjectiveTransform {
    fn from_projective(transform: ProjectiveTransform) -> Result<Self> {
        let mut coefficients = [0.0_f32; 9];
        for (destination, source) in coefficients.iter_mut().zip(transform.coefficients) {
            if source < -f64::from(f32::MAX) || source > f64::from(f32::MAX) {
                return Err(Error::InvalidInput {
                    field: "perspective.matrix",
                    violation: InputViolation::OutOfRange,
                });
            }
            *destination = source as f32;
        }
        if coefficients.iter().any(|value| !value.is_finite()) {
            return Err(Error::InvalidInput {
                field: "perspective.matrix",
                violation: InputViolation::NonFinite,
            });
        }
        Ok(Self { coefficients })
    }

    fn map_pixel(self, x: u32, y: u32) -> Result<(f64, f64)> {
        let y = y as f32;
        let row_x = y * self.coefficients[1] + self.coefficients[2];
        let row_y = y * self.coefficients[4] + self.coefficients[5];
        let row_z = y * self.coefficients[7] + self.coefficients[8];
        let x = f64::from(x);
        let denominator = f64::from(row_z) + f64::from(self.coefficients[6]) * x;
        if !denominator.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.denominator",
                violation: InputViolation::NonFinite,
            });
        }
        if denominator == 0.0 {
            return Err(Error::InvalidInput {
                field: "perspective.denominator",
                violation: InputViolation::DegenerateGeometry,
            });
        }

        let mapped_x =
            ((f64::from(row_x) + f64::from(self.coefficients[0]) * x) / denominator) as f32;
        let mapped_y =
            ((f64::from(row_y) + f64::from(self.coefficients[3]) * x) / denominator) as f32;
        if !mapped_x.is_finite() || !mapped_y.is_finite() {
            return Err(Error::InvalidInput {
                field: "perspective.output",
                violation: InputViolation::NonFinite,
            });
        }
        Ok((f64::from(mapped_x), f64::from(mapped_y)))
    }
}

/// A resize/pad plan matching the M2 classic detector preprocessing contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DetectorResizePlan {
    source: ImageDimensions,
    padded: ImageDimensions,
    resized: ImageDimensions,
    ratio_h: f32,
    ratio_w: f32,
}

impl DetectorResizePlan {
    /// Returns the decoded source dimensions before small-image padding.
    #[must_use]
    pub(crate) const fn source(self) -> ImageDimensions {
        self.source
    }

    /// Returns dimensions after the classic small-image padding step.
    #[must_use]
    pub(crate) const fn padded(self) -> ImageDimensions {
        self.padded
    }

    /// Returns dimensions passed to the detector tensor preprocessor.
    #[must_use]
    pub(crate) const fn resized(self) -> ImageDimensions {
        self.resized
    }

    /// Returns the detector resize ratio for the padded image height.
    #[must_use]
    pub(crate) const fn ratio_h(self) -> f32 {
        self.ratio_h
    }

    /// Returns the detector resize ratio for the padded image width.
    #[must_use]
    pub(crate) const fn ratio_w(self) -> f32 {
        self.ratio_w
    }
}

/// A bounded minimum-area quadrilateral derived from checked polygon vertices.
///
/// This is an internal mathematical candidate for later detector work. It does
/// not establish OpenCV `minAreaRect` equivalence, contour semantics, or a
/// public detector result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MinimumAreaQuadCandidate {
    quadrilateral: Quadrilateral,
    short_side: f64,
}

impl MinimumAreaQuadCandidate {
    /// Returns the candidate corners in classic top-left-first order.
    #[must_use]
    pub(crate) const fn quadrilateral(self) -> Quadrilateral {
        self.quadrilateral
    }

    /// Returns the shorter side length before any detector-output scaling.
    #[must_use]
    pub(crate) const fn short_side(self) -> f64 {
        self.short_side
    }
}

/// Builds the fixed M2 classic detector resize plan from checked dimensions.
#[must_use]
pub(crate) fn classic_detector_resize_plan(source: ImageDimensions) -> DetectorResizePlan {
    resize_plan_for_limits(
        source,
        CLASSIC_DETECTOR_LIMIT_SIDE_LENGTH,
        CLASSIC_DETECTOR_MAX_SIDE_LENGTH,
    )
}

/// Orders, clips, and filters one classic detector quadrilateral.
///
/// This mirrors the legacy `order_points_clockwise`, `clip_det_res`, and
/// minimum-side filter sequence for an already decoded detector quadrilateral.
/// A filtered or no-longer-convex quadrilateral yields `None` rather than an
/// invalid public geometry value.
#[must_use]
pub(crate) fn classic_order_clip_filter_quad(
    points: [Point; 4],
    image: ImageDimensions,
) -> Option<Quadrilateral> {
    let ordered = classic_order_points_clockwise(points)?;
    let clipped = ordered.map(|point| clip_point_to_image(point, image));
    let width = truncated_distance(clipped[0], clipped[1]);
    let height = truncated_distance(clipped[0], clipped[3]);
    if width <= 3 || height <= 3 {
        return None;
    }
    Quadrilateral::new(clipped).ok()
}

/// Builds a no-allocation plan for the legacy perspective crop operation.
///
/// The input must use the classic `top-left, top-right, bottom-right,
/// bottom-left` order. Edge lengths use the legacy truncating `int` behavior;
/// dimensions that truncate to zero are rejected rather than passed to a pixel
/// warp. The transform maps to the pre-rotation destination rectangle whose
/// corners are `(0, 0)`, `(width, 0)`, `(width, height)`, and `(0, height)`.
pub(crate) fn classic_perspective_crop_plan(
    source: Quadrilateral,
) -> Result<ClassicPerspectiveCropPlan> {
    let source_coordinates = source.points().map(point_coordinates);
    let warp_width = truncated_crop_extent(
        euclidean_distance(source_coordinates[0], source_coordinates[1]).max(euclidean_distance(
            source_coordinates[2],
            source_coordinates[3],
        )),
        "classic_crop.width",
    )?;
    let warp_height = truncated_crop_extent(
        euclidean_distance(source_coordinates[0], source_coordinates[3]).max(euclidean_distance(
            source_coordinates[1],
            source_coordinates[2],
        )),
        "classic_crop.height",
    )?;
    let source_points = source.points().map(|point| (point.x(), point.y()));
    let destination = [
        (0.0, 0.0),
        (warp_width as f32, 0.0),
        (warp_width as f32, warp_height as f32),
        (0.0, warp_height as f32),
    ];
    let source_to_warp = homography_for_corners(source_points, destination)?;
    let warp_to_source = source_to_warp.inverse()?;
    let sampling_warp_to_source = SamplingProjectiveTransform::from_projective(warp_to_source)?;
    let rotates_counter_clockwise = f64::from(warp_height) / f64::from(warp_width) >= 1.5;

    Ok(ClassicPerspectiveCropPlan {
        source,
        source_to_warp,
        warp_to_source,
        sampling_warp_to_source,
        warp_width,
        warp_height,
        rotates_counter_clockwise,
    })
}

/// Scales one DB detector quadrilateral from map coordinates to source coordinates.
///
/// This mirrors the `DBPostProcess.boxes_from_bitmap` scale, ties-to-even
/// rounding, and inclusive `[0, source_dimension]` clamp. It deliberately does
/// not apply the later classic order/clip/filter pass, whose bounds are
/// `[0, source_dimension - 1]`.
#[must_use]
pub(crate) fn classic_rescale_detector_quad(
    points: [Point; 4],
    map: ImageDimensions,
    source: ImageDimensions,
) -> [Point; 4] {
    points.map(|point| {
        let x = scale_detector_coordinate(point.x(), map.width(), source.width());
        let y = scale_detector_coordinate(point.y(), map.height(), source.height());
        point_unchecked(x, y)
    })
}

/// Sorts classic detector quadrilaterals into legacy reading order in place.
///
/// Callers must provide quadrilaterals whose first point is the classic
/// top-left corner. The stable initial y/x ordering and backwards same-row
/// swaps intentionally mirror legacy `sorted_boxes` behavior.
pub(crate) fn classic_sort_quadrilaterals(quadrilaterals: &mut [Quadrilateral]) {
    quadrilaterals.sort_by(|left, right| {
        let left_first = left.points()[0];
        let right_first = right.points()[0];
        compare_finite_coordinates(left_first.y(), right_first.y())
            .then_with(|| compare_finite_coordinates(left_first.x(), right_first.x()))
    });

    for index in 0..quadrilaterals.len().saturating_sub(1) {
        for comparison_index in (0..=index).rev() {
            let current = quadrilaterals[comparison_index].points()[0];
            let next = quadrilaterals[comparison_index + 1].points()[0];
            if (next.y() - current.y()).abs() < CLASSIC_READING_ORDER_ROW_TOLERANCE
                && next.x() < current.x()
            {
                quadrilaterals.swap(comparison_index, comparison_index + 1);
            } else {
                break;
            }
        }
    }
}

fn resize_plan_for_limits(
    source: ImageDimensions,
    limit_side_length: u32,
    max_side_length: u32,
) -> DetectorResizePlan {
    debug_assert!(limit_side_length > 0);
    debug_assert!(max_side_length > 0);

    let padded = if u64::from(source.width()) + u64::from(source.height()) < 64 {
        dimensions_unchecked(source.width().max(32), source.height().max(32))
    } else {
        source
    };

    let padded_height = padded.height();
    let padded_width = padded.width();
    let longest_side = padded_height.max(padded_width);
    let initial_ratio = if longest_side > limit_side_length {
        f64::from(limit_side_length) / f64::from(longest_side)
    } else {
        1.0
    };

    let mut resized_height = truncating_resize(padded_height, initial_ratio);
    let mut resized_width = truncating_resize(padded_width, initial_ratio);
    let resized_longest_side = resized_height.max(resized_width);
    if resized_longest_side > max_side_length {
        let max_ratio = f64::from(max_side_length) / f64::from(resized_longest_side);
        resized_height = truncating_resize(resized_height, max_ratio);
        resized_width = truncating_resize(resized_width, max_ratio);
    }

    resized_height = round_to_detector_stride(resized_height);
    resized_width = round_to_detector_stride(resized_width);
    let resized = dimensions_unchecked(resized_width, resized_height);

    DetectorResizePlan {
        source,
        padded,
        resized,
        ratio_h: resized.height() as f32 / padded.height() as f32,
        ratio_w: resized.width() as f32 / padded.width() as f32,
    }
}

fn classic_order_points_clockwise(points: [Point; 4]) -> Option<[Point; 4]> {
    let top_left_index = index_of_extreme(points, point_sum, true);
    let bottom_right_index = index_of_extreme(points, point_sum, false);
    if top_left_index == bottom_right_index {
        return None;
    }

    let remaining = points
        .into_iter()
        .enumerate()
        .filter_map(|(index, point)| {
            (index != top_left_index && index != bottom_right_index).then_some(point)
        })
        .collect::<Vec<_>>();
    let [first_remaining, second_remaining] = remaining.try_into().ok()?;

    let top_right = if point_difference(first_remaining) <= point_difference(second_remaining) {
        first_remaining
    } else {
        second_remaining
    };
    let bottom_left = if point_difference(first_remaining) >= point_difference(second_remaining) {
        first_remaining
    } else {
        second_remaining
    };

    Some([
        points[top_left_index],
        top_right,
        points[bottom_right_index],
        bottom_left,
    ])
}

fn compare_finite_coordinates(left: f32, right: f32) -> Ordering {
    match left.partial_cmp(&right) {
        Some(ordering) => ordering,
        None => Ordering::Equal,
    }
}

fn point_coordinates(point: Point) -> (f64, f64) {
    (f64::from(point.x()), f64::from(point.y()))
}

fn euclidean_distance(first: (f64, f64), second: (f64, f64)) -> f64 {
    (first.0 - second.0).hypot(first.1 - second.1)
}

/// Returns the signed shoelace area of an ordered checked polygon.
#[must_use]
pub(crate) fn polygon_signed_area(polygon: &Polygon) -> f64 {
    let points = polygon.points();
    let twice_area = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(current, next)| {
            f64::from(current.x()) * f64::from(next.y())
                - f64::from(next.x()) * f64::from(current.y())
        })
        .sum::<f64>();
    twice_area * 0.5
}

/// Returns the non-negative area used by classic DB unclip distance calculation.
#[must_use]
pub(crate) fn polygon_area(polygon: &Polygon) -> f64 {
    polygon_signed_area(polygon).abs()
}

/// Returns the closed Euclidean perimeter of an ordered checked polygon.
#[must_use]
pub(crate) fn polygon_perimeter(polygon: &Polygon) -> f64 {
    let points = polygon.points();
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(current, next)| {
            euclidean_distance(point_coordinates(*current), point_coordinates(*next))
        })
        .sum()
}

/// Derives a bounded minimum-area quadrilateral candidate from a checked polygon.
///
/// The implementation builds a convex hull and examines the bounding rectangle
/// aligned to each hull edge. It deliberately remains private and candidate-only:
/// matching OpenCV's rectangle orientation, rounding, and contour interaction is
/// later `DET-003` evidence work. A valid but numerically unrepresentable
/// candidate is reported as `None`; allocation failure is an explicit backend
/// error instead of an unbounded allocation attempt.
pub(crate) fn minimum_area_quad_candidate(
    polygon: &Polygon,
) -> Result<Option<MinimumAreaQuadCandidate>> {
    let hull = convex_hull_vertices(polygon)?;
    if hull.len() < 3 {
        return Ok(None);
    }

    let mut best = None;
    for index in 0..hull.len() {
        let first = hull[index];
        let second = hull[(index + 1) % hull.len()];
        let edge_x = second.0 - first.0;
        let edge_y = second.1 - first.1;
        let edge_length = edge_x.hypot(edge_y);
        if edge_length == 0.0 || !edge_length.is_finite() {
            continue;
        }

        let horizontal_axis = (edge_x / edge_length, edge_y / edge_length);
        let vertical_axis = (-horizontal_axis.1, horizontal_axis.0);
        let first_horizontal = coordinate_projection(hull[0], horizontal_axis);
        let first_vertical = coordinate_projection(hull[0], vertical_axis);
        let mut minimum_horizontal = first_horizontal;
        let mut maximum_horizontal = first_horizontal;
        let mut minimum_vertical = first_vertical;
        let mut maximum_vertical = first_vertical;

        for point in &hull[1..] {
            let horizontal = coordinate_projection(*point, horizontal_axis);
            let vertical = coordinate_projection(*point, vertical_axis);
            minimum_horizontal = minimum_horizontal.min(horizontal);
            maximum_horizontal = maximum_horizontal.max(horizontal);
            minimum_vertical = minimum_vertical.min(vertical);
            maximum_vertical = maximum_vertical.max(vertical);
        }

        let width = maximum_horizontal - minimum_horizontal;
        let height = maximum_vertical - minimum_vertical;
        let area = width * height;
        if width <= 0.0 || height <= 0.0 || !area.is_finite() {
            continue;
        }

        let candidate = MinimumAreaRectangle {
            corners: [
                coordinates_from_projections(
                    minimum_horizontal,
                    minimum_vertical,
                    horizontal_axis,
                    vertical_axis,
                ),
                coordinates_from_projections(
                    maximum_horizontal,
                    minimum_vertical,
                    horizontal_axis,
                    vertical_axis,
                ),
                coordinates_from_projections(
                    maximum_horizontal,
                    maximum_vertical,
                    horizontal_axis,
                    vertical_axis,
                ),
                coordinates_from_projections(
                    minimum_horizontal,
                    maximum_vertical,
                    horizontal_axis,
                    vertical_axis,
                ),
            ],
            short_side: width.min(height),
            area,
        };
        match best {
            None => best = Some(candidate),
            Some(current) if candidate.area < current.area => best = Some(candidate),
            Some(_) => {}
        }
    }

    let Some(best) = best else {
        return Ok(None);
    };
    let Some(first) = point_from_coordinates(best.corners[0]) else {
        return Ok(None);
    };
    let Some(second) = point_from_coordinates(best.corners[1]) else {
        return Ok(None);
    };
    let Some(third) = point_from_coordinates(best.corners[2]) else {
        return Ok(None);
    };
    let Some(fourth) = point_from_coordinates(best.corners[3]) else {
        return Ok(None);
    };
    let Some(ordered) = classic_order_points_clockwise([first, second, third, fourth]) else {
        return Ok(None);
    };
    let Ok(quadrilateral) = Quadrilateral::new(ordered) else {
        return Ok(None);
    };

    Ok(Some(MinimumAreaQuadCandidate {
        quadrilateral,
        short_side: best.short_side,
    }))
}

#[derive(Clone, Copy, Debug)]
struct MinimumAreaRectangle {
    corners: [(f64, f64); 4],
    short_side: f64,
    area: f64,
}

fn convex_hull_vertices(polygon: &Polygon) -> Result<Vec<(f64, f64)>> {
    let input_points = polygon.points();
    let mut sorted = Vec::new();
    sorted
        .try_reserve_exact(input_points.len())
        .map_err(|_| Error::Backend {
            message: "minimum-area hull allocation failed",
        })?;
    sorted.extend(input_points.iter().copied().map(point_coordinates));
    sorted.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
    });
    sorted.dedup();
    if sorted.len() < 3 {
        return Ok(sorted);
    }

    let hull_capacity = sorted.len().saturating_mul(2).saturating_sub(2);
    let mut lower = Vec::new();
    lower
        .try_reserve_exact(hull_capacity)
        .map_err(|_| Error::Backend {
            message: "minimum-area hull allocation failed",
        })?;
    for point in sorted.iter().copied() {
        while lower.len() >= 2
            && signed_turn(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper = Vec::new();
    upper
        .try_reserve_exact(hull_capacity)
        .map_err(|_| Error::Backend {
            message: "minimum-area hull allocation failed",
        })?;
    for point in sorted.iter().rev().copied() {
        while upper.len() >= 2
            && signed_turn(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    Ok(lower)
}

fn signed_turn(origin: (f64, f64), first: (f64, f64), second: (f64, f64)) -> f64 {
    (first.0 - origin.0) * (second.1 - origin.1) - (first.1 - origin.1) * (second.0 - origin.0)
}

fn coordinate_projection(point: (f64, f64), axis: (f64, f64)) -> f64 {
    point.0 * axis.0 + point.1 * axis.1
}

fn coordinates_from_projections(
    horizontal: f64,
    vertical: f64,
    horizontal_axis: (f64, f64),
    vertical_axis: (f64, f64),
) -> (f64, f64) {
    (
        horizontal * horizontal_axis.0 + vertical * vertical_axis.0,
        horizontal * horizontal_axis.1 + vertical * vertical_axis.1,
    )
}

fn point_from_coordinates(coordinates: (f64, f64)) -> Option<Point> {
    let (x, y) = coordinates;
    if !x.is_finite()
        || !y.is_finite()
        || x < -f64::from(f32::MAX)
        || x > f64::from(f32::MAX)
        || y < -f64::from(f32::MAX)
        || y > f64::from(f32::MAX)
    {
        return None;
    }
    Point::new(x as f32, y as f32).ok()
}

fn scale_detector_coordinate(coordinate: f32, map_length: u32, source_length: u32) -> f32 {
    let scaled = f64::from(coordinate) / f64::from(map_length) * f64::from(source_length);
    let clipped = scaled.clamp(0.0, f64::from(source_length));
    round_ties_to_even(clipped) as f32
}

fn truncated_crop_extent(value: f64, field: &'static str) -> Result<u32> {
    if !value.is_finite() {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::NonFinite,
        });
    }

    let truncated = value.trunc();
    if truncated < 1.0 {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::DegenerateGeometry,
        });
    }
    if truncated > f64::from(u32::MAX) {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(truncated as u32)
}

fn homography_for_corners(
    source: [(f32, f32); 4],
    destination: [(f32, f32); 4],
) -> Result<ProjectiveTransform> {
    let mut matrix = [[0.0; 9]; 8];
    for (index, ((source_x, source_y), (destination_x, destination_y))) in
        source.into_iter().zip(destination).enumerate()
    {
        // Keep all horizontal equations before all vertical equations. OpenCV
        // builds its getPerspectiveTransform system in this order; numerical
        // row order affects the resulting finite-precision LU solution.
        matrix[index] = [
            f64::from(source_x),
            f64::from(source_y),
            1.0,
            0.0,
            0.0,
            0.0,
            f64::from(-source_x * destination_x),
            f64::from(-source_y * destination_x),
            f64::from(destination_x),
        ];
        matrix[index + 4] = [
            0.0,
            0.0,
            0.0,
            f64::from(source_x),
            f64::from(source_y),
            1.0,
            f64::from(-source_x * destination_y),
            f64::from(-source_y * destination_y),
            f64::from(destination_y),
        ];
    }

    let solution = solve_eight_by_eight(matrix)?;
    let coefficients = [
        solution[0],
        solution[1],
        solution[2],
        solution[3],
        solution[4],
        solution[5],
        solution[6],
        solution[7],
        1.0,
    ];
    if coefficients.iter().any(|value| !value.is_finite()) {
        return Err(Error::InvalidInput {
            field: "perspective.matrix",
            violation: InputViolation::NonFinite,
        });
    }
    Ok(ProjectiveTransform { coefficients })
}

fn solve_eight_by_eight(mut matrix: [[f64; 9]; 8]) -> Result<[f64; 8]> {
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return Err(Error::InvalidInput {
            field: "perspective.matrix",
            violation: InputViolation::NonFinite,
        });
    }

    // This follows the pivot/elimination order of OpenCV's default `DECOMP_LU`
    // solver for the 8-by-8 getPerspectiveTransform system. Keep the RHS
    // separate from the factor matrix so its arithmetic remains observable.
    for pivot_column in 0..8 {
        let mut pivot_row = pivot_column;
        for candidate_row in (pivot_column + 1)..8 {
            if matrix[candidate_row][pivot_column].abs() > matrix[pivot_row][pivot_column].abs() {
                pivot_row = candidate_row;
            }
        }
        if matrix[pivot_row][pivot_column].abs() < f64::EPSILON * 100.0 {
            return Err(Error::InvalidInput {
                field: "perspective.matrix",
                violation: InputViolation::DegenerateGeometry,
            });
        }
        if pivot_row != pivot_column {
            matrix.swap(pivot_column, pivot_row);
        }

        let reciprocal_negative_pivot = -1.0 / matrix[pivot_column][pivot_column];
        let pivot_values = matrix[pivot_column];
        for lower_row in matrix.iter_mut().skip(pivot_column + 1) {
            let factor = lower_row[pivot_column] * reciprocal_negative_pivot;
            for (entry, pivot_entry) in lower_row[pivot_column + 1..8]
                .iter_mut()
                .zip(pivot_values[pivot_column + 1..8].iter())
            {
                *entry += factor * *pivot_entry;
            }
            lower_row[8] += factor * pivot_values[8];
        }
    }

    let mut solution = matrix.map(|row| row[8]);
    for row in (0..8).rev() {
        let mut value = solution[row];
        for column in (row + 1)..8 {
            value -= matrix[row][column] * solution[column];
        }
        solution[row] = value / matrix[row][row];
    }
    if solution.iter().any(|value| !value.is_finite()) {
        return Err(Error::InvalidInput {
            field: "perspective.matrix",
            violation: InputViolation::NonFinite,
        });
    }
    Ok(solution)
}

fn projective_point(x: f64, y: f64) -> Result<Point> {
    let x = projective_component(x, "perspective.output.x")?;
    let y = projective_component(y, "perspective.output.y")?;
    Point::new(x, y).map_err(|_| Error::InvalidInput {
        field: "perspective.output",
        violation: InputViolation::NonFinite,
    })
}

fn projective_component(value: f64, field: &'static str) -> Result<f32> {
    if !value.is_finite() {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::NonFinite,
        });
    }
    if value < -f64::from(f32::MAX) || value > f64::from(f32::MAX) {
        return Err(Error::InvalidInput {
            field,
            violation: InputViolation::OutOfRange,
        });
    }
    Ok(value as f32)
}

fn index_of_extreme(points: [Point; 4], measure: fn(Point) -> f32, select_minimum: bool) -> usize {
    let mut selected_index = 0;
    let mut selected_measure = measure(points[selected_index]);
    for (index, point) in points.into_iter().enumerate().skip(1) {
        let current_measure = measure(point);
        let replaces_selected = if select_minimum {
            current_measure < selected_measure
        } else {
            current_measure > selected_measure
        };
        if replaces_selected {
            selected_index = index;
            selected_measure = current_measure;
        }
    }
    selected_index
}

fn point_sum(point: Point) -> f32 {
    point.x() + point.y()
}

fn point_difference(point: Point) -> f32 {
    point.y() - point.x()
}

fn clip_point_to_image(point: Point, image: ImageDimensions) -> Point {
    let max_x = image.width() - 1;
    let max_y = image.height() - 1;
    let x = point.x().clamp(0.0, max_x as f32) as u32;
    let y = point.y().clamp(0.0, max_y as f32) as u32;
    point_unchecked(x as f32, y as f32)
}

fn truncated_distance(first: Point, second: Point) -> u32 {
    let horizontal = f64::from(first.x()) - f64::from(second.x());
    let vertical = f64::from(first.y()) - f64::from(second.y());
    (horizontal.hypot(vertical)) as u32
}

fn point_unchecked(x: f32, y: f32) -> Point {
    match Point::new(x, y) {
        Ok(point) => point,
        Err(error) => unreachable!("clip generated an invalid point: {error}"),
    }
}

fn dimensions_unchecked(width: u32, height: u32) -> ImageDimensions {
    match ImageDimensions::new(width, height) {
        Ok(dimensions) => dimensions,
        Err(error) => unreachable!("resize plan generated invalid dimensions: {error}"),
    }
}

fn truncating_resize(value: u32, ratio: f64) -> u32 {
    (f64::from(value) * ratio) as u32
}

fn round_to_detector_stride(value: u32) -> u32 {
    let units = round_ties_to_even(f64::from(value) / f64::from(CLASSIC_DETECTOR_RESIZE_STRIDE));
    units.max(1).saturating_mul(CLASSIC_DETECTOR_RESIZE_STRIDE)
}

fn round_ties_to_even(value: f64) -> u32 {
    let lower = value.floor();
    let fraction = value - lower;
    let rounded = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else if (lower as u64).is_multiple_of(2) {
        lower
    } else {
        lower + 1.0
    };
    rounded as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMUM_AREA_CANDIDATE_INPUT: &str =
        include_str!("../tests/fixtures/classic-v1-geometry-min-area-candidate/input.csv");
    const MINIMUM_AREA_CANDIDATE_EXPECTED: &str =
        include_str!("../tests/fixtures/classic-v1-geometry-min-area-candidate/expected.csv");

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        match ImageDimensions::new(width, height) {
            Ok(value) => value,
            Err(error) => panic!("expected valid dimensions, got {error}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "actual {actual} did not equal expected {expected}"
        );
    }

    fn assert_f64_close(actual: f64, expected: f64) {
        assert_f64_close_with_tolerance(actual, expected, 1.0e-4);
    }

    fn assert_f64_close_with_tolerance(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual {actual} did not equal expected {expected} within {tolerance}"
        );
    }

    fn assert_point_close(actual: Point, expected: Point) {
        assert_point_close_with_tolerance(actual, expected, 1.0e-3);
    }

    fn assert_point_close_with_tolerance(actual: Point, expected: Point, tolerance: f32) {
        assert!(
            (actual.x() - expected.x()).abs() <= tolerance,
            "x coordinate {} did not equal {} within {tolerance}",
            actual.x(),
            expected.x()
        );
        assert!(
            (actual.y() - expected.y()).abs() <= tolerance,
            "y coordinate {} did not equal {} within {tolerance}",
            actual.y(),
            expected.y()
        );
    }

    fn must_ok<T>(value: Result<T>) -> T {
        match value {
            Ok(result) => result,
            Err(error) => panic!("expected success, got {error}"),
        }
    }

    fn must_some<T>(value: Option<T>) -> T {
        match value {
            Some(result) => result,
            None => panic!("expected retained quadrilateral"),
        }
    }

    fn parse_or_panic<T: core::str::FromStr>(value: &str, field: &str, line_number: usize) -> T
    where
        T::Err: core::fmt::Display,
    {
        match value.parse() {
            Ok(parsed) => parsed,
            Err(error) => {
                panic!("fixture line {line_number} has invalid {field} value {value:?}: {error}")
            }
        }
    }

    fn parse_fixture_points(fixture: &str, fixture_name: &str) -> Vec<Point> {
        fixture
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
            .map(|(line_index, line)| {
                let mut values = line.split(',');
                let x = match values.next() {
                    Some(value) => parse_or_panic::<f32>(value, "x", line_index + 1),
                    None => panic!("{fixture_name} line {} is missing x", line_index + 1),
                };
                let y = match values.next() {
                    Some(value) => parse_or_panic::<f32>(value, "y", line_index + 1),
                    None => panic!("{fixture_name} line {} is missing y", line_index + 1),
                };
                if values.next().is_some() {
                    panic!("{fixture_name} line {} has extra values", line_index + 1);
                }
                point(x, y)
            })
            .collect()
    }

    fn fixture_comment_value<'a>(fixture: &'a str, prefix: &str) -> &'a str {
        match fixture.lines().find_map(|line| line.strip_prefix(prefix)) {
            Some(value) => value.trim(),
            None => panic!("fixture is missing comment prefix {prefix:?}"),
        }
    }

    #[test]
    fn classic_plan_preserves_a_stride_aligned_normal_image() {
        let source = dimensions(640, 480);
        let plan = classic_detector_resize_plan(source);
        assert_eq!(plan.source(), source);
        assert_eq!(plan.padded(), source);
        assert_eq!(plan.resized(), source);
        assert_close(plan.ratio_h(), 1.0);
        assert_close(plan.ratio_w(), 1.0);
    }

    #[test]
    fn classic_plan_downscales_the_longest_side_then_records_independent_ratios() {
        let plan = classic_detector_resize_plan(dimensions(1_000, 500));
        assert_eq!(plan.resized(), dimensions(960, 480));
        assert_close(plan.ratio_h(), 0.96);
        assert_close(plan.ratio_w(), 0.96);
    }

    #[test]
    fn classic_plan_pads_small_images_before_resizing() {
        let plan = classic_detector_resize_plan(dimensions(40, 20));
        assert_eq!(plan.source(), dimensions(40, 20));
        assert_eq!(plan.padded(), dimensions(40, 32));
        assert_eq!(plan.resized(), dimensions(32, 32));
        assert_close(plan.ratio_h(), 1.0);
        assert_close(plan.ratio_w(), 0.8);
    }

    #[test]
    fn classic_plan_uses_python_style_ties_to_even_stride_rounding() {
        let plan = classic_detector_resize_plan(dimensions(48, 80));
        assert_eq!(plan.resized(), dimensions(64, 64));
        assert_close(plan.ratio_h(), 0.8);
        assert_close(plan.ratio_w(), 64.0 / 48.0);
    }

    #[test]
    fn secondary_maximum_side_limit_is_applied_before_stride_rounding() {
        let plan = resize_plan_for_limits(dimensions(16_000, 100), 8_000, 4_000);
        assert_eq!(plan.resized(), dimensions(4_000, 32));
        assert_close(plan.ratio_w(), 0.25);
        assert_close(plan.ratio_h(), 0.32);
    }

    fn point(x: f32, y: f32) -> Point {
        match Point::new(x, y) {
            Ok(value) => value,
            Err(error) => panic!("expected valid point, got {error}"),
        }
    }

    fn quadrilateral(left: f32, top: f32, width: f32, height: f32) -> Quadrilateral {
        match Quadrilateral::new([
            point(left, top),
            point(left + width, top),
            point(left + width, top + height),
            point(left, top + height),
        ]) {
            Ok(value) => value,
            Err(error) => panic!("expected valid quadrilateral, got {error}"),
        }
    }

    fn polygon(points: Vec<Point>) -> Polygon {
        must_ok(Polygon::new(points))
    }

    #[test]
    fn classic_quad_order_matches_sum_difference_selection() {
        let result = classic_order_clip_filter_quad(
            [
                point(100.0, 100.0),
                point(0.0, 0.0),
                point(0.0, 100.0),
                point(100.0, 0.0),
            ],
            dimensions(101, 101),
        );
        let quad = must_some(result);
        assert_eq!(
            quad.points(),
            [
                point(0.0, 0.0),
                point(100.0, 0.0),
                point(100.0, 100.0),
                point(0.0, 100.0),
            ]
        );
    }

    #[test]
    fn classic_quad_clip_truncates_after_clamping_to_inclusive_image_bounds() {
        let result = classic_order_clip_filter_quad(
            [
                point(25.9, 15.9),
                point(-1.9, -1.2),
                point(-0.4, 15.1),
                point(21.7, -0.1),
            ],
            dimensions(20, 15),
        );
        let quad = must_some(result);
        assert_eq!(
            quad.points(),
            [
                point(0.0, 0.0),
                point(19.0, 0.0),
                point(19.0, 14.0),
                point(0.0, 14.0),
            ]
        );
    }

    #[test]
    fn classic_quad_filter_rejects_width_or_height_at_most_three() {
        let result = classic_order_clip_filter_quad(
            [
                point(0.0, 0.0),
                point(3.0, 0.0),
                point(3.0, 10.0),
                point(0.0, 10.0),
            ],
            dimensions(20, 20),
        );
        assert!(result.is_none());
    }

    #[test]
    fn classic_quad_filter_rejects_geometry_that_degenerates_after_clipping() {
        let result = classic_order_clip_filter_quad(
            [
                point(-10.0, -10.0),
                point(-5.0, -10.0),
                point(-5.0, 20.0),
                point(-10.0, 20.0),
            ],
            dimensions(20, 20),
        );
        assert!(result.is_none());
    }

    #[test]
    fn classic_sort_uses_backwards_same_row_swaps() {
        let first = quadrilateral(80.0, 0.0, 8.0, 8.0);
        let second = quadrilateral(20.0, 5.0, 8.0, 8.0);
        let third = quadrilateral(10.0, 9.0, 8.0, 8.0);
        let mut boxes = [first, second, third];

        classic_sort_quadrilaterals(&mut boxes);

        assert_eq!(boxes, [third, second, first]);
    }

    #[test]
    fn classic_sort_does_not_merge_rows_at_the_ten_pixel_boundary() {
        let upper = quadrilateral(70.0, 0.0, 8.0, 8.0);
        let lower = quadrilateral(10.0, 10.0, 8.0, 8.0);
        let mut boxes = [upper, lower];

        classic_sort_quadrilaterals(&mut boxes);

        assert_eq!(boxes, [upper, lower]);
    }

    #[test]
    fn classic_sort_preserves_equal_top_left_input_order() {
        let first = quadrilateral(10.0, 10.0, 8.0, 8.0);
        let second = quadrilateral(10.0, 10.0, 9.0, 8.0);
        let mut boxes = [first, second];

        classic_sort_quadrilaterals(&mut boxes);

        assert_eq!(boxes, [first, second]);
    }

    #[test]
    fn classic_crop_plan_maps_rectangle_corners_in_both_directions() {
        let source = quadrilateral(10.0, 20.0, 100.0, 60.0);
        let plan = must_ok(classic_perspective_crop_plan(source));
        assert_eq!(plan.source(), source);
        assert_eq!((plan.warp_width(), plan.warp_height()), (100, 60));
        assert!(!plan.rotates_counter_clockwise());
        assert_eq!((plan.output_width(), plan.output_height()), (100, 60));

        let source_points = source.points();
        let warp_points = [
            point(0.0, 0.0),
            point(100.0, 0.0),
            point(100.0, 60.0),
            point(0.0, 60.0),
        ];
        for (source_point, warp_point) in source_points.into_iter().zip(warp_points) {
            assert_point_close(must_ok(plan.map_source_to_warp(source_point)), warp_point);
            assert_point_close(must_ok(plan.map_warp_to_source(warp_point)), source_point);
        }
    }

    #[test]
    fn classic_crop_plan_truncates_edges_and_rotates_at_the_ratio_boundary() {
        let source = quadrilateral(0.0, 0.0, 10.9, 15.0);
        let plan = must_ok(classic_perspective_crop_plan(source));

        assert_eq!((plan.warp_width(), plan.warp_height()), (10, 15));
        assert!(plan.rotates_counter_clockwise());
        assert_eq!((plan.output_width(), plan.output_height()), (15, 10));
    }

    #[test]
    fn classic_crop_plan_preserves_odd_and_extreme_aspect_dimensions() {
        let cases = [
            (7.0, 5.0, false, (7, 5)),
            (1.0, 16_384.0, true, (16_384, 1)),
            (16_384.0, 1.0, false, (16_384, 1)),
        ];

        for (width, height, rotates_counter_clockwise, output_dimensions) in cases {
            let source = quadrilateral(-5.0, 11.0, width, height);
            let plan = must_ok(classic_perspective_crop_plan(source));

            assert_eq!(
                (plan.warp_width(), plan.warp_height()),
                (width as u32, height as u32)
            );
            assert_eq!(plan.rotates_counter_clockwise(), rotates_counter_clockwise);
            assert_eq!(
                (plan.output_width(), plan.output_height()),
                output_dimensions
            );

            let source_points = source.points();
            let warp_points = [
                point(0.0, 0.0),
                point(width, 0.0),
                point(width, height),
                point(0.0, height),
            ];
            for (source_point, warp_point) in source_points.into_iter().zip(warp_points) {
                assert_point_close(must_ok(plan.map_source_to_warp(source_point)), warp_point);
                assert_point_close(must_ok(plan.map_warp_to_source(warp_point)), source_point);
            }
        }
    }

    #[test]
    fn classic_crop_plan_round_trips_an_oblique_quadrilateral() {
        let source = must_ok(Quadrilateral::new([
            point(10.0, 20.0),
            point(110.0, 25.0),
            point(96.0, 90.0),
            point(4.0, 72.0),
        ]));
        let plan = must_ok(classic_perspective_crop_plan(source));
        let warp_points = [
            point(0.0, 0.0),
            point(plan.warp_width() as f32, 0.0),
            point(plan.warp_width() as f32, plan.warp_height() as f32),
            point(0.0, plan.warp_height() as f32),
        ];
        for (source_point, warp_point) in source.points().into_iter().zip(warp_points) {
            assert_point_close(must_ok(plan.map_source_to_warp(source_point)), warp_point);
            assert_point_close(must_ok(plan.map_warp_to_source(warp_point)), source_point);
        }
        let interior = point(52.0, 48.0);

        let mapped = must_ok(plan.map_source_to_warp(interior));
        let restored = must_ok(plan.map_warp_to_source(mapped));

        assert_point_close(restored, interior);
    }

    #[test]
    fn classic_crop_plan_round_trips_a_grid_across_diverse_convex_quadrilaterals() {
        let cases = [
            [
                point(0.0, 0.0),
                point(31.0, 0.0),
                point(31.0, 17.0),
                point(0.0, 17.0),
            ],
            [
                point(3.0, 4.0),
                point(37.0, 6.0),
                point(31.0, 25.0),
                point(1.0, 21.0),
            ],
            [
                point(12.0, 8.0),
                point(90.0, 17.0),
                point(69.0, 83.0),
                point(4.0, 64.0),
            ],
            [
                point(-5.0, 1.0),
                point(127.0, 2.0),
                point(126.0, 5.0),
                point(-6.0, 4.0),
            ],
        ];
        let fractions = [0.0, 0.2, 0.5, 0.8, 1.0];

        for points in cases {
            let source = must_ok(Quadrilateral::new(points));
            let plan = must_ok(classic_perspective_crop_plan(source));
            for horizontal_fraction in fractions {
                for vertical_fraction in fractions {
                    let warp = point(
                        plan.warp_width() as f32 * horizontal_fraction,
                        plan.warp_height() as f32 * vertical_fraction,
                    );
                    let source_point = must_ok(plan.map_warp_to_source(warp));
                    let restored = must_ok(plan.map_source_to_warp(source_point));
                    assert_point_close_with_tolerance(restored, warp, 1.0e-2);

                    let (source_x, source_y) =
                        must_ok(plan.map_warp_coordinates_to_source(
                            f64::from(warp.x()),
                            f64::from(warp.y()),
                        ));
                    let restored_from_sampler_coordinates =
                        must_ok(plan.map_source_to_warp(point(source_x as f32, source_y as f32)));
                    assert_point_close_with_tolerance(
                        restored_from_sampler_coordinates,
                        warp,
                        1.0e-2,
                    );
                }
            }
        }
    }

    #[test]
    fn classic_crop_plan_handles_a_thin_skewed_quad_and_rejects_non_finite_sampler_coordinates() {
        let source = must_ok(Quadrilateral::new([
            point(0.0, 0.0),
            point(16_000.0, 0.1),
            point(15_999.0, 1.5),
            point(-1.0, 1.0),
        ]));
        let plan = must_ok(classic_perspective_crop_plan(source));
        assert_eq!((plan.warp_width(), plan.warp_height()), (16_000, 1));

        for (warp_x, warp_y) in [(0.125, 0.125), (8_000.5, 0.5), (15_999.75, 0.875)] {
            let (source_x, source_y) = must_ok(plan.map_warp_coordinates_to_source(warp_x, warp_y));
            let restored =
                must_ok(plan.map_source_to_warp(point(source_x as f32, source_y as f32)));
            assert_point_close_with_tolerance(
                restored,
                point(warp_x as f32, warp_y as f32),
                1.0e-2,
            );
        }

        for (warp_x, warp_y) in [
            (f64::NAN, 0.0),
            (0.0, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NAN),
        ] {
            assert!(matches!(
                plan.map_warp_coordinates_to_source(warp_x, warp_y),
                Err(Error::InvalidInput {
                    field: "perspective.input",
                    violation: InputViolation::NonFinite,
                })
            ));
        }
    }

    #[test]
    fn classic_crop_plan_matches_recorded_opencv_interior_mappings() {
        // The fixture records cv2.getPerspectiveTransform matrices for these
        // self-authored BGR cases. Expected values below are independent
        // cv2.perspectiveTransform evaluations at non-corner source points;
        // this checks source-to-warp mapping rather than only our own inverse
        // round trip. The crop fixture remains the authoritative provenance
        // record for the matrices and capture environment.
        const CAPTURED_OPENCV_CROP_ORACLE: &str =
            include_str!("../tests/fixtures/classic-v1-crop-oracle/capture.json");
        for fixture_id in [
            "classic-v1-crop-oracle-phase-projective-bgr-8x8",
            "classic-v1-crop-oracle-single-pixel-bgr-3x3",
            "classic-v1-crop-oracle-tall-thin-projective-bgr-3x9",
        ] {
            assert!(
                CAPTURED_OPENCV_CROP_ORACLE.contains(fixture_id),
                "fixture record is missing {fixture_id}"
            );
        }

        let phase_plan = must_ok(classic_perspective_crop_plan(must_ok(Quadrilateral::new(
            [
                point(0.125, 0.375),
                point(6.875, 0.625),
                point(6.625, 6.875),
                point(0.375, 6.625),
            ],
        ))));
        assert_point_close_with_tolerance(
            must_ok(phase_plan.map_source_to_warp(point(1.25, 2.75))),
            point(0.945_537_9, 2.135_404_6),
            2.0e-4,
        );
        assert_point_close_with_tolerance(
            must_ok(phase_plan.map_source_to_warp(point(5.5, 4.125))),
            point(4.858_648_3, 3.291_792_2),
            2.0e-4,
        );

        let single_pixel_plan = must_ok(classic_perspective_crop_plan(must_ok(
            Quadrilateral::new([
                point(0.49, 0.49),
                point(1.49, 0.49),
                point(1.49, 1.49),
                point(0.49, 1.49),
            ]),
        )));
        assert_point_close_with_tolerance(
            must_ok(single_pixel_plan.map_source_to_warp(point(0.8, 0.9))),
            point(0.31, 0.409_999_97),
            2.0e-4,
        );

        let tall_thin_plan = must_ok(classic_perspective_crop_plan(must_ok(Quadrilateral::new(
            [
                point(0.4, 0.1),
                point(1.8, 0.2),
                point(1.6, 7.9),
                point(0.2, 7.6),
            ],
        ))));
        assert_point_close_with_tolerance(
            must_ok(tall_thin_plan.map_source_to_warp(point(1.0, 3.3))),
            point(0.494_374_3, 2.909_733_8),
            2.0e-4,
        );
        assert_point_close_with_tolerance(
            must_ok(tall_thin_plan.map_source_to_warp(point(0.75, 6.1))),
            point(0.368_594_86, 5.517_241),
            2.0e-4,
        );
    }

    #[test]
    fn classic_crop_plan_matches_captured_opencv_inverse_mapping_oracle() {
        // The sidecar was captured from cv2.getPerspectiveTransform with the
        // destination/source order reversed, followed by
        // cv2.perspectiveTransform. It checks the pre-rotation warp-to-source
        // direction used by the private crop sampler against all fourteen reviewed
        // BGR cases, including each destination boundary and one interior
        // coordinate per case. It is a self-authored, environment-specific
        // component oracle rather than a general OpenCV-equivalence claim.
        const CAPTURED_OPENCV_CROP_ORACLE: &str =
            include_str!("../tests/fixtures/classic-v1-crop-oracle/capture.json");
        const CAPTURED_OPENCV_INVERSE_MAPPING_ORACLE: &str =
            include_str!("../tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv");
        const EXPECTED_FIXTURE_IDS: [&str; 14] = [
            "classic-v1-crop-oracle-identity-bgr-3x2",
            "classic-v1-crop-oracle-border-replicate-bgr-3x2",
            "classic-v1-crop-oracle-projective-bgr-4x3",
            "classic-v1-crop-oracle-tall-rotation-bgr-2x3",
            "classic-v1-crop-oracle-interior-projective-bgr-7x6",
            "classic-v1-crop-oracle-edge-projective-bgr-5x4",
            "classic-v1-crop-oracle-tall-projective-bgr-4x7",
            "classic-v1-crop-oracle-phase-projective-bgr-8x8",
            "classic-v1-crop-oracle-single-pixel-bgr-3x3",
            "classic-v1-crop-oracle-tall-thin-projective-bgr-3x9",
            "classic-v1-crop-oracle-cubic-rounding-bgr-8x10",
            "classic-v1-crop-oracle-cubic-weight-order-bgr-5x10",
            "classic-v1-crop-oracle-sampling-matrix-bgr-12x11",
            "classic-v1-crop-oracle-perspective-lu-bgr-12x13",
        ];

        assert!(
            CAPTURED_OPENCV_INVERSE_MAPPING_ORACLE
                .contains("# schema_version: paddleocr-rust/crop-inverse-mappings/v1")
        );

        let mut mapping_count = 0;
        for (index, line) in CAPTURED_OPENCV_INVERSE_MAPPING_ORACLE.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let line_number = index + 1;
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(
                fields.len(),
                15,
                "inverse-mapping fixture line {line_number} has an unexpected field count"
            );
            let fixture_id = fields[0];
            assert!(
                EXPECTED_FIXTURE_IDS.contains(&fixture_id),
                "inverse-mapping fixture line {line_number} has an unknown fixture {fixture_id}"
            );
            assert!(
                CAPTURED_OPENCV_CROP_ORACLE.contains(fixture_id),
                "crop capture document is missing {fixture_id}"
            );

            let source = must_ok(Quadrilateral::new([
                point(
                    parse_or_panic::<f32>(fields[1], "source_x0", line_number),
                    parse_or_panic::<f32>(fields[2], "source_y0", line_number),
                ),
                point(
                    parse_or_panic::<f32>(fields[3], "source_x1", line_number),
                    parse_or_panic::<f32>(fields[4], "source_y1", line_number),
                ),
                point(
                    parse_or_panic::<f32>(fields[5], "source_x2", line_number),
                    parse_or_panic::<f32>(fields[6], "source_y2", line_number),
                ),
                point(
                    parse_or_panic::<f32>(fields[7], "source_x3", line_number),
                    parse_or_panic::<f32>(fields[8], "source_y3", line_number),
                ),
            ]));
            let plan = must_ok(classic_perspective_crop_plan(source));
            assert_eq!(
                plan.warp_width(),
                parse_or_panic::<u32>(fields[9], "pre_rotation_width", line_number),
                "inverse-mapping fixture line {line_number} has an unexpected crop width"
            );
            assert_eq!(
                plan.warp_height(),
                parse_or_panic::<u32>(fields[10], "pre_rotation_height", line_number),
                "inverse-mapping fixture line {line_number} has an unexpected crop height"
            );

            let (actual_x, actual_y) = must_ok(plan.map_warp_coordinates_to_source(
                parse_or_panic::<f64>(fields[11], "warp_x", line_number),
                parse_or_panic::<f64>(fields[12], "warp_y", line_number),
            ));
            assert_f64_close_with_tolerance(
                actual_x,
                parse_or_panic::<f64>(fields[13], "expected_source_x", line_number),
                2.0e-4,
            );
            assert_f64_close_with_tolerance(
                actual_y,
                parse_or_panic::<f64>(fields[14], "expected_source_y", line_number),
                2.0e-4,
            );
            mapping_count += 1;
        }

        assert_eq!(mapping_count, 70, "unexpected inverse-mapping sample count");
        for fixture_id in EXPECTED_FIXTURE_IDS {
            let sample_count = CAPTURED_OPENCV_INVERSE_MAPPING_ORACLE
                .lines()
                .filter(|line| line.starts_with(fixture_id))
                .count();
            assert_eq!(sample_count, 5, "unexpected sample count for {fixture_id}");
        }
    }

    #[test]
    fn classic_crop_plan_rejects_an_edge_that_truncates_to_zero() {
        let source = quadrilateral(0.0, 0.0, 0.9, 10.0);

        assert!(matches!(
            classic_perspective_crop_plan(source),
            Err(Error::InvalidInput {
                field: "classic_crop.width",
                violation: InputViolation::DegenerateGeometry,
            })
        ));
    }

    #[test]
    fn classic_detector_rescale_uses_ties_to_even_and_inclusive_source_bounds() {
        let rescaled = classic_rescale_detector_quad(
            [
                point(1.0, 1.0),
                point(2.0, 2.0),
                point(-1.0, -1.0),
                point(3.0, 3.0),
            ],
            dimensions(2, 2),
            dimensions(5, 7),
        );

        assert_eq!(
            rescaled,
            [
                point(2.0, 4.0),
                point(5.0, 7.0),
                point(0.0, 0.0),
                point(5.0, 7.0),
            ]
        );
    }

    #[test]
    fn classic_detector_rescale_keeps_binary_half_ties_to_even() {
        let rescaled = classic_rescale_detector_quad(
            [
                point(0.25, 0.25),
                point(0.75, 0.75),
                point(1.25, 1.25),
                point(1.75, 1.75),
            ],
            dimensions(2, 2),
            dimensions(4, 4),
        );

        assert_eq!(
            rescaled,
            [
                point(0.0, 0.0),
                point(2.0, 2.0),
                point(2.0, 2.0),
                point(4.0, 4.0),
            ]
        );
    }

    #[test]
    fn classic_detector_rescale_precedes_the_final_exclusive_image_clip() {
        let source = dimensions(5, 7);
        let rescaled = classic_rescale_detector_quad(
            [
                point(0.0, 0.0),
                point(2.0, 0.0),
                point(2.0, 2.0),
                point(0.0, 2.0),
            ],
            dimensions(2, 2),
            source,
        );
        let filtered = must_some(classic_order_clip_filter_quad(rescaled, source));

        assert_eq!(
            filtered.points(),
            [
                point(0.0, 0.0),
                point(4.0, 0.0),
                point(4.0, 6.0),
                point(0.0, 6.0),
            ]
        );
    }

    #[test]
    fn polygon_metrics_preserve_orientation_and_closed_perimeter() {
        let counter_clockwise = polygon(vec![
            point(0.0, 0.0),
            point(4.0, 0.0),
            point(4.0, 3.0),
            point(0.0, 3.0),
        ]);
        let clockwise = polygon(vec![
            point(0.0, 0.0),
            point(0.0, 3.0),
            point(4.0, 3.0),
            point(4.0, 0.0),
        ]);

        assert_eq!(polygon_signed_area(&counter_clockwise), 12.0);
        assert_eq!(polygon_signed_area(&clockwise), -12.0);
        assert_eq!(polygon_area(&counter_clockwise), 12.0);
        assert_eq!(polygon_area(&clockwise), 12.0);
        assert_eq!(polygon_perimeter(&counter_clockwise), 14.0);
        assert_eq!(polygon_perimeter(&clockwise), 14.0);
    }

    #[test]
    fn minimum_area_candidate_reduces_concave_and_collinear_vertices_to_a_rectangle() {
        let input = polygon(vec![
            point(4.0, 3.0),
            point(8.0, 3.0),
            point(12.0, 3.0),
            point(12.0, 9.0),
            point(8.0, 6.0),
            point(4.0, 9.0),
            point(4.0, 3.0),
        ]);

        let candidate = must_some(must_ok(minimum_area_quad_candidate(&input)));

        assert_eq!(
            candidate.quadrilateral().points(),
            [
                point(4.0, 3.0),
                point(12.0, 3.0),
                point(12.0, 9.0),
                point(4.0, 9.0),
            ]
        );
        assert_f64_close(candidate.short_side(), 6.0);
        let candidate_polygon = polygon(candidate.quadrilateral().points().to_vec());
        assert_f64_close(polygon_area(&candidate_polygon), 48.0);
    }

    #[test]
    fn minimum_area_candidate_matches_self_authored_fixture() {
        let input = polygon(parse_fixture_points(
            MINIMUM_AREA_CANDIDATE_INPUT,
            "minimum-area candidate input",
        ));
        let expected = parse_fixture_points(
            MINIMUM_AREA_CANDIDATE_EXPECTED,
            "minimum-area candidate expected",
        );
        let expected_short_side = parse_or_panic::<f64>(
            fixture_comment_value(MINIMUM_AREA_CANDIDATE_EXPECTED, "# short_side:"),
            "short_side",
            0,
        );

        let candidate = must_some(must_ok(minimum_area_quad_candidate(&input)));

        assert_eq!(
            candidate.quadrilateral().points().as_slice(),
            expected.as_slice()
        );
        assert_eq!(candidate.short_side(), expected_short_side);
    }

    #[test]
    fn minimum_area_candidate_is_stable_across_rotated_polygon_order() {
        let top_left = point(10.0, 20.0);
        let top_right = point(16.4, 24.8);
        let bottom_right = point(14.6, 27.2);
        let bottom_left = point(8.2, 22.4);
        let center = point(12.3, 23.6);
        let forward = polygon(vec![top_left, top_right, bottom_right, bottom_left, center]);
        let reverse = polygon(vec![bottom_left, bottom_right, top_right, top_left, center]);

        let first = must_some(must_ok(minimum_area_quad_candidate(&forward)));
        let second = must_some(must_ok(minimum_area_quad_candidate(&reverse)));
        for (actual, expected) in first.quadrilateral().points().into_iter().zip([
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        ]) {
            assert_point_close(actual, expected);
        }
        assert_eq!(first.quadrilateral(), second.quadrilateral());
        assert_f64_close(first.short_side(), 3.0);
        assert_f64_close(second.short_side(), 3.0);
        let candidate_polygon = polygon(first.quadrilateral().points().to_vec());
        assert_f64_close(polygon_area(&candidate_polygon), 24.0);
    }

    #[test]
    fn minimum_area_candidate_encloses_a_triangle_with_its_short_side() {
        let input = polygon(vec![point(0.0, 0.0), point(4.0, 0.0), point(0.0, 3.0)]);

        let candidate = must_some(must_ok(minimum_area_quad_candidate(&input)));

        assert_eq!(
            candidate.quadrilateral().points(),
            [
                point(0.0, 0.0),
                point(4.0, 0.0),
                point(4.0, 3.0),
                point(0.0, 3.0),
            ]
        );
        assert_f64_close(candidate.short_side(), 3.0);
        let candidate_polygon = polygon(candidate.quadrilateral().points().to_vec());
        assert_f64_close(polygon_area(&candidate_polygon), 12.0);
    }
}
