use crate::biome::{PreparedBiome, level_zero_biome_pages};
use crate::biome_rules::{Biome, biome_from_potential_class, material_for, resource_modulus};
use crate::water::{PreparedWater, level_zero_water_pages};
use crate::{
    CHUNK_TILES, ELEVATION_LEVEL_CENTIMETERS, ElevationPage, EnvironmentError, PotentialBiomePage,
    PreparedEnvironment, Ratio, WaterPage,
};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

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
    pub material: GroundMaterial,
    pub biome: Biome,
    pub vegetation_provenance: Provenance,
    pub water: WaterKind,
    pub elevation_provenance: Provenance,
    pub water_provenance: Provenance,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapChunkGenerator {
    geography_key: [u8; 32],
    procedural_seed: u64,
    width_tiles: i32,
    elevation: Option<Arc<PreparedElevation>>,
    water: Option<Arc<PreparedWater>>,
    biome: Option<Arc<PreparedBiome>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedElevation {
    samples_per_axis: u16,
    compression: Ratio,
    pages: BTreeMap<(u16, u16), ElevationPage>,
}

impl MapChunkGenerator {
    pub const fn new(geography_key: [u8; 32], procedural_seed: u64, width_tiles: i32) -> Self {
        Self {
            geography_key,
            procedural_seed,
            width_tiles,
            elevation: None,
            water: None,
            biome: None,
        }
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
        let level_zero = crate::environment::level_zero_pages(environment, pages)?;
        Ok(Self {
            geography_key: self.geography_key,
            procedural_seed: self.procedural_seed,
            width_tiles: self.width_tiles,
            elevation: Some(Arc::new(PreparedElevation {
                samples_per_axis: environment.samples_per_axis,
                compression,
                pages: level_zero,
            })),
            water: self.water,
            biome: self.biome,
        })
    }

    /// Binds independently prepared water coverage to a terrain query. A
    /// missing water pyramid is valid only when no pages were supplied.
    pub fn with_prepared_water(
        self,
        environment: &PreparedEnvironment,
        pages: Vec<WaterPage>,
    ) -> Result<Self, EnvironmentError> {
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
        })
    }

    /// Binds source potential-biome classes to a terrain query. Zero and
    /// unrecognized source classes deliberately retain the procedural fallback.
    pub fn with_prepared_biomes(
        self,
        environment: &PreparedEnvironment,
        pages: Vec<PotentialBiomePage>,
    ) -> Result<Self, EnvironmentError> {
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
        })
    }

    pub fn tile_at(&self, tile: TileCoord) -> Option<Tile> {
        (tile.x >= 0 && tile.y >= 0 && tile.x < self.width_tiles && tile.y < self.width_tiles)
            .then(|| self.sample_tile(tile))
    }

    pub fn chunk(&self, x: i32, y: i32) -> Chunk {
        let mut tiles = Vec::with_capacity((CHUNK_TILES * CHUNK_TILES) as usize);
        let mut resources = Vec::new();
        for local_y in 0..CHUNK_TILES {
            for local_x in 0..CHUNK_TILES {
                let tile = TileCoord::new(x * CHUNK_TILES + local_x, y * CHUNK_TILES + local_y);
                let Some(sample) = self.tile_at(tile) else {
                    continue;
                };
                tiles.push(sample);
                if let Some(node) = self.resource_at(tile, sample) {
                    resources.push(node);
                }
            }
        }
        Chunk {
            x,
            y,
            tiles,
            resources,
        }
    }

    pub fn object_at(&self, tile: TileCoord) -> Option<ResourceNode> {
        self.tile_at(tile)
            .and_then(|sample| self.resource_at(tile, sample))
    }

    pub fn resource_by_id(&self, id: u64) -> Option<ResourceNode> {
        if id & 1 != 0 {
            return None;
        }
        let tile = TileCoord::new(((id >> 1) & 0x3_ffff) as i32, (id >> 19) as i32);
        self.object_at(tile).filter(|node| node.id == id)
    }

    fn sample_tile(&self, tile: TileCoord) -> Tile {
        let broad = signed_noise(
            self.geography_key,
            b"relief",
            tile.x.div_euclid(8),
            tile.y.div_euclid(8),
        );
        let local = signed_noise(self.geography_key, b"relief-detail", tile.x, tile.y) / 8;
        let fallback_height = broad.saturating_mul(25).saturating_add(local);
        let fallback_water = if unsigned_noise(
            self.geography_key,
            b"water",
            tile.x.div_euclid(16),
            tile.y.div_euclid(16),
        )
        .is_multiple_of(97)
        {
            WaterKind::Lake
        } else if unsigned_noise(
            self.geography_key,
            b"river",
            tile.x.div_euclid(4),
            tile.y.div_euclid(4),
        )
        .is_multiple_of(521)
        {
            WaterKind::River
        } else {
            WaterKind::None
        };
        let fallback_biome = match unsigned_noise(
            self.geography_key,
            b"biome",
            tile.x.div_euclid(32),
            tile.y.div_euclid(32),
        ) % 10
        {
            0 => Biome::Tropical,
            1 => Biome::Boreal,
            2 => Biome::Woodland,
            3 => Biome::Savanna,
            4 => Biome::Steppe,
            5 => Biome::Desert,
            6 => Biome::Tundra,
            7 => Biome::Alpine,
            8 => Biome::Polar,
            _ => Biome::Temperate,
        };
        let (biome, vegetation_provenance) = self
            .biome
            .as_ref()
            .and_then(|biome| biome.class_at(tile, self.width_tiles))
            .and_then(biome_from_potential_class)
            .map(|biome| (biome, Provenance::SourceDerived))
            .unwrap_or((fallback_biome, Provenance::Fallback));
        let (
            geographic_height_centimeters,
            game_height_level,
            elevation_provenance,
            water,
            water_provenance,
        ) = self
            .elevation
            .as_ref()
            .and_then(|elevation| {
                elevation
                    .height_at(tile, self.width_tiles)
                    .map(|height| (height, elevation.compression))
            })
            .map(|(height, compression)| {
                let game_height = i64::from(height)
                    .saturating_mul(i64::from(compression.denominator))
                    / (i64::from(ELEVATION_LEVEL_CENTIMETERS) * i64::from(compression.numerator));
                (
                    height,
                    game_height.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
                    Provenance::SourceDerived,
                    // Elevation cannot identify water: inland depressions can be dry,
                    // while coastlines require independent, coherent water geometry.
                    fallback_water,
                    Provenance::Fallback,
                )
            })
            .unwrap_or((
                fallback_height,
                (fallback_height / ELEVATION_LEVEL_CENTIMETERS)
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                Provenance::Fallback,
                fallback_water,
                Provenance::Fallback,
            ));
        let (water, water_provenance) = self
            .water
            .as_ref()
            .and_then(|water| water.coverage_at(tile, self.width_tiles))
            .map(|coverage| match coverage {
                0 => (fallback_water, Provenance::Fallback),
                1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
                _ => (WaterKind::Ocean, Provenance::SourceDerived),
            })
            .unwrap_or((water, water_provenance));
        let material = match water {
            WaterKind::None => material_for(biome, geographic_height_centimeters),
            WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
            WaterKind::Shallow => GroundMaterial::Shore,
        };
        Tile {
            geographic_height_centimeters,
            game_height_level,
            material,
            biome,
            vegetation_provenance,
            water,
            elevation_provenance,
            water_provenance,
            passable: water == WaterKind::None && material != GroundMaterial::Ice,
        }
    }

    fn resource_at(&self, tile: TileCoord, sample: Tile) -> Option<ResourceNode> {
        if !sample.passable {
            return None;
        }
        let value = unsigned_noise(self.geography_key, b"objects", tile.x, tile.y)
            ^ self.procedural_seed.rotate_left(17);
        let (kind, object, amount) = match value % resource_modulus(sample.biome) {
            0 => (ResourceKind::Wood, ObjectKind::Tree, 100),
            1 if value.is_multiple_of(257) => (ResourceKind::Food, ObjectKind::ForageBush, 125),
            2 if value.is_multiple_of(521) => (ResourceKind::Gold, ObjectKind::GoldDeposit, 800),
            3 if value % 521 == 1 => (ResourceKind::Stone, ObjectKind::StoneDeposit, 350),
            _ => return None,
        };
        Some(ResourceNode {
            id: resource_id(tile, 0),
            tile,
            kind,
            object,
            initial_amount: amount,
            visual_variant: (value >> 8) as u8,
        })
    }
}

