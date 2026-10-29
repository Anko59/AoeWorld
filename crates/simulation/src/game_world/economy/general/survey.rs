use super::*;
use aoe_core::{ChunkCoord, SPATIAL_CHUNK_TILES, TileCoord};
pub(super) struct Survey {
    pub enemies: Vec<GameUnit>,
    pub buildings: Vec<EntityId>,
    pub complete: bool,
}
impl GameWorld {
    pub(super) fn general_survey(
        &mut self,
        id: EntityId,
        work: &mut GeneralWork,
    ) -> Option<Survey> {
        let u = self.unit(id)?;
        let center = u.position.tile_floor();
        let first = ChunkCoord {
            x: (center.x - 24).div_euclid(SPATIAL_CHUNK_TILES),
            y: (center.y - 24).div_euclid(SPATIAL_CHUNK_TILES),
        };
        let last = ChunkCoord {
            x: (center.x + 24).div_euclid(SPATIAL_CHUNK_TILES),
            y: (center.y + 24).div_euclid(SPATIAL_CHUNK_TILES),
        };
        let width = (last.x - first.x + 1) as usize;
        let count = width * (last.y - first.y + 1) as usize;
        let start = self.economy.generals.armies.get(&id)?.scan;
        let (mut chunk, mut offset) = start;
        let mut visits = 0;
        let mut units = Vec::new();
        while chunk < count && visits < 1024 {
            let key = ChunkCoord {
                x: first.x + (chunk % width) as i32,
                y: first.y + (chunk / width) as i32,
            };
            let bucket = self.chunks.get(&key).map_or(&[][..], Vec::as_slice);
            while offset < bucket.len() && visits < 1024 {
                if let Some(v) = self.unit(bucket[offset]).filter(|v| {
                    v.hp > 0
                        && !self.embarked(v.id)
                        && distance(u.position, v.position) <= (24 * 1024_i64).pow(2)
                }) {
                    units.push(v);
                }
                visits += 1;
                offset += 1;
            }
            if offset >= bucket.len() {
                chunk += 1;
                offset = 0;
            }
        }
        work.unit_visits += visits;
        let a = self.economy.generals.armies.get_mut(&id)?;
        a.scan = if chunk == count {
            (0, 0)
        } else {
            (chunk, offset)
        };
        let complete = start == (0, 0) && chunk == count;
        units.sort_unstable_by_key(|v| v.id);
        for v in units.iter().copied().filter(|v| v.player == u.player) {
            self.claim_general_unit(id, v);
        }
        let enemies = units
            .into_iter()
            .filter(|v| {
                v.player != u.player
                    && distance(u.position, v.position) <= (12 * 1024_i64).pow(2)
                    && self.unit_visible_to(u.player, v.id)
            })
            .collect();
        // Fixed 25x25 tile census through the building occupancy index. Never
        // inspect the world's building table or hidden enemy structures.
        let mut buildings = BTreeSet::new();
        for y in center.y - 12..=center.y + 12 {
            for x in center.x - 12..=center.x + 12 {
                work.building_visits += 1;
                if let Some(b) = self
                    .building_at_tile(TileCoord::new(x, y))
                    .and_then(|id| self.building(id))
                    && b.player != u.player
                    && b.hp > 0
                    && self.building_visible_to(u.player, b.footprint)
                {
                    buildings.insert(b.id);
                }
            }
        }
        Some(Survey {
            enemies,
            buildings: buildings.into_iter().collect(),
            complete,
        })
    }
}
