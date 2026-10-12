//! One bounded strategic raster per map package, sampled from the same
//! composed landscape query that produces terrain chunks. It is presentation
//! data for the minimap and whole-map overview, never a gameplay input.
use super::{page_residency, request_cancellation};
use crate::AppState;
use aoe_core::TileCoord;
use aoe_map::{
    EnvironmentPageError, GroundMaterial, LandscapePoint, MapChunkGenerator, MapPackage, WaterKind,
};
use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::{
    collections::VecDeque,
    sync::{Arc, atomic::Ordering},
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
pub(crate) const MAX_CACHED_MINIMAPS: usize = 4;
/// Wall-clock budget for one raster. An exhausted budget fails the request
/// instead of returning a partial raster; nothing partial is cached.
pub(crate) const MINIMAP_TIME_BUDGET: Duration = Duration::from_secs(30);
/// Shade nibble of level ground; 0 is fully shaded and 15 fully lit.
pub(crate) const FLAT_SHADE: u8 = 8;
/// Summed per-axis rise, in per-mille, that changes the shade by one step.
const SHADE_STEP_PER_MILLE: i64 = 25;

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

/// Deterministic raster of the composed landscape: `MINIMAP_HEADER_BYTES`
/// then row-major bytes, class in the low nibble and relief shade in the high
/// nibble. Performs exactly `samples²` point queries and allocates at most
/// `MAX_MINIMAP_BYTES` plus four bytes of height per sample.
pub(crate) fn minimap_raster(
    generator: &MapChunkGenerator,
    tiles_per_side: u64,
    geographic_millimeters_per_tile: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, MinimapError> {
    let samples = minimap_samples(tiles_per_side);
    if samples == 0 {
        return Err(MinimapError::EmptyMap);
    }
    let side = usize::from(samples);
    let mut classes = Vec::with_capacity(side * side);
    let mut heights = Vec::with_capacity(side * side);
    for y in 0..samples {
        let tile_y = minimap_tile(tiles_per_side, samples, y).ok_or(MinimapError::OutsideMap)?;
        for x in 0..samples {
            let tile_x =
                minimap_tile(tiles_per_side, samples, x).ok_or(MinimapError::OutsideMap)?;
            let point = generator
                .landscape_point_with_cancel(TileCoord::new(tile_x, tile_y), cancelled)
                .map_err(MinimapError::Page)?
                .ok_or(MinimapError::OutsideMap)?;
            let mut class = classify(&point);
            if !matches!(
                class,
                MinimapClass::Ocean | MinimapClass::Lake | MinimapClass::River
            ) && generator
                .river_within_with_cancel(
                    TileCoord::new(tile_x, tile_y),
                    (tiles_per_side.div_ceil(u64::from(samples)) * 181).div_ceil(256) as i32,
                    cancelled,
                )
                .map_err(MinimapError::Page)?
            {
                class = MinimapClass::River;
            }
            classes.push(class);
            heights.push(point.tile.terrain.geographic_height_centimeters);
        }
    }
    let spacing_millimeters = (u128::from(tiles_per_side)
        * u128::from(geographic_millimeters_per_tile.max(1))
        / u128::from(samples))
    .clamp(1, i64::MAX as u128) as i64;
    let mut raster =
        Vec::with_capacity((MINIMAP_HEADER_BYTES + side * side).min(MAX_MINIMAP_BYTES));
    raster.extend_from_slice(&MINIMAP_MAGIC);
    raster.extend_from_slice(&samples.to_le_bytes());
    raster.extend_from_slice(&[0, 0]);
    for y in 0..side {
        for x in 0..side {
            let class = classes[y * side + x];
            let shade = if class as u8 >= MinimapClass::Shore as u8 {
                FLAT_SHADE
            } else {
                shade(&heights, side, x, y, spacing_millimeters)
            };
            raster.push(class as u8 | shade << 4);
        }
    }
    Ok(raster)
}

/// Light from the north-west: ground rising toward the south-east is lit.
fn shade(heights: &[i32], side: usize, x: usize, y: usize, spacing_millimeters: i64) -> u8 {
    let axis = |before: usize, after: usize, low: usize, high: usize| {
        if high == low {
            return 0;
        }
        (i64::from(heights[after]) - i64::from(heights[before])) / (high - low) as i64
    };
    let (left, right) = (x.saturating_sub(1), (x + 1).min(side - 1));
    let (up, down) = (y.saturating_sub(1), (y + 1).min(side - 1));
    let rise_centimeters = axis(y * side + left, y * side + right, left, right)
        + axis(up * side + x, down * side + x, up, down);
    let per_mille = rise_centimeters.saturating_mul(10_000) / spacing_millimeters;
    (i64::from(FLAT_SHADE) + per_mille / SHADE_STEP_PER_MILLE).clamp(0, 15) as u8
}

pub(crate) fn package_minimap(
    package: &MapPackage,
    provider: Option<Arc<crate::map_store::PageResidency>>,
    cancelled: &dyn Fn() -> bool,
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
    minimap_raster(
        &generator,
        package.estimate.tiles_per_side,
        package.estimate.geographic_millimeters_per_tile,
        cancelled,
    )
}

/// Serves the cached raster, computing it once per package. The cache lock
/// is held while computing, so at most one raster is generated at a time and
/// concurrent requests for the same map reuse the first result.
pub(in crate::maps) async fn minimap(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut cache = state.minimaps.lock().await;
    let raster = if let Some(raster) = cache.get(&content_hash) {
        raster
    } else {
        let (_cancellation_guard, cancelled) = request_cancellation();
        let provider = page_residency(&state, &package, cancelled.clone())
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;
        // The deadline is enforced here rather than in the sampling loop:
        // returning drops the guard, which cancels the blocking task.
        let task = tokio::task::spawn_blocking(move || {
            package_minimap(&package, provider, &|| cancelled.load(Ordering::Acquire))
        });
        let raster: Arc<[u8]> = tokio::time::timeout(MINIMAP_TIME_BUDGET, task)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map_err(|error| match error {
                MinimapError::Page(EnvironmentPageError::Cancelled) => {
                    StatusCode::SERVICE_UNAVAILABLE
                }
                _ => StatusCode::NOT_FOUND,
            })?
            .into();
        cache.insert(content_hash, raster.clone());
        raster
    };
    drop(cache);
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        raster.to_vec(),
    )
        .into_response())
}
