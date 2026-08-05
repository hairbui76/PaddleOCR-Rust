// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! The layout ordering object model: `LayoutBlock`, `LayoutRegion`, and the
//! `xycut_enhanced` pipeline over them.
//!
//! Roadmap item `STRUCT-001`, the object-model slice — the unit the roadmap
//! named after the leaf primitives landed.
//!
//! # Shape
//!
//! Upstream passes shared, mutable Python objects everywhere: a block sits in
//! the region's map, in sorted lists, and in a parent's `child_blocks` at the
//! same time, and every site mutates it. This port uses an **index arena**
//! instead: [`OrderPage`] owns every [`OrderBlock`], and regions, sorted lists,
//! and child lists hold indices. Same aliasing, no `Rc<RefCell<…>>`.
//!
//! # Quirks preserved on purpose
//!
//! - **`width`, `height`, and `area` freeze at construction.** Upstream sets
//!   them in `__init__` and never recomputes, so after `append_child_block`
//!   unions the parent's bbox, `short_side_length` still derives from the
//!   *original* box. Recomputing would be tidier and would diverge.
//! - **`update_region_label` resets a vision block's `num_of_lines` to `1`**
//!   and re-derives its direction from the region's.
//! - **`reference_insert` can read a stale distance**: its `distance` is only
//!   assigned when a sorted block lies above the reference, and a block that
//!   does not reuses the previous iteration's value. Mirrored with an
//!   `Option` that skips the comparison until first assigned, which matches
//!   every input that does not crash upstream outright.
//!
//! Everything here is verified two ways: unit fixtures for the helpers with
//! subtle boundaries, and end-to-end page orderings captured from the
//! executed upstream in `tests/fixtures/classic-v1-layout-order/`. The
//! `region` label — a whole [`OrderPage`] ordered as one block inside a page
//! of regions, built with [`OrderBlock::from_region_page`] — is covered by the
//! nested orderings in `tests/fixtures/classic-v1-region-order/`.
#![allow(dead_code)]

use crate::reading_order::{
    Axis, HeuristicBox, label_weights, nearest_edge_distance, projection_overlap_ratio,
    recursive_xy_cut, recursive_yx_cut, sort_plain_blocks,
};

/// `XYCUT_SETTINGS`, as upstream spells them.
const CHILD_OVERLAP_THRESHOLD: f64 = 0.1;
const EDGE_TOLERANCE_LEN: f64 = 2.0;
const EDGE_WEIGHT: f64 = 1e4;
const UP_EDGE_WEIGHT: f64 = 1.0;
/// `distance_weight_map` has no `left_edge_weight` key, so the lookup falls to
/// its default — the map's `down_edge_weight` entry is dead. Preserved.
const LEFT_EDGE_WEIGHT: f64 = 0.0001;
const CROSS_LAYOUT_WORDS_THRESHOLD: f64 = 10.0;

/// A block's reading direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dir {
    /// Reads left to right.
    Horizontal,
    /// Reads top to bottom, columns right to left.
    Vertical,
}

impl Dir {
    const fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

/// Membership tests for `BLOCK_LABEL_MAP`, entry for entry.
fn is_header(label: &str) -> bool {
    matches!(label, "header" | "header_image")
}
fn is_doc_title(label: &str) -> bool {
    label == "doc_title"
}
fn is_paragraph_title(label: &str) -> bool {
    matches!(
        label,
        "paragraph_title" | "abstract_title" | "reference_title" | "content_title"
    )
}
fn is_vision(label: &str) -> bool {
    matches!(label, "image" | "table" | "chart" | "flowchart" | "figure")
}
fn is_vision_title(label: &str) -> bool {
    matches!(
        label,
        "table_title" | "chart_title" | "figure_title" | "figure_table_chart_title"
    )
}
fn is_footer(label: &str) -> bool {
    matches!(label, "footer" | "footer_image" | "footnote")
}
fn is_unordered(label: &str) -> bool {
    matches!(label, "aside_text" | "seal" | "number" | "formula_number")
}
fn is_text(label: &str) -> bool {
    label == "text"
}
fn is_region(label: &str) -> bool {
    label == "region"
}

/// One layout block in the arena.
#[derive(Clone, Debug)]
pub struct OrderBlock {
    /// The layout label.
    pub label: String,
    /// The ordering role, assigned by `update_region_label` and the child
    /// matchers. `None` until assigned.
    pub order_label: Option<String>,
    /// The bounding box, integer as upstream casts it. Mutated by shrinking,
    /// child unions, and the vertical-sort flip.
    pub bbox: [i64; 4],
    /// The pre-union bbox, kept from the first child append.
    ori_bbox: Option<[i64; 4]>,
    /// Frozen at construction; see the module docs.
    width: f64,
    /// Frozen at construction.
    height: f64,
    /// Frozen at construction.
    area: f64,
    /// First text segment's leading coordinate.
    pub seg_start: f64,
    /// Last text segment's trailing coordinate.
    pub seg_end: f64,
    /// Text lines inside the block.
    pub num_of_lines: u32,
    /// Mean text line height.
    pub text_line_height: f64,
    /// Mean text line width.
    pub text_line_width: f64,
    /// Arena index, set by [`OrderPage::new`].
    pub index: usize,
    /// Reading direction and its derived fields.
    pub direction: Dir,
    /// `LayoutRegion.euclidean_distance`: only `region`-labelled blocks carry
    /// a real value; plain blocks keep the infinite default and never reach
    /// the comparisons that read it.
    pub euclidean_distance: f64,
    /// Child arena indices, in append order.
    child_blocks: Vec<usize>,
}

impl OrderBlock {
    /// Builds a block from a spec; the arena assigns `index`.
    #[must_use]
    pub fn new(label: &str, bbox: [i64; 4]) -> Self {
        let width = (bbox[2] - bbox[0]) as f64;
        let height = (bbox[3] - bbox[1]) as f64;
        let mut block = Self {
            label: label.to_owned(),
            order_label: None,
            bbox,
            ori_bbox: None,
            width,
            height,
            area: width * height,
            seg_start: bbox[0] as f64,
            seg_end: bbox[2] as f64,
            num_of_lines: 1,
            text_line_height: 1.0,
            text_line_width: 1.0,
            index: 0,
            direction: Dir::Horizontal,
            euclidean_distance: f64::INFINITY,
            child_blocks: Vec::new(),
        };
        block.direction = block.bbox_direction();
        block
    }

    /// Builds the `region`-labelled block a whole [`OrderPage`] becomes when
    /// it is ordered inside a page of regions, the way `LayoutRegion` doubles
    /// as a `LayoutBlock`: direction, text-line means, and the euclidean
    /// distance come from the inner page, and `num_of_lines` is fixed at 10.
    #[must_use]
    pub fn from_region_page(inner: &OrderPage) -> Self {
        let mut block = Self::new("region", inner.region.bbox);
        block.direction = inner.region.direction;
        block.num_of_lines = 10;
        block.text_line_height = inner.region.text_line_height;
        block.text_line_width = inner.region.text_line_width;
        block.euclidean_distance = region_euclidean_distance(inner);
        block
    }

    fn bbox_direction(&self) -> Dir {
        if self.width >= self.height {
            Dir::Horizontal
        } else {
            Dir::Vertical
        }
    }

    fn heuristic_bbox(&self) -> HeuristicBox {
        [
            self.bbox[0] as f64,
            self.bbox[1] as f64,
            self.bbox[2] as f64,
            self.bbox[3] as f64,
        ]
    }

    /// `start_coordinate` along the block's own direction.
    fn start_coordinate(&self) -> i64 {
        match self.direction {
            Dir::Horizontal => self.bbox[0],
            Dir::Vertical => self.bbox[1],
        }
    }
    fn end_coordinate(&self) -> i64 {
        match self.direction {
            Dir::Horizontal => self.bbox[2],
            Dir::Vertical => self.bbox[3],
        }
    }
    fn secondary_start_coordinate(&self) -> i64 {
        match self.direction {
            Dir::Horizontal => self.bbox[1],
            Dir::Vertical => self.bbox[0],
        }
    }
    fn secondary_end_coordinate(&self) -> i64 {
        match self.direction {
            Dir::Horizontal => self.bbox[3],
            Dir::Vertical => self.bbox[2],
        }
    }
    /// Frozen sides: horizontal blocks are `height` short and `width` long.
    fn short_side(&self) -> f64 {
        match self.direction {
            Dir::Horizontal => self.height,
            Dir::Vertical => self.width,
        }
    }
    fn long_side(&self) -> f64 {
        match self.direction {
            Dir::Horizontal => self.width,
            Dir::Vertical => self.height,
        }
    }
    fn centroid(&self) -> (f64, f64) {
        (
            (self.bbox[0] + self.bbox[2]) as f64 / 2.0,
            (self.bbox[1] + self.bbox[3]) as f64 / 2.0,
        )
    }

