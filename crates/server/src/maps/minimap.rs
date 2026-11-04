//! One bounded strategic raster per map package, sampled from the same
//! composed landscape query that produces terrain chunks. It is presentation
//! data for the minimap and whole-map overview, never a gameplay input.
use aoe_core::TileCoord;
use aoe_map::{
    EnvironmentPageError, GroundMaterial, LandscapePoint, MapChunkGenerator, MapPackage, WaterKind,
};
use rayon::prelude::*;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, LazyLock},
    time::Duration,
};

/// Largest raster axis. Smaller maps use one sample per tile instead.
pub(crate) const MAX_MINIMAP_SAMPLES_PER_AXIS: u16 = 512;
/// Magic, little-endian axis and two reserved zero bytes.
pub(crate) const MINIMAP_HEADER_BYTES: usize = 8;
pub(crate) const MINIMAP_MAGIC: [u8; 4] = *b"AWMM";
/// Hard response and per-entry cache budget: header plus one byte per sample.
pub(crate) const MAX_MINIMAP_BYTES: usize =
    MINIMAP_HEADER_BYTES + (MAX_MINIMAP_SAMPLES_PER_AXIS as usize).pow(2);
/// Rasters retained across requests; at most `4 * MAX_MINIMAP_BYTES` bytes.
pub(crate) const MAX_CACHED_MINIMAPS: usize = 8;
/// Wall-clock budget for one raster. An exhausted budget fails the request
/// instead of returning a partial raster; nothing partial is cached.
pub(crate) const MINIMAP_TIME_BUDGET: Duration = Duration::from_secs(30);
/// Rasters allowed to finish in the background after their request timed out.
pub(crate) const MAX_BACKGROUND_MINIMAPS: usize = 2;
/// Shade nibble of level ground; 0 is fully shaded and 15 fully lit.
pub(crate) const FLAT_SHADE: u8 = 8;
/// Narrowest river the minimap draws, in 1/256 tiles: 3.5 tiles, the rivers a
/// 4.7 m³/s cutoff kept before small rivers returned at every map size. Narrower
/// streams are real water on the map but would turn the strategic raster
/// into a haze of single pixels, so they are omitted there.
pub(crate) const MINIMAP_MIN_RIVER_WIDTH_UNITS: u16 = 896;
/// Tiles of slack when proving a water sample belongs to no drawn river:
/// the physical channel is warped by up to one and a half tiles and varies by
/// half a tile around the nominal centreline footprint.
const MINIMAP_WARP_MARGIN_TILES: i32 = 3;
/// Summed per-axis rise, in per-mille, that changes the shade by one step.
const SHADE_STEP_PER_MILLE: i64 = 25;

#[path = "minimap_relief.rs"]
mod relief;

/// Low nibble of every raster byte. Values are part of the client contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum MinimapClass {
    Grass,
    DryGrass,
    Forest,
    Soil,
    Sand,
    Rock,
    Snow,
    Ice,
    Shore,
    River,
    Lake,
    Ocean,
    Cropland,
    Pasture,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinimapError {
    EmptyMap,
    OutsideMap,
    Page(EnvironmentPageError),
}

/// Most-recently-used rasters by package content hash.
#[derive(Debug, Default)]
pub(crate) struct MinimapCache {
    entries: VecDeque<(String, Arc<[u8]>)>,
    /// Rasters still computing after a request hit its deadline.
    pub(in crate::maps) pending: BTreeSet<String>,
}

impl MinimapCache {
    pub(crate) fn get(&mut self, hash: &str) -> Option<Arc<[u8]>> {
        let index = self.entries.iter().position(|(key, _)| key == hash)?;
        let entry = self.entries.remove(index)?;
        let raster = entry.1.clone();
        self.entries.push_back(entry);
        Some(raster)
    }

