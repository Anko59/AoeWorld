//! Who is present in each 32-tile chunk: per player, a count of entries (the
//! units of a bucket, or the building tiles of a chunk). The sleep proof of
//! combat acquisition reads it instead of every unit and tile it covers.
use aoe_core::{ChunkCoord, PlayerId, SPATIAL_CHUNK_TILES, TileCoord};

/// Side of the finer cells a second summary can count tile entries in
/// (`add_cell_tile`): the 4 x 4 cells of a chunk.
pub(crate) const PRESENCE_CELL_TILES: i32 = 8;

/// Keyed reads only, never iterated for a decision: a hash map makes each of
/// the many per-look presence reads a constant-time probe.
type Counts = std::collections::HashMap<
    u64,
    Vec<(PlayerId, u32)>,
    std::hash::BuildHasherDefault<crate::game_world::movement_step::CellHasher>,
>;

fn key(chunk: ChunkCoord) -> u64 {
    u64::from(chunk.x as u32) | (u64::from(chunk.y as u32) << 32)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ChunkPresence {
    chunks: Counts,
}

fn add(counts: &mut Counts, chunk: ChunkCoord, player: PlayerId) {
    let players = counts.entry(key(chunk)).or_default();
    match players.iter_mut().find(|(p, _)| *p == player) {
        Some((_, count)) => *count += 1,
        None => players.push((player, 1)),
    }
}

fn remove(counts: &mut Counts, chunk: ChunkCoord, player: PlayerId) {
    let Some(players) = counts.get_mut(&key(chunk)) else {
        return;
    };
    let Some(at) = players.iter().position(|(p, _)| *p == player) else {
        return;
    };
    players[at].1 -= 1;
    if players[at].1 == 0 {
        players.swap_remove(at);
        if players.is_empty() {
            counts.remove(&key(chunk));
        }
    }
}

impl ChunkPresence {
    pub(crate) fn add(&mut self, chunk: ChunkCoord, player: PlayerId) {
        add(&mut self.chunks, chunk, player);
    }

    pub(crate) fn remove(&mut self, chunk: ChunkCoord, player: PlayerId) {
        remove(&mut self.chunks, chunk, player);
    }

    pub(crate) fn add_tile(&mut self, tile: TileCoord, player: PlayerId) {
        add(&mut self.chunks, tile_chunk(tile), player);
    }

    pub(crate) fn remove_tile(&mut self, tile: TileCoord, player: PlayerId) {
        remove(&mut self.chunks, tile_chunk(tile), player);
    }

    /// `add_tile` for a summary kept per `PRESENCE_CELL_TILES` cell.
    pub(crate) fn add_cell_tile(&mut self, tile: TileCoord, player: PlayerId) {
        add(&mut self.chunks, tile_cell(tile), player);
    }

    pub(crate) fn remove_cell_tile(&mut self, tile: TileCoord, player: PlayerId) {
        remove(&mut self.chunks, tile_cell(tile), player);
    }

    /// Players present in `chunk`.
    pub(crate) fn players(&self, chunk: ChunkCoord) -> impl Iterator<Item = PlayerId> + '_ {
        self.chunks
            .get(&key(chunk))
            .into_iter()
            .flatten()
            .map(|&(p, _)| p)
    }

    /// For a per-cell summary: the players present in the cell of `tile`.
    pub(crate) fn players_at_cell(&self, tile: TileCoord) -> impl Iterator<Item = PlayerId> + '_ {
        self.players(tile_cell(tile))
    }

    /// For a per-cell summary: whether any player but `player` is present in
    /// a cell `rect` touches.
    pub(crate) fn others_in_cells(&self, rect: aoe_core::TileRect, player: PlayerId) -> bool {
        let size = PRESENCE_CELL_TILES;
        (rect.min.y.div_euclid(size)..=(rect.max.y - 1).div_euclid(size)).any(|y| {
            (rect.min.x.div_euclid(size)..=(rect.max.x - 1).div_euclid(size))
                .any(|x| self.others(ChunkCoord { x, y }, player))
        })
    }

    /// For a per-chunk summary: whether any player but `player` is present in
    /// a chunk `rect` touches.
    pub(crate) fn others_in_chunks(&self, rect: aoe_core::TileRect, player: PlayerId) -> bool {
        let size = SPATIAL_CHUNK_TILES;
        (rect.min.y.div_euclid(size)..=(rect.max.y - 1).div_euclid(size)).any(|y| {
            (rect.min.x.div_euclid(size)..=(rect.max.x - 1).div_euclid(size))
                .any(|x| self.others(ChunkCoord { x, y }, player))
        })
    }

    /// Whether any player but `player` is present in `chunk`.
    pub(crate) fn others(&self, chunk: ChunkCoord, player: PlayerId) -> bool {
        self.chunks
            .get(&key(chunk))
            .is_some_and(|players| players.iter().any(|&(p, _)| p != player))
    }

    pub(crate) fn clear(&mut self) {
        self.chunks.clear();
    }

    /// The counts in a form that ignores the order of players in a chunk.
    #[cfg(test)]
    pub(crate) fn normalized(
        &self,
    ) -> std::collections::BTreeMap<ChunkCoord, std::collections::BTreeMap<PlayerId, u32>> {
        self.chunks
            .iter()
            .map(|(&at, players)| {
                let chunk = ChunkCoord {
                    x: at as u32 as i32,
                    y: (at >> 32) as u32 as i32,
                };
                (chunk, players.iter().copied().collect())
            })
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

/// The bit of `tile`'s cell among the 4 x 4 cells of `chunk`.
pub(crate) fn cell_bit(chunk: ChunkCoord, tile: TileCoord) -> u16 {
    let at = |v: i32, o: i32| (v - o * SPATIAL_CHUNK_TILES).clamp(0, SPATIAL_CHUNK_TILES - 1);
    1 << (at(tile.y, chunk.y) / PRESENCE_CELL_TILES * 4 + at(tile.x, chunk.x) / PRESENCE_CELL_TILES)
}

/// The cells of `chunk` that `bounds` touches, as `cell_bit` numbers them.
pub(crate) fn cell_mask(chunk: ChunkCoord, bounds: aoe_core::TileRect) -> u16 {
    let (x0, y0) = (chunk.x * SPATIAL_CHUNK_TILES, chunk.y * SPATIAL_CHUNK_TILES);
    if bounds.max.x <= x0
        || bounds.max.y <= y0
        || bounds.min.x >= x0 + SPATIAL_CHUNK_TILES
        || bounds.min.y >= y0 + SPATIAL_CHUNK_TILES
    {
        return 0;
    }
    let lo = |v: i32, o: i32| (v - o).clamp(0, SPATIAL_CHUNK_TILES - 1) / PRESENCE_CELL_TILES;
    let hi = |v: i32, o: i32| (v - 1 - o).clamp(0, SPATIAL_CHUNK_TILES - 1) / PRESENCE_CELL_TILES;
    let mut mask = 0;
    for y in lo(bounds.min.y, y0)..=hi(bounds.max.y, y0) {
        for x in lo(bounds.min.x, x0)..=hi(bounds.max.x, x0) {
            mask |= 1 << (y * 4 + x);
        }
    }
    mask
}

fn tile_cell(tile: TileCoord) -> ChunkCoord {
    ChunkCoord {
        x: tile.x.div_euclid(PRESENCE_CELL_TILES),
        y: tile.y.div_euclid(PRESENCE_CELL_TILES),
    }
}

fn tile_chunk(tile: TileCoord) -> ChunkCoord {
    ChunkCoord {
        x: tile.x.div_euclid(SPATIAL_CHUNK_TILES),
        y: tile.y.div_euclid(SPATIAL_CHUNK_TILES),
    }
}
