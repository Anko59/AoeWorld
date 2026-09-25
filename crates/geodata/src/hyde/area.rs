use super::PreparedHistoricalLandUse;
use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS};
use aoe_map::{FieldPyramid, PyramidLevel, ordered_land_use_page_root};
use geometry::{Polygon, equal_area_transform, intersection_area, point_order, project_polygon};
use std::cmp::Ordering;

mod geometry;

/// A HYDE cell's explicit coverage state. `OutsideCoverage` is also assigned
/// to target area left uncovered by the supplied source-cell window.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum HydeAreaState {
    Land,
    Lake,
    Ocean,
    NoData,
    OutsideCoverage,
}

/// A WGS84 polygon vertex, in longitude/latitude degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HydeGeographicPoint {
    pub longitude_degrees: f64,
    pub latitude_degrees: f64,
}

/// One source cell and its extensive HYDE quantities. Land cells require all
/// three values. Crop and grazing areas are square kilometers; population is a
/// count. Crop and grazing cannot exceed the projected source-cell land area,
/// separately or together. Other coverage states must not carry land
/// quantities.
#[derive(Clone, Debug, PartialEq)]
pub struct HydeSourceAreaCell {
    pub polygon: Vec<HydeGeographicPoint>,
    pub state: HydeAreaState,
    pub crop_area_square_kilometers: Option<f64>,
    pub grazing_area_square_kilometers: Option<f64>,
    pub population: Option<f64>,
}

/// One target cell polygon in WGS84. Call allocation with a complete bounded
/// source window for these target cells; uncovered target area becomes outside.
#[derive(Clone, Debug, PartialEq)]
pub struct HydeTargetAreaCell {
    pub polygon: Vec<HydeGeographicPoint>,
}

/// Unrounded area and quantity totals for one target cell. These values are
/// additive and must be reduced before conversion to page percentages/density.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HydeAreaAllocation {
    pub land_area_square_meters: f64,
    pub lake_area_square_meters: f64,
    pub ocean_area_square_meters: f64,
    pub nodata_area_square_meters: f64,
    pub outside_area_square_meters: f64,
    pub crop_area_square_kilometers: f64,
    pub grazing_area_square_kilometers: f64,
    pub population: f64,
}

const MAX_HYDE_AREA_SOURCE_CELLS: usize = 1_000_000;
const MAX_HYDE_AREA_TARGET_CELLS: usize = 64 * 64;
const MAX_HYDE_AREA_VERTICES: usize = 4_194_304;
const MAX_HYDE_AREA_CANDIDATES: u64 = 64_000_000;

#[derive(Clone, Debug)]
struct SourceCell {
    polygon: Polygon,
    state: HydeAreaState,
    crop_km2: f64,
    grazing_km2: f64,
    population: f64,
}