    pub(crate) fn insert(&mut self, hash: String, raster: Arc<[u8]>) {
        self.entries.retain(|(key, _)| *key != hash);
        self.entries.push_back((hash, raster));
        while self.entries.len() > MAX_CACHED_MINIMAPS {
            self.entries.pop_front();
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Raster axis for a map: one sample per tile up to the fixed maximum.
pub(crate) fn minimap_samples(tiles_per_side: u64) -> u16 {
    tiles_per_side.min(u64::from(MAX_MINIMAP_SAMPLES_PER_AXIS)) as u16
}

/// Tile under the center of raster sample `index` on one axis.
pub(crate) fn minimap_tile(tiles_per_side: u64, samples: u16, index: u16) -> Option<i32> {
    let samples = u64::from(samples);
    if samples == 0 || u64::from(index) >= samples {
        return None;
    }
    let numerator = (u64::from(index) * 2 + 1).checked_mul(tiles_per_side)?;
    i32::try_from((numerator / (samples * 2)).min(tiles_per_side.checked_sub(1)?)).ok()
}

/// Class of one composed landscape point: water first, then canopy, then
/// the composed ground material.
pub(crate) fn classify(point: &LandscapePoint) -> MinimapClass {
    let terrain = &point.tile.terrain;
    match terrain.water {
        WaterKind::Ocean => return MinimapClass::Ocean,
        WaterKind::Lake => return MinimapClass::Lake,
        WaterKind::River => return MinimapClass::River,
        WaterKind::Shallow => return MinimapClass::Shore,
        WaterKind::None => {}
    }
    match point.historical_land_use {
        aoe_map::historical_parcels::LandUse::Crop => return MinimapClass::Cropland,
        aoe_map::historical_parcels::LandUse::Grazing => return MinimapClass::Pasture,
        _ => {}
    }
    if point.tile.appearance.canopy_strength > 0 {
        return MinimapClass::Forest;
    }
    match terrain.material {
        GroundMaterial::TemperateGrass | GroundMaterial::LushGrass | GroundMaterial::WornGrass => {
            MinimapClass::Grass
        }
        GroundMaterial::DryGrass | GroundMaterial::GrassDirt => MinimapClass::DryGrass,
        GroundMaterial::ForestFloor | GroundMaterial::Dirt | GroundMaterial::Mud => {
            MinimapClass::Soil
        }
        GroundMaterial::Sand | GroundMaterial::Gravel => MinimapClass::Sand,
        GroundMaterial::Rock => MinimapClass::Rock,
        GroundMaterial::Snow => MinimapClass::Snow,
        GroundMaterial::Ice => MinimapClass::Ice,
        GroundMaterial::Shore
        | GroundMaterial::Water
        | GroundMaterial::DeepWater
        | GroundMaterial::Ford => MinimapClass::Shore,
    }
}

/// Class of the dry land around an omitted stream: the first dry sample on a
/// ring beyond the stream's bank, so a desert or a snowfield keeps its own
/// colour. Grass only where no dry land is within reach.
fn surrounding_land_class(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    reach: i32,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<MinimapClass, MinimapError> {
    for distance in [reach, reach * 2] {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let probe = TileCoord::new(
                tile.x.saturating_add(dx * distance),
                tile.y.saturating_add(dy * distance),
            );
            let Some(point) = generator
                .landscape_surface_with_cancel(probe, cancelled)
                .map_err(MinimapError::Page)?
            else {
                continue;
            };
            if point.tile.terrain.water == WaterKind::None {
                return Ok(classify(&point));
            }
        }
    }
    Ok(MinimapClass::Grass)
}

/// Deterministic raster of the composed landscape: `MINIMAP_HEADER_BYTES`
/// then row-major bytes, class in the low nibble and relief shade in the high
/// nibble. Performs exactly `samples²` point queries and allocates at most
/// `MAX_MINIMAP_BYTES` plus at most 24 bytes of scratch per sample.
#[cfg(test)]
pub(crate) fn minimap_raster(
    generator: &MapChunkGenerator,
    tiles_per_side: u64,
    geographic_millimeters_per_tile: u64,
    elevation_samples_per_axis: u16,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    minimap_raster_at_samples(
        generator,
        tiles_per_side,
        geographic_millimeters_per_tile,
        elevation_samples_per_axis,
        minimap_samples(tiles_per_side),
        cancelled,
    )
}

pub(crate) fn minimap_raster_at_samples(
    generator: &MapChunkGenerator,
    tiles_per_side: u64,
    geographic_millimeters_per_tile: u64,
    elevation_samples_per_axis: u16,
    samples: u16,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    let samples = samples.min(minimap_samples(tiles_per_side));
    if samples == 0 {
        return Err(MinimapError::EmptyMap);
    }
    let side = usize::from(samples);
    let geometry = generator.has_river_geometry();
    static WORKERS: LazyLock<Option<rayon::ThreadPool>> =
        LazyLock::new(|| rayon::ThreadPoolBuilder::new().num_threads(4).build().ok());
    let workers = WORKERS
        .as_ref()
        .ok_or(MinimapError::Page(EnvironmentPageError::Unavailable))?;
    let mut points = vec![(MinimapClass::Grass, 0); side * side];
    let band_rows = side.div_ceil(4);
    workers.install(|| {
        points
            .par_chunks_mut(side * band_rows)
            .enumerate()
            .try_for_each(|(band, band_points)| {
                let generator = generator.fork_queries();
                for (y, row) in band_points.chunks_mut(side).enumerate() {
                    let y = (band * band_rows + y) as u16;
                    let tile_y =
                        minimap_tile(tiles_per_side, samples, y).ok_or(MinimapError::OutsideMap)?;
                    for (x, output) in row.iter_mut().enumerate() {
                        let x = x as u16;
                        let tile_x = minimap_tile(tiles_per_side, samples, x)
                            .ok_or(MinimapError::OutsideMap)?;
                        let point = generator
                            .landscape_surface_with_cancel(
                                TileCoord::new(tile_x, tile_y),
                                cancelled,
                            )
                            .map_err(MinimapError::Page)?
                            .ok_or(MinimapError::OutsideMap)?;
                        let mut class = classify(&point);
                        // A cited land correction is authoritative at this raster sample.
                        // Ordinary dry sample centres still discover subpixel rivers.
                        let corrected_land = point.tile.terrain.water == WaterKind::None
                            && point.tile.terrain.water_provenance
                                == aoe_map::Provenance::HistoricallyCorrected;
                        let footprint = (tiles_per_side.div_ceil(u64::from(samples)) * 181)
                            .div_ceil(256) as i32;
                        let tile = TileCoord::new(tile_x, tile_y);
                        if class == MinimapClass::River && geometry {
                            // Physical water counts only where a drawn river is nearby.
                            // Rivers without source geometry keep their tile width.
                            if !generator
                                .river_within_with_cancel(
                                    tile,
                                    footprint + MINIMAP_WARP_MARGIN_TILES,
                                    MINIMAP_MIN_RIVER_WIDTH_UNITS,
                                    cancelled,
                                )
                                .map_err(MinimapError::Page)?
                            {
                                class = surrounding_land_class(
                                    &generator,
                                    tile,
                                    footprint + MINIMAP_WARP_MARGIN_TILES + 2,
                                    cancelled,
                                )?;
                            }
                        } else if geometry
                            && !corrected_land
                            && !matches!(
                                class,
                                MinimapClass::Ocean | MinimapClass::Lake | MinimapClass::River
                            )
                            && generator
                                .river_within_with_cancel(
                                    tile,
                                    footprint,
                                    MINIMAP_MIN_RIVER_WIDTH_UNITS,
                                    cancelled,
                                )
                                .map_err(MinimapError::Page)?
                        {
                            class = MinimapClass::River;
                        }
                        *output = (class, point.tile.terrain.geographic_height_centimeters);
                    }
                }
                Ok(())
            })
    })?;
    let (classes, heights): (Vec<_>, Vec<_>) = points.into_iter().unzip();
    let spacing_millimeters = (u128::from(tiles_per_side)
        * u128::from(geographic_millimeters_per_tile.max(1))
        / u128::from(samples))
    .clamp(1, i64::MAX as u128) as i64;
    let shades = relief::shades(
        &heights,
        side,
        spacing_millimeters,
        relief::radius(samples, tiles_per_side, elevation_samples_per_axis),
    );
    drop(heights);
    let mut raster =
        Vec::with_capacity((MINIMAP_HEADER_BYTES + side * side).min(MAX_MINIMAP_BYTES));
    raster.extend_from_slice(&MINIMAP_MAGIC);
    raster.extend_from_slice(&samples.to_le_bytes());
    raster.extend_from_slice(&[0, 0]);
    for y in 0..side {
        for x in 0..side {
            let class = classes[y * side + x];
            let shade = if matches!(
                class,
                MinimapClass::Shore
                    | MinimapClass::River
                    | MinimapClass::Lake
                    | MinimapClass::Ocean
            ) {
                FLAT_SHADE
            } else {
                shades[y * side + x]
            };
            raster.push(class as u8 | shade << 4);
        }
    }
    Ok(raster)
}

pub(crate) fn package_minimap(
    package: &MapPackage,
    provider: Option<Arc<crate::map_store::PageResidency>>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    package_raster(
        package,
        provider,
        minimap_samples(package.estimate.tiles_per_side),
        cancelled,
    )
}

fn package_raster(
    package: &MapPackage,
    provider: Option<Arc<crate::map_store::PageResidency>>,
    samples: u16,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    let generator = if package.environment.samples_per_axis == 0 {
        package.generator()
    } else {
        package
            .generator_with_page_provider(
                provider.ok_or(MinimapError::Page(EnvironmentPageError::Missing))?,
            )
            .map_err(|_| MinimapError::Page(EnvironmentPageError::Invalid))?
    };
    minimap_raster_at_samples(
        &generator,
        package.estimate.tiles_per_side,
        package.estimate.geographic_millimeters_per_tile,
        package.environment.samples_per_axis,
        samples,
        cancelled,
    )
}

#[path = "minimap_http.rs"]
mod http;
#[cfg(test)]
pub(in crate::maps) use http::finish;
pub(in crate::maps) use http::{coarse_minimap, minimap};
