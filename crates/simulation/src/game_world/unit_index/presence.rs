//! Who is present in each 32-tile chunk: per player, a count of entries (the
//! units of a bucket, or the building tiles of a chunk). The sleep proof of
//! combat acquisition reads it instead of every unit and tile it covers.
use aoe_core::{ChunkCoord, PlayerId, SPATIAL_CHUNK_TILES, TileCoord};
use std::collections::BTreeMap;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ChunkPresence {
    chunks: BTreeMap<ChunkCoord, Vec<(PlayerId, u32)>>,
}

impl ChunkPresence {
    pub(crate) fn add(&mut self, chunk: ChunkCoord, player: PlayerId) {
        let players = self.chunks.entry(chunk).or_default();
        match players.iter_mut().find(|(p, _)| *p == player) {
            Some((_, count)) => *count += 1,
            None => players.push((player, 1)),
        }
    }

    pub(crate) fn remove(&mut self, chunk: ChunkCoord, player: PlayerId) {
        let Some(players) = self.chunks.get_mut(&chunk) else {
            return;
        };
        let Some(at) = players.iter().position(|(p, _)| *p == player) else {
            return;
        };
        players[at].1 -= 1;
        if players[at].1 == 0 {
            players.swap_remove(at);
            if players.is_empty() {
                self.chunks.remove(&chunk);
            }
        }
    }

    pub(crate) fn add_tile(&mut self, tile: TileCoord, player: PlayerId) {
        self.add(tile_chunk(tile), player);
    }

    pub(crate) fn remove_tile(&mut self, tile: TileCoord, player: PlayerId) {
        self.remove(tile_chunk(tile), player);
    }

    /// Whether any player but `player` is present in `chunk`.
    pub(crate) fn others(&self, chunk: ChunkCoord, player: PlayerId) -> bool {
        self.chunks
            .get(&chunk)
            .is_some_and(|players| players.iter().any(|&(p, _)| p != player))
    }

    pub(crate) fn clear(&mut self) {
        self.chunks.clear();
    }

    /// The counts in a form that ignores the order of players in a chunk.
    #[cfg(test)]
    pub(crate) fn normalized(&self) -> BTreeMap<ChunkCoord, BTreeMap<PlayerId, u32>> {
        self.chunks
            .iter()
            .map(|(&chunk, players)| (chunk, players.iter().copied().collect()))
            .collect()
    }
}

impl crate::game_world::StoredUnit {
    /// The owner of a living unit; `None` for a corpse, which is present in
    /// its bucket but not in the summary.
    pub(crate) fn living_owner(&self) -> Option<PlayerId> {
        self.state.death_tick.is_none().then_some(self.state.player)
    }
}

fn tile_chunk(tile: TileCoord) -> ChunkCoord {
    ChunkCoord {
        x: tile.x.div_euclid(SPATIAL_CHUNK_TILES),
        y: tile.y.div_euclid(SPATIAL_CHUNK_TILES),
    }
}
