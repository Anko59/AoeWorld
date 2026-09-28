use super::values::{
    RasterValue, classify_land_lake_value, classify_raster_value, quantity_value,
    validate_target_geography,
};
use super::{
    HYDE_600_MEMBERS, HydeAreaState, HydeGeographicPoint, HydeSourceAreaCell, HydeTargetAreaCell,
};
use crate::GeodataError;
use gdal::config::{
    clear_thread_local_config_option, get_thread_local_config_option,
    set_thread_local_config_option,
};
use gdal::{Dataset, GeoTransformEx, raster::ResampleAlg};
use std::path::Path;

const MAX_SOURCE_WINDOW_CELLS: usize = 1_000_000;
const LONGITUDE_PERIOD_DEGREES: f64 = 360.0;
const HYDE_600_WIDTH: usize = 4_320;
const HYDE_600_HEIGHT: usize = 2_160;
const HYDE_600_ROUNDED_CELL_DEGREES: f64 = 0.083_333_3;
const HYDE_600_CELL_DEGREES: f64 = 1.0 / 12.0;
const SPHERICAL_EARTH_RADIUS_KILOMETERS: f64 = 6_371.0;
const VALID_AREA_ROUNDING_TOLERANCE_SQUARE_KILOMETERS: f64 = 0.0001;
const AAI_GRID_DATATYPE_OPTION: &str = "AAIGRID_DATATYPE";

#[derive(Clone, Copy)]
pub(super) struct PixelWindow {
    pub(super) left: usize,
    pub(super) top: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

#[derive(Clone, Copy)]
pub(super) struct SourceWindow {
    pub(super) pixels: PixelWindow,
    pub(super) longitude_offset_degrees: f64,
}

pub(super) struct RasterSource {
    dataset: Dataset,
    width: usize,
    height: usize,
    transform: [f64; 6],
    inverse: [f64; 6],
    nodata: Option<f64>,
}

pub(super) struct ArchiveReader {
    crop: RasterSource,
    grazing: RasterSource,
    population: RasterSource,
    land_lake: RasterSource,
    valid_land: RasterSource,
}

impl RasterSource {
    pub(super) fn open(archive: &Path, member: &str) -> Result<Self, GeodataError> {
        let path = super::super::extract_member(archive, member)?;
        let dataset = open_aai_grid_float64(&path)?;
        let (width, height) = dataset.raster_size();
        let transform = canonical_hyde_600_transform(width, height, dataset.geo_transform()?);
        let inverse = transform.invert()?;
        let nodata = dataset.rasterband(1)?.no_data_value();
        Ok(Self {
            dataset,
            width,
            height,
            transform,
            inverse,
            nodata,
        })
    }

    pub(super) fn values(&self, window: PixelWindow) -> Result<Vec<RasterValue>, GeodataError> {
        let buffer = self.dataset.rasterband(1)?.read_as::<f64>(
            (window.left as isize, window.top as isize),
            (window.width, window.height),
            (window.width, window.height),
            Some(ResampleAlg::NearestNeighbour),
        )?;
        Ok(buffer
            .data()
            .iter()
            .map(|&value| classify_raster_value(value, self.nodata))
            .collect())
    }

    pub(super) fn cell_polygon(
        &self,
        column: usize,
        row: usize,
        longitude_offset_degrees: f64,
    ) -> Vec<HydeGeographicPoint> {
        let at = |column: usize, row: usize| HydeGeographicPoint {
            longitude_degrees: self.transform[0]
                + column as f64 * self.transform[1]
                + row as f64 * self.transform[2]
                + longitude_offset_degrees,
            latitude_degrees: self.transform[3]
                + column as f64 * self.transform[4]
                + row as f64 * self.transform[5],
        };
        vec![
            at(column, row),
            at(column + 1, row),
            at(column + 1, row + 1),
            at(column, row + 1),
        ]
    }

