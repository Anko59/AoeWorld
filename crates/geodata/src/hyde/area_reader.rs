pub(super) use self::values::validate_polar_footprint;
#[cfg(test)]
pub(super) use self::values::validate_target_geography;
use super::HydeAreaAllocation;
use super::{
    HYDE_600_MEMBERS, HYDE_AREA_PREPROCESSING_IDENTITY, HydeAreaState, HydeGeographicPoint,
    HydeSourceAreaCell, HydeTargetAreaCell, PreparedHistoricalLandUse, allocate_hyde_area_window,
    area_stream::HistoricalPageStream,
    geographic_correction::GeographicHistoricalCorrectionDocument,
};
use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, local_aeqd_definition};
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, MapRequest};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use std::path::Path;

const PAGE_SAMPLES: u16 = ENVIRONMENT_PAGE_SAMPLES as u16;
// Keep each allocator call comfortably below its expanded-polygon vertex
// budget. The streamed field still publishes fixed 64×64 pages.
const MAX_ALLOCATION_SOURCE_CELLS: usize = 100_000;

#[derive(Clone, Copy)]
pub(super) struct PageBounds {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
}

#[path = "area_reader/source.rs"]
mod source;
#[cfg(test)]
mod tests;
#[path = "area_reader/values.rs"]
mod values;
use self::source::{ArchiveReader, RasterSource};
use values::classify_land_lake_value;

/// Reads bounded archive windows and creates the production area-allocated
/// historical field. Missing land quantities fail instead of becoming zero.
pub fn prepare_hyde_area_600(
    baseline_archive: &Path,
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedHistoricalLandUse, GeodataError> {
    if !(2..=super::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "area-aware HYDE grid is outside historical bounds",
        ));
    }
    let corrections = GeographicHistoricalCorrectionDocument::empty(
        request,
        samples_per_axis,
        HYDE_AREA_PREPROCESSING_IDENTITY,
    )?;
    prepare_hyde_area_600_with_corrections(
        baseline_archive,
        supplementary_archive,
        request,
        samples_per_axis,
        &corrections,
    )
}

