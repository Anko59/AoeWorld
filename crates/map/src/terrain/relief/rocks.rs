//! Loose rocks: purely visual stones on bare rock, as scree beside cliffs, a
//! few on snow and in dry grassland. They never block and are no resource.
//! Density follows a continuous world-anchored cluster field, so rocks lie in
//! loose groups with empty ground between them.
use super::{relief_key, smooth};
use crate::{Biome, GroundMaterial, MapChunkGenerator};
use aoe_core::TileCoord;

/// Highest chance of a rock on one tile, in thousandths (dense scree).
pub(crate) const MAX_ROCK_PER_THOUSAND: u64 = 280;
/// Offsets that count as "beside a cliff": the scree apron below and around.
pub(crate) const SCREE_REACH: [(i32, i32); 8] = [
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (2, 2),
    (-2, 2),
    (2, -2),
    (-2, -2),
];

impl MapChunkGenerator {
    /// Chance in thousandths that an open tile carries a loose rock.
    pub(crate) fn rock_per_thousand(
        &self,
        tile: TileCoord,
        material: GroundMaterial,
        biome: Biome,
        beside_cliff: bool,
    ) -> u64 {
        let base = if beside_cliff {
            140
        } else {
            match (material, biome) {
                (GroundMaterial::Rock, _) => 45,
                (GroundMaterial::Snow, _) => 8,
                (GroundMaterial::DryGrass, Biome::Steppe | Biome::Savanna | Biome::Woodland) => 10,
                (GroundMaterial::Sand, Biome::Desert) => 6,
                _ => return 0,
            }
        };
        // 0 in the open half of the field, up to twice the base in a cluster.
        let field = smooth(
            relief_key(self, 0x726f_636b_7300 ^ self.procedural_seed),
            i64::from(tile.x),
            i64::from(tile.y),
            14,
        );
        let weight = (field - if beside_cliff { 64 } else { 120 }).clamp(0, 64) as u64;
        base * weight / 32
    }
}