    pub(super) fn windows_for_targets(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<Vec<SourceWindow>, GeodataError> {
        self.windows_for_targets_with_limit(targets, MAX_SOURCE_WINDOW_CELLS)
    }

    fn windows_for_targets_with_limit(
        &self,
        targets: &[HydeTargetAreaCell],
        max_source_cells: usize,
    ) -> Result<Vec<SourceWindow>, GeodataError> {
        validate_target_geography(targets)?;
        let mut min_longitude = f64::INFINITY;
        let mut max_longitude = f64::NEG_INFINITY;
        for point in targets.iter().flat_map(|target| &target.polygon) {
            min_longitude = min_longitude.min(point.longitude_degrees);
            max_longitude = max_longitude.max(point.longitude_degrees);
        }
        if !min_longitude.is_finite() || !max_longitude.is_finite() {
            return Err(GeodataError::Preparation("HYDE target page is empty"));
        }

        let (first_turn, last_turn) = if self.has_global_longitude_period() {
            let (raster_west, raster_east) = self.longitude_extent();
            (
                ((min_longitude - raster_east) / LONGITUDE_PERIOD_DEGREES).ceil() as i32,
                ((max_longitude - raster_west) / LONGITUDE_PERIOD_DEGREES).floor() as i32,
            )
        } else {
            (0, 0)
        };
        if last_turn < first_turn {
            return Ok(Vec::new());
        }
        if last_turn - first_turn > 3 {
            return Err(GeodataError::Preparation(
                "HYDE target page exceeds the wrap-window limit",
            ));
        }

        let mut windows = Vec::new();
        let mut total_cells = 0_usize;
        for turn in first_turn..=last_turn {
            let longitude_offset = f64::from(turn) * LONGITUDE_PERIOD_DEGREES;
            let mut min_column = f64::INFINITY;
            let mut min_row = f64::INFINITY;
            let mut max_column = f64::NEG_INFINITY;
            let mut max_row = f64::NEG_INFINITY;
            for point in targets.iter().flat_map(|target| &target.polygon) {
                let (column, row) = self.inverse.apply(
                    point.longitude_degrees - longitude_offset,
                    point.latitude_degrees,
                );
                if !column.is_finite() || !row.is_finite() {
                    return Err(GeodataError::Coordinate);
                }
                min_column = min_column.min(column);
                min_row = min_row.min(row);
                max_column = max_column.max(column);
                max_row = max_row.max(row);
            }
            let left = min_column.floor().clamp(0.0, self.width as f64) as usize;
            let top = min_row.floor().clamp(0.0, self.height as f64) as usize;
            let right = max_column.ceil().clamp(0.0, self.width as f64) as usize;
            let bottom = max_row.ceil().clamp(0.0, self.height as f64) as usize;
            if right <= left || bottom <= top {
                continue;
            }
            let width = right - left;
            let height = bottom - top;
            let cells = width.saturating_mul(height);
            total_cells = total_cells.saturating_add(cells);
            if total_cells > max_source_cells {
                return Err(GeodataError::Preparation(
                    "HYDE archive windows exceed the source-cell limit",
                ));
            }
            windows.push(SourceWindow {
                pixels: PixelWindow {
                    left,
                    top,
                    width,
                    height,
                },
                longitude_offset_degrees: longitude_offset,
            });
        }
        Ok(windows)
    }

    pub(super) fn source_cell_count(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<usize, GeodataError> {
        Ok(self
            .windows_for_targets_with_limit(targets, usize::MAX)?
            .iter()
            .map(|window| window.pixels.width.saturating_mul(window.pixels.height))
            .fold(0_usize, usize::saturating_add))
    }

    fn spherical_cell_area_square_kilometers(&self, row: usize) -> Result<f64, GeodataError> {
        if row >= self.height
            || self.transform[2].abs() > 1.0e-12
            || self.transform[4].abs() > 1.0e-12
            || self.transform[1].abs() <= 0.0
            || self.transform[5].abs() <= 0.0
        {
            return Err(GeodataError::Preparation(
                "HYDE valid-land grid has invalid geographic cell bounds",
            ));
        }
        let north = self.transform[3] + row as f64 * self.transform[5];
        let south = north + self.transform[5];
        if north.abs().max(south.abs()) > 90.0 + 1.0e-8 {
            return Err(GeodataError::Preparation(
                "HYDE valid-land grid has invalid geographic cell bounds",
            ));
        }
        let area = spherical_cell_area_square_kilometers(self.transform[1].abs(), north, south);
        if !area.is_finite() || area <= 0.0 {
            return Err(GeodataError::Preparation(
                "HYDE valid-land grid has invalid geographic cell bounds",
            ));
        }
        Ok(area)
    }

    fn has_global_longitude_period(&self) -> bool {
        self.transform[2].abs() <= 1.0e-12
            && self.transform[4].abs() <= 1.0e-12
            && ((self.width as f64 * self.transform[1].abs()) - LONGITUDE_PERIOD_DEGREES).abs()
                <= 1.0e-6
    }

    fn longitude_extent(&self) -> (f64, f64) {
        let opposite = self.transform[0] + self.width as f64 * self.transform[1];
        (
            self.transform[0].min(opposite),
            self.transform[0].max(opposite),
        )
    }
}

fn open_aai_grid_float64(path: &Path) -> Result<Dataset, GeodataError> {
    let previous = get_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "")?;
    set_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "Float64")?;
    let opened = Dataset::open(path);
    let restored = if previous.is_empty() {
        clear_thread_local_config_option(AAI_GRID_DATATYPE_OPTION)
    } else {
        set_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, &previous)
    };
    match opened {
        Ok(dataset) => {
            restored?;
            Ok(dataset)
        }
        Err(error) => {
            restored?;
            Err(error.into())
        }
    }
}