pub fn prepare_hyde_area_600_with_corrections(
    baseline_archive: &Path,
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
    corrections: &GeographicHistoricalCorrectionDocument,
) -> Result<PreparedHistoricalLandUse, GeodataError> {
    // Ordinary overview requests still use 128; this independent historical
    // path can stream up to 1024 without a full-grid allocation buffer.
    if !(2..=super::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "area-aware HYDE grid is outside historical bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    corrections.validate_for(request, samples_per_axis, HYDE_AREA_PREPROCESSING_IDENTITY)?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    validate_polar_footprint(request, estimate.effective_side_meters)?;
    let reader = ArchiveReader::open(baseline_archive, supplementary_archive)?;
    let page_transform = target_to_wgs84(request)?;
    let mut stream = HistoricalPageStream::new(samples_per_axis)?;
    let mut page_y = 0;
    while page_y < samples_per_axis {
        let height = (samples_per_axis - page_y).min(PAGE_SAMPLES);
        let mut page_x = 0;
        while page_x < samples_per_axis {
            let width = (samples_per_axis - page_x).min(PAGE_SAMPLES);
            let mut page_allocations = allocate_historical_page(
                &reader,
                &page_transform,
                request,
                estimate.effective_side_meters,
                samples_per_axis,
                PageBounds {
                    x: page_x,
                    y: page_y,
                    width,
                    height,
                },
            )?;
            corrections.apply_page_validated(
                page_x,
                page_y,
                width,
                height,
                &mut page_allocations,
            )?;
            stream.push(
                page_x / PAGE_SAMPLES,
                page_y / PAGE_SAMPLES,
                page_allocations,
            )?;
            page_x += width;
        }
        page_y += height;
    }
    stream.finish()
}

pub(super) fn prepare_hyde_lake_coverage(
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<Vec<u8>, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "HYDE lake grid is outside overview bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    validate_polar_footprint(request, estimate.effective_side_meters)?;
    let land_lake = RasterSource::open(supplementary_archive, HYDE_600_MEMBERS[3])?;
    let transform = target_to_wgs84(request)?;
    let mut values = vec![0_u8; usize::from(samples_per_axis).pow(2)];
    let mut page_y = 0;
    while page_y < samples_per_axis {
        let height = (samples_per_axis - page_y).min(PAGE_SAMPLES);
        let mut page_x = 0;
        while page_x < samples_per_axis {
            let width = (samples_per_axis - page_x).min(PAGE_SAMPLES);
            let bounds = PageBounds {
                x: page_x,
                y: page_y,
                width,
                height,
            };
            let allocations = allocate_lake_page(
                &land_lake,
                &transform,
                request,
                estimate.effective_side_meters,
                samples_per_axis,
                bounds,
            )?;
            for (local_index, allocation) in allocations.iter().enumerate() {
                let total_area = allocation.covered_area_square_meters();
                if allocation.outside_area_square_meters > total_area.max(1.0) * 1.0e-9
                    || total_area <= 0.0
                {
                    return Err(GeodataError::Preparation(
                        "HYDE lake grid does not fully cover a target cell",
                    ));
                }
                let row = local_index / usize::from(width);
                let column = local_index % usize::from(width);
                let index = usize::from(page_y + row as u16) * usize::from(samples_per_axis)
                    + usize::from(page_x + column as u16);
                values[index] = (allocation.lake_area_square_meters / total_area * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8;
            }
            page_x += width;
        }
        page_y += height;
    }
    Ok(values)
}

pub(super) fn target_to_wgs84(request: MapRequest) -> Result<CoordTransform, GeodataError> {
    let mut projected = SpatialRef::from_definition(&local_aeqd_definition(
        request.center_latitude_e7,
        request.center_longitude_e7,
    ))
    .map_err(|_| GeodataError::Projection)?;
    let mut geographic = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    projected.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    geographic.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    CoordTransform::new(&projected, &geographic).map_err(|_| GeodataError::Projection)
}

pub(super) fn target_page(
    transform: &CoordTransform,
    side_meters: u64,
    axis: u16,
    page: PageBounds,
    center_longitude_degrees: f64,
) -> Result<(Vec<HydeTargetAreaCell>, Vec<usize>), GeodataError> {
    let side = side_meters as f64;
    let spacing = side / f64::from(axis);
    let mut east = Vec::with_capacity(usize::from(page.width) * usize::from(page.height) * 4);
    let mut north = Vec::with_capacity(east.capacity());
    let mut indices = Vec::with_capacity(usize::from(page.width) * usize::from(page.height));
    for row in page.y..page.y + page.height {
        for column in page.x..page.x + page.width {
            let west = -side / 2.0 + f64::from(column) * spacing;
            let east_edge = west + spacing;
            let north_edge = side / 2.0 - f64::from(row) * spacing;
            let south = north_edge - spacing;
            east.extend([west, east_edge, east_edge, west]);
            north.extend([north_edge, north_edge, south, south]);
            indices.push(usize::from(row) * usize::from(axis) + usize::from(column));
        }
    }
    transform
        .transform_coords(&mut east, &mut north, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    for longitude in &mut east {
        while *longitude - center_longitude_degrees > 180.0 {
            *longitude -= 360.0;
        }
        while *longitude - center_longitude_degrees < -180.0 {
            *longitude += 360.0;
        }
    }
    let targets = east
        .chunks_exact(4)
        .zip(north.chunks_exact(4))
        .map(|(longitudes, latitudes)| HydeTargetAreaCell {
            polygon: longitudes
                .iter()
                .zip(latitudes)
                .map(
                    |(&longitude_degrees, &latitude_degrees)| HydeGeographicPoint {
                        longitude_degrees,
                        latitude_degrees,
                    },
                )
                .collect(),
        })
        .collect();
    Ok((targets, indices))
}

fn allocate_historical_page(
    reader: &ArchiveReader,
    transform: &CoordTransform,
    request: MapRequest,
    side_meters: u64,
    axis: u16,
    bounds: PageBounds,
) -> Result<Vec<HydeAreaAllocation>, GeodataError> {
    let center_longitude = f64::from(request.center_longitude_e7) / 10_000_000.0;
    let (targets, _) = target_page(transform, side_meters, axis, bounds, center_longitude)?;
    if reader.source_cell_count(&targets)? > MAX_ALLOCATION_SOURCE_CELLS
        && let Some(children) = split_page_bounds(bounds)
    {
        let mut allocations = vec![HydeAreaAllocation::default(); page_cell_count(bounds)];
        for child in children {
            let child_allocations =
                allocate_historical_page(reader, transform, request, side_meters, axis, child)?;
            place_child_values(bounds, child, &child_allocations, &mut allocations);
        }
        return Ok(allocations);
    }

    let sources = reader.source_cells(&targets)?;
    let allocations = allocate_hyde_area_window(
        &sources,
        &targets,
        request.center_latitude_e7,
        request.center_longitude_e7,
    )?;
    for allocation in &allocations {
        if allocation.outside_area_square_meters
            > allocation.covered_area_square_meters().max(1.0) * 1.0e-9
        {
            return Err(GeodataError::Preparation(
                "HYDE source coverage does not contain the requested target page",
            ));
        }
    }
    Ok(allocations)
}

fn allocate_lake_page(
    land_lake: &RasterSource,
    transform: &CoordTransform,
    request: MapRequest,
    side_meters: u64,
    axis: u16,
    bounds: PageBounds,
) -> Result<Vec<HydeAreaAllocation>, GeodataError> {
    let center_longitude = f64::from(request.center_longitude_e7) / 10_000_000.0;
    let (targets, _) = target_page(transform, side_meters, axis, bounds, center_longitude)?;
    if land_lake.source_cell_count(&targets)? > MAX_ALLOCATION_SOURCE_CELLS
        && let Some(children) = split_page_bounds(bounds)
    {
        let mut allocations = vec![HydeAreaAllocation::default(); page_cell_count(bounds)];
        for child in children {
            let child_allocations =
                allocate_lake_page(land_lake, transform, request, side_meters, axis, child)?;
            place_child_values(bounds, child, &child_allocations, &mut allocations);
        }
        return Ok(allocations);
    }

    let windows = land_lake.windows_for_targets(&targets)?;
    if windows.is_empty() {
        return Err(GeodataError::Preparation(
            "HYDE lake grid does not cover the requested target page",
        ));
    }
    let mut sources = Vec::new();
    for source_window in windows {
        let window = source_window.pixels;
        let mask = land_lake.values(window)?;
        sources.reserve(mask.len());
        for row in 0..window.height {
            for column in 0..window.width {
                let state = classify_land_lake_value(mask[row * window.width + column])?;
                let (crop, grazing, population) = if state == HydeAreaState::Land {
                    // Lake coverage uses only the mask geometry, but the
                    // shared allocator requires explicit land quantities.
                    (Some(0.0), Some(0.0), Some(0.0))
                } else {
                    (None, None, None)
                };
                sources.push(HydeSourceAreaCell {
                    polygon: land_lake.cell_polygon(
                        window.left + column,
                        window.top + row,
                        source_window.longitude_offset_degrees,
                    ),
                    state,
                    crop_area_square_kilometers: crop,
                    grazing_area_square_kilometers: grazing,
                    population,
                    valid_land_area_square_kilometers: None,
                });
            }
        }
    }
    allocate_hyde_area_window(
        &sources,
        &targets,
        request.center_latitude_e7,
        request.center_longitude_e7,
    )
}

fn split_page_bounds(bounds: PageBounds) -> Option<Vec<PageBounds>> {
    if bounds.width <= 1 && bounds.height <= 1 {
        return None;
    }
    let widths = if bounds.width > 1 {
        vec![bounds.width / 2, bounds.width - bounds.width / 2]
    } else {
        vec![bounds.width]
    };
    let heights = if bounds.height > 1 {
        vec![bounds.height / 2, bounds.height - bounds.height / 2]
    } else {
        vec![bounds.height]
    };
    let mut children = Vec::with_capacity(widths.len() * heights.len());
    let mut y = bounds.y;
    for height in heights {
        let mut x = bounds.x;
        for &width in &widths {
            children.push(PageBounds {
                x,
                y,
                width,
                height,
            });
            x += width;
        }
        y += height;
    }
    Some(children)
}

fn page_cell_count(bounds: PageBounds) -> usize {
    usize::from(bounds.width) * usize::from(bounds.height)
}

fn place_child_values<T: Copy>(
    parent: PageBounds,
    child: PageBounds,
    child_values: &[T],
    parent_values: &mut [T],
) {
    debug_assert_eq!(child_values.len(), page_cell_count(child));
    debug_assert_eq!(parent_values.len(), page_cell_count(parent));
    for row in 0..usize::from(child.height) {
        let source_start = row * usize::from(child.width);
        let target_start = (usize::from(child.y - parent.y) + row) * usize::from(parent.width)
            + usize::from(child.x - parent.x);
        parent_values[target_start..target_start + usize::from(child.width)]
            .copy_from_slice(&child_values[source_start..source_start + usize::from(child.width)]);
    }
}

#[cfg(test)]
pub(super) type ArchivePageForTest = (
    Vec<HydeSourceAreaCell>,
    Vec<HydeTargetAreaCell>,
    Vec<HydeAreaAllocation>,
);

#[cfg(test)]
pub(super) fn allocate_archive_page_for_test(
    baseline_archive: &Path,
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<ArchivePageForTest, GeodataError> {
    if samples_per_axis > PAGE_SAMPLES {
        return Err(GeodataError::Preparation("test grid exceeds one page"));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    let reader = ArchiveReader::open(baseline_archive, supplementary_archive)?;
    let transform = target_to_wgs84(request)?;
    let (targets, _) = target_page(
        &transform,
        estimate.effective_side_meters,
        samples_per_axis,
        PageBounds {
            x: 0,
            y: 0,
            width: samples_per_axis,
            height: samples_per_axis,
        },
        f64::from(request.center_longitude_e7) / 10_000_000.0,
    )?;
    let sources = reader.source_cells(&targets)?;
    let allocations = allocate_hyde_area_window(
        &sources,
        &targets,
        request.center_latitude_e7,
        request.center_longitude_e7,
    )?;
    Ok((sources, targets, allocations))
}
