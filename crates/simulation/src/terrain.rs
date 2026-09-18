use aoe_core::{TileCoord, WorldConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UniformGrass {
    seed: u64,
}

impl UniformGrass {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn material_at(self, tile: TileCoord, config: WorldConfig) -> Option<u8> {
        if tile.x < 0 || tile.y < 0 || tile.x >= config.width_tiles || tile.y >= config.height_tiles
        {
            return None;
        }
        let x = tile.x.rem_euclid(8) as u64;
        let y = tile.y.rem_euclid(8) as u64;
        Some(((x.wrapping_mul(37) + y.wrapping_mul(17) + self.seed) % 8) as u8)
    }
}
