//! Area-averaged Copernicus GLO-90 elevation for landscape overview maps.
//!
//! A landscape overview compresses real land by the request's scale, so one
//! sample covers hundreds of meters (469 m on a Large map). The ETOPO 60
//! arc-second source is 1.85 km per cell, which is blocky at that scale. Here
//! every sample is the area mean of the GLO-90 pixels inside its cell, instead
//! of one point read. Where GLO-90 has no tile (open ocean) or no valid pixel,
//! the uncovered part of the cell takes the ETOPO value at the cell center.
//! Footprints that need more than [`MAX_OVERVIEW_DEM_TILES`] tiles, or that
//! touch a pole or the antimeridian, keep the ETOPO-only overview.
use super::{
    Bounds, DemResolution, Tile, geographic_extent, public_tile_source, tile_prefix, tile_source_id,
};
use crate::{
    CacheError, GeodataError, KnownSource, SourceCache,
    elevation::{MAX_LANDSCAPE_DIRECT_ELEVATION_SAMPLES_PER_AXIS, PreparedElevation},
    local_aeqd_definition,
};
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, MapRequest};
use gdal::{
    Dataset, GeoTransformEx,
    raster::ResampleAlg,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const OVERVIEW_DEM_IDENTITY: &str = "copernicus-glo90-area-mean-gdal-0.19";
pub(crate) const OVERVIEW_ETOPO_FILL_IDENTITY: &str = "etopo-overview-gdal-0.19-glo90-fill";

/// Large maps need 48 tiles at Paris and about 72 at 65 degrees north; 96
/// reaches 70 degrees. A GLO-90 tile is 2.7 MB at 62 N and 5.4 MB at 48 N, so
/// the cap bounds a first transfer near 0.5 GB, far under the 6 GiB job budget.
pub(crate) const MAX_OVERVIEW_DEM_TILES: usize = 96;
/// The cell boxes of edge samples reach half a cell beyond the footprint.
const FOOTPRINT_PAD_E7: i32 = 100_000;
/// Row-major 64 by 64 pages touch at most four tiles, so a few suffice.
const RESIDENT_TILES: usize = 8;
/// A GLO-90 tile is 1,200 by 1,200 pixels; refuse far larger rasters.
const MAX_TILE_PIXELS: usize = 4 * 1024 * 1024;
const PAGE: usize = ENVIRONMENT_PAGE_SAMPLES as usize;

/// The tiles a request needs, or `None` when the footprint is unsupported or
/// too large and the ETOPO-only overview applies.
pub(crate) fn plan_overview_tiles(request: MapRequest) -> Result<Option<Bounds>, GeodataError> {
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let side = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid map request estimate"))?
        .effective_side_meters;
    let bounds = match geographic_extent(request, side, FOOTPRINT_PAD_E7) {
        Ok(bounds) => bounds,
        Err(GeodataError::Preparation(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok((bounds.tile_count() <= MAX_OVERVIEW_DEM_TILES).then_some(bounds))
}

/// Verified tiles for the planned bounds. Cached tiles need no network; a
/// tile that is not cached is looked up and transferred through the same
/// allowlisted, checksum-verified path as detailed preparation. A tile the
/// provider does not publish (open ocean) is simply absent.
pub(crate) fn acquire_overview_tiles(
    cache: &SourceCache,
    bounds: Bounds,
    cancelled: &AtomicBool,
) -> Result<Vec<Tile>, GeodataError> {
    let resolution = DemResolution::Glo90;
    let mut tiles = Vec::new();
    let mut missing: Vec<(i32, i32, KnownSource)> = Vec::new();
    for latitude in bounds.min_latitude..=bounds.max_latitude {
        for longitude in bounds.min_longitude..=bounds.max_longitude {
            if cancelled.load(Ordering::SeqCst) {
                return Err(GeodataError::Cache(CacheError::Cancelled));
            }
            let prefix = tile_prefix(latitude, longitude, resolution.suffix());
            if let Some(lock) = cache.known_lock(&tile_source_id(&prefix, resolution))? {
                tiles.push(Tile {
                    latitude,
                    longitude,
                    path: cache.object_path(&lock)?,
                    lock,
                });
            } else if let Some(source) = public_tile_source(latitude, longitude, resolution)? {
                missing.push((latitude, longitude, source));
            }
        }
    }
    let sources: Vec<KnownSource> = missing
        .iter()
        .map(|(_, _, source)| source.clone())
        .collect();
    cache.estimate_known_acquisition(&sources)?;
    for (latitude, longitude, source) in missing {
        let lock = cache.acquire_known(&source, cancelled)?;
        tiles.push(Tile {
            latitude,
            longitude,
            path: cache.object_path(&lock)?,
            lock,
        });
    }
    tiles.sort_by_key(|tile| (tile.latitude, tile.longitude));
    Ok(tiles)
}

/// Elevation for a landscape overview grid: every sample is the mean of the
/// GLO-90 pixels in its cell, with `coarse` filling what the tiles lack.
pub(crate) fn prepare_area_averaged_elevation(
    coarse: &Path,
    request: MapRequest,
    samples_per_axis: u16,
    tiles: &[Tile],
) -> Result<PreparedElevation, GeodataError> {
    if !(2..=MAX_LANDSCAPE_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "working grid is outside direct bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    let side = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?
        .effective_side_meters;
    let mut sampler = AreaSampler::new(coarse, request, side as f64, tiles)?;
    let axis = usize::from(samples_per_axis);
    let mut heights = vec![0_i32; axis * axis];
    for origin_y in (0..axis).step_by(PAGE) {
        for origin_x in (0..axis).step_by(PAGE) {
            sampler.fill_page(axis, origin_x, origin_y, &mut heights)?;
        }
    }
    crate::elevation::pyramid_from_heights(side, samples_per_axis, heights)
}

struct AreaSampler<'a> {
    to_wgs84: CoordTransform,
    side: f64,
    tiles: &'a [Tile],
    index: BTreeMap<(i32, i32), usize>,
    resident: Vec<(usize, LoadedTile)>,
    coarse: CoarseSampler,
}

impl<'a> AreaSampler<'a> {
    fn new(
        coarse: &Path,
        request: MapRequest,
        side: f64,
        tiles: &'a [Tile],
    ) -> Result<Self, GeodataError> {
        let definition =
            local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
        let mut target =
            SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
        let mut wgs84 = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
        target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        wgs84.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        let index = tiles
            .iter()
            .enumerate()
            .map(|(position, tile)| ((tile.latitude, tile.longitude), position))
            .collect();
        Ok(Self {
            to_wgs84: CoordTransform::new(&target, &wgs84).map_err(|_| GeodataError::Projection)?,
            side,
            tiles,
            index,
            resident: Vec::with_capacity(RESIDENT_TILES),
            coarse: CoarseSampler::open(coarse)?,
        })
    }

    fn fill_page(
        &mut self,
        axis: usize,
        origin_x: usize,
        origin_y: usize,
        heights: &mut [i32],
    ) -> Result<(), GeodataError> {
        let width = (axis - origin_x).min(PAGE);
        let height = (axis - origin_y).min(PAGE);
        let count = width * height;
        let spacing = self.side / axis as f64;
        let half = spacing / 2.0;
        // Centers, then each center moved half a cell east and north: the two
        // half-cell vectors give the cell's extent in longitude and latitude.
        let mut east = Vec::with_capacity(3 * count);
        let mut north = Vec::with_capacity(3 * count);
        for (dx, dy) in [(0.0, 0.0), (half, 0.0), (0.0, half)] {
            for row in 0..height {
                for column in 0..width {
                    east.push(-self.side / 2.0 + ((origin_x + column) as f64 + 0.5) * spacing + dx);
                    north.push(self.side / 2.0 - ((origin_y + row) as f64 + 0.5) * spacing + dy);
                }
            }
        }
        self.to_wgs84
            .transform_coords(&mut east, &mut north, &mut [])
            .map_err(|_| GeodataError::Coordinate)?;
        for row in 0..height {
            for column in 0..width {
                let i = row * width + column;
                let (longitude, latitude) = (east[i], north[i]);
                // Bounding box of the (slightly rotated) cell; the rotation is
                // the meridian convergence, about 2 degrees at 240 km.
                let half_longitude =
                    (east[count + i] - longitude).abs() + (east[2 * count + i] - longitude).abs();
                let half_latitude =
                    (north[count + i] - latitude).abs() + (north[2 * count + i] - latitude).abs();
                let mean = self.mean(longitude, latitude, half_longitude, half_latitude)?;
                let centimeters = (mean * 100.0).round();
                if !centimeters.is_finite()
                    || centimeters < f64::from(i32::MIN)
                    || centimeters > f64::from(i32::MAX)
                {
                    return Err(GeodataError::Preparation(
                        "source elevation is outside centimeter bounds",
                    ));
                }
                heights[(origin_y + row) * axis + origin_x + column] = centimeters as i32;
            }
        }
        Ok(())
    }

    /// Area mean, in meters, of the cell centered on `(longitude, latitude)`.
    fn mean(
        &mut self,
        longitude: f64,
        latitude: f64,
        half_longitude: f64,
        half_latitude: f64,
    ) -> Result<f64, GeodataError> {
        let (west, east) = (longitude - half_longitude, longitude + half_longitude);
        let (south, north) = (latitude - half_latitude, latitude + half_latitude);
        let (mut weighted, mut covered) = (0.0, 0.0);
        for tile_latitude in south.floor() as i32..=north.floor() as i32 {
            for tile_longitude in west.floor() as i32..=east.floor() as i32 {
                let Some(&position) = self.index.get(&(tile_latitude, tile_longitude)) else {
                    continue;
                };
                self.tile(position)?.accumulate(
                    west,
                    east,
                    south,
                    north,
                    &mut weighted,
                    &mut covered,
                );
            }
        }
        let total = (east - west) * (north - south);
        let uncovered = (total - covered).max(0.0);
        if uncovered > total * 1e-9 {
            weighted += self.coarse.value(longitude, latitude)? * uncovered;
            covered += uncovered;
        }
        Ok(weighted / covered)
    }

    fn tile(&mut self, position: usize) -> Result<&LoadedTile, GeodataError> {
        if let Some(slot) = self.resident.iter().position(|(held, _)| *held == position) {
            let entry = self.resident.remove(slot);
            self.resident.push(entry);
        } else {
            if self.resident.len() == RESIDENT_TILES {
                self.resident.remove(0);
            }
            let loaded = LoadedTile::read(&self.tiles[position])?;
            self.resident.push((position, loaded));
        }
        self.resident
            .last()
            .map(|(_, loaded)| loaded)
            .ok_or(GeodataError::Preparation("tile cache miss"))
    }
}

/// One GLO-90 tile in memory; invalid pixels are NaN and count as uncovered.
struct LoadedTile {
    west: f64,
    north: f64,
    pixel_width: f64,
    pixel_height: f64,
    width: usize,
    height: usize,
    heights: Vec<f32>,
}

impl LoadedTile {
    fn read(tile: &Tile) -> Result<Self, GeodataError> {
        let dataset = Dataset::open(&tile.path)?;
        let transform = dataset.geo_transform()?;
        let (width, height) = dataset.raster_size();
        if transform[1] <= 0.0
            || transform[5] >= 0.0
            || transform[2] != 0.0
            || transform[4] != 0.0
            || width == 0
            || height == 0
            || width.saturating_mul(height) > MAX_TILE_PIXELS
        {
            return Err(GeodataError::Preparation(
                "Copernicus tile geotransform is unsupported",
            ));
        }
        // GDAL may shift a point-registered grid by half a pixel; a tile that
        // is off by more than a pixel is not the degree its name claims.
        if (transform[0] - f64::from(tile.longitude)).abs() > transform[1]
            || (transform[3] - f64::from(tile.latitude + 1)).abs() > -transform[5]
        {
            return Err(GeodataError::Preparation(
                "Copernicus tile does not cover its named degree",
            ));
        }
        let band = dataset.rasterband(1)?;
        let nodata = band.no_data_value();
        let mut heights = band
            .read_as::<f32>((0, 0), (width, height), (width, height), None)?
            .data()
            .to_vec();
        for value in &mut heights {
            if nodata.is_some_and(|missing| f64::from(*value) == missing) || !value.is_finite() {
                *value = f32::NAN;
            }
        }
        Ok(Self {
            west: transform[0],
            north: transform[3],
            pixel_width: transform[1],
            pixel_height: -transform[5],
            width,
            height,
            heights,
        })
    }

    /// Adds the area-weighted heights of the box's overlap with this tile.
    fn accumulate(
        &self,
        west: f64,
        east: f64,
        south: f64,
        north: f64,
        weighted: &mut f64,
        covered: &mut f64,
    ) {
        let left = ((west - self.west) / self.pixel_width).max(0.0);
        let right = ((east - self.west) / self.pixel_width).min(self.width as f64);
        let top = ((self.north - north) / self.pixel_height).max(0.0);
        let bottom = ((self.north - south) / self.pixel_height).min(self.height as f64);
        if right <= left || bottom <= top {
            return;
        }
        let pixel_area = self.pixel_width * self.pixel_height;
        let last_row = (bottom.ceil() as usize).min(self.height);
        let last_column = (right.ceil() as usize).min(self.width);
        for row in top.floor() as usize..last_row {
            let row_weight = bottom.min(row as f64 + 1.0) - top.max(row as f64);
            let line = &self.heights[row * self.width..(row + 1) * self.width];
            let first_column = left.floor() as usize;
            for (offset, value) in line[first_column..last_column].iter().enumerate() {
                let column = first_column + offset;
                if value.is_nan() {
                    continue;
                }
                let weight = row_weight
                    * (right.min(column as f64 + 1.0) - left.max(column as f64))
                    * pixel_area;
                *weighted += weight * f64::from(*value);
                *covered += weight;
            }
        }
    }
}

/// Nearest-cell reads of the ETOPO raster in its native EPSG:4326 grid.
struct CoarseSampler {
    dataset: Dataset,
    inverse: [f64; 6],
    width: isize,
    height: isize,
    nodata: Option<f64>,
}

impl CoarseSampler {
    fn open(path: &Path) -> Result<Self, GeodataError> {
        let dataset = Dataset::open(path)?;
        let (width, height) = dataset.raster_size();
        let inverse = dataset.geo_transform()?.invert()?;
        let nodata = dataset.rasterband(1)?.no_data_value();
        Ok(Self {
            dataset,
            inverse,
            width: width as isize,
            height: height as isize,
            nodata,
        })
    }

    fn value(&self, longitude: f64, latitude: f64) -> Result<f64, GeodataError> {
        let (pixel, line) = self.inverse.apply(longitude, latitude);
        let (pixel, line) = (pixel.floor() as isize, line.floor() as isize);
        if pixel < 0 || line < 0 || pixel >= self.width || line >= self.height {
            return Err(GeodataError::Preparation(
                "source does not cover the requested footprint",
            ));
        }
        let value = self
            .dataset
            .rasterband(1)?
            .read_as::<f64>(
                (pixel, line),
                (1, 1),
                (1, 1),
                Some(ResampleAlg::NearestNeighbour),
            )?
            .data()[0];
        if !value.is_finite() || self.nodata.is_some_and(|missing| value == missing) {
            return Err(GeodataError::Preparation(
                "source contains elevation nodata",
            ));
        }
        Ok(value)
    }
}
