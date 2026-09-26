//! Page-at-a-time reduction of unrounded historical quantities.
use super::{PreparedHistoricalLandUse, area::add_allocation};
use crate::GeodataError;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HistoricalLandUsePage, PyramidLevel,
    ordered_land_use_page_root,
};
use std::collections::{BTreeMap, BTreeSet};

use super::HydeAreaAllocation;

const PAGE: u16 = ENVIRONMENT_PAGE_SAMPLES as u16;

struct Pending {
    values: Vec<HydeAreaAllocation>,
    received: u8,
}

struct Level {
    axis: u16,
    seen: BTreeSet<(u16, u16)>,
    pending: BTreeMap<(u16, u16), Pending>,
}

/// Accepts allocated target pages in any order, writes each historical page,
/// and reduces extensive quantities before rounding the next level. At 1024
/// samples per axis, even an adversarial order retains at most the incomplete
/// parent pages, rather than a million-cell allocation grid.
pub struct HistoricalPageStream {
    levels: Vec<Level>,
    pages: Vec<HistoricalLandUsePage>,
}

impl HistoricalPageStream {
    pub fn new(axis: u16) -> Result<Self, GeodataError> {
        if !(2..=super::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&axis) {
            return Err(GeodataError::Preparation(
                "historical stream grid is outside bounds",
            ));
        }
        let mut levels = Vec::new();
        let mut level_axis = axis;
        loop {
            levels.push(Level {
                axis: level_axis,
                seen: BTreeSet::new(),
                pending: BTreeMap::new(),
            });
            if level_axis == 1 {
                break;
            }
            level_axis = level_axis.div_ceil(2);
        }
        Ok(Self {
            levels,
            pages: Vec::new(),
        })
    }

    /// `x` and `y` are page coordinates, not sample coordinates. Edge-page
    /// dimensions are derived from the declared field axis.
    pub fn push(
        &mut self,
        x: u16,
        y: u16,
        values: Vec<HydeAreaAllocation>,
    ) -> Result<(), GeodataError> {
        self.ingest(0, x, y, values)
    }

    pub fn finish(mut self) -> Result<PreparedHistoricalLandUse, GeodataError> {
        for level in &self.levels {
            let pages_per_axis = level.axis.div_ceil(PAGE);
            if level.seen.len() != usize::from(pages_per_axis).pow(2) || !level.pending.is_empty() {
                return Err(GeodataError::Preparation(
                    "historical stream is missing a page",
                ));
            }
        }
        self.pages.sort_by_key(|page| (page.level, page.y, page.x));
        let mut fields = Vec::with_capacity(self.levels.len());
        for (level_index, level) in self.levels.iter().enumerate() {
            let pages = self
                .pages
                .iter()
                .filter(|page| usize::from(page.level) == level_index)
                .cloned()
                .collect::<Vec<_>>();
            fields.push(PyramidLevel {
                samples_per_axis: level.axis,
                ordered_page_root: ordered_land_use_page_root(&pages)?,
            });
        }
        Ok(PreparedHistoricalLandUse {
            field: FieldPyramid { levels: fields },
            pages: self.pages,
        })
    }

    fn ingest(
        &mut self,
        level_index: usize,
        x: u16,
        y: u16,
        values: Vec<HydeAreaAllocation>,
    ) -> Result<(), GeodataError> {
        let level = &mut self.levels[level_index];
        let (width, height) = page_shape(level.axis, x, y)?;
        if values.len() != usize::from(width) * usize::from(height)
            || values.iter().any(|cell| !cell.is_valid())
            || !level.seen.insert((x, y))
        {
            return Err(GeodataError::Preparation(
                "historical stream page is invalid or duplicated",
            ));
        }
        let rounded = values
            .iter()
            .map(|cell| cell.to_land_use())
            .collect::<Vec<_>>();
        self.pages.push(HistoricalLandUsePage {
            level: level_index as u8,
            x,
            y,
            width: width as u8,
            height: height as u8,
            crop_percent: rounded.iter().map(|value| value.crop_percent).collect(),
            grazing_percent: rounded.iter().map(|value| value.grazing_percent).collect(),
            population_pressure_per_square_kilometer: rounded
                .iter()
                .map(|value| value.population_pressure_per_square_kilometer)
                .collect(),
            coverage: values.iter().map(|value| value.to_coverage()).collect(),
        });
        if level_index + 1 == self.levels.len() {
            return Ok(());
        }
        let reduced = reduce_page(width, height, &values)?;
        let parent_x = x / 2;
        let parent_y = y / 2;
        let parent_axis = self.levels[level_index + 1].axis;
        let (parent_width, parent_height) = page_shape(parent_axis, parent_x, parent_y)?;
        let (reduced_width, reduced_height) = (width.div_ceil(2), height.div_ceil(2));
        let child_pages = self.levels[level_index].axis.div_ceil(PAGE);
        let parent = self.levels[level_index + 1]
            .pending
            .entry((parent_x, parent_y))
            .or_insert_with(|| Pending {
                values: vec![
                    HydeAreaAllocation::default();
                    usize::from(parent_width) * usize::from(parent_height)
                ],
                received: 0,
            });
        let offset_x = (x % 2) * (PAGE / 2);
        let offset_y = (y % 2) * (PAGE / 2);
        for row in 0..reduced_height {
            for column in 0..reduced_width {
                let destination = usize::from(offset_y + row) * usize::from(parent_width)
                    + usize::from(offset_x + column);
                parent.values[destination] =
                    reduced[usize::from(row) * usize::from(reduced_width) + usize::from(column)];
            }
        }
        parent.received += 1;
        let expected_x = if parent_x * 2 + 1 < child_pages { 2 } else { 1 };
        let expected_y = if parent_y * 2 + 1 < child_pages { 2 } else { 1 };
        if parent.received == expected_x * expected_y {
            let complete = self.levels[level_index + 1]
                .pending
                .remove(&(parent_x, parent_y))
                .ok_or(GeodataError::Preparation(
                    "historical stream parent page is missing",
                ))?;
            self.ingest(level_index + 1, parent_x, parent_y, complete.values)?;
        }
        Ok(())
    }
}

fn page_shape(axis: u16, x: u16, y: u16) -> Result<(u16, u16), GeodataError> {
    let left = x.saturating_mul(PAGE);
    let top = y.saturating_mul(PAGE);
    if left >= axis || top >= axis {
        return Err(GeodataError::Preparation(
            "historical stream page is outside its grid",
        ));
    }
    Ok(((axis - left).min(PAGE), (axis - top).min(PAGE)))
}

fn reduce_page(
    width: u16,
    height: u16,
    values: &[HydeAreaAllocation],
) -> Result<Vec<HydeAreaAllocation>, GeodataError> {
    let reduced_width = width.div_ceil(2);
    let reduced_height = height.div_ceil(2);
    let mut reduced = vec![
        HydeAreaAllocation::default();
        usize::from(reduced_width) * usize::from(reduced_height)
    ];
    for row in 0..height {
        for column in 0..width {
            let destination =
                usize::from(row / 2) * usize::from(reduced_width) + usize::from(column / 2);
            add_allocation(
                &mut reduced[destination],
                values[usize::from(row) * usize::from(width) + usize::from(column)],
            );
        }
    }
    if reduced.iter().any(|cell| !cell.is_valid()) {
        return Err(GeodataError::Preparation(
            "historical stream reduction overflowed",
        ));
    }
    Ok(reduced)
}

#[cfg(test)]
#[path = "../tests/hyde_area_stream.rs"]
mod tests;
