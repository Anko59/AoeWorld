use super::{Home, STEWARD_RULES, StewardWork};
use crate::{BuildingKind, GameBuilding, GameUnit, GameWorld, UnitKind};
use aoe_core::{ChunkCoord, EntityId, PlayerId, SPATIAL_CHUNK_TILES};
use std::collections::BTreeSet;

/// The domain's buildings: one sweep of the player's buildings when the
/// domain starts or its zone changes, then kept by events (a new building is
/// added when placed, a removed one drops out when read), never re-swept on
/// every visit. A work budget splits the sweep; it is never a size limit.
#[derive(Clone, Debug, Default)]
pub(super) struct BuildingScan {
    pub after: Option<(BuildingKind, EntityId)>,
    pub known: BTreeSet<EntityId>,
    pub swept: bool,
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
    /// Exhausted ore probes are shared by workers for this pass only.
    pub ore_misses: [bool; 4],
}
impl GameWorld {
    /// A new building joins every domain covering it, sweep or not.
    pub(crate) fn steward_building_added(
        &mut self,
        id: EntityId,
        player: PlayerId,
        footprint: aoe_core::TileRect,
    ) {
        for home in self.economy.steward_homes.values_mut() {
            if home.player == player && home.covers(footprint, 0) {
                home.building_scan.known.insert(id);
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
        // The snapshot's copy of the domain leaves its member list behind.
        let members = std::mem::take(&mut self.economy.steward_homes.get_mut(&id)?.members);
        let home = self.economy.steward_homes.get(&id)?.clone();
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            h.members = members;
        }
        let player = self.building(home.town_center?)?.player;
        // One census sweep after activation or a zone change; afterwards the
        // member list is kept by events and no visit walks the box again.
        let complete_units = home.census_done || self.steward_census(id, &home, player, work);
        let mut units = Vec::new();
        let mut boats = Vec::new();
        let mut gone = Vec::new();
        for &member in &self.economy.steward_homes[&id].members {
            match self.unit(member) {
                // Membership follows every tile change and zone change, so
                // a member stands on this domain's ground: no lookup again.
                Some(u) if u.player == player && u.hp > 0 && !self.embarked(u.id) => {
                    if u.kind == UnitKind::FishingShip {
                        boats.push(u);
                    } else {
                        units.push(u);
                    }
                }
                _ => gone.push(member),
            }
        }
        for member in gone {
            self.steward_set_member(member, None, None);
        }
        units.sort_unstable_by_key(|u| u.id);
        boats.sort_unstable_by_key(|u| u.id);
        let covers = |b: &crate::GameBuilding| {
            if b.kind == BuildingKind::Dock {
                home.covers_shore(b.footprint, 0)
            } else {
                home.covers(b.footprint, 0)
            }
        };
        let mut scan = home.building_scan.clone();
        if !scan.swept {
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
            let finished = candidates.peek().is_none();
            for (kind, id) in batch {
                work.building_visits += 1;
                scan.after = Some((kind, id));
                if self.building(id).is_some_and(|b| covers(&b)) {
                    scan.known.insert(id);
                }
            }
            if finished {
                scan.after = None;
                scan.swept = true;
            }
        }
        // Read current state, not stale queue/garrison copies from prior slices.
        scan.known.retain(|id| {
            self.building(*id)
                .is_some_and(|b| b.player == player && covers(&b))
        });
        let mut buildings: Vec<_> = scan
            .known
            .iter()
            .filter_map(|id| self.building(*id))
            .map(|b| b.into_owned())
            .collect();
        let complete_buildings = scan.swept;
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            h.building_scan = scan;
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
            ore_misses: [false; 4],
        })
    }
}

