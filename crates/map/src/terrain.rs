use crate::{CHUNK_TILES, ELEVATION_LEVEL_CENTIMETERS};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};

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
pub enum Biome {
    Temperate,
    Boreal,
    Tropical,
    Woodland,
    Savanna,
    Steppe,
    Desert,
    Tundra,
    Alpine,
    Polar,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapChunkGenerator {
    geography_key: [u8; 32],
    procedural_seed: u64,
    width_tiles: i32,
}

impl MapChunkGenerator {
    pub const fn new(geography_key: [u8; 32], procedural_seed: u64, width_tiles: i32) -> Self {
        Self {
            geography_key,
            procedural_seed,
            width_tiles,
        }
    }

    pub fn tile_at(self, tile: TileCoord) -> Option<Tile> {
        (tile.x >= 0 && tile.y >= 0 && tile.x < self.width_tiles && tile.y < self.width_tiles)
            .then(|| self.sample_tile(tile))
    }

    pub fn chunk(self, x: i32, y: i32) -> Chunk {
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

    fn sample_tile(self, tile: TileCoord) -> Tile {
        let broad = signed_noise(
            self.geography_key,
            b"relief",
            tile.x.div_euclid(8),
            tile.y.div_euclid(8),
        );
        let local = signed_noise(self.geography_key, b"relief-detail", tile.x, tile.y) / 8;
        let geographic_height_centimeters = broad.saturating_mul(25).saturating_add(local);
        let water = if unsigned_noise(
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
        let biome = match unsigned_noise(
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
        let material = match water {
            WaterKind::None => material_for(biome, geographic_height_centimeters),
            WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
            WaterKind::Shallow => GroundMaterial::Shore,
        };
        Tile {
            geographic_height_centimeters,
            game_height_level: (geographic_height_centimeters / ELEVATION_LEVEL_CENTIMETERS)
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                as i16,
            material,
            biome,
            water,
            elevation_provenance: Provenance::Fallback,
            water_provenance: Provenance::Fallback,
            passable: water == WaterKind::None && material != GroundMaterial::Ice,
        }
    }

    fn resource_at(self, tile: TileCoord, sample: Tile) -> Option<ResourceNode> {
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

fn material_for(biome: Biome, height: i32) -> GroundMaterial {
    if height > 3_500 {
        return GroundMaterial::Rock;
    }
    match biome {
        Biome::Tropical => GroundMaterial::LushGrass,
        Biome::Boreal | Biome::Tundra | Biome::Polar => GroundMaterial::Snow,
        Biome::Woodland => GroundMaterial::ForestFloor,
        Biome::Savanna | Biome::Steppe => GroundMaterial::DryGrass,
        Biome::Desert => GroundMaterial::Sand,
        Biome::Alpine => GroundMaterial::Rock,
        Biome::Temperate => GroundMaterial::TemperateGrass,
    }
}

fn resource_modulus(biome: Biome) -> u64 {
    match biome {
        Biome::Tropical | Biome::Temperate | Biome::Boreal => 3,
        Biome::Woodland => 6,
        Biome::Savanna => 12,
        Biome::Steppe => 96,
        Biome::Desert | Biome::Tundra | Biome::Alpine | Biome::Polar => u64::MAX,
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
