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
    /// The region labels `known` was last checked against: labels are
    /// replaced whenever the zone moves, so while they stay the same no
    /// building's coverage has moved.
    pub zone: Option<std::sync::Arc<super::territory::Regions>>,
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
    /// Per-visit memos of the job search, dropped whenever a job is given
    /// (`forget_jobs`): builders per building, idle unloaded villagers with
    /// their tiles, and the starting-village rule.
    pub attendance: Option<std::collections::BTreeMap<EntityId, usize>>,
    pub helpers: Option<Vec<(EntityId, aoe_core::TileCoord)>>,
    pub starting_village: Option<bool>,
    /// This visit's domain offer, once the job pass has computed it.
    pub offer: Option<u64>,
}
/// The kinds a Steward pass reads: drop-offs (Town Centers, camps, Mills,
/// Docks), Houses and Farms. Villager training happens at Town Centers.
fn steward_reads(kind: BuildingKind) -> bool {
    matches!(
        kind,
        BuildingKind::House | BuildingKind::Farm | BuildingKind::Dock
    ) || kind.rules().accepts.iter().any(|&accepts| accepts)
}

impl Snapshot {
    /// A job was given: builders, idle helpers and housing are read again.
    pub fn forget_jobs(&mut self) {
        (self.attendance, self.helpers, self.starting_village) = (None, None, None);
    }
}

impl GameWorld {
    /// A new building of a kind the passes read joins every domain covering
    /// it, sweep or not.
    pub(crate) fn steward_building_added(
        &mut self,
        id: EntityId,
        player: PlayerId,
        kind: BuildingKind,
        footprint: aoe_core::TileRect,
    ) {
        if !steward_reads(kind) {
            return;
        }
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
        // The snapshot's copy of the domain leaves behind every per-worker
        // record no pass reads from it (members, workers, retries, tenure,
        // held jobs, stalls) and the building sweep, which is worked on in
        // place: a visit copies a few fields, not the domain's collections.
        let live = self.economy.steward_homes.get_mut(&id)?;
        let kept = (
            std::mem::take(&mut live.members),
            std::mem::take(&mut live.workers),
            std::mem::take(&mut live.unemployed),
            std::mem::take(&mut live.assigned_at),
            std::mem::take(&mut live.held_jobs),
            std::mem::take(&mut live.stalls),
            std::mem::take(&mut live.building_scan),
        );
        let home = live.clone();
        let live = self.economy.steward_homes.get_mut(&id)?;
        let mut scan;
        (
            live.members,
            live.workers,
            live.unemployed,
            live.assigned_at,
            live.held_jobs,
            live.stalls,
            scan,
        ) = kept;
        let Some(player) = home
            .town_center
            .and_then(|c| self.building(c))
            .map(|b| b.player)
        else {
            self.economy.steward_homes.get_mut(&id)?.building_scan = scan;
            return None;
        };
        // One census sweep after activation or a zone change; afterwards the
        // member list is kept by events and no visit walks the box again.
        let span = self.steward_span("steward_census");
        let complete_units = home.census_done || self.steward_census(id, &home, player, work);
        drop(span);
        let span = self.steward_span("steward_members_copy");
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
        drop(span);
        let span = self.steward_span("steward_building_sweep");
        if !scan.swept {
            let cursor = scan.after;
            let first_kind = cursor.map_or(BuildingKind::TownCenter, |(kind, _)| kind);
            let mut candidates = self
                .economy
                .dropoffs
                .range((player, first_kind)..=(player, BuildingKind::Road))
                // Walls, roads, towers and military buildings are never read.
                .filter(|((_, kind), _)| steward_reads(*kind))
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
                if self.economy.buildings.get(&id).is_some_and(covers) {
                    scan.known.insert(id);
                }
            }
            if finished {
                scan.after = None;
                scan.swept = true;
            }
        }
        // Read current state, not stale queue/garrison copies from prior slices.
        // Ownership, place, garrison and queue lengths are read from the
        // stored building, which is brought up to the phase clock and copied.
        // Only the kinds a pass reads are known: villagers garrisoned in a
        // tower or castle do not count towards the domain's villagers.
        drop(span);
        let span = self.steward_span("steward_building_copy");
        let stored = &self.economy.buildings;
        // Docks are owned by distance to the rival centres, not by the zone,
        // so only they are rechecked while the zone stays put (#978).
        let same_zone = scan
            .zone
            .as_ref()
            .zip(home.regions.as_ref())
            .is_some_and(|(seen, now)| std::sync::Arc::ptr_eq(seen, now));
        scan.known.retain(|id| {
            stored.get(id).is_some_and(|b| {
                b.player == player && ((same_zone && b.kind != BuildingKind::Dock) || covers(b))
            })
        });
        scan.zone = home.regions.clone();
        let mut buildings = Vec::new();
        let mut garrisoned = 0;
        for id in &scan.known {
            let Some(b) = stored.get(id) else { continue };
            garrisoned += b
                .garrison
                .iter()
                .filter(|g| g.unit.kind == UnitKind::Villager && g.unit.hp > 0)
                .count();
            if steward_reads(b.kind)
                && let Some(b) = self.building(*id)
            {
                buildings.push(b.into_owned());
            }
        }
        drop(span);
        let complete_buildings = scan.swept;
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            h.building_scan = scan;
        }
        let queued = buildings
            .iter()
            .flat_map(|b| &b.training_queue)
            .filter(|entry| entry.kind == UnitKind::Villager)
            .count();
        let villagers = units.len() + garrisoned;
        // Pending population is player-wide and changes while a census runs.
        let global_queued = self
            .economy
            .active_training
            .iter()
            .filter_map(|id| stored.get(id))
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
            attendance: None,
            helpers: None,
            starting_village: None,
            offer: None,
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
            // Applied during a census sweep too: a unit that steps past the
            // sweep's cursor is still placed by its own tile change.
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

#[cfg(test)]
impl GameWorld {
    /// Test hook: the buildings a Steward's domain currently knows.
    pub(crate) fn steward_known_buildings(&self, id: EntityId) -> Vec<EntityId> {
        self.economy
            .steward_homes
            .get(&id)
            .map(|h| h.building_scan.known.iter().copied().collect())
            .unwrap_or_default()
    }
}
