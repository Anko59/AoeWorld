use crate::biome::{PreparedBiome, level_zero_biome_pages};
use crate::biome_rules::Biome;
use crate::land_use::{HistoricalLandUse, level_zero_land_use_pages};
use crate::water::{PreparedWater, level_zero_water_pages};
use crate::{
    CHUNK_TILES, ELEVATION_LEVEL_CENTIMETERS, ElevationPage, EnvironmentError,
    EnvironmentPageError, EnvironmentPageProvider, HistoricalLandUsePage, HydrologyObservation,
    PotentialBiomePage, PreparedEnvironment, Ratio, WaterPage,
};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod clearing;
mod elevation;
mod fallback;
mod provider;
mod resources;
mod surface;
use elevation::PreparedElevation;
pub use surface::{EdgePassability, SurfaceDiagonal, SurfaceKind, TileSurface};

fn validate_vector_environment(environment: &PreparedEnvironment) -> Result<(), EnvironmentError> {
    if environment.hydrology_evidence.is_some() {
        return Err(EnvironmentError::InvalidIndex);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GroundMaterial {
    TemperateGrass,
    DryGrass,
    LushGrass,
    ForestFloor,
    Dirt,
    Sand,
    Rock,
    Mud,
    Snow,
    Ice,
    Shore,
    Water,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WaterKind {
    None,
    Shallow,
    Lake,
    River,
    Ocean,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Provenance {
    SourceDerived,
    ModelDerived,
    Procedural,
    Fallback,
    HistoricallyCorrected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResourceKind {
    Food,
    Wood,
    Gold,
    Stone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ObjectKind {
    Tree,
    ForageBush,
    GoldDeposit,
    StoneDeposit,
    Decoration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub geographic_height_centimeters: i32,
    pub game_height_level: i16,
    pub surface: TileSurface,
    pub material: GroundMaterial,
    pub biome: Biome,
    pub vegetation_provenance: Provenance,
    pub water: WaterKind,
    pub elevation_provenance: Provenance,
    pub water_provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hydrology_observation: Option<HydrologyObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modern_land_cover_class: Option<u8>,
    pub passable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceNode {
    pub id: u64,
    pub tile: TileCoord,
    pub kind: ResourceKind,
    pub object: ObjectKind,
    pub initial_amount: u16,
    pub visual_variant: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub x: i32,
    pub y: i32,
    pub tiles: Vec<Tile>,
    pub resources: Vec<ResourceNode>,
}

#[derive(Clone, Debug)]
pub struct MapChunkGenerator {
    geography_key: [u8; 32],
    procedural_seed: u64,
    width_tiles: i32,
    elevation: Option<Arc<PreparedElevation>>,
    water: Option<Arc<PreparedWater>>,
    biome: Option<Arc<PreparedBiome>>,
    historical_land_use: Option<Arc<HistoricalLandUse>>,
    provider: Option<Arc<dyn EnvironmentPageProvider>>,
    provider_environment: Option<Arc<PreparedEnvironment>>,
    provider_compression: Option<Ratio>,
    elevation_sampling_recipe: u16,
}

impl MapChunkGenerator {
    pub fn new(geography_key: [u8; 32], procedural_seed: u64, width_tiles: i32) -> Self {
        Self {
            geography_key,
            procedural_seed,
            width_tiles,
            elevation: None,
            water: None,
            biome: None,
            historical_land_use: None,
            provider: None,
            provider_environment: None,
            provider_compression: None,
            elevation_sampling_recipe: crate::GENERATION_RECIPE_VERSION,
        }
    }

    pub(crate) fn with_elevation_sampling_recipe(mut self, recipe: u16) -> Self {
        self.elevation_sampling_recipe = recipe;
        self
    }

    pub(crate) const fn generation_recipe_version(&self) -> u16 {
        self.elevation_sampling_recipe
    }

    /// Binds an immutable page provider without retaining a complete page
    /// vector. The provider owns source access and residency; this generator
    /// retains only package metadata and deterministic terrain inputs.
    pub fn with_page_provider(
        mut self,
        compression: Ratio,
        environment: PreparedEnvironment,
        provider: Arc<dyn EnvironmentPageProvider>,
    ) -> Result<Self, EnvironmentError> {
        environment.validate()?;
        if environment.samples_per_axis == 0 {
            return Err(EnvironmentError::InvalidPyramid);
        }
        self.provider = Some(provider);
        self.provider_environment = Some(Arc::new(environment));
        self.provider_compression = Some(compression);
        Ok(self)
    }

    /// Binds verified level-zero elevation pages to this otherwise pure terrain
    /// query. Callers retain page loading in an adapter and pass only the
    /// complete page set for the immutable package.
    pub fn with_prepared_elevation(
        self,
        compression: Ratio,
        environment: &PreparedEnvironment,
        pages: Vec<ElevationPage>,
    ) -> Result<Self, EnvironmentError> {
        validate_vector_environment(environment)?;
        let level_zero = crate::environment::level_zero_pages(environment, pages)?;
        Ok(Self {
            geography_key: self.geography_key,
            procedural_seed: self.procedural_seed,
            width_tiles: self.width_tiles,
            elevation: Some(Arc::new(PreparedElevation {
                samples_per_axis: environment.samples_per_axis,
                compression,
                pages: level_zero,
                sampling_recipe: self.elevation_sampling_recipe,
            })),
            water: self.water,
            biome: self.biome,
            historical_land_use: self.historical_land_use,
            provider: self.provider,
            provider_environment: self.provider_environment,
            provider_compression: self.provider_compression,
            elevation_sampling_recipe: self.elevation_sampling_recipe,
        })
    }

    /// Binds independently prepared water coverage to a terrain query. A
    /// missing water pyramid is valid only when no pages were supplied.
    pub fn with_prepared_water(
        self,
        environment: &PreparedEnvironment,
        pages: Vec<WaterPage>,
    ) -> Result<Self, EnvironmentError> {
        validate_vector_environment(environment)?;
        let Some(field) = &environment.water else {
            return pages
                .is_empty()
                .then_some(self)
                .ok_or(EnvironmentError::InvalidPyramid);
        };
        let level_zero = level_zero_water_pages(field, pages)?;
        Ok(Self {
            geography_key: self.geography_key,
            procedural_seed: self.procedural_seed,
            width_tiles: self.width_tiles,
            elevation: self.elevation,
            water: Some(Arc::new(PreparedWater::new(
                environment.samples_per_axis,
                level_zero,
            ))),
            biome: self.biome,
            historical_land_use: self.historical_land_use,
            provider: self.provider,
            provider_environment: self.provider_environment,
            provider_compression: self.provider_compression,
            elevation_sampling_recipe: self.elevation_sampling_recipe,
        })
    }

    /// Binds source potential-biome classes to a terrain query. Zero and
    /// unrecognized source classes deliberately retain the procedural fallback.
    pub fn with_prepared_biomes(
        self,
        environment: &PreparedEnvironment,
        pages: Vec<PotentialBiomePage>,
    ) -> Result<Self, EnvironmentError> {
        validate_vector_environment(environment)?;
        let Some(field) = &environment.vegetation else {
            return pages
                .is_empty()
                .then_some(self)
                .ok_or(EnvironmentError::InvalidPyramid);
        };
        let level_zero = level_zero_biome_pages(field, pages)?;
        Ok(Self {
            geography_key: self.geography_key,
            procedural_seed: self.procedural_seed,
            width_tiles: self.width_tiles,
            elevation: self.elevation,
            water: self.water,
            biome: Some(Arc::new(PreparedBiome::new(
                environment.samples_per_axis,
                level_zero,
            ))),
            historical_land_use: self.historical_land_use,
            provider: self.provider,
            provider_environment: self.provider_environment,
            provider_compression: self.provider_compression,
            elevation_sampling_recipe: self.elevation_sampling_recipe,
        })
    }

    /// Binds verified HYDE 600 AD land-use fields to the terrain query.
    pub fn with_historical_land_use(
        self,
        environment: &PreparedEnvironment,
        pages: Vec<HistoricalLandUsePage>,
    ) -> Result<Self, EnvironmentError> {
        validate_vector_environment(environment)?;
        let Some(field) = &environment.historical_land_use else {
            return pages
                .is_empty()
                .then_some(self)
                .ok_or(EnvironmentError::InvalidPyramid);
        };
        let level_zero = level_zero_land_use_pages(field, pages)?;
        Ok(Self {
            geography_key: self.geography_key,
            procedural_seed: self.procedural_seed,
            width_tiles: self.width_tiles,
            elevation: self.elevation,
            water: self.water,
            biome: self.biome,
            historical_land_use: Some(Arc::new(HistoricalLandUse::new(
                environment
                    .historical_samples_per_axis()
                    .ok_or(EnvironmentError::InvalidPyramid)?,
                level_zero,
            ))),
            provider: self.provider,
            provider_environment: self.provider_environment,
            provider_compression: self.provider_compression,
            elevation_sampling_recipe: self.elevation_sampling_recipe,
        })
    }

    pub fn tile_at(&self, tile: TileCoord) -> Option<Tile> {
        if self.provider.is_some() {
            return self.tile_at_with_cancel(tile, &|| false).ok().flatten();
        }
        (tile.x >= 0 && tile.y >= 0 && tile.x < self.width_tiles && tile.y < self.width_tiles)
            .then(|| self.sample_tile(tile))
    }

    /// Fallible source-backed tile query. A provider failure is never mapped
    /// to procedural terrain or an absent successful tile.
    pub fn tile_at_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Tile>, EnvironmentPageError> {
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width_tiles || tile.y >= self.width_tiles {
            return Ok(None);
        }
        if self.provider.is_some() {
            provider::sample_tile(self, tile, cancelled).map(Some)
        } else {
            Ok(Some(self.sample_tile(tile)))
        }
    }

    pub fn chunk(&self, x: i32, y: i32) -> Result<Chunk, EnvironmentPageError> {
        self.chunk_with_cancel(x, y, &|| false)
    }

    /// Fallible bounded chunk query. At most one returned chunk and the page
    /// handles needed for its tiles are retained by the provider cache.
    pub fn chunk_with_cancel(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Chunk, EnvironmentPageError> {
        let mut tiles = Vec::with_capacity((CHUNK_TILES * CHUNK_TILES) as usize);
        let mut resources = Vec::new();
        for local_y in 0..CHUNK_TILES {
            for local_x in 0..CHUNK_TILES {
                let tile = TileCoord::new(x * CHUNK_TILES + local_x, y * CHUNK_TILES + local_y);
                let Some(sample) = self.tile_at_with_cancel(tile, cancelled)? else {
                    continue;
                };
                tiles.push(sample);
                let node = if self.provider.is_some() {
                    provider::resource_at(self, tile, sample, cancelled)?
                } else {
                    self.resource_at(tile, sample)
                };
                if let Some(node) = node {
                    resources.push(node);
                }
            }
        }
        Ok(Chunk {
            x,
            y,
            tiles,
            resources,
        })
    }

    pub fn object_at(&self, tile: TileCoord) -> Option<ResourceNode> {
        if self.provider.is_some() {
            return self.object_at_with_cancel(tile, &|| false).ok().flatten();
        }
        self.tile_at(tile)
            .and_then(|sample| self.resource_at(tile, sample))
    }

    pub fn object_at_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        let Some(sample) = self.tile_at_with_cancel(tile, cancelled)? else {
            return Ok(None);
        };
        if self.provider.is_some() {
            provider::resource_at(self, tile, sample, cancelled)
        } else {
            Ok(self.resource_at(tile, sample))
        }
    }

    /// Applies the deterministic crop/grazing roll used to clear procedural
    /// vegetation from tiles with historical land-use evidence.
    pub fn is_tree_suppressed_by_historical_land_use(
        &self,
        tile: TileCoord,
        crop_percent: u8,
        grazing_percent: u8,
    ) -> bool {
        let value = unsigned_noise(self.geography_key, b"objects", tile.x, tile.y)
            ^ self.procedural_seed.rotate_left(17);
        value % 100 < u64::from(crop_percent) + u64::from(grazing_percent)
    }

    pub fn resource_by_id(&self, id: u64) -> Option<ResourceNode> {
        self.resource_by_id_with_cancel(id, &|| false)
            .ok()
            .flatten()
    }

    pub fn resource_by_id_with_cancel(
        &self,
        id: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if id & 1 != 0 || id >> 37 != 0 {
            return Ok(None);
        }
        let tile = TileCoord::new(((id >> 1) & 0x3_ffff) as i32, (id >> 19) as i32);
        Ok(self
            .object_at_with_cancel(tile, cancelled)?
            .filter(|node| node.id == id))
    }
}

fn quantize_game_height(height: i32, compression: Ratio) -> i16 {
    let game_height = i64::from(height).saturating_mul(i64::from(compression.denominator))
        / (i64::from(ELEVATION_LEVEL_CENTIMETERS) * i64::from(compression.numerator));
    game_height.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

const fn compression_fallback() -> Ratio {
    Ratio {
        numerator: 1,
        denominator: 1,
    }
}

fn resource_id(tile: TileCoord, slot: u8) -> u64 {
    ((tile.y as u64) << 19) | ((tile.x as u64) << 1) | u64::from(slot)
}

fn unsigned_noise(key: [u8; 32], domain: &[u8], x: i32, y: i32) -> u64 {
    let mut hash = blake3::Hasher::new_keyed(&key);
    hash.update(domain);
    hash.update(&x.to_le_bytes());
    hash.update(&y.to_le_bytes());
    u64::from_le_bytes(hash.finalize().as_bytes()[..8].try_into().unwrap_or([0; 8]))
}

fn signed_noise(key: [u8; 32], domain: &[u8], x: i32, y: i32) -> i32 {
    (unsigned_noise(key, domain, x, y) % 4_001) as i32 - 2_000
}

#[cfg(test)]
mod tests;