/// Allocates a bounded source window onto target cells in a request-centered
/// Lambert azimuthal equal-area plane. Every extensive quantity uses the full
/// source-cell area as its denominator; no quantity is redistributed into a
/// partial selection. Source and target polygons are simple, with at most 16
/// input vertices; deterministic 0.1-degree densification makes curved shared
/// boundaries conserve area before triangulated overlap. Polygons that touch
/// or cross a pole or the antimeridian are rejected because this bounded API
/// does not implement wrap-safe spherical geometry. A call accepts at most
/// 1,000,000 source cells and one 64×64 target page; callers should process
/// target pages independently.
pub fn allocate_hyde_area_window(
    source_cells: &[HydeSourceAreaCell],
    target_cells: &[HydeTargetAreaCell],
    center_latitude_e7: i32,
    center_longitude_e7: i32,
) -> Result<Vec<HydeAreaAllocation>, GeodataError> {
    validate_limits(source_cells.len(), target_cells.len())?;
    validate_center(center_latitude_e7, center_longitude_e7)?;
    let total_vertices = source_cells
        .iter()
        .map(|cell| cell.polygon.len())
        .chain(target_cells.iter().map(|cell| cell.polygon.len()))
        .sum::<usize>();
    if total_vertices > MAX_HYDE_AREA_VERTICES {
        return Err(GeodataError::Preparation(
            "HYDE allocation vertex limit exceeded",
        ));
    }

    let transform = equal_area_transform(center_latitude_e7, center_longitude_e7)?;
    let mut source_polygons = Vec::with_capacity(source_cells.len());
    let mut target_polygons = Vec::with_capacity(target_cells.len());
    let mut densified_vertices = 0_usize;
    for source in source_cells {
        let polygon = project_polygon(&source.polygon, &transform)?;
        densified_vertices = densified_vertices.saturating_add(polygon.points.len());
        if densified_vertices > MAX_HYDE_AREA_VERTICES {
            return Err(GeodataError::Preparation(
                "HYDE allocation densified vertex limit exceeded",
            ));
        }
        let (crop_km2, grazing_km2, population) = source_quantities(source, polygon.area)?;
        source_polygons.push(SourceCell {
            polygon,
            state: source.state,
            crop_km2,
            grazing_km2,
            population,
        });
    }
    for target in target_cells {
        let polygon = project_polygon(&target.polygon, &transform)?;
        densified_vertices = densified_vertices.saturating_add(polygon.points.len());
        if densified_vertices > MAX_HYDE_AREA_VERTICES {
            return Err(GeodataError::Preparation(
                "HYDE allocation densified vertex limit exceeded",
            ));
        }
        target_polygons.push(polygon);
    }

    source_polygons.sort_by(source_order);
    let mut target_order = (0..target_polygons.len()).collect::<Vec<_>>();
    target_order.sort_by(|&left, &right| {
        let a = target_polygons[left].bounds;
        let b = target_polygons[right].bounds;
        a.min_x
            .total_cmp(&b.min_x)
            .then_with(|| a.min_y.total_cmp(&b.min_y))
            .then_with(|| a.max_x.total_cmp(&b.max_x))
            .then_with(|| a.max_y.total_cmp(&b.max_y))
            .then_with(|| polygon_order(&target_polygons[left], &target_polygons[right]))
    });

    let mut allocations = vec![HydeAreaAllocation::default(); target_polygons.len()];
    let mut target_cursor = 0_usize;
    let mut active_targets = Vec::<usize>::new();
    let mut candidates = 0_u64;
    for source in &source_polygons {
        let source_bounds = source.polygon.bounds;
        while target_cursor < target_order.len()
            && target_polygons[target_order[target_cursor]].bounds.min_x <= source_bounds.max_x
        {
            active_targets.push(target_order[target_cursor]);
            target_cursor += 1;
        }
        active_targets.retain(|&index| target_polygons[index].bounds.max_x >= source_bounds.min_x);
        for &target_index in &active_targets {
            let target = &target_polygons[target_index];
            if target.bounds.max_y < source_bounds.min_y
                || target.bounds.min_y > source_bounds.max_y
            {
                continue;
            }
            candidates += 1;
            if candidates > MAX_HYDE_AREA_CANDIDATES {
                return Err(GeodataError::Preparation(
                    "HYDE allocation overlap limit exceeded",
                ));
            }
            let overlap = intersection_area(&source.polygon, target);
            if overlap <= 0.0 {
                continue;
            }
            add_overlap(&mut allocations[target_index], source, overlap);
        }
    }

    for (allocation, target) in allocations.iter_mut().zip(&target_polygons) {
        let covered = allocation.covered_area_square_meters();
        let tolerance = target.area.max(1.0) * 1.0e-9;
        if covered > target.area + tolerance {
            return Err(GeodataError::Preparation(
                "HYDE source cells overlap within a target cell",
            ));
        }
        allocation.outside_area_square_meters += (target.area - covered).max(0.0);
    }
    if allocations.iter().any(|allocation| !allocation.is_valid()) {
        return Err(GeodataError::Preparation(
            "HYDE allocation quantity overflowed",
        ));
    }
    Ok(allocations)
}

