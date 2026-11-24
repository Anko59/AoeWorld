//! A mobile Steward's current TownCenter domain and budgeted economy.
use crate::{EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, WorldPosition};
use aoe_map::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};
mod adopt;
mod construction;
mod farming;
mod hash;
mod jobs;
mod manager;
mod navy;
mod priority;
mod rules;
mod search;
mod snapshot;
mod status;
mod territory;
mod watchdog;
pub use rules::{STEWARD_RULES, StewardRules, StewardSettings, StewardWork};
pub use status::{StewardStatus, StewardWait};
pub use watchdog::STEWARD_STALL_SECONDS;
#[cfg(test)]
pub(crate) use watchdog::steward_stall;

#[derive(Clone, Debug)]
pub(crate) struct Home {
    pub(super) id: EntityId,
    pub(super) player: PlayerId,
    /// Other active town_centers of the same player whose circles can overlap.
    pub(super) rivals: Vec<(EntityId, WorldPosition)>,
    pub(crate) town_center: Option<EntityId>,
    pub(crate) center: WorldPosition,
    pub(crate) workers: BTreeSet<EntityId>,
    /// Idle villagers no work was found for, with their next retry.
    unemployed: BTreeMap<EntityId, manager::Unemployed>,
    pub(super) worker_after: Option<EntityId>,
    /// Villagers and boats of the player whose last stop lies in this zone,
    /// kept by events (placed on the map, stopped on a new tile, zone change).
    pub(super) members: BTreeSet<EntityId>,
    /// The one census sweep after activation or a zone change has finished.
    pub(super) census_done: bool,
    pub(super) scan_chunk: usize,
    pub(super) scan_offset: usize,
    building_scan: snapshot::BuildingScan,
    pub(super) site_after: usize,
    pub(super) farm_after: Option<usize>,
    pub(super) repair_after: Option<EntityId>,
    /// First tick at which another shortage rebalance may be tried.
    pub(super) rebalance_after: u64,
    pub(super) rebalance_cursor: Option<EntityId>,
    pub(super) assigned_at: BTreeMap<EntityId, u64>,
    pub(super) repair_sites: BTreeMap<EntityId, usize>,
    held_jobs: BTreeMap<EntityId, jobs::HeldJob>,
    resources: search::Search,
    regions: Option<std::sync::Arc<territory::Regions>>,
    walking: Option<std::sync::Arc<super::land::walking::Walking>>,
    walk_recheck: bool,
    navy: navy::Fleet,
    pub(crate) stalls: BTreeMap<EntityId, watchdog::Stall>,
    pub(super) status: StewardStatus,
    pub(crate) settings: StewardSettings,
}
impl Home {
    /// The owner's villager cap for this Steward, never above the storage bound.
    pub(super) fn cap(&self) -> usize {
        usize::from(self.settings.cap).min(STEWARD_RULES.max_villagers)
    }
    /// The zone changed: the unit and building sweeps start again from the
    /// first chunk and the first building.
    pub(super) fn restart_sweeps(&mut self) {
        self.census_done = false;
        (self.scan_chunk, self.scan_offset) = (0, 0);
        self.building_scan.swept = false;
        self.building_scan.after = None;
    }
    pub(super) fn contains(&self, at: WorldPosition) -> bool {
        let dx = i64::from(at.x) - i64::from(self.center.x);
        let dy = i64::from(at.y) - i64::from(self.center.y);
        let r = i64::from(STEWARD_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        dx * dx + dy * dy <= r * r
    }
}
#[derive(Debug, Default)]
pub(crate) struct Manager {
    pub(crate) active: BTreeSet<EntityId>,
    /// Active domains per player with their centres; derived from `active`.
    pub(super) by_player: BTreeMap<PlayerId, BTreeMap<EntityId, WorldPosition>>,
    pub(crate) controlled: crate::game_world::changes::Reports<(EntityId, ResourceKind)>,
    /// Reverse owner of each bounded resource-refusal context.
    pub(super) refusal_owners: BTreeMap<EntityId, EntityId>,
    /// Only accepted owner orders on an already managed worker add membership.
    /// Live units bound this set; snapshots remove idle entries, deaths remove directly.
    pub(super) manual: BTreeSet<EntityId>,
    /// Worker -> destination until an automatic arrival is adopted or cancelled.
    pub(super) transfers: BTreeMap<EntityId, EntityId>,
    /// Villagers sent to another domain for want of work, until they work.
    pub(super) relocated: BTreeSet<EntityId>,
    /// Only set during an authoritative advance; never across an API boundary.
    pub(crate) automatic: bool,
    /// Issuing domain before a new worker has a controller. Transient within
    /// one admitted domain pass, including ordinary resource replacements.
    pub(super) ordering: Option<EntityId>,
    pub(super) work: StewardWork,
    /// Each tracked unit's domain (if any) and the tile it was placed from.
    pub(super) member_of: BTreeMap<EntityId, (Option<EntityId>, aoe_core::TileCoord)>,
}
impl GameWorld {
    pub(crate) fn register_steward(&mut self, id: EntityId) {
        let unit = self.unit(id).expect("newly spawned Steward");
        let player = unit.player;
        self.economy.steward_homes.insert(
            id,
            Home {
                id,
                player,
                rivals: Vec::new(),
                town_center: None,
                center: unit.position,
                workers: BTreeSet::new(),
                unemployed: BTreeMap::new(),
                worker_after: None,
                members: BTreeSet::new(),
                census_done: false,
                scan_chunk: 0,
                scan_offset: 0,
                building_scan: snapshot::BuildingScan::default(),
                site_after: 0,
                farm_after: None,
                repair_after: None,
                rebalance_after: 0,
                rebalance_cursor: None,
                assigned_at: BTreeMap::new(),
                repair_sites: BTreeMap::new(),
                held_jobs: BTreeMap::new(),
                resources: search::Search::default(),
                regions: None,
                walking: None,
                walk_recheck: false,
                navy: navy::Fleet::default(),
                stalls: BTreeMap::new(),
                status: StewardStatus::default(),
                settings: StewardSettings::default(),
            },
        );
    }
    pub fn steward_settings(&self, id: EntityId) -> Option<StewardSettings> {
        self.economy
            .steward_homes
            .get(&id)
            .map(|home| home.settings)
    }
    /// The resource this Steward's villagers would choose first right now.
    pub fn steward_favoured_resource(&self, id: EntityId) -> Option<ResourceKind> {
        let home = self.economy.steward_homes.get(&id)?;
        Some(
            self.steward_priority(home.player, home.settings.weights)?
                .kinds()[0],
        )
    }
    /// Owner command: change one of the owner's Stewards. A refused change
    /// leaves every setting untouched.
    pub fn issue_steward_settings(
        &mut self,
        player: PlayerId,
        steward: EntityId,
        settings: StewardSettings,
    ) -> Result<(), EconomyError> {
        let home = self
            .economy
            .steward_homes
            .get_mut(&steward)
            .ok_or(EconomyError::UnknownEntity)?;
        if home.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !settings.is_valid() {
            return Err(EconomyError::InvalidStewardSettings);
        }
        home.settings = settings;
        home.status.settings = settings;
        let Some(town_center) = home.town_center else {
            return Ok(());
        };
        if let Some(entry) = self
            .building_mut(town_center)
            .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == steward))
            && let Some(report) = &mut entry.steward
        {
            report.settings = settings;
        }
        Ok(())
    }
    pub fn steward_town_center(&self, id: EntityId) -> Option<EntityId> {
        self.economy
            .steward_homes
            .get(&id)
            .and_then(|home| home.town_center)
    }
    pub(crate) fn set_steward_town_center(&mut self, id: EntityId, town_center: Option<EntityId>) {
        let footprint = town_center
            .and_then(|c| self.building(c))
            .map(|b| b.footprint);
        let center = footprint.map(|rect| {
            WorldPosition::new(
                (rect.min.x + rect.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
                (rect.min.y + rect.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
            )
        });
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
            if h.town_center != town_center {
                h.walking = None;
                h.regions = None;
                h.restart_sweeps();
            }
            h.town_center = town_center;
            if let Some(center) = center {
                h.center = center;
            }
        }
        let known = self
            .economy
            .steward_homes
            .get(&id)
            .map(|h| (h.player, h.center));
        if let Some((player, center)) = known.filter(|_| town_center.is_some()) {
            self.economy.stewards.active.insert(id);
            self.economy
                .stewards
                .by_player
                .entry(player)
                .or_default()
                .insert(id, center);
        } else {
            self.economy.stewards.active.remove(&id);
            if let Some((player, _)) = known {
                let members = self.economy.stewards.by_player.entry(player).or_default();
                members.remove(&id);
                if members.is_empty() {
                    self.economy.stewards.by_player.remove(&player);
                }
            }
            let (workers, refused) = self
                .economy
                .steward_homes
                .get_mut(&id)
                .map(|h| {
                    h.stalls.clear();
                    h.held_jobs.clear();
                    h.repair_after = None;
                    h.rebalance_after = 0;
                    h.rebalance_cursor = None;
                    h.repair_sites.clear();
                    let refused = h.resources.clear_workers();
                    h.status = StewardStatus {
                        settings: h.settings,
                        ..StewardStatus::default()
                    };
                    h.resources = search::Search::default();
                    h.regions = None;
                    h.walking = None;
                    let ships = std::mem::take(&mut h.navy.ships);
                    h.workers.extend(ships);
                    h.navy = navy::Fleet::default();
                    h.worker_after = None;
                    h.unemployed.clear();
                    h.scan_chunk = 0;
                    h.scan_offset = 0;
                    h.census_done = false;
                    h.building_scan = snapshot::BuildingScan::default();
                    h.site_after = 0;
                    h.farm_after = None;
                    (std::mem::take(&mut h.workers), refused)
                })
                .unwrap_or_default();
            self.steward_release_members(id);
            for worker in workers {
                self.economy.stewards.controlled.remove(&worker);
            }
            for worker in refused {
                if self.economy.stewards.refusal_owners.get(&worker) == Some(&id) {
                    self.economy.stewards.refusal_owners.remove(&worker);
                }
            }
        }
        if let Some((player, _)) = known {
            if self.economy.walking.stewards.set(player, id, footprint) {
                // Until the player's zones are walked again (within a second),
                // the old ones would still give a neighbour's land to a domain:
                // ownership falls back to nearest-centre circles meanwhile.
                let members: Vec<_> = self
                    .economy
                    .stewards
                    .by_player
                    .get(&player)
                    .map(|m| m.keys().copied().collect())
                    .unwrap_or_default();
                for member in members {
                    if let Some(home) = self.economy.steward_homes.get_mut(&member) {
                        home.walking = None;
                        home.regions = None;
                        home.restart_sweeps();
                    }
                }
            }
            self.refresh_steward_territory(player);
        }
    }
    pub(crate) fn forget_steward(&mut self, id: EntityId) {
        self.forget_steward_worker(id);
        self.set_steward_town_center(id, None);
        self.economy.steward_homes.remove(&id);
    }
    fn active_steward(&self, id: EntityId) -> bool {
        self.economy.stewards.active.contains(&id)
            && self
                .steward_town_center(id)
                .and_then(|c| self.building(c))
                .is_some_and(|b| {
                    b.hp > 0
                        && b.is_complete()
                        && b.garrison.iter().any(|g| g.unit.id == id && g.unit.hp > 0)
                })
    }
    /// The managing Steward's unit ID, only while this worker is in its active domain.
    pub fn steward_controller(&self, id: EntityId) -> Option<EntityId> {
        // Frame construction asks for every visible unit. Most have no manager:
        // reject those before another world-unit lookup or shelter inspection.
        let (steward, _) = *self.economy.stewards.controlled.get(&id)?;
        let unit = self.unit(id).filter(|u| {
            matches!(u.kind, UnitKind::Villager | UnitKind::FishingShip)
                && u.hp > 0
                && !self.embarked(id)
        })?;
        let home = self.economy.steward_homes.get(&steward)?;
        (home.owns_resident(unit.kind, unit.position)
            && self.active_steward(steward)
            && self
                .building(home.town_center?)
                .is_some_and(|b| b.player == unit.player))
        .then_some(steward)
    }
    pub fn steward_work(&self) -> StewardWork {
        self.economy.stewards.work
    }
    pub(crate) fn player_override(&mut self, id: EntityId) {
        if !self.economy.stewards.automatic {
            self.vassal_player_override(id);
            self.economy.stewards.transfers.remove(&id);
            if self.economy.stewards.controlled.contains_key(&id) {
                self.economy.stewards.manual.insert(id);
            }
            self.forget_steward_refusals(id);
            self.release_steward_worker(id);
        }
    }
    pub(crate) fn forget_steward_worker(&mut self, id: EntityId) {
        self.economy.stewards.transfers.remove(&id);
        self.economy.stewards.relocated.remove(&id);
        self.economy.stewards.manual.remove(&id);
        self.forget_steward_refusals(id);
        self.release_steward_worker(id);
    }
    fn release_steward_worker(&mut self, id: EntityId) {
        if let Some((steward, _)) = self.economy.stewards.controlled.remove(&id) {
            if self.economy.stewards.refusal_owners.get(&id) == Some(&steward) {
                self.forget_steward_refusals(id);
            }
            if let Some(home) = self.economy.steward_homes.get_mut(&steward) {
                home.workers.remove(&id);
                home.assigned_at.remove(&id);
                home.held_jobs.remove(&id);
                home.navy.ships.remove(&id);
                home.resources.forget_worker(id);
            }
        }
    }
    pub(super) fn claim_steward_worker(
        &mut self,
        steward: EntityId,
        worker: EntityId,
        kind: ResourceKind,
    ) {
        self.claim_steward(steward, worker, kind, false);
    }
    /// A transfer's arrival is claimed even above the owner's cap.
    pub(super) fn claim_steward_arrival(
        &mut self,
        steward: EntityId,
        worker: EntityId,
        kind: ResourceKind,
    ) {
        self.claim_steward(steward, worker, kind, true);
    }
    fn claim_steward(
        &mut self,
        steward: EntityId,
        worker: EntityId,
        kind: ResourceKind,
        over_cap: bool,
    ) {
        self.economy.stewards.transfers.remove(&worker);
        if self
            .economy
            .stewards
            .controlled
            .get(&worker)
            .is_some_and(|(owner, _)| *owner != steward)
        {
            self.release_steward_worker(worker);
        }
        if self
            .economy
            .stewards
            .refusal_owners
            .get(&worker)
            .is_some_and(|owner| *owner != steward)
        {
            self.forget_steward_refusals(worker);
        }
        let ship = self
            .unit(worker)
            .is_some_and(|u| u.kind == UnitKind::FishingShip);
        if let Some(home) = self.economy.steward_homes.get_mut(&steward) {
            if ship {
                if !home.navy.ships.contains(&worker)
                    && home.navy.ships.len() >= STEWARD_RULES.max_villagers
                {
                    return;
                }
                home.navy.ships.insert(worker);
            } else {
                if !over_cap && !home.workers.contains(&worker) && home.workers.len() >= home.cap()
                {
                    return;
                }
                home.workers.insert(worker);
            }
            home.assigned_at.insert(worker, self.tick.0);
            self.economy
                .stewards
                .controlled
                .insert(worker, (steward, kind));
        }
    }
    /// Restrict autonomous replacement resources and drop-offs to the domain.
    pub(crate) fn managed_target_allowed(&self, worker: EntityId, at: WorldPosition) -> bool {
        self.steward_controller(worker)
            .or(self.economy.stewards.ordering)
            .and_then(|v| self.economy.steward_homes.get(&v))
            .is_none_or(|h| {
                self.unit(worker).is_some_and(|u| {
                    u.player == h.player
                        && match u.kind {
                            UnitKind::FishingShip => h.owns_water(at),
                            UnitKind::Villager => {
                                h.owns(at)
                                    && h.connects_tile(u.position.tile_floor(), at.tile_floor())
                            }
                            _ => h.owns(at),
                        }
                })
            })
    }
    /// Latest explanation of what this Steward's domain is doing.
    pub fn steward_status(&self, id: EntityId) -> Option<StewardStatus> {
        self.economy
            .steward_homes
            .get(&id)
            .filter(|_| self.economy.stewards.active.contains(&id))
            .map(|h| h.status)
    }
}