    fn order_label_is(&self, name: &str) -> bool {
        self.order_label.as_deref() == Some(name)
    }
}

/// The region-level state `LayoutRegion` derives from its blocks.
#[derive(Clone, Debug)]
pub struct OrderRegion {
    /// The page (or region) bounding box.
    pub bbox: [i64; 4],
    /// The region's reading direction, from the majority of normal text.
    pub direction: Dir,
    /// Mean text line height over normal text blocks, default `10`.
    pub text_line_height: f64,
    /// Mean text line width over normal text blocks, default `20`.
    pub text_line_width: f64,
    header_idxes: Vec<usize>,
    doc_title_idxes: Vec<usize>,
    paragraph_title_idxes: Vec<usize>,
    vision_idxes: Vec<usize>,
    vision_title_idxes: Vec<usize>,
    footer_idxes: Vec<usize>,
    unordered_idxes: Vec<usize>,
    normal_text_idxes: Vec<usize>,
}

impl OrderRegion {
    fn direction_start_index(&self) -> usize {
        match self.direction {
            Dir::Horizontal => 0,
            Dir::Vertical => 1,
        }
    }
    fn direction_end_index(&self) -> usize {
        self.direction_start_index() + 2
    }
    fn secondary_start_index(&self) -> usize {
        match self.direction {
            Dir::Horizontal => 1,
            Dir::Vertical => 0,
        }
    }
    fn direction_center(&self) -> f64 {
        (self.bbox[self.direction_start_index()] + self.bbox[self.direction_end_index()]) as f64
            / 2.0
    }
}

/// The arena: every block of one page, plus the region built over them.
#[derive(Clone, Debug)]
pub struct OrderPage {
    /// The blocks, indexed by `OrderBlock::index`.
    pub blocks: Vec<OrderBlock>,
    /// The region state.
    pub region: OrderRegion,
}

impl OrderPage {
    /// Builds the region the way `LayoutRegion.__init__` does.
    #[must_use]
    pub fn new(page_bbox: [i64; 4], mut blocks: Vec<OrderBlock>) -> Self {
        let mut region = OrderRegion {
            bbox: page_bbox,
            direction: Dir::Horizontal,
            text_line_height: 10.0,
            text_line_width: 20.0,
            header_idxes: Vec::new(),
            doc_title_idxes: Vec::new(),
            paragraph_title_idxes: Vec::new(),
            vision_idxes: Vec::new(),
            vision_title_idxes: Vec::new(),
            footer_idxes: Vec::new(),
            unordered_idxes: Vec::new(),
            normal_text_idxes: Vec::new(),
        };
        let mut horizontal_normal = 0_usize;
        let mut heights = Vec::new();
        let mut widths = Vec::new();
        for (index, block) in blocks.iter_mut().enumerate() {
            block.index = index;
            let label = block.label.as_str();
            if is_header(label) {
                region.header_idxes.push(index);
            } else if is_doc_title(label) {
                region.doc_title_idxes.push(index);
            } else if is_paragraph_title(label) {
                region.paragraph_title_idxes.push(index);
            } else if is_vision(label) {
                region.vision_idxes.push(index);
            } else if is_vision_title(label) {
                region.vision_title_idxes.push(index);
            } else if is_footer(label) {
                region.footer_idxes.push(index);
            } else if is_unordered(label) {
                region.unordered_idxes.push(index);
            } else {
                region.normal_text_idxes.push(index);
                heights.push(block.text_line_height);
                widths.push(block.text_line_width);
                if block.direction == Dir::Horizontal {
                    horizontal_normal += 1;
                }
            }
        }
        region.direction =
            if horizontal_normal as f64 >= region.normal_text_idxes.len() as f64 * 0.5 {
                Dir::Horizontal
            } else {
                Dir::Vertical
            };
        if !widths.is_empty() {
            region.text_line_width = widths.iter().sum::<f64>() / widths.len() as f64;
        }
        if !heights.is_empty() {
            region.text_line_height = heights.iter().sum::<f64>() / heights.len() as f64;
        }
        Self { blocks, region }
    }

    /// `append_child_block`: union the parent's bbox and take the child's tree.
    fn append_child(&mut self, parent: usize, child: usize) {
        if self.blocks[parent].child_blocks.is_empty() {
            self.blocks[parent].ori_bbox = Some(self.blocks[parent].bbox);
        }
        let (pb, cb) = (self.blocks[parent].bbox, self.blocks[child].bbox);
        self.blocks[parent].bbox = [
            pb[0].min(cb[0]),
            pb[1].min(cb[1]),
            pb[2].max(cb[2]),
            pb[3].max(cb[3]),
        ];
        let mut taken = vec![child];
        taken.extend(std::mem::take(&mut self.blocks[child].child_blocks));
        self.blocks[parent].child_blocks.extend(taken);
    }