/// Builds historical pages from unrounded allocations. All pyramid levels are
/// sums of extensive quantities and valid-land areas; rounding occurs only as
/// each final page is written.
pub fn prepare_hyde_area_pyramid(
    samples_per_axis: u16,
    allocations: Vec<HydeAreaAllocation>,
) -> Result<PreparedHistoricalLandUse, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS.min(1024)).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "area-aware HYDE grid is outside direct bounds",
        ));
    }
    if allocations.len() != usize::from(samples_per_axis).pow(2) {
        return Err(GeodataError::Preparation(
            "historical allocation grid shape is invalid",
        ));
    }
    if allocations.iter().any(|cell| !cell.is_valid()) {
        return Err(GeodataError::Preparation(
            "historical allocation contains an invalid quantity",
        ));
    }

    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut axis = samples_per_axis;
    let mut values = allocations;
    loop {
        let rounded = values
            .iter()
            .map(|value| value.to_land_use())
            .collect::<Vec<_>>();
        let level_pages = super::pages_for(levels.len() as u8, axis, &rounded)?;
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_land_use_page_root(&level_pages)?,
        });
        pages.extend(level_pages);
        if axis == 1 {
            break;
        }
        values = reduce_area_grid(axis, &values)?;
        axis = axis.div_ceil(2);
    }
    Ok(PreparedHistoricalLandUse {
        field: FieldPyramid { levels },
        pages,
    })
}

impl HydeAreaAllocation {
    fn covered_area_square_meters(&self) -> f64 {
        self.land_area_square_meters
            + self.lake_area_square_meters
            + self.ocean_area_square_meters
            + self.nodata_area_square_meters
            + self.outside_area_square_meters
    }

    fn add_state_area(&mut self, state: HydeAreaState, area: f64) {
        match state {
            HydeAreaState::Land => self.land_area_square_meters += area,
            HydeAreaState::Lake => self.lake_area_square_meters += area,
            HydeAreaState::Ocean => self.ocean_area_square_meters += area,
            HydeAreaState::NoData => self.nodata_area_square_meters += area,
            HydeAreaState::OutsideCoverage => self.outside_area_square_meters += area,
        }
    }

    fn to_land_use(self) -> super::LandUseValue {
        let land_km2 = self.land_area_square_meters / 1_000_000.0;
        if land_km2 <= 0.0 {
            return super::LandUseValue {
                crop_percent: 0,
                grazing_percent: 0,
                population_pressure_per_square_kilometer: 0,
            };
        }
        let crop_percent = rounded_percent(self.crop_area_square_kilometers / land_km2);
        let grazing_percent =
            rounded_percent(self.grazing_area_square_kilometers / land_km2).min(100 - crop_percent);
        super::LandUseValue {
            crop_percent,
            grazing_percent,
            population_pressure_per_square_kilometer: (self.population / land_km2)
                .round()
                .clamp(0.0, f64::from(u16::MAX))
                as u16,
        }
    }

    pub(super) fn is_valid(&self) -> bool {
        [
            self.land_area_square_meters,
            self.lake_area_square_meters,
            self.ocean_area_square_meters,
            self.nodata_area_square_meters,
            self.outside_area_square_meters,
            self.crop_area_square_kilometers,
            self.grazing_area_square_kilometers,
            self.population,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0)
    }
}

fn rounded_percent(fraction: f64) -> u8 {
    (fraction * 100.0).round().clamp(0.0, 100.0) as u8
}

fn reduce_area_grid(
    axis: u16,
    values: &[HydeAreaAllocation],
) -> Result<Vec<HydeAreaAllocation>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation(
            "historical allocation grid shape is invalid",
        ));
    }
    let next_axis = axis.div_ceil(2);
    let mut reduced = vec![HydeAreaAllocation::default(); usize::from(next_axis).pow(2)];
    for y in 0..next_axis {
        for x in 0..next_axis {
            let destination = usize::from(y) * usize::from(next_axis) + usize::from(x);
            for source_y in y * 2..((y + 1) * 2).min(axis) {
                for source_x in x * 2..((x + 1) * 2).min(axis) {
                    let source =
                        values[usize::from(source_y) * usize::from(axis) + usize::from(source_x)];
                    add_allocation(&mut reduced[destination], source);
                }
            }
        }
    }
    if reduced.iter().any(|cell| !cell.is_valid()) {
        return Err(GeodataError::Preparation(
            "historical allocation reduction overflowed",
        ));
    }
    Ok(reduced)
}