impl PreparedElevation {
    fn height_at(&self, tile: TileCoord, width_tiles: i32) -> Option<i32> {
        let tile_axis = u64::try_from(width_tiles.checked_sub(1)?).ok()?;
        let source_axis = u64::from(self.samples_per_axis.checked_sub(1)?);
        let x =
            u16::try_from((u64::try_from(tile.x).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let y =
            u16::try_from((u64::try_from(tile.y).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let page_size = u16::from(crate::ENVIRONMENT_PAGE_SAMPLES);
        let page = self.pages.get(&(x / page_size, y / page_size))?;
        let local_x = usize::from(x % page_size);
        let local_y = usize::from(y % page_size);
        (local_x < usize::from(page.width) && local_y < usize::from(page.height)).then(|| {
            page.geographic_height_centimeters[local_y * usize::from(page.width) + local_x]
        })
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
mod tests {
    use super::*;

    fn generator(seed: u64) -> MapChunkGenerator {
        MapChunkGenerator::new([3; 32], seed, 128)
    }

    #[test]
    fn chunks_are_order_independent_and_have_stable_shared_tiles() {
        let first = generator(1).chunk(0, 0);
        let second = generator(1).chunk(0, 0);
        assert_eq!(first, second);
        assert_eq!(
            generator(1).tile_at(TileCoord::new(31, 5)),
            generator(1).chunk(0, 0).tiles.get(5 * 32 + 31).copied()
        );
    }

    #[test]
    fn procedural_seed_changes_objects_without_moving_relief() {
        let tile = TileCoord::new(13, 8);
        assert_eq!(
            generator(1)
                .tile_at(tile)
                .expect("tile")
                .geographic_height_centimeters,
            generator(2)
                .tile_at(tile)
                .expect("tile")
                .geographic_height_centimeters
        );
        let first = (0..4)
            .flat_map(|y| (0..4).flat_map(move |x| generator(1).chunk(x, y).resources))
            .collect::<Vec<_>>();
        let second = (0..4)
            .flat_map(|y| (0..4).flat_map(move |x| generator(2).chunk(x, y).resources))
            .collect::<Vec<_>>();
        assert_ne!(first, second);
    }

    #[test]
    fn resources_have_one_collision_free_slot_per_tile() {
        let chunk = generator(1).chunk(0, 0);
        let mut ids = chunk
            .resources
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), chunk.resources.len());
    }
}