    /// `get_child_blocks`: restore the parent's bbox, drain the children.
    fn take_children(&mut self, parent: usize) -> Vec<usize> {
        if let Some(original) = self.blocks[parent].ori_bbox {
            self.blocks[parent].bbox = original;
        }
        std::mem::take(&mut self.blocks[parent].child_blocks)
    }
}

/// `get_nearest_blocks`: neighbours overlapping on `direction`'s projection.
fn nearest_blocks(
    page: &OrderPage,
    block: usize,
    refs: &[usize],
    overlap_threshold: f64,
    direction: Dir,
) -> (Vec<usize>, Vec<usize>) {
    let sort_index = match direction {
        Dir::Horizontal => 1,
        Dir::Vertical => 0,
    };
    let this = &page.blocks[block];
    let mut prev = Vec::new();
    let mut post = Vec::new();
    for &other in refs {
        if page.blocks[other].index == this.index {
            continue;
        }
        let ratio = projection_overlap_small(
            this.heuristic_bbox(),
            page.blocks[other].heuristic_bbox(),
            direction,
        );
        if ratio > overlap_threshold {
            if page.blocks[other].bbox[sort_index] <= this.bbox[sort_index] {
                prev.push(other);
            } else {
                post.push(other);
            }
        }
    }
    prev.sort_by_key(|&i| std::cmp::Reverse(page.blocks[i].bbox[sort_index]));
    post.sort_by_key(|&i| page.blocks[i].bbox[sort_index]);
    (prev, post)
}

/// `calculate_projection_overlap_ratio` with `mode="small"`.
fn projection_overlap_small(a: HeuristicBox, b: HeuristicBox, direction: Dir) -> f64 {
    let (start, end) = match direction {
        Dir::Horizontal => (0, 2),
        Dir::Vertical => (1, 3),
    };
    let overlap = a[end].min(b[end]) - a[start].max(b[start]);
    if overlap <= 0.0 {
        return 0.0;
    }
    let reference = (a[end] - a[start]).min(b[end] - b[start]);
    if reference <= 0.0 {
        return 0.0;
    }
    overlap / reference
}

/// `calculate_overlap_ratio`: area IoU (`union`) or over the smaller (`small`).
pub(crate) fn overlap_ratio(a: HeuristicBox, b: HeuristicBox, small: bool) -> f64 {
    let width = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let height = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let intersection = width * height;
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let reference = if small {
        area_a.min(area_b)
    } else {
        area_a + area_b - intersection
    };
    if reference <= 0.0 {
        0.0
    } else {
        intersection / reference
    }
}

/// `caculate_euclidean_dist`, misspelling theirs.
fn euclidean_dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The region's `euclidean_distance`: the nearest block corner to the origin
/// (or the top-right corner for vertical regions).
fn region_euclidean_distance(page: &OrderPage) -> f64 {
    let blocks = &page.blocks;
    if blocks.is_empty() {
        return 0.0;
    }
    blocks
        .iter()
        .map(|block| match page.region.direction {
            Dir::Horizontal => {
                euclidean_dist((block.bbox[0] as f64, block.bbox[1] as f64), (0.0, 0.0))
            }
            Dir::Vertical => euclidean_dist(
                (block.bbox[2] as f64, block.bbox[1] as f64),
                (page.region.bbox[2] as f64, 0.0),
            ),
        })
        .fold(f64::INFINITY, f64::min)
}

/// `calculate_discontinuous_projection`: merge intervals on one axis.
pub(crate) fn discontinuous_projection(
    boxes: &[[i64; 4]],
    direction: Dir,
) -> (Vec<(i64, i64)>, Vec<usize>) {
    let (lo, hi) = match direction {
        Dir::Horizontal => (0, 2),
        Dir::Vertical => (1, 3),
    };
    let mut intervals: Vec<(i64, i64)> = boxes.iter().map(|b| (b[lo], b[hi])).collect();
    intervals.sort_by_key(|&(start, _)| start);
    let mut merged = Vec::new();
    let mut counts = Vec::new();
    let (mut current_start, mut current_end) = intervals[0];
    let mut count = 1_usize;
    for &(start, end) in &intervals[1..] {
        if start <= current_end {
            count += 1;
            current_end = current_end.max(end);
        } else {
            merged.push((current_start, current_end));
            counts.push(count);
            current_start = start;
            current_end = end;
            count = 1;
        }
    }
    merged.push((current_start, current_end));
    counts.push(count);
    (merged, counts)
}

/// `find_local_minima_flat_regions` over a projection histogram.
///
/// Returns `None` for one-or-fewer minima but `Some([])` for an **empty**
/// input — upstream returns an early `[]` there and `None` later, and the
/// two are different values to the caller. Preserved as captured.
pub(crate) fn local_minima_flat_regions(values: &[u32]) -> Option<Vec<(usize, usize)>> {
    let n = values.len();
    if n == 0 {
        return Some(Vec::new());
    }
    let mut regions = Vec::new();
    let mut start = 0_usize;
    for i in 1..n {
        if values[i] != values[i - 1] {
            if (start == 0 || values[start - 1] > values[start]) && values[i] > values[start] {
                regions.push((start, i - 1));
            }
            start = i;
        }
    }
    if regions.len() > 1 {
        Some(regions[1..].to_vec())
    } else {
        None
    }
}

/// `shrink_overlapping_boxes`: split slight overlaps at their midpoint.
pub(crate) fn shrink_overlapping_boxes(page: &mut OrderPage, order: &[usize], direction: Dir) {
    if order.is_empty() {
        return;
    }
    let mut current = order[0];
    for &next in &order[1..] {
        let a = page.blocks[current].heuristic_bbox();
        let b = page.blocks[next].heuristic_bbox();
        let cut_iou = projection_overlap_ratio(
            a,
            b,
            match direction {
                Dir::Horizontal => Axis::Horizontal,
                Dir::Vertical => Axis::Vertical,
            },
        );
        let match_iou = projection_overlap_ratio(
            a,
            b,
            match direction {
                Dir::Horizontal => Axis::Vertical,
                Dir::Vertical => Axis::Horizontal,
            },
        );
        let (a_box, b_box) = (page.blocks[current].bbox, page.blocks[next].bbox);
        if direction == Dir::Vertical {
            let (y2, y1p) = (a_box[3], b_box[1]);
            if (match_iou > 0.0 && cut_iou > 0.0 && cut_iou < 0.1)
                || y2 == y1p
                || (y2 - y1p).abs() <= 3
            {
                let overlap_min = a_box[1].max(b_box[1]);
                let overlap_max = a_box[3].min(b_box[3]);
                let split = (overlap_min + overlap_max) / 2;
                let (low, high) = (split - 1, split + 1);
                if a_box[1] < b_box[1] {
                    page.blocks[current].bbox[3] = low;
                    page.blocks[next].bbox[1] = high;
                } else {
                    page.blocks[current].bbox[1] = low;
                    page.blocks[next].bbox[3] = high;
                }
            }
        } else {
            let (x2, x1p) = (a_box[2], b_box[0]);
            if (match_iou > 0.0 && cut_iou > 0.0 && cut_iou < 0.1)
                || x2 == x1p
                || (x2 - x1p).abs() <= 3
            {
                let overlap_min = a_box[0].max(b_box[0]);
                let overlap_max = a_box[2].min(b_box[2]);
                let split = (overlap_min + overlap_max) / 2;
                let (low, high) = (split - 1, split + 1);
                if a_box[0] < b_box[0] {
                    page.blocks[current].bbox[2] = low;
                    page.blocks[next].bbox[0] = high;
                } else {
                    page.blocks[current].bbox[0] = low;
                    page.blocks[next].bbox[2] = high;
                }
            }
        }
        current = next;
    }
}

/// The child matchers, one per parent role.
mod children {
    use super::*;

    pub(super) fn doc_title(page: &mut OrderPage, block: usize) {
        let refs = page.region.normal_text_idxes.clone();
        let direction = page.blocks[block].direction;
        let (prev, post) = nearest_blocks(page, block, &refs, CHILD_OVERLAP_THRESHOLD, direction);
        for candidate in [prev.first().copied(), post.first().copied()]
            .into_iter()
            .flatten()
        {
            let parent = &page.blocks[block];
            let child = &page.blocks[candidate];
            let same_direction = child.direction == parent.direction;
            let short_ok = child.short_side() < parent.short_side() * 0.8;
            let long_ok = child.long_side() < parent.long_side()
                || child.long_side() > 1.5 * parent.long_side();
            let distance =
                nearest_edge_distance(parent.heuristic_bbox(), child.heuristic_bbox(), [1.0; 4]);
            if same_direction
                && is_text(&child.label)
                && short_ok
                && long_ok
                && child.num_of_lines < 3
                && distance < child.text_line_height * 2.0
            {
                page.blocks[candidate].order_label = Some("doc_title_text".to_owned());
                page.append_child(block, candidate);
                page.region.normal_text_idxes.retain(|&i| i != candidate);
            }
        }
        for candidate in refs {
            if page.blocks[candidate].order_label_is("doc_title_text") {
                continue;
            }
            let parent = &page.blocks[block];
            let child = &page.blocks[candidate];
            let same_direction = child.direction == parent.direction;
            let ratio = overlap_ratio(parent.heuristic_bbox(), child.heuristic_bbox(), true);
            if ratio > 0.9 && same_direction {
                page.blocks[candidate].order_label = Some("doc_title_text".to_owned());
                page.append_child(block, candidate);
                page.region.normal_text_idxes.retain(|&i| i != candidate);
            }
        }
    }

    pub(super) fn paragraph_title(page: &mut OrderPage, block: usize) {
        if page.blocks[block].order_label_is("sub_paragraph_title") {
            return;
        }
        let refs: Vec<usize> = page
            .region
            .paragraph_title_idxes
            .iter()
            .chain(page.region.normal_text_idxes.iter())
            .copied()
            .collect();
        let direction = page.blocks[block].direction;
        let (prev, post) = nearest_blocks(page, block, &refs, CHILD_OVERLAP_THRESHOLD, direction);
        for side in [prev, post] {
            for candidate in side {
                if !is_paragraph_title(&page.blocks[candidate].label) {
                    break;
                }
                let parent = &page.blocks[block];
                let child = &page.blocks[candidate];
                let min_line_height = parent.text_line_height.min(child.text_line_height);
                let distance = nearest_edge_distance(
                    parent.heuristic_bbox(),
                    child.heuristic_bbox(),
                    [1.0; 4],
                );
                let same_direction = child.direction == parent.direction;
                let same_start = (child.start_coordinate() - parent.start_coordinate()).abs()
                    < (min_line_height * 2.0) as i64;
                if same_direction && same_start && distance <= min_line_height * 1.5 {
                    page.blocks[candidate].order_label = Some("sub_paragraph_title".to_owned());
                    page.append_child(block, candidate);
                    page.region
                        .paragraph_title_idxes
                        .retain(|&i| i != candidate);
                }
            }
        }
    }

    /// `update_region_child_blocks`: any overlapped smaller region becomes a
    /// `sub_region` child of the larger one.
    pub(super) fn region(page: &mut OrderPage, block: usize) {
        for candidate in 0..page.blocks.len() {
            if candidate == block {
                continue;
            }
            let iou = overlap_ratio(
                page.blocks[block].heuristic_bbox(),
                page.blocks[candidate].heuristic_bbox(),
                false,
            );
            if iou > 0.0
                && page.blocks[block].area > page.blocks[candidate].area
                && !page.blocks[candidate].order_label_is("sub_region")
            {
                page.blocks[candidate].order_label = Some("sub_region".to_owned());
                page.append_child(block, candidate);
                page.region.normal_text_idxes.retain(|&i| i != candidate);
            }
        }
    }

