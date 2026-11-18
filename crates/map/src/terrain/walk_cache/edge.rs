//! Edge rule between two adjacent tiles from cached chunk data. Most edges a
//! route search probes join two tiles of one chunk; those read the chunk once.
use super::*;

impl MapChunkGenerator {
    pub(in crate::terrain) fn cached_edge(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<EdgePassability>, EnvironmentPageError> {
        let (Lookup::Cell(a), Lookup::Cell(b)) = self.walk_lookup_pair(from, to, cancelled)? else {
            return Ok(None);
        };
        Ok(Some(
            if !a.passable
                || !b.passable
                || !a.walkable_surface
                || !b.walkable_surface
                || (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() > 1
            {
                EdgePassability::Blocked
            } else {
                EdgePassability::Passable
            },
        ))
    }

    /// `walk_lookup` of both tiles; one thread-local chunk read when they
    /// share a recent chunk, otherwise the two ordinary lookups.
    fn walk_lookup_pair(
        &self,
        a: TileCoord,
        b: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(Lookup, Lookup), EnvironmentPageError> {
        let key = |t: TileCoord| (t.x.div_euclid(CHUNK_TILES), t.y.div_euclid(CHUNK_TILES));
        let inside =
            |t: TileCoord| t.x >= 0 && t.y >= 0 && t.x < self.width_tiles && t.y < self.width_tiles;
        if key(a) == key(b) && inside(a) && inside(b) && !self.has_static_navigation() {
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            let (id, at) = (self.derived.id, key(a));
            let read = |chunk: &WalkChunk, t| chunk.cell(t).map_or(Lookup::OutOfMap, Lookup::Cell);
            let found = RECENT.with(|recent| {
                recent
                    .borrow()
                    .0
                    .iter()
                    .flatten()
                    .find_map(|(owner, key, chunk)| {
                        (*owner == id && *key == at).then(|| (read(chunk, a), read(chunk, b)))
                    })
            });
            if let Some(found) = found {
                return Ok(found);
            }
        }
        Ok((
            self.walk_lookup(a, cancelled)?,
            self.walk_lookup(b, cancelled)?,
        ))
    }
}
