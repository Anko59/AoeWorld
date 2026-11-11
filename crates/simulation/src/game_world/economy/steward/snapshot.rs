use super::{Home, STEWARD_RULES, StewardWork};
use crate::{BuildingKind, GameBuilding, GameUnit, GameWorld, UnitKind};
use aoe_core::{ChunkCoord, EntityId, PlayerId, SPATIAL_CHUNK_TILES};
use std::collections::BTreeSet;

/// A work budget splits discovery; it must never become a kingdom size limit.
#[derive(Clone, Debug, Default)]
pub(super) struct BuildingScan {
    pub after: Option<(BuildingKind, EntityId)>,
    pub known: BTreeSet<EntityId>,
}

pub(super) struct Snapshot {
    pub steward: EntityId,
    pub home: Home,
    pub player: PlayerId,
    pub units: Vec<GameUnit>,
    pub boats: Vec<GameUnit>,
    pub buildings: Vec<GameBuilding>,
    pub complete: bool,
    pub villagers: usize,
    pub queued: usize,
    pub global_queued: usize,
    /// The stalled node/kind the worker now being tasked must not choose again.
    pub avoid: Option<super::watchdog::Stall>,
    /// A dry-Food retask wants a Farm, not another wild forager.
    pub farm_first: bool,
    /// Only reused within this authoritative pass; accepted jobs retain state.
    pub offers: [Vec<aoe_map::ResourceNode>; 4],
}
impl GameWorld {
    /// An owner may create a low-kind building behind a pending scan cursor.
    pub(crate) fn steward_building_added(
        &mut self,
        id: EntityId,
        player: PlayerId,
        footprint: aoe_core::TileRect,
    ) {
        for home in self.economy.steward_homes.values_mut() {
            if home.player == player
                && home.covers(footprint, 0)
                && let Some(scan) = home.building_scan.as_mut()
            {
                scan.known.insert(id);
            }
        }
    }

    /// Spatial buckets only, with a retained position when crowds exceed the
    /// scan bound. A partial census never authorizes villager production.
    pub(super) fn steward_snapshot(
        &mut self,
        id: EntityId,
        work: &mut StewardWork,
    ) -> Option<Snapshot> {
        self.steward_ensure_regions(id);
        let home = self.economy.steward_homes.get(&id)?.clone();
        let player = self.building(home.town_center?)?.player;
        let center = home.center.tile_floor();
        let r = STEWARD_RULES.radius_tiles;
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
        let mut boats = Vec::new();
        while chunk < count && visits < STEWARD_RULES.unit_visits_per_domain {
            let key = ChunkCoord {
                x: first.x + (chunk % width) as i32,
                y: first.y + (chunk / width) as i32,
            };
            let bucket = self.chunks.get(&key).map_or(&[][..], Vec::as_slice);
            while offset < bucket.len() && visits < STEWARD_RULES.unit_visits_per_domain {
                let unit = self.unit(bucket[offset]);
                visits += 1;
                offset += 1;
                if let Some(u) = unit
                    && u.player == player
                    && u.hp > 0
                    && matches!(u.kind, UnitKind::Villager | UnitKind::FishingShip)
                    && !self.embarked(u.id)
                    && self.vassal_controller(u.id).is_none()
                    && home.owns(u.position)
                {
                    if u.kind == UnitKind::FishingShip {
                        boats.push(u);
                    } else {
                        units.push(u);
                    }
                }
            }
            if offset >= bucket.len() {
                chunk += 1;
                offset = 0;
            }
        }
        work.unit_visits += visits;
        let complete_units = chunk == count && start == (0, 0);
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            (h.scan_chunk, h.scan_offset) = if chunk == count {
                (0, 0)
            } else {
                (chunk, offset)
            };
        }
        units.sort_unstable_by_key(|u| u.id);
        boats.sort_unstable_by_key(|u| u.id);
        let mut scan = home.building_scan.clone().unwrap_or_default();
        let cursor = scan.after;
        let first_kind = cursor.map_or(BuildingKind::TownCenter, |(kind, _)| kind);
        let mut candidates = self
            .economy
            .dropoffs
            .range((player, first_kind)..=(player, BuildingKind::Road))
            .flat_map(|((_, kind), ids)| {
                let start = cursor
                    .filter(|(previous, _)| previous == kind)
                    .map_or(0, |(_, id)| {
                        ids.partition_point(|candidate| *candidate <= id)
                    });
                ids[start..].iter().map(move |id| (*kind, *id))
            })
            .peekable();
        let batch: Vec<_> = candidates
            .by_ref()
            .take(STEWARD_RULES.building_visits_per_domain)
            .collect();
        let complete_buildings = candidates.peek().is_none();
        for (kind, id) in batch {
            work.building_visits += 1;
            scan.after = Some((kind, id));
            if self
                .building(id)
                .is_some_and(|b| home.covers(b.footprint, 0))
            {
                scan.known.insert(id);
            }
        }
        // Read current state, not stale queue/garrison copies from prior slices.
        let mut buildings: Vec<_> = scan
            .known
            .iter()
            .filter_map(|id| self.building(*id))
            .filter(|b| b.player == player && home.covers(b.footprint, 0))
            .cloned()
            .collect();
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            h.building_scan = (!complete_buildings).then_some(scan);
        }
        let queued = buildings
            .iter()
            .flat_map(|b| &b.training_queue)
            .filter(|entry| entry.kind == UnitKind::Villager)
            .count();
        let villagers = units.len()
            + buildings
                .iter()
                .flat_map(|b| &b.garrison)
                .filter(|g| g.unit.kind == UnitKind::Villager && g.unit.hp > 0)
                .count();
        // Pending population is player-wide and changes while a census runs.
        let global_queued = self
            .economy
            .active_training
            .iter()
            .filter_map(|id| self.building(*id))
            .filter(|b| b.player == player)
            .map(|b| b.training_queue.len())
            .sum();
        buildings.sort_unstable_by_key(|b| b.id);
        Some(Snapshot {
            steward: id,
            home,
            player,
            units,
            boats,
            buildings,
            complete: complete_units && complete_buildings,
            villagers,
            queued,
            global_queued,
            avoid: None,
            farm_first: false,
            offers: Default::default(),
        })
    }
}