    pub(super) fn vision(page: &mut OrderPage, block: usize) {
        let refs: Vec<usize> = page
            .region
            .normal_text_idxes
            .iter()
            .chain(page.region.vision_title_idxes.iter())
            .copied()
            .collect();
        let mut has_footnote = false;
        let mut has_title = false;
        let directions = [
            page.blocks[block].direction,
            page.blocks[block].direction.other(),
        ];
        for direction in directions {
            let (prev, post) =
                nearest_blocks(page, block, &refs, CHILD_OVERLAP_THRESHOLD, direction);
            for (side, is_post) in [(prev, false), (post, true)] {
                for candidate in side {
                    let child_label = page.blocks[candidate].label.clone();
                    if !(is_post || is_text(&child_label) || is_vision_title(&child_label)) {
                        break;
                    }
                    if is_post && has_footnote && is_text(&child_label) {
                        break;
                    }
                    let parent = &page.blocks[block];
                    let child = &page.blocks[candidate];
                    let distance = nearest_edge_distance(
                        parent.heuristic_bbox(),
                        child.heuristic_bbox(),
                        [1.0; 4],
                    );
                    let parent_center = parent.centroid();
                    let child_center = child.centroid();
                    if is_vision_title(&child_label) && distance <= child.text_line_height * 2.0 {
                        has_title = true;
                        page.blocks[candidate].order_label = Some("vision_title".to_owned());
                        page.append_child(block, candidate);
                        page.region.vision_title_idxes.retain(|&i| i != candidate);
                    }
                    if is_text(&child_label) {
                        let parent = &page.blocks[block];
                        let child = &page.blocks[candidate];
                        if !has_footnote
                            && child.direction == parent.direction
                            && child.long_side() < parent.long_side()
                            && distance <= child.text_line_height * 2.0
                        {
                            let centred = child.short_side() < parent.short_side()
                                && child.long_side() < 0.5 * parent.long_side()
                                && (parent_center.0 - child_center.0).abs() < 10.0;
                            let left_aligned =
                                (parent.bbox[0] - child.bbox[0]) < 10 && child.num_of_lines == 1;
                            let right_aligned =
                                (parent.bbox[2] - child.bbox[2]) < 10 && child.num_of_lines == 1;
                            if centred || left_aligned || right_aligned {
                                has_footnote = true;
                                if is_post {
                                    page.blocks[candidate].label = "vision_footnote".to_owned();
                                }
                                page.blocks[candidate].order_label =
                                    Some("vision_footnote".to_owned());
                                page.append_child(block, candidate);
                                page.region.normal_text_idxes.retain(|&i| i != candidate);
                            }
                        }
                        break;
                    }
                }
            }
            if has_title {
                break;
            }
        }
        for candidate in refs {
            if !page.region.normal_text_idxes.contains(&candidate) {
                continue;
            }
            let ratio = overlap_ratio(
                page.blocks[block].heuristic_bbox(),
                page.blocks[candidate].heuristic_bbox(),
                true,
            );
            if ratio > 0.9 {
                page.blocks[candidate].label = "vision_footnote".to_owned();
                page.blocks[candidate].order_label = Some("vision_footnote".to_owned());
                page.append_child(block, candidate);
                page.region.normal_text_idxes.retain(|&i| i != candidate);
            }
        }
    }
}

/// `update_region_label`, including dispatch into the child matchers.
fn update_region_label(page: &mut OrderPage, block: usize) {
    let label = page.blocks[block].label.clone();
    let order = if is_header(&label) {
        "header"
    } else if is_doc_title(&label) {
        "doc_title"
    } else if is_paragraph_title(&label) && page.blocks[block].order_label.is_none() {
        "paragraph_title"
    } else if is_vision(&label) {
        page.blocks[block].num_of_lines = 1;
        let region_direction = page.region.direction;
        page.blocks[block].direction = region_direction;
        "vision"
    } else if is_footer(&label) {
        "footer"
    } else if is_unordered(&label) {
        "unordered"
    } else if is_region(&label) {
        "region"
    } else {
        "normal_text"
    };
    if page.blocks[block].order_label.is_none() || order != "paragraph_title" {
        page.blocks[block].order_label = Some(order.to_owned());
    }
    match order {
        "doc_title" => children::doc_title(page, block),
        "paragraph_title" => children::paragraph_title(page, block),
        "vision" => children::vision(page, block),
        "region" => children::region(page, block),
        _ => {}
    }
}

/// `get_layout_structure`: mark blocks that span other columns.
fn mark_cross_layout(page: &mut OrderPage, members: &mut [usize], region_direction: Dir) {
    members.sort_by(|&a, &b| {
        let (ba, bb) = (&page.blocks[a], &page.blocks[b]);
        ba.bbox[0]
            .cmp(&bb.bbox[0])
            .then(ba.width.total_cmp(&bb.width))
    });
    let masked = |page: &OrderPage, i: usize| {
        matches!(
            page.blocks[i].order_label.as_deref(),
            Some("doc_title" | "cross_layout" | "cross_reference")
        )
    };
    let axis = match region_direction {
        Dir::Horizontal => Axis::Horizontal,
        Dir::Vertical => Axis::Vertical,
    };
    let secondary = match region_direction {
        Dir::Horizontal => Axis::Vertical,
        Dir::Vertical => Axis::Horizontal,
    };
    for position in 0..members.len() {
        let block = members[position];
        if masked(page, block) {
            continue;
        }
        'refs: for (ref_position, &reference) in members.iter().enumerate() {
            if position == ref_position || masked(page, reference) {
                continue;
            }
            let bbox_iou = overlap_ratio(
                page.blocks[block].heuristic_bbox(),
                page.blocks[reference].heuristic_bbox(),
                false,
            );
            if bbox_iou > 0.0 {
                if page.blocks[reference].order_label_is("vision") {
                    page.blocks[reference].order_label = Some("cross_layout".to_owned());
                    break 'refs;
                }
                if bbox_iou > 0.1 && page.blocks[block].area < page.blocks[reference].area {
                    page.blocks[block].order_label = Some("cross_layout".to_owned());
                    break 'refs;
                }
            }
            let match_iou = projection_overlap_ratio(
                page.blocks[block].heuristic_bbox(),
                page.blocks[reference].heuristic_bbox(),
                axis,
            );
            if match_iou > 0.0 {
                for (second_position, &second) in members.iter().enumerate() {
                    if second_position == position
                        || second_position == ref_position
                        || masked(page, second)
                    {
                        continue;
                    }
                    let second_iou = overlap_ratio(
                        page.blocks[block].heuristic_bbox(),
                        page.blocks[second].heuristic_bbox(),
                        false,
                    );
                    if second_iou > 0.1 {
                        if page.blocks[second].order_label_is("vision") {
                            page.blocks[second].order_label = Some("cross_layout".to_owned());
                            break;
                        }
                        if page.blocks[block].order_label_is("vision")
                            || page.blocks[block].area < page.blocks[second].area
                        {
                            page.blocks[block].order_label = Some("cross_layout".to_owned());
                            break;
                        }
                    }
                    let second_match = projection_overlap_ratio(
                        page.blocks[block].heuristic_bbox(),
                        page.blocks[second].heuristic_bbox(),
                        axis,
                    );
                    let ref_match = projection_overlap_ratio(
                        page.blocks[reference].heuristic_bbox(),
                        page.blocks[second].heuristic_bbox(),
                        axis,
                    );
                    let ref_secondary = projection_overlap_ratio(
                        page.blocks[reference].heuristic_bbox(),
                        page.blocks[second].heuristic_bbox(),
                        secondary,
                    );
                    if second_match > 0.0 && ref_match == 0.0 && ref_secondary > 0.0 {
                        let qualified = page.blocks[block].order_label_is("vision")
                            || page.blocks[block].order_label_is("region")
                            || (page.blocks[reference].order_label_is("normal_text")
                                && page.blocks[second].order_label_is("normal_text")
                                && page.blocks[reference].long_side()
                                    > page.blocks[reference].text_line_height
                                        * CROSS_LAYOUT_WORDS_THRESHOLD
                                && page.blocks[second].long_side()
                                    > page.blocks[second].text_line_height
                                        * CROSS_LAYOUT_WORDS_THRESHOLD);
                        if qualified {
                            page.blocks[block].order_label =
                                Some(if page.blocks[block].label == "reference" {
                                    "cross_reference".to_owned()
                                } else {
                                    "cross_layout".to_owned()
                                });
                        }
                    }
                }
            }
        }
    }
}

/// The four insertion strategies of `match_unsorted_blocks`.
mod insert {
    use super::*;

    pub(super) fn reference(page: &OrderPage, block: usize, sorted: &mut Vec<usize>) {
        let mut min_distance = f64::INFINITY;
        let mut nearest = 0_usize;
        let mut distance: Option<f64> = None;
        for (position, &candidate) in sorted.iter().enumerate() {
            let cb = &page.blocks[candidate];
            if cb.bbox[3] <= page.blocks[block].bbox[1] {
                distance = Some(-((cb.bbox[2] * 10 + cb.bbox[3]) as f64));
            }
            if let Some(value) = distance
                && value < min_distance
            {
                min_distance = value;
                nearest = position;
            }
        }
        sorted.insert((nearest + 1).min(sorted.len()), block);
    }

