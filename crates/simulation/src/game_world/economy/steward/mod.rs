//! A mobile Steward's current TownCenter domain and budgeted economy.
use crate::{EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, TileRect, WorldPosition};
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
    pub(super) worker_after: Option<EntityId>,
    pub(super) scan_chunk: usize,
    pub(super) scan_offset: usize,
    pub(super) site_after: usize,
    pub(super) farm_after: Option<usize>,
    pub(super) repair_after: Option<EntityId>,
    pub(super) repair_sites: BTreeMap<EntityId, usize>,
    resources: search::Search,
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
    pub(super) fn contains(&self, at: WorldPosition) -> bool {
        let dx = i64::from(at.x) - i64::from(self.center.x);
        let dy = i64::from(at.y) - i64::from(self.center.y);
        let r = i64::from(STEWARD_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        dx * dx + dy * dy <= r * r
    }
    /// Every margin-expanded corner lies in the circle and the building's
    /// centre lies in this Steward's own territory.
    pub(super) fn covers(&self, rect: TileRect, margin: i32) -> bool {
        self.owns(WorldPosition::new(
            (rect.min.x + rect.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
            (rect.min.y + rect.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
        )) && [rect.min.x - margin, rect.max.x + margin]
            .into_iter()
            .all(|x| {
                [rect.min.y - margin, rect.max.y + margin]
                    .into_iter()
                    .all(|y| {
                        self.contains(WorldPosition::new(
                            x * FIXED_SUBUNITS_PER_TILE,
                            y * FIXED_SUBUNITS_PER_TILE,
                        ))
                    })
            })
    }
}
#[derive(Debug, Default)]
pub(crate) struct Manager {
    pub(super) active: BTreeSet<EntityId>,
    /// Active domains per player with their centres; derived from `active`.
    pub(super) by_player: BTreeMap<PlayerId, BTreeMap<EntityId, WorldPosition>>,
    pub(super) controlled: BTreeMap<EntityId, (EntityId, ResourceKind)>,
    /// Reverse owner of each bounded resource-refusal context.
    pub(super) refusal_owners: BTreeMap<EntityId, EntityId>,
    /// Only accepted owner orders on an already managed worker add membership.
    /// Live units bound this set; snapshots remove idle entries, deaths remove directly.
    pub(super) manual: BTreeSet<EntityId>,
    /// Worker -> destination until an automatic arrival is adopted or cancelled.
    pub(super) transfers: BTreeMap<EntityId, EntityId>,
    pub(super) cursor: Option<EntityId>,
    /// Only set during an authoritative advance; never across an API boundary.
    pub(crate) automatic: bool,
    /// Issuing domain before a new worker has a controller. Transient within
    /// one admitted domain pass, including ordinary resource replacements.
    pub(super) ordering: Option<EntityId>,
    pub(super) work: StewardWork,
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
                worker_after: None,
                scan_chunk: 0,
                scan_offset: 0,
                site_after: 0,
                farm_after: None,
                repair_after: None,
                repair_sites: BTreeMap::new(),
                resources: search::Search::default(),
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
            .economy
            .buildings
            .get_mut(&town_center)
            .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == steward))
            && let Some(report) = &mut entry.steward
        {
            report.settings = settings;
        }
        Ok(())
    }
    /// Kill idle, unclaimed own villagers in this active Steward's territory.
    pub fn issue_steward_dismiss(
        &mut self,
        player: PlayerId,
        steward: EntityId,
    ) -> Result<(), EconomyError> {
        let home = self
            .economy
            .steward_homes
            .get(&steward)
            .ok_or(EconomyError::UnknownEntity)?;
        if home.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !self.active_steward(steward) {
            return Err(EconomyError::InvalidTarget);
        }
        let center = home.center.tile_floor();
        let r = STEWARD_RULES.radius_tiles;
        let victims: Vec<_> = self
            .query(
                TileRect::from_xywh(center.x - r, center.y - r, 2 * r + 1, 2 * r + 1)
                    .clamp(self.config.width_tiles, self.config.height_tiles),
            )
            .0
            .into_iter()
            .filter(|u| {
                u.player == player
                    && u.kind == UnitKind::Villager
                    && u.hp > 0
                    && matches!(u.task, crate::UnitTask::Idle | crate::UnitTask::Carrying)
                    && !u.moving
                    && !u.planning
                    && !self.economy.gatherers.contains_key(&u.id)
                    && !self.economy.builders.contains_key(&u.id)
                    && !self.economy.garrison_orders.contains_key(&u.id)
                    && home.owns(u.position)
                    && self.steward_controller(u.id).is_none()
            })
            .map(|u| u.id)
            .collect();
        for id in victims {
            if let Some(&index) = self.lookup.get(&id) {
                self.kill_unit(index);
            }
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
        let center = town_center.and_then(|c| self.building(c)).map(|b| {
            WorldPosition::new(
                (b.footprint.min.x + b.footprint.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
                (b.footprint.min.y + b.footprint.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
            )
        });
        if let Some(h) = self.economy.steward_homes.get_mut(&id) {
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
                    h.repair_after = None;
                    h.repair_sites.clear();
                    let refused = h.resources.clear_workers();
                    h.status = StewardStatus {
                        settings: h.settings,
                        ..StewardStatus::default()
                    };
                    h.resources = search::Search::default();
                    let ships = std::mem::take(&mut h.navy.ships);
                    h.workers.extend(ships);
                    h.navy = navy::Fleet::default();
                    h.worker_after = None;
                    h.scan_chunk = 0;
                    h.scan_offset = 0;
                    h.site_after = 0;
                    h.farm_after = None;
                    (std::mem::take(&mut h.workers), refused)
                })
                .unwrap_or_default();
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
        (home.owns(unit.position)
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
            self.release_vassal_unit(id);
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
        if self.vassal_controller(worker).is_some() {
            return;
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
                if !home.workers.contains(&worker) && home.workers.len() >= home.cap() {
                    return;
                }
                home.workers.insert(worker);
            }
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
            .is_none_or(|h| h.owns(at) && self.unit(worker).is_some_and(|u| u.player == h.player))
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