impl GameWorld {
    /// Continues this domain's census sweep of its box (bounded per visit);
    /// every owned villager and boat it meets becomes a member. True when the
    /// sweep has finished.
    fn steward_census(
        &mut self,
        id: EntityId,
        home: &Home,
        player: PlayerId,
        work: &mut StewardWork,
    ) -> bool {
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
        if (chunk, offset) == (0, 0) {
            // A new sweep (activation or zone change): members off the
            // domain's ground now leave before it starts adding.
            let stale: Vec<_> = self.economy.steward_homes[&id]
                .members
                .iter()
                .copied()
                .filter(|m| {
                    self.unit(*m)
                        .is_none_or(|u| !home.owns_resident(u.kind, u.position))
                })
                .collect();
            for unit in stale {
                self.steward_set_member(unit, None, None);
            }
        }
        let mut visits = 0;
        let mut found = Vec::new();
        while chunk < count && visits < STEWARD_RULES.unit_visits_per_domain {
            let key = ChunkCoord {
                x: first.x + (chunk % width) as i32,
                y: first.y + (chunk / width) as i32,
            };
            let bucket = self.chunks.get(&key).map_or(&[][..], Vec::as_slice);
            while offset < bucket.len() && visits < STEWARD_RULES.unit_visits_per_domain {
                visits += 1;
                if let Some(u) = self.unit(bucket[offset])
                    && u.player == player
                    && u.hp > 0
                    && matches!(u.kind, UnitKind::Villager | UnitKind::FishingShip)
                    && home.owns_resident(u.kind, u.position)
                {
                    found.push((u.id, u.position.tile_floor()));
                }
                offset += 1;
            }
            if offset >= bucket.len() {
                chunk += 1;
                offset = 0;
            }
        }
        work.unit_visits += visits;
        for (unit, tile) in found {
            self.steward_set_member(unit, Some(id), Some(tile));
        }
        let done = chunk == count;
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            (h.scan_chunk, h.scan_offset) = if done { (0, 0) } else { (chunk, offset) };
            h.census_done = done;
        }
        done
    }

    /// Records `unit` as a member of `domain` (or of none), placed from `tile`.
    pub(super) fn steward_set_member(
        &mut self,
        unit: EntityId,
        domain: Option<EntityId>,
        tile: Option<aoe_core::TileCoord>,
    ) {
        let m = &mut self.economy.stewards;
        let old = match tile {
            Some(tile) => m.member_of.insert(unit, (domain, tile)),
            None => m.member_of.remove(&unit),
        };
        if let Some((Some(previous), _)) = old
            && Some(previous) != domain
            && let Some(home) = self.economy.steward_homes.get_mut(&previous)
        {
            home.members.remove(&unit);
        }
        if let Some(domain) = domain
            && let Some(home) = self.economy.steward_homes.get_mut(&domain)
        {
            home.members.insert(unit);
        }
    }

    /// A domain that stops: its members look for their domain again.
    pub(super) fn steward_release_members(&mut self, id: EntityId) {
        let members = self
            .economy
            .steward_homes
            .get_mut(&id)
            .map(|h| std::mem::take(&mut h.members))
            .unwrap_or_default();
        for unit in members {
            self.economy.stewards.member_of.remove(&unit);
            self.economy.walking.moved.push(unit);
        }
    }

    /// This tick's events: units placed on the map, and villagers and boats
    /// that movement stepped onto a new tile, join the domain owning it. The
    /// cost follows tile changes, never a scan of every changed unit.
    pub(super) fn steward_track_members(&mut self) {
        let moved = std::mem::take(&mut self.economy.walking.moved);
        if self.economy.stewards.by_player.is_empty() {
            // Without a domain there is nothing to join; activation sweeps.
            return;
        }
        for &id in &moved {
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let u = self.units[index].state;
            if u.hp == 0 || !matches!(u.kind, UnitKind::Villager | UnitKind::FishingShip) {
                continue;
            }
            let Some(domains) = self.economy.stewards.by_player.get(&u.player) else {
                continue;
            };
            // Domains still sweeping their box find their units themselves:
            // after a load, that sweep is the one pass over each box.
            if domains.keys().all(|d| {
                self.economy
                    .steward_homes
                    .get(d)
                    .is_none_or(|h| !h.census_done)
            }) {
                continue;
            }
            let tile = u.position.tile_floor();
            if self
                .economy
                .stewards
                .member_of
                .get(&id)
                .is_some_and(|&(_, at)| at == tile)
            {
                continue;
            }
            let owner = domains.keys().copied().find(|d| {
                self.economy
                    .steward_homes
                    .get(d)
                    .is_some_and(|h| h.owns_resident(u.kind, u.position))
            });
            self.steward_set_member(id, owner, Some(tile));
        }
    }
}