    /// `euclidean_insert`: before the first sorted region whose distance to
    /// the reference corner is strictly larger.
    pub(super) fn euclidean(page: &OrderPage, block: usize, sorted: &mut Vec<usize>) {
        let distance = page.blocks[block].euclidean_distance;
        let position = sorted
            .iter()
            .position(|&candidate| page.blocks[candidate].euclidean_distance > distance)
            .unwrap_or(sorted.len());
        sorted.insert(position, block);
    }

    pub(super) fn manhattan(page: &OrderPage, block: usize, sorted: &mut Vec<usize>) {
        let mut min_distance = f64::INFINITY;
        let mut nearest = 0_usize;
        let b = &page.blocks[block].bbox;
        for (position, &candidate) in sorted.iter().enumerate() {
            let c = &page.blocks[candidate].bbox;
            // `_manhattan_distance(block.bbox, sorted_block.bbox)`: upstream
            // hands four-element boxes to a two-point function, so only the
            // top-left corners are compared. Preserved.
            let distance = (b[0] - c[0]).abs() as f64 + (b[1] - c[1]).abs() as f64;
            if distance < min_distance {
                min_distance = distance;
                nearest = position;
            }
        }
        sorted.insert((nearest + 1).min(sorted.len()), block);
    }

    pub(super) fn weighted(page: &OrderPage, block: usize, sorted: &mut Vec<usize>) {
        let region = &page.region;
        let this = &page.blocks[block];
        let mut tolerance = EDGE_TOLERANCE_LEN;
        let (x1, y1, x2, _y2) = (this.bbox[0], this.bbox[1], this.bbox[2], this.bbox[3]);
        let mut min_weighted = f64::INFINITY;
        let mut min_up_edge = f64::INFINITY;
        let mut nearest = 0_usize;
        for (position, &candidate) in sorted.iter().enumerate() {
            let cb = &page.blocks[candidate];
            let weight = label_weights(
                this.order_label.as_deref().unwrap_or(""),
                this.direction == Dir::Vertical,
            );
            let mut edge =
                nearest_edge_distance(this.heuristic_bbox(), cb.heuristic_bbox(), weight);
            if is_doc_title(&this.label) {
                tolerance = tolerance.max(region.text_line_width.max(1.0));
            }
            if this.label == "abstract" {
                tolerance *= 2.0;
                edge = edge.max(0.1) * 10.0;
            }
            let up_edge = match region.direction {
                Dir::Horizontal => cb.bbox[1] as f64,
                Dir::Vertical => -(cb.bbox[2] as f64),
            };
            let left_edge = match region.direction {
                Dir::Horizontal => cb.bbox[0] as f64,
                Dir::Vertical => cb.bbox[1] as f64,
            };
            let below = match region.direction {
                Dir::Horizontal => cb.bbox[3] < y1,
                Dir::Vertical => cb.bbox[0] > x2,
            };
            let ordered_role = !this.order_label_is("unordered")
                || is_doc_title(&this.label)
                || is_paragraph_title(&this.label)
                || is_vision(&this.label);
            let (mut up_edge, left_edge) = if ordered_role && below {
                (-up_edge, -left_edge)
            } else {
                (up_edge, left_edge)
            };
            if (min_up_edge - up_edge).abs() <= tolerance {
                up_edge = min_up_edge;
            }
            let weighted =
                edge * EDGE_WEIGHT + up_edge * UP_EDGE_WEIGHT + left_edge * LEFT_EDGE_WEIGHT;
            min_up_edge = min_up_edge.min(up_edge);
            if weighted < min_weighted {
                min_weighted = weighted;
                nearest = position;
                // The tie-break between inserting before or after the nearest
                // block: compare along y, then x, then squared centre distance.
                let (sorted_distance, block_distance) = if (y1 / 2 - cb.bbox[1] / 2).abs() > 0 {
                    (cb.bbox[1] as f64, y1 as f64)
                } else if region.direction == Dir::Horizontal {
                    if (x1 / 2 - x2 / 2).abs() > 0 {
                        (cb.bbox[0] as f64, x1 as f64)
                    } else {
                        let cc = cb.centroid();
                        let bc = this.centroid();
                        (cc.0 * cc.0 + cc.1 * cc.1, bc.0 * bc.0 + bc.1 * bc.1)
                    }
                } else if (x1 - x2).abs() > 0 {
                    (-(cb.bbox[2] as f64), -(x2 as f64))
                } else {
                    let cc = cb.centroid();
                    let bc = this.centroid();
                    (cc.0 * cc.0 + cc.1 * cc.1, bc.0 * bc.0 + bc.1 * bc.1)
                };
                if block_distance > sorted_distance {
                    nearest = position + 1;
                    if position < sorted.len() - 1
                        && (is_vision(&this.label) || is_vision_title(&this.label))
                    {
                        let (start, _) = seg_flag(page, sorted[position + 1], sorted[position]);
                        if !start {
                            nearest += 1;
                        }
                    }
                } else if position > 0 && (is_vision(&this.label) || is_vision_title(&this.label)) {
                    let (start, _) = seg_flag(page, sorted[position], sorted[position - 1]);
                    if !start {
                        nearest = position - 1;
                    }
                }
            }
        }
        sorted.insert(nearest.min(sorted.len()), block);
    }
}

/// `get_seg_flag` over two arena blocks, through the markdown port.
fn seg_flag(page: &OrderPage, block: usize, previous: usize) -> (bool, bool) {
    let geometry = |b: &OrderBlock| crate::markdown::BlockGeometry {
        start: b.start_coordinate() as f64,
        end: b.end_coordinate() as f64,
        seg_start: b.seg_start,
        seg_end: b.seg_end,
        lines: b.num_of_lines,
        width: b.width,
    };
    crate::markdown::paragraph_continues(
        geometry(&page.blocks[block]),
        Some(geometry(&page.blocks[previous])),
    )
}

/// `get_cut_blocks`: split blocks into bands at the cut coordinates.
fn cut_blocks(
    page: &OrderPage,
    mut members: Vec<usize>,
    cut_direction: Dir,
    mut cut_coordinates: Vec<i64>,
    mask: &[&str],
) -> Vec<Vec<usize>> {
    let axis_end = match cut_direction {
        Dir::Horizontal => 2,
        Dir::Vertical => 3,
    };
    members.sort_by_key(|&i| page.blocks[i].bbox[axis_end]);
    cut_coordinates.push(i64::MAX);
    cut_coordinates.sort_unstable();
    cut_coordinates.dedup();
    let mut groups = Vec::new();
    let mut cursor = 0_usize;
    for cut in cut_coordinates {
        let mut group = Vec::new();
        while cursor < members.len() {
            let block = &page.blocks[members[cursor]];
            if block.bbox[axis_end] > cut {
                break;
            }
            let masked = block
                .order_label
                .as_deref()
                .is_some_and(|l| mask.contains(&l));
            if !masked {
                group.push(members[cursor]);
            }
            cursor += 1;
        }
        if !group.is_empty() {
            groups.push(group);
        }
    }
    groups
}

/// `get_blocks_by_direction_interval`.
fn blocks_in_interval(
    page: &OrderPage,
    mut members: Vec<usize>,
    start: i64,
    end: i64,
    direction: Dir,
) -> Vec<usize> {
    let axis = match direction {
        Dir::Horizontal => 0,
        Dir::Vertical => 1,
    };
    members.sort_by_key(|&i| page.blocks[i].bbox[axis + 2]);
    members
        .into_iter()
        .filter(|&i| page.blocks[i].bbox[axis] >= start && page.blocks[i].bbox[axis + 2] <= end)
        .collect()
}

const PRE_PROCESS_MASK: [&str; 8] = [
    "header",
    "unordered",
    "footer",
    "vision_footnote",
    "sub_paragraph_title",
    "doc_title_text",
    "vision_title",
    "sub_region",
];

