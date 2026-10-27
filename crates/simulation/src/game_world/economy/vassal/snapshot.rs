use super::{Home, VASSAL_RULES, VassalWork};
use crate::{BuildingKind, GameBuilding, GameUnit, GameWorld, UnitKind};
use aoe_core::{ChunkCoord, EntityId, PlayerId, SPATIAL_CHUNK_TILES};

pub(super) struct Snapshot {
    pub vassal: EntityId,
    pub home: Home,
    pub player: PlayerId,
    pub units: Vec<GameUnit>,
    pub buildings: Vec<GameBuilding>,
    pub complete: bool,
    pub villagers: usize,
    pub queued: usize,
    pub global_queued: usize,
    pub allocated: [usize; 4],
}
impl GameWorld {
    /// Spatial buckets only, with a retained position when crowds exceed the
    /// scan bound. A partial census never authorizes villager production.
    pub(super) fn vassal_snapshot(
        &mut self,
        id: EntityId,
        work: &mut VassalWork,
    ) -> Option<Snapshot> {
        let home = self.economy.vassal_homes.get(&id)?.clone();
        let player = self.building(home.castle)?.player;
        let center = home.center.tile_floor();
        let r = VASSAL_RULES.radius_tiles;
        let first = ChunkCoord {
            x: (center.x - r).div_euclid(SPATIAL_CHUNK_TILES),
            y: (center.y - r).div_euclid(SPATIAL_CHUNK_TILES),
        };
        let last = ChunkCoord {
            x: (center.x + r).div_euclid(SPATIAL_CHUNK_TILES),
            y: (center.y + r).div_euclid(SPATIAL_CHUNK_TILES),
        };
        let width = (last.x - first.x + 1) as usize;
        let count = width * (last.y - first.y + 1) as usize;
        let mut chunk = home.scan_chunk.min(count);
        let mut offset = home.scan_offset;
        let start = (chunk, offset);
        let mut visits = 0;
        let mut units = Vec::new();
        while chunk < count && visits < VASSAL_RULES.unit_visits_per_domain {
            let key = ChunkCoord {
                x: first.x + (chunk % width) as i32,
                y: first.y + (chunk / width) as i32,
            };
            let bucket = self.chunks.get(&key).map_or(&[][..], Vec::as_slice);
            while offset < bucket.len() && visits < VASSAL_RULES.unit_visits_per_domain {
                let unit = self.unit(bucket[offset]);
                visits += 1;
                offset += 1;
                if let Some(u) = unit
                    && u.player == player
                    && u.hp > 0
                    && u.kind == UnitKind::Villager
                    && !self.embarked(u.id)
                    && home.contains(u.position)
                {
                    units.push(u);
                }
            }
            if offset >= bucket.len() {
                chunk += 1;
                offset = 0;
            }
        }
        work.unit_visits += visits;
        let complete_units = chunk == count && start == (0, 0);
        if let Some(h) = self.economy.vassal_homes.get_mut(&id) {
            (h.scan_chunk, h.scan_offset) = if chunk == count {
                (0, 0)
            } else {
                (chunk, offset)
            };
        }
        units.sort_unstable_by_key(|u| u.id);
        let building_count: usize = self
            .economy
            .dropoffs
            .range((player, BuildingKind::TownCenter)..=(player, BuildingKind::Bridge))
            .map(|(_, ids)| ids.len())
            .sum();
        let mut buildings = Vec::new();
        let mut queued = 0;
        let mut global_queued = 0;
        let mut villagers = units.len();
        for b in self
            .player_buildings(player)
            .take(VASSAL_RULES.building_visits_per_domain)
        {
            work.building_visits += 1;
            global_queued += b.training_queue.len();
            if !home.covers(b.footprint, 0) {
                continue;
            }
            queued += b
                .training_queue
                .iter()
                .filter(|e| e.kind == UnitKind::Villager)
                .count();
            villagers += b
                .garrison
                .iter()
                .filter(|g| g.unit.kind == UnitKind::Villager && g.unit.hp > 0)
                .count();
            buildings.push(b.clone());
        }
        buildings.sort_unstable_by_key(|b| b.id);
        let mut allocated = [0; 4];
        for u in &units {
            let kind = self
                .gathering_kind(u.id)
                .or_else(|| self.economy.vassals.controlled.get(&u.id).map(|(_, k)| *k));
            if let Some(k) = kind {
                allocated[k as usize] += 1;
            }
        }
        Some(Snapshot {
            vassal: id,
            home,
            player,
            units,
            buildings,
            complete: complete_units && building_count <= VASSAL_RULES.building_visits_per_domain,
            villagers,
            queued,
            global_queued,
            allocated,
        })
    }
}
