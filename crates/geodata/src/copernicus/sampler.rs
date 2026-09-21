use super::{Bounds, GeodataError, MapRequest, PAGE, Tile};
use aoe_map::ElevationPage;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use gdal::{Dataset, raster::ResampleAlg};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

pub(super) struct Sampler {
    target_to_wgs84: CoordTransform,
    side: f64,
    bounds: Bounds,
    absent_tiles: BTreeSet<(i32, i32)>,
    pub(super) tiles: Vec<Tile>,
    overview_ocean: Vec<u8>,
}

impl Sampler {
    pub(super) fn new(
        request: MapRequest,
        side: u64,
        bounds: Bounds,
        absent_tiles: BTreeSet<(i32, i32)>,
        tiles: Vec<Tile>,
        overview_ocean: Vec<u8>,
    ) -> Result<Self, GeodataError> {
        let definition =
            crate::local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
        let mut target =
            SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
        let mut wgs84 = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
        target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        wgs84.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        let target_to_wgs84 =
            CoordTransform::new(&target, &wgs84).map_err(|_| GeodataError::Projection)?;
        Ok(Self {
            target_to_wgs84,
            side: side as f64,
            bounds,
            absent_tiles,
            tiles,
            overview_ocean,
        })
    }

    pub(super) fn page(
        &mut self,
        axis: u16,
        level: u8,
        x: u16,
        y: u16,
    ) -> Result<ElevationPage, GeodataError> {
        let width = (axis - x.saturating_mul(PAGE)).min(PAGE) as u8;
        let height = (axis - y.saturating_mul(PAGE)).min(PAGE) as u8;
        let spacing = self.side / f64::from(axis);
        let mut east = Vec::with_capacity(usize::from(width) * usize::from(height));
        let mut north = Vec::with_capacity(east.capacity());
        let mut overview_indices = Vec::with_capacity(east.capacity());
        for row in 0..u16::from(height) {
            for column in 0..u16::from(width) {
                let global_x = x * PAGE + column;
                let global_y = y * PAGE + row;
                east.push(-self.side / 2.0 + (f64::from(global_x) + 0.5) * spacing);
                north.push(self.side / 2.0 - (f64::from(global_y) + 0.5) * spacing);
                let overview_x = super::pyramid::coarse_coordinate(axis, global_x);
                let overview_y = super::pyramid::coarse_coordinate(axis, global_y);
                overview_indices.push(usize::from(overview_y) * 128 + usize::from(overview_x));
            }
        }
        self.target_to_wgs84
            .transform_coords(&mut east, &mut north, &mut [])
            .map_err(|_| GeodataError::Coordinate)?;
        let mut open = BTreeMap::new();
        let mut values = Vec::with_capacity(east.len());
        for ((longitude, latitude), overview_index) in
            east.into_iter().zip(north).zip(overview_indices)
        {
            let primary_key = (latitude.floor() as i32, longitude.floor() as i32);
            if !self.bounds.contains(primary_key.0, primary_key.1) {
                return Err(GeodataError::Preparation(
                    "Copernicus tile bounds did not cover a requested coordinate",
                ));
            }
            let candidate_keys = tile_candidate_keys(latitude, longitude);
            let mut sample = None;
            for tile_key in candidate_keys {
                let Some(tile) = self
                    .tiles
                    .iter()
                    .find(|tile| (tile.latitude, tile.longitude) == tile_key)
                else {
                    continue;
                };
                if let Entry::Vacant(entry) = open.entry(tile_key) {
                    entry.insert(OpenTile::open(tile)?);
                }
                if let Some(value) = open
                    .get_mut(&tile_key)
                    .ok_or(GeodataError::Preparation("tile cache miss"))?
                    .sample(longitude, latitude)?
                {
                    sample = Some(value);
                    break;
                }
            }
            values.push(missing_tile_value(
                self.bounds,
                &self.absent_tiles,
                primary_key,
                sample,
                self.overview_ocean.get(overview_index).copied(),
            )?);
        }
        Ok(ElevationPage {
            level,
            x,
            y,
            width,
            height,
            geographic_height_centimeters: values,
        })
    }
}

pub(super) fn missing_tile_value(
    bounds: Bounds,
    absent_tiles: &BTreeSet<(i32, i32)>,
    primary_key: (i32, i32),
    sample: Option<i32>,
    overview_ocean_percent: Option<u8>,
) -> Result<i32, GeodataError> {
    if let Some(value) = sample {
        return Ok(value);
    }
    if !bounds.contains(primary_key.0, primary_key.1) {
        return Err(GeodataError::Preparation(
            "Copernicus tile bounds did not cover a requested coordinate",
        ));
    }
    if absent_tiles.contains(&primary_key) && overview_ocean_percent == Some(100) {
        return Ok(0);
    }
    Err(GeodataError::Preparation(
        "selected Copernicus tiles do not cover the coordinate",
    ))
}

pub(super) fn tile_candidate_keys(latitude: f64, longitude: f64) -> Vec<(i32, i32)> {
    let latitude = latitude.floor() as i32;
    let longitude = longitude.floor() as i32;
    let mut keys = Vec::with_capacity(9);
    for delta_latitude in -1..=1 {
        for delta_longitude in -1..=1 {
            keys.push((latitude + delta_latitude, longitude + delta_longitude));
        }
    }
    keys
}

struct OpenTile {
    dataset: Dataset,
    transform: [f64; 6],
    width: isize,
    height: isize,
    nodata: Option<f64>,
}

impl OpenTile {
    fn open(tile: &Tile) -> Result<Self, GeodataError> {
        let dataset = Dataset::open(&tile.path)?;
        let transform = dataset.geo_transform()?;
        if transform[1] <= 0.0 || transform[5] >= 0.0 || transform[2] != 0.0 || transform[4] != 0.0
        {
            return Err(GeodataError::Preparation(
                "Copernicus tile geotransform is unsupported",
            ));
        }
        let (width, height) = dataset.raster_size();
        let nodata = dataset.rasterband(1)?.no_data_value();
        Ok(Self {
            dataset,
            transform,
            width: width as isize,
            height: height as isize,
            nodata,
        })
    }

    fn sample(&mut self, longitude: f64, latitude: f64) -> Result<Option<i32>, GeodataError> {
        let pixel = ((longitude - self.transform[0]) / self.transform[1]).floor() as isize;
        let line = ((latitude - self.transform[3]) / self.transform[5]).floor() as isize;
        if pixel < 0 || line < 0 || pixel >= self.width || line >= self.height {
            return Ok(None);
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
            return Err(GeodataError::Preparation("Copernicus tile contains nodata"));
        }
        let centimeters = value * 100.0;
        if centimeters < i32::MIN as f64 || centimeters > i32::MAX as f64 {
            return Err(GeodataError::Preparation(
                "Copernicus elevation exceeds centimeter bounds",
            ));
        }
        Ok(Some(centimeters.round() as i32))
    }
}