/// `pre_process`: label roles, match children, and pre-cut the page.
fn pre_process(page: &mut OrderPage) -> Vec<Vec<usize>> {
    let indices: Vec<usize> = (0..page.blocks.len()).collect();
    let mut pre_cut_idxes = Vec::new();
    for &block in &indices {
        let already_masked = page.blocks[block]
            .order_label
            .as_deref()
            .is_some_and(|l| PRE_PROCESS_MASK.contains(&l));
        if !already_masked {
            update_region_label(page, block);
        }
        let b = &page.blocks[block];
        let tolerance = match b.direction {
            Dir::Horizontal => b.long_side() as i64 / 5,
            Dir::Vertical => b.short_side() as i64 / 10,
        };
        let center = (b.bbox[page.region.direction_start_index()]
            + b.bbox[page.region.direction_end_index()]) as f64
            / 2.0;
        if (center - page.region.direction_center()).abs() <= tolerance as f64 {
            pre_cut_idxes.push(block);
        }
    }

    let cut_direction = page.region.direction.other();
    let mut cut_coordinates: Vec<i64> = Vec::new();
    let unmasked: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&i| {
            !page.blocks[i]
                .order_label
                .as_deref()
                .is_some_and(|l| PRE_PROCESS_MASK.contains(&l))
        })
        .collect();
    let all_boxes: Vec<[i64; 4]> = unmasked.iter().map(|&i| page.blocks[i].bbox).collect();
    if all_boxes.is_empty() {
        return Vec::new();
    }
    let mut discontinuous: Vec<(i64, i64)> = Vec::new();
    if !pre_cut_idxes.is_empty() {
        let (merged, counts) = discontinuous_projection(&all_boxes, cut_direction);
        discontinuous = merged;
        for &idx in &pre_cut_idxes {
            let b = &page.blocks[idx];
            let masked = b
                .order_label
                .as_deref()
                .is_some_and(|l| PRE_PROCESS_MASK.contains(&l));
            if !masked && b.direction.other() == cut_direction {
                let pair = (b.secondary_start_coordinate(), b.secondary_end_coordinate());
                if let Some(found) = discontinuous.iter().position(|&i| i == pair)
                    && counts[found] == 1
                {
                    cut_coordinates.push(pair.0);
                    cut_coordinates.push(pair.1);
                }
            }
        }
    }

    // A page of regions forces the pre-cut path: every primary-direction gap
    // becomes a cut, whatever the secondary projection looks like.
    let is_region_page = is_region(&page.blocks[0].label);
    let secondary_boxes: Vec<[i64; 4]> = unmasked
        .iter()
        .copied()
        .filter(|&i| !page.blocks[i].order_label_is("vision"))
        .map(|i| page.blocks[i].bbox)
        .collect();
    if !secondary_boxes.is_empty() {
        let (secondary_merged, _) =
            discontinuous_projection(&secondary_boxes, page.region.direction);
        if secondary_merged.len() == 1 || is_region_page {
            if discontinuous.is_empty() {
                discontinuous = discontinuous_projection(&all_boxes, cut_direction).0;
            }
            let mut current = discontinuous[0];
            let mut pre_cut_coordinate = cut_coordinates
                .iter()
                .copied()
                .filter(|&c| c < current.1)
                .max()
                .unwrap_or(0)
                .max(current.0);
            for &interval in &discontinuous[1..] {
                let gap = interval.0 - current.1;
                if gap as f64 >= page.region.text_line_height * 3.0 || is_region_page {
                    cut_coordinates.push(current.1);
                } else if gap as f64 > page.region.text_line_height * 1.2 {
                    let pre = blocks_in_interval(
                        page,
                        indices.clone(),
                        pre_cut_coordinate,
                        current.1,
                        cut_direction,
                    );
                    let post = blocks_in_interval(
                        page,
                        indices.clone(),
                        current.1,
                        interval.1,
                        cut_direction,
                    );
                    let projection_axis = match cut_direction {
                        Dir::Horizontal => Axis::Vertical,
                        Dir::Vertical => Axis::Horizontal,
                    };
                    let gap_intervals = |members: &[usize]| -> Vec<[i64; 4]> {
                        let boxes: Vec<crate::reading_order::OrderBox> =
                            members.iter().map(|&i| page.blocks[i].bbox).collect();
                        let Ok(histogram) =
                            crate::reading_order::projection(&boxes, projection_axis)
                        else {
                            return Vec::new();
                        };
                        let Some(minima) = local_minima_flat_regions(&histogram) else {
                            return Vec::new();
                        };
                        minima
                            .into_iter()
                            .map(|(start, end)| {
                                let mut b = [0_i64; 4];
                                let axis = match cut_direction {
                                    Dir::Horizontal => 1,
                                    Dir::Vertical => 0,
                                };
                                b[axis] = start as i64;
                                b[axis + 2] = end as i64;
                                b
                            })
                            .collect()
                    };
                    let pre_gaps = gap_intervals(&pre);
                    let post_gaps = gap_intervals(&post);
                    let max_gaps = pre_gaps.len().max(post_gaps.len());
                    if max_gaps > 0 {
                        let mut combined = pre_gaps;
                        combined.extend(post_gaps);
                        let (merged, _) =
                            discontinuous_projection(&combined, page.region.direction);
                        if merged.len() != max_gaps {
                            pre_cut_coordinate = current.1;
                            cut_coordinates.push(current.1);
                        }
                    }
                }
                current = interval;
            }
        }
    }

    let mut groups = cut_blocks(
        page,
        indices,
        cut_direction,
        cut_coordinates,
        &PRE_PROCESS_MASK,
    );
    if page.region.direction == Dir::Vertical {
        groups.reverse();
    }
    groups
}

/// `sort_by_xycut` over arena blocks.
fn sort_by_xycut(boxes: &[[i64; 4]], direction: Dir) -> Vec<usize> {
    let order_boxes: Vec<crate::reading_order::OrderBox> = boxes.to_vec();
    let indices: Vec<usize> = (0..boxes.len()).collect();
    let mut result = Vec::new();
    let outcome = match direction {
        Dir::Vertical => recursive_yx_cut(&order_boxes, &indices, &mut result),
        Dir::Horizontal => recursive_xy_cut(&order_boxes, &indices, &mut result),
    };
    if outcome.is_err() {
        return indices;
    }
    result
}

/// `match_unsorted_blocks`.
fn match_unsorted(page: &mut OrderPage, sorted: &mut Vec<usize>, unsorted: Vec<usize>) {
    let boxes: Vec<HeuristicBox> = unsorted
        .iter()
        .map(|&i| page.blocks[i].heuristic_bbox())
        .collect();
    let order = sort_plain_blocks(
        &boxes,
        page.region.text_line_height,
        page.region.text_line_width,
        page.region.direction == Dir::Vertical,
    );
    for (position, &slot) in order.iter().enumerate() {
        let block = unsorted[slot];
        // A `region`-labelled block is matched as a region regardless of the
        // order label a cross-layout pass may have stamped on it.
        let role = if is_region(&page.blocks[block].label) {
            "region".to_owned()
        } else {
            page.blocks[block]
                .order_label
                .clone()
                .unwrap_or_else(|| "other".to_owned())
        };
        if position == 0 && role == "doc_title" {
            sorted.insert(0, block);
            continue;
        }
        match role.as_str() {
            "cross_layout" | "paragraph_title" | "doc_title" | "vision_title" | "vision" => {
                insert::weighted(page, block, sorted);
            }
            "cross_reference" => insert::reference(page, block, sorted),
            "region" => insert::euclidean(page, block, sorted),
            _ => insert::manhattan(page, block, sorted),
        }
    }
}

/// `insert_child_blocks` + `sort_child_blocks` over the final list.
fn expand_children(page: &mut OrderPage, ordered: &mut Vec<usize>) {
    let mut position = 0_usize;
    while position < ordered.len() {
        let block = ordered[position];
        if !page.blocks[block].child_blocks.is_empty() {
            let mut family = page.take_children(block);
            family.push(block);
            // `sort_child_blocks`: a family of regions sorts by euclidean
            // distance; everything else by the coordinate keys.
            if is_region(&page.blocks[family[0]].label) {
                family.sort_by(|&a, &b| {
                    page.blocks[a]
                        .euclidean_distance
                        .total_cmp(&page.blocks[b].euclidean_distance)
                });
                ordered[position] = family[0];
                for (offset, &member) in family[1..].iter().enumerate() {
                    ordered.insert(position + 1 + offset, member);
                }
                position += 1;
                continue;
            }
            let direction = page.blocks[family[0]].direction;
            family.sort_by(|&a, &b| {
                let (ba, bb) = (&page.blocks[a], &page.blocks[b]);
                let key = |x: &OrderBlock| {
                    let c = x.centroid();
                    match direction {
                        Dir::Horizontal => {
                            (x.bbox[1] as f64, x.bbox[0] as f64, c.0 * c.0 + c.1 * c.1)
                        }
                        Dir::Vertical => (
                            -(x.bbox[2] as f64),
                            x.bbox[1] as f64,
                            -c.0 * c.0 + c.1 * c.1,
                        ),
                    }
                };
                let (ka, kb) = (key(ba), key(bb));
                ka.0.total_cmp(&kb.0)
                    .then(ka.1.total_cmp(&kb.1))
                    .then(ka.2.total_cmp(&kb.2))
            });
            ordered[position] = family[0];
            for (offset, &member) in family[1..].iter().enumerate() {
                ordered.insert(position + 1 + offset, member);
            }
        }
        position += 1;
    }
}