fn add_allocation(target: &mut HydeAreaAllocation, source: HydeAreaAllocation) {
    target.land_area_square_meters += source.land_area_square_meters;
    target.lake_area_square_meters += source.lake_area_square_meters;
    target.ocean_area_square_meters += source.ocean_area_square_meters;
    target.nodata_area_square_meters += source.nodata_area_square_meters;
    target.outside_area_square_meters += source.outside_area_square_meters;
    target.crop_area_square_kilometers += source.crop_area_square_kilometers;
    target.grazing_area_square_kilometers += source.grazing_area_square_kilometers;
    target.population += source.population;
}

fn add_overlap(target: &mut HydeAreaAllocation, source: &SourceCell, overlap_m2: f64) {
    target.add_state_area(source.state, overlap_m2);
    if source.state == HydeAreaState::Land {
        let share = overlap_m2 / source.polygon.area;
        target.crop_area_square_kilometers += source.crop_km2 * share;
        target.grazing_area_square_kilometers += source.grazing_km2 * share;
        target.population += source.population * share;
    }
}

fn source_quantities(
    source: &HydeSourceAreaCell,
    land_area_square_meters: f64,
) -> Result<(f64, f64, f64), GeodataError> {
    if source.state != HydeAreaState::Land {
        if source.crop_area_square_kilometers.is_some()
            || source.grazing_area_square_kilometers.is_some()
            || source.population.is_some()
        {
            return Err(GeodataError::Preparation(
                "non-land HYDE cell carries land quantities",
            ));
        }
        return Ok((0.0, 0.0, 0.0));
    }
    let (Some(crop), Some(grazing), Some(population)) = (
        source.crop_area_square_kilometers,
        source.grazing_area_square_kilometers,
        source.population,
    ) else {
        return Err(GeodataError::Preparation(
            "HYDE land cell lacks a required value",
        ));
    };
    if [crop, grazing, population]
        .into_iter()
        .any(|value| !value.is_finite() || value < 0.0)
    {
        return Err(GeodataError::Preparation(
            "HYDE land cell has an invalid quantity",
        ));
    }
    let land_area_square_kilometers = land_area_square_meters / 1_000_000.0;
    if crop > land_area_square_kilometers || grazing > land_area_square_kilometers {
        return Err(GeodataError::Preparation(
            "HYDE land quantity exceeds source-cell land area",
        ));
    }
    if crop + grazing > land_area_square_kilometers {
        return Err(GeodataError::Preparation(
            "HYDE crop and grazing quantities exceed source-cell capacity",
        ));
    }
    Ok((crop, grazing, population))
}

fn validate_limits(sources: usize, targets: usize) -> Result<(), GeodataError> {
    if sources > MAX_HYDE_AREA_SOURCE_CELLS || targets > MAX_HYDE_AREA_TARGET_CELLS {
        return Err(GeodataError::Preparation(
            "HYDE allocation window exceeds its cell limit",
        ));
    }
    Ok(())
}

fn validate_center(latitude_e7: i32, longitude_e7: i32) -> Result<(), GeodataError> {
    if !(-900_000_000..=900_000_000).contains(&latitude_e7)
        || !(-1_800_000_000..=1_800_000_000).contains(&longitude_e7)
    {
        return Err(GeodataError::Preparation(
            "HYDE allocation center is outside WGS84",
        ));
    }
    Ok(())
}

fn source_order(left: &SourceCell, right: &SourceCell) -> Ordering {
    polygon_order(&left.polygon, &right.polygon)
        .then_with(|| left.state.cmp(&right.state))
        .then_with(|| left.crop_km2.total_cmp(&right.crop_km2))
        .then_with(|| left.grazing_km2.total_cmp(&right.grazing_km2))
        .then_with(|| left.population.total_cmp(&right.population))
}

fn polygon_order(left: &Polygon, right: &Polygon) -> Ordering {
    left.points
        .iter()
        .zip(&right.points)
        .map(|(a, b)| point_order(a, b))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or_else(|| left.points.len().cmp(&right.points.len()))
}

#[cfg(test)]
#[path = "../tests/hyde_area.rs"]
mod tests;
