//! Fog exploration in a save: which tiles each player has ever seen.
use super::GameWorld;
use aoe_core::{PlayerId, TileCoord};

/// One player's explored 32x32-tile chunks with their bitsets.
pub type ExploredChunks = Vec<((i32, i32), [u64; 16])>;

impl GameWorld {
    pub(crate) fn explored_records(&self) -> Vec<(PlayerId, ExploredChunks)> {
        self.vision
            .players
            .iter()
            .filter(|(_, vision)| !vision.explored.is_empty())
            .map(|(player, vision)| {
                (
                    *player,
                    vision
                        .explored
                        .iter()
                        .map(|(at, bits)| (*at, *bits))
                        .collect(),
                )
            })
            .collect()
    }

    /// Marks the tiles explored again, as seen and then out of sight, so the
    /// strategic overview remembers them too. Call before any unit is placed.
    pub(crate) fn restore_explored(&mut self, records: &[(PlayerId, ExploredChunks)]) {
        let (width, height) = (self.config.width_tiles, self.config.height_tiles);
        for (player, chunks) in records {
            let vision = self.vision.players.entry(*player).or_default();
            // Bumped first, so the fog stamps below carry the new revision.
            vision.revision += 1;
            vision.reset_stamp = vision.revision;
            for ((cx, cy), bits) in chunks {
                for index in 0..1024_i32 {
                    if bits[(index / 64) as usize] & (1_u64 << (index % 64)) == 0 {
                        continue;
                    }
                    let tile = TileCoord::new(cx * 32 + index % 32, cy * 32 + index / 32);
                    if tile.x >= width || tile.y >= height {
                        continue;
                    }
                    vision.set_explored(tile);
                    vision.touch_overview(tile, true, width, height);
                    vision.touch_overview(tile, false, width, height);
                }
            }
        }
    }
}