/// `xycut_enhanced`: the whole ordering pipeline over one page.
///
/// Returns the blocks' arena indices in reading order, children expanded.
#[must_use]
pub fn xycut_enhanced_order(page: &mut OrderPage) -> Vec<usize> {
    if page.blocks.is_empty() {
        return Vec::new();
    }
    let pre_cut_list = pre_process(page);

    let sort_set = |page: &OrderPage, set: &[usize]| -> Vec<usize> {
        let boxes: Vec<HeuristicBox> = set
            .iter()
            .map(|&i| page.blocks[i].heuristic_bbox())
            .collect();
        sort_plain_blocks(
            &boxes,
            page.region.text_line_height,
            page.region.text_line_width,
            page.region.direction == Dir::Vertical,
        )
        .into_iter()
        .map(|slot| set[slot])
        .collect()
    };
    let header_blocks = sort_set(page, &page.region.header_idxes.clone());
    let footer_blocks = sort_set(page, &page.region.footer_idxes.clone());
    let unordered_blocks = sort_set(page, &page.region.unordered_idxes.clone());

    let mut final_order: Vec<usize> = header_blocks;
    let mut unsorted: Vec<usize> = Vec::new();
    let mut sorted_by_pre_cuts: Vec<usize> = Vec::new();

    for mut group in pre_cut_list {
        let mut sorted: Vec<usize> = Vec::new();
        let mut doc_titles: Vec<usize> = Vec::new();
        let mut xy_cut_members: Vec<usize> = Vec::new();

        // A pre-cut group of regions only runs the cross-layout pass when the
        // group is one primary-direction band; otherwise the group keeps its
        // cut order (the pass would also re-sort it).
        let group_is_regions = group
            .first()
            .is_some_and(|&i| is_region(&page.blocks[i].label));
        if group_is_regions {
            let bboxes: Vec<[i64; 4]> = group.iter().map(|&i| page.blocks[i].bbox).collect();
            let (bands, _) = discontinuous_projection(&bboxes, page.region.direction);
            if bands.len() == 1 {
                mark_cross_layout(page, &mut group, page.region.direction);
            }
        } else {
            mark_cross_layout(page, &mut group, page.region.direction);
        }

        for &block in &group {
            let role = page.blocks[block].order_label.as_deref().unwrap_or("");
            if !matches!(
                role,
                "cross_layout" | "cross_reference" | "doc_title" | "unordered"
            ) {
                xy_cut_members.push(block);
            } else if page.blocks[block].label == "doc_title" {
                doc_titles.push(block);
            } else {
                unsorted.push(block);
            }
        }

        if !xy_cut_members.is_empty() {
            let bboxes: Vec<[i64; 4]> = xy_cut_members
                .iter()
                .map(|&i| page.blocks[i].bbox)
                .collect();
            let max_lines = xy_cut_members
                .iter()
                .map(|&i| page.blocks[i].num_of_lines)
                .max()
                .unwrap_or(1);
            let (bands, _) = discontinuous_projection(&bboxes, page.region.direction);

            // `deepcopy(xy_cut_blocks)` + the vertical flip: cloned state, so
            // the arena's boxes stay untouched.
            let mut sortable: Vec<(usize, [i64; 4])> = xy_cut_members
                .iter()
                .map(|&i| (i, page.blocks[i].bbox))
                .collect();
            if page.region.direction == Dir::Vertical {
                for (_, bbox) in &mut sortable {
                    *bbox = [-bbox[0], bbox[1], -bbox[2], bbox[3]];
                }
            }
            let single_band = bands.len() == 1 || max_lines == 1;
            let (primary_quant, primary_axis, secondary_axis) = if single_band {
                (
                    (page.region.text_line_height as i64 / 2).max(1),
                    page.region.secondary_start_index(),
                    page.region.direction_start_index(),
                )
            } else {
                (
                    (page.region.text_line_width as i64 / 2).max(1),
                    page.region.direction_start_index(),
                    page.region.secondary_start_index(),
                )
            };
            sortable.sort_by_key(|(_, bbox)| {
                (
                    bbox[primary_axis].div_euclid(primary_quant),
                    bbox[secondary_axis],
                )
            });

            // `shrink_overlapping_boxes` on the clones.
            let mut shadow = OrderPage {
                blocks: sortable
                    .iter()
                    .map(|&(index, bbox)| {
                        let mut clone = page.blocks[index].clone();
                        clone.bbox = bbox;
                        clone
                    })
                    .collect(),
                region: page.region.clone(),
            };
            let shadow_order: Vec<usize> = (0..shadow.blocks.len()).collect();
            shrink_overlapping_boxes(&mut shadow, &shadow_order, page.region.direction.other());

            let shrunk: Vec<[i64; 4]> = shadow.blocks.iter().map(|b| b.bbox).collect();
            let cut_direction = if single_band {
                page.region.direction.other()
            } else {
                page.region.direction
            };
            let ordered = sort_by_xycut(&shrunk, cut_direction);
            sorted = ordered.into_iter().map(|slot| sortable[slot].0).collect();
        }

        match_unsorted(page, &mut sorted, doc_titles);
        // Regions never wait for the cross-cut match at the end: they are
        // matched (by euclidean distance) inside their own pre-cut.
        if unsorted
            .first()
            .is_some_and(|&i| is_region(&page.blocks[i].label))
        {
            let regions = std::mem::take(&mut unsorted);
            match_unsorted(page, &mut sorted, regions);
        }
        sorted_by_pre_cuts.extend(sorted);
    }

    let mut final_sorted = sorted_by_pre_cuts;
    match_unsorted(page, &mut final_sorted, unsorted);
    final_order.extend(final_sorted);
    final_order.extend(footer_blocks);
    final_order.extend(unordered_blocks);

    expand_children(page, &mut final_order);
    final_order
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    const FIXTURE: &str = include_str!("../tests/fixtures/classic-v1-layout-order/expected.json");

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

    fn read_i64_box(value: &Value) -> [i64; 4] {
        let b = items(value, "box");
        [
            b[0].as_i64().unwrap_or(0),
            b[1].as_i64().unwrap_or(0),
            b[2].as_i64().unwrap_or(0),
            b[3].as_i64().unwrap_or(0),
        ]
    }

    fn read_i64_boxes(value: &Value) -> Vec<[i64; 4]> {
        items(value, "boxes").iter().map(read_i64_box).collect()
    }

    fn read_usizes(value: &Value) -> Vec<usize> {
        items(value, "indices")
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect()
    }

    fn read_direction(value: &Value) -> Dir {
        if value.as_str() == Some("vertical") {
            Dir::Vertical
        } else {
            Dir::Horizontal
        }
    }

    fn build_page(case: &Value) -> OrderPage {
        let bbox = read_i64_box(&case["page_bbox"]);
        let blocks = items(&case["blocks"], "blocks")
            .iter()
            .map(|spec| {
                let spec = items(spec, "spec");
                let mut block =
                    OrderBlock::new(spec[0].as_str().unwrap_or(""), read_i64_box(&spec[1]));
                block.num_of_lines = spec[2].as_u64().unwrap_or(1) as u32;
                block.text_line_height = spec[3].as_f64().unwrap_or(1.0);
                block.text_line_width = spec[4].as_f64().unwrap_or(1.0);
                block
            })
            .collect();
        OrderPage::new(bbox, blocks)
    }

    /// Every captured page: region stats, final order, and order labels.
    #[test]
    fn the_captured_page_orders_are_reproduced() {
        let fixture = fixture();
        let pages = items(&fixture["pages"], "pages");
        assert_eq!(pages.len(), 14);
        for case in pages {
            let name = case["case"].as_str().unwrap_or("?");
            let mut page = build_page(case);

            let expected_direction = case["region_direction"].as_str().unwrap_or("");
            let actual_direction = match page.region.direction {
                Dir::Horizontal => "horizontal",
                Dir::Vertical => "vertical",
            };
            assert_eq!(actual_direction, expected_direction, "{name}: direction");
            assert!(
                (page.region.text_line_height
                    - case["region_text_line_height"].as_f64().unwrap_or(0.0))
                .abs()
                    < 1e-9,
                "{name}: line height"
            );

            let order = xycut_enhanced_order(&mut page);
            let expected = read_usizes(&case["order"]);
            assert_eq!(order, expected, "{name}: order");

            let labels = match case["order_labels"].as_object() {
                Some(labels) => labels,
                None => panic!("{name}: order_labels"),
            };
            for (index_text, expected_label) in labels {
                let index: usize = index_text.parse().unwrap_or(usize::MAX);
                assert_eq!(
                    page.blocks[index].order_label.as_deref(),
                    expected_label.as_str(),
                    "{name}: order_label of block {index}"
                );
            }
        }
    }

    const REGION_FIXTURE: &str =
        include_str!("../tests/fixtures/classic-v1-region-order/expected.json");

    fn build_spec_blocks(specs: &Value) -> Vec<OrderBlock> {
        items(specs, "blocks")
            .iter()
            .map(|spec| {
                let spec = items(spec, "spec");
                let mut block =
                    OrderBlock::new(spec[0].as_str().unwrap_or(""), read_i64_box(&spec[1]));
                block.num_of_lines = spec[2].as_u64().unwrap_or(1) as u32;
                block.text_line_height = spec[3].as_f64().unwrap_or(1.0);
                block.text_line_width = spec[4].as_f64().unwrap_or(1.0);
                block
            })
            .collect()
    }

    /// The nested `sort_layout_parsing_blocks` ordering: the page of regions
    /// first, then each region's own blocks, flattened.
    #[test]
    fn the_captured_region_orders_are_reproduced() {
        let fixture: Value = match serde_json::from_str(REGION_FIXTURE) {
            Ok(value) => value,
            Err(error) => panic!("region fixture: {error}"),
        };
        let cases = items(&fixture["cases"], "cases");
        assert_eq!(cases.len(), 7);
        for case in cases {
            let name = case["case"].as_str().unwrap_or("?");

            let mut inner_pages = Vec::new();
            for region in items(&case["regions"], "regions") {
                let bbox = read_i64_box(&region["bbox"]);
                let page = OrderPage::new(bbox, build_spec_blocks(&region["blocks"]));
                let direction = match page.region.direction {
                    Dir::Horizontal => "horizontal",
                    Dir::Vertical => "vertical",
                };
                assert_eq!(
                    direction,
                    region["direction"].as_str().unwrap_or(""),
                    "{name}: region direction"
                );
                assert!(
                    (page.region.text_line_height
                        - region["text_line_height"].as_f64().unwrap_or(0.0))
                    .abs()
                        < 1e-9,
                    "{name}: region line height"
                );
                assert!(
                    (page.region.text_line_width
                        - region["text_line_width"].as_f64().unwrap_or(0.0))
                    .abs()
                        < 1e-9,
                    "{name}: region line width"
                );
                let block = OrderBlock::from_region_page(&page);
                assert!(
                    (block.euclidean_distance
                        - region["euclidean_distance"].as_f64().unwrap_or(0.0))
                    .abs()
                        < 1e-9,
                    "{name}: region euclidean distance"
                );
                inner_pages.push((page, block));
            }

            let page_bbox = read_i64_box(&case["page_bbox"]);
            let region_blocks: Vec<OrderBlock> =
                inner_pages.iter().map(|(_, block)| block.clone()).collect();
            let mut outer = OrderPage::new(page_bbox, region_blocks);
            let page_direction = match outer.region.direction {
                Dir::Horizontal => "horizontal",
                Dir::Vertical => "vertical",
            };
            assert_eq!(
                page_direction,
                case["page_direction"].as_str().unwrap_or(""),
                "{name}: page direction"
            );

            let region_order = xycut_enhanced_order(&mut outer);
            assert_eq!(
                region_order,
                read_usizes(&case["region_order"]),
                "{name}: region order"
            );

            let mut flat: Vec<[usize; 2]> = Vec::new();
            for &region_index in &region_order {
                let mut inner = inner_pages[region_index].0.clone();
                for block_index in xycut_enhanced_order(&mut inner) {
                    flat.push([region_index, block_index]);
                }
            }
            let expected_flat: Vec<[usize; 2]> = items(&case["flat_order"], "flat_order")
                .iter()
                .map(|pair| {
                    let pair = items(pair, "pair");
                    [
                        pair[0].as_u64().unwrap_or(0) as usize,
                        pair[1].as_u64().unwrap_or(0) as usize,
                    ]
                })
                .collect();
            assert_eq!(flat, expected_flat, "{name}: flat order");
        }
    }

    #[test]
    fn the_captured_projections_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["discontinuous_projections"], "projections") {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes = read_i64_boxes(&case["boxes"]);
            let direction = read_direction(&case["direction"]);
            let (merged, counts) = discontinuous_projection(&boxes, direction);
            let expected: Vec<(i64, i64)> = items(&case["intervals"], "intervals")
                .iter()
                .map(|pair| {
                    let pair = items(pair, "pair");
                    (pair[0].as_i64().unwrap_or(0), pair[1].as_i64().unwrap_or(0))
                })
                .collect();
            assert_eq!(merged, expected, "{name}: intervals");
            assert_eq!(counts, read_usizes(&case["counts"]), "{name}: counts");
        }
    }

    #[test]
    fn the_captured_shrinks_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["shrinks"], "shrinks") {
            let name = case["case"].as_str().unwrap_or("?");
            let boxes = read_i64_boxes(&case["boxes"]);
            let mut page = OrderPage::new(
                [0, 0, 1000, 1000],
                boxes.iter().map(|&b| OrderBlock::new("text", b)).collect(),
            );
            let order: Vec<usize> = (0..boxes.len()).collect();
            let direction = read_direction(&case["direction"]);
            shrink_overlapping_boxes(&mut page, &order, direction);
            let expected = read_i64_boxes(&case["shrunk"]);
            for (block, want) in page.blocks.iter().zip(&expected) {
                assert_eq!(block.bbox, *want, "{name}");
            }
        }
    }

    /// `reference_insert`, including its stale-distance quirk: a sorted block
    /// that is not above the reference reuses the previous iteration's
    /// distance rather than computing one.
    #[test]
    fn the_captured_reference_inserts_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["reference_inserts"], "inserts") {
            let name = case["case"].as_str().unwrap_or("?");
            let sorted_boxes = read_i64_boxes(&case["sorted"]);
            let mut blocks: Vec<OrderBlock> = sorted_boxes
                .iter()
                .map(|&b| OrderBlock::new("text", b))
                .collect();
            blocks.push(OrderBlock::new("reference", read_i64_box(&case["block"])));
            let page = OrderPage::new([0, 0, 1000, 1000], blocks);
            let mut order: Vec<usize> = (0..sorted_boxes.len()).collect();
            insert::reference(&page, sorted_boxes.len(), &mut order);
            assert_eq!(order, read_usizes(&case["result"]), "{name}");
        }
    }

    #[test]
    fn the_captured_minima_and_overlaps_are_reproduced() {
        let fixture = fixture();
        for case in items(&fixture["local_minima"], "minima") {
            let name = case["case"].as_str().unwrap_or("?");
            let values: Vec<u32> = items(&case["values"], "values")
                .iter()
                .map(|v| v.as_u64().unwrap_or(0) as u32)
                .collect();
            let actual = local_minima_flat_regions(&values);
            if case["regions"].is_null() {
                assert!(actual.is_none(), "{name}: expected None");
            } else {
                let expected: Vec<(usize, usize)> = items(&case["regions"], "regions")
                    .iter()
                    .map(|pair| {
                        let pair = items(pair, "pair");
                        (
                            pair[0].as_u64().unwrap_or(0) as usize,
                            pair[1].as_u64().unwrap_or(0) as usize,
                        )
                    })
                    .collect();
                assert_eq!(actual, Some(expected), "{name}");
            }
        }
        for case in items(&fixture["overlap_ratios"], "overlaps") {
            let name = case["case"].as_str().unwrap_or("?");
            let read = |key: &str| -> HeuristicBox {
                let b = items(&case[key], "box");
                [
                    b[0].as_f64().unwrap_or(0.0),
                    b[1].as_f64().unwrap_or(0.0),
                    b[2].as_f64().unwrap_or(0.0),
                    b[3].as_f64().unwrap_or(0.0),
                ]
            };
            let actual = overlap_ratio(
                read("first"),
                read("second"),
                case["mode"].as_str() == Some("small"),
            );
            let expected = case["ratio"].as_f64().unwrap_or(f64::NAN);
            assert!(
                (actual - expected).abs() < 1e-12,
                "{name}: {actual} vs {expected}"
            );
        }
    }
}
