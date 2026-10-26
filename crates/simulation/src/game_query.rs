#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameQueryStats {
    pub visited_chunks: u32,
    pub candidate_units: u32,
    pub returned_units: u32,
}

impl crate::GameWorld {
    /// The units stored at positions `range` (clamped to the population), in
    /// storage order; lets a caller split the population across workers.
    pub fn units_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> impl Iterator<Item = crate::GameUnit> + '_ {
        let end = range.end.min(self.units.len());
        self.units[range.start.min(end)..end]
            .iter()
            .map(|unit| unit.state)
    }

    /// One past the largest entity id ever allocated. Ids are dense, so a
    /// table indexed by id is a cheap map for a whole tick.
    pub fn entity_id_bound(&self) -> usize {
        self.next_entity_id as usize
    }

    /// Appends the ids bucketed in every spatial chunk that touches `rect`.
    /// They are candidates only: a unit may stand in a touched chunk but
    /// outside `rect`. An invalid `rect` appends nothing, as [`Self::query`].
    pub fn collect_chunk_unit_ids(
        &self,
        rect: aoe_core::TileRect,
        out: &mut Vec<aoe_core::EntityId>,
    ) {
        use aoe_core::{ChunkCoord, SPATIAL_CHUNK_TILES};
        if !rect.valid(self.config.width_tiles, self.config.height_tiles) {
            return;
        }
        let (first_x, last_x) = (
            rect.min.x.div_euclid(SPATIAL_CHUNK_TILES),
            (rect.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES),
        );
        let (first_y, last_y) = (
            rect.min.y.div_euclid(SPATIAL_CHUNK_TILES),
            (rect.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES),
        );
        for y in first_y..=last_y {
            for x in first_x..=last_x {
                if let Some(bucket) = self.chunks.get(&ChunkCoord { x, y }) {
                    out.extend_from_slice(bucket);
                }
            }
        }
    }
}