impl ArchiveReader {
    pub(super) fn open(baseline: &Path, supplementary: &Path) -> Result<Self, GeodataError> {
        let reader = Self {
            crop: RasterSource::open(baseline, HYDE_600_MEMBERS[0])?,
            grazing: RasterSource::open(baseline, HYDE_600_MEMBERS[1])?,
            population: RasterSource::open(baseline, HYDE_600_MEMBERS[2])?,
            land_lake: RasterSource::open(supplementary, HYDE_600_MEMBERS[3])?,
            valid_land: RasterSource::open(supplementary, HYDE_600_MEMBERS[4])?,
        };
        reader.validate_grids()?;
        Ok(reader)
    }

    fn validate_grids(&self) -> Result<(), GeodataError> {
        let reference = &self.land_lake;
        for source in [
            &self.crop,
            &self.grazing,
            &self.population,
            &self.valid_land,
        ] {
            if source.width != reference.width
                || source.height != reference.height
                || source
                    .transform
                    .iter()
                    .zip(reference.transform)
                    .any(|(left, right)| (left - right).abs() > right.abs().max(1.0) * 1.0e-12)
            {
                return Err(GeodataError::Preparation(
                    "HYDE source members use different grids",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn source_cells(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<Vec<HydeSourceAreaCell>, GeodataError> {
        let windows = self.land_lake.windows_for_targets(targets)?;
        let mut cells = Vec::new();
        for source_window in windows {
            let window = source_window.pixels;
            let land_lake = self.land_lake.values(window)?;
            let crop = self.crop.values(window)?;
            let grazing = self.grazing.values(window)?;
            let population = self.population.values(window)?;
            let valid_land = self.valid_land.values(window)?;
            cells.reserve(land_lake.len());
            for row in 0..window.height {
                for column in 0..window.width {
                    let index = row * window.width + column;
                    let state = classify_land_lake_value(land_lake[index])?;
                    let (crop, grazing, population, valid_area) = if state == HydeAreaState::Land {
                        let (Some(crop), Some(grazing), Some(population), Some(valid_area)) = (
                            quantity_value(crop[index])?,
                            quantity_value(grazing[index])?,
                            quantity_value(population[index])?,
                            quantity_value(valid_land[index])?,
                        ) else {
                            return Err(GeodataError::Preparation(
                                "HYDE land cell is missing a required quantity",
                            ));
                        };
                        let max_valid_area = self
                            .valid_land
                            .spherical_cell_area_square_kilometers(window.top + row)?;
                        if valid_area_exceeds_capacity(valid_area, max_valid_area) {
                            return Err(GeodataError::Preparation(
                                "HYDE valid-land area exceeds spherical source-cell area",
                            ));
                        }
                        (
                            Some(crop),
                            Some(grazing),
                            Some(population),
                            Some(valid_area),
                        )
                    } else {
                        (None, None, None, None)
                    };
                    cells.push(HydeSourceAreaCell {
                        polygon: self.land_lake.cell_polygon(
                            window.left + column,
                            window.top + row,
                            source_window.longitude_offset_degrees,
                        ),
                        state,
                        crop_area_square_kilometers: crop,
                        grazing_area_square_kilometers: grazing,
                        population,
                        valid_land_area_square_kilometers: valid_area,
                    });
                }
            }
        }
        Ok(cells)
    }

    pub(super) fn source_cell_count(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<usize, GeodataError> {
        self.land_lake.source_cell_count(targets)
    }
}

fn canonical_hyde_600_transform(width: usize, height: usize, transform: [f64; 6]) -> [f64; 6] {
    let rounded_north = -90.0 + HYDE_600_HEIGHT as f64 * HYDE_600_ROUNDED_CELL_DEGREES;
    let header_matches = width == HYDE_600_WIDTH
        && height == HYDE_600_HEIGHT
        && (transform[0] + 180.0).abs() <= 1.0e-6
        && (transform[1] - HYDE_600_ROUNDED_CELL_DEGREES).abs() <= 5.0e-8
        && transform[2].abs() <= 1.0e-12
        && (transform[3] - rounded_north).abs() <= 1.0e-6
        && transform[4].abs() <= 1.0e-12
        && (transform[5] + HYDE_600_ROUNDED_CELL_DEGREES).abs() <= 5.0e-8;
    if header_matches {
        [
            -180.0,
            HYDE_600_CELL_DEGREES,
            0.0,
            90.0,
            0.0,
            -HYDE_600_CELL_DEGREES,
        ]
    } else {
        transform
    }
}

fn spherical_cell_area_square_kilometers(
    longitude_width_degrees: f64,
    north_latitude_degrees: f64,
    south_latitude_degrees: f64,
) -> f64 {
    SPHERICAL_EARTH_RADIUS_KILOMETERS.powi(2)
        * longitude_width_degrees.to_radians()
        * (north_latitude_degrees.to_radians().sin() - south_latitude_degrees.to_radians().sin())
            .abs()
}

fn valid_area_exceeds_capacity(valid_area: f64, spherical_cell_area: f64) -> bool {
    valid_area > spherical_cell_area + VALID_AREA_ROUNDING_TOLERANCE_SQUARE_KILOMETERS
}

#[cfg(test)]
#[path = "tests/source.rs"]
mod tests;
