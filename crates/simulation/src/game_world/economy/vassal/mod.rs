//! Castle-bound defensive domains. No active domains means no census or allocation.
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind, UnitTask};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use std::collections::{BTreeMap, BTreeSet};
mod defence;
mod demo;
mod development;
mod hash;
mod plan;
mod ring;
mod road_demo;
mod roads;
mod threat;

pub const VASSAL_LIMIT: usize = 20;
/// The domain a Vassal develops, fortifies and defends: a Steward's circle.
pub const VASSAL_RADIUS: i32 = super::steward::STEWARD_RULES.radius_tiles;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VassalWait {
    #[default]
    Building,
    Reserve,
    Builders,
    Blocked,
    Paused,
    Developed,
    Age,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VassalStatus {
    pub villagers: u8,
    pub military: u8,
    pub development: bool,
    pub developed: bool,
    pub alert: bool,
    pub next: Option<BuildingKind>,
    pub wait: VassalWait,
    pub completed: u16,
    pub total: u16,
    pub responding: u16,
}
#[derive(Debug)]
pub(crate) struct Domain {
    castle: EntityId,
    player: PlayerId,
    center: TileCoord,
    workers: BTreeSet<EntityId>,
    soldiers: BTreeSet<EntityId>,
    development: bool,
    cursor: usize,
    refused: BTreeMap<TileCoord, u64>,
    progress: Option<(EntityId, u64, u64)>,
    status: VassalStatus,
    /// Cached plan (see `plan::REPLAN_TICKS`), its age and the tick it was made.
    plan: Vec<plan::Site>,
    plan_age: crate::Age,
    planned: u64,
    /// Where the latest raider was seen: the threatened side is walled first.
    hot: Option<TileCoord>,
}
#[derive(Clone, Copy, Debug)]
struct Recall {
    vassal: EntityId,
    destination: Option<WorldPosition>,
    position: WorldPosition,
    movement: Option<WorldPosition>,
    solo: bool,
    called: WorldPosition,
}
#[derive(Debug, Default)]
pub(crate) struct Manager {
    domains: BTreeMap<EntityId, Domain>,
    controlled: BTreeMap<EntityId, EntityId>,
    /// An owner-commanded soldier is not recruited again when its order ends.
    pub(super) manual: BTreeSet<EntityId>,
    recalls: BTreeMap<EntityId, Recall>,
    returns: BTreeMap<EntityId, WorldPosition>,
    cursor: Option<EntityId>,
}
impl GameWorld {
    pub(crate) fn activate_vassal(&mut self, id: EntityId, castle: EntityId) {
        let Some(b) = self
            .building(castle)
            .filter(|b| b.kind == BuildingKind::Castle)
        else {
            return;
        };
        let center = TileCoord::new(
            (b.footprint.min.x + b.footprint.max.x) / 2,
            (b.footprint.min.y + b.footprint.max.y) / 2,
        );
        self.economy.vassals.domains.insert(
            id,
            Domain {
                castle,
                player: b.player,
                center,
                workers: BTreeSet::new(),
                soldiers: BTreeSet::new(),
                development: true,
                cursor: 0,
                refused: BTreeMap::new(),
                progress: None,
                plan: Vec::new(),
                plan_age: crate::Age::Dark,
                planned: 0,
                hot: None,
                status: VassalStatus {
                    development: true,
                    ..Default::default()
                },
            },
        );
    }
    pub fn vassal_status(&self, id: EntityId) -> Option<VassalStatus> {
        self.economy.vassals.domains.get(&id).map(|d| d.status)
    }
    pub fn vassal_controller(&self, id: EntityId) -> Option<EntityId> {
        self.economy.vassals.controlled.get(&id).copied()
    }
    pub fn issue_vassal_development(
        &mut self,
        player: PlayerId,
        id: EntityId,
        enabled: bool,
    ) -> Result<(), EconomyError> {
        let d = self
            .economy
            .vassals
            .domains
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if d.player != player {
            return Err(EconomyError::NotOwner);
        }
        d.development = enabled;
        d.status.development = enabled;
        if !enabled {
            self.release_vassal_builders(id);
        }
        Ok(())
    }
    fn release_vassal_builders(&mut self, id: EntityId) {
        let workers = self
            .economy
            .vassals
            .domains
            .get_mut(&id)
            .map(|d| std::mem::take(&mut d.workers))
            .unwrap_or_default();
        for worker in workers {
            self.economy.vassals.controlled.remove(&worker);
            if let Some(u) = self.unit(worker) {
                let _ = self.issue_stop(u.player, worker);
            }
        }
    }
    pub(crate) fn deactivate_vassal(&mut self, id: EntityId) {
        self.release_vassal_builders(id);
        if let Some(d) = self.economy.vassals.domains.remove(&id) {
            for soldier in d.soldiers {
                self.economy.vassals.controlled.remove(&soldier);
            }
        }
        if self.economy.vassals.domains.is_empty() {
            self.economy.vassals.cursor = None;
        }
        self.finish_vassal_recall(id);
    }
    pub(crate) fn release_vassal_unit(&mut self, id: EntityId) {
        if let Some(v) = self.economy.vassals.controlled.remove(&id)
            && let Some(d) = self.economy.vassals.domains.get_mut(&v)
        {
            d.workers.remove(&id);
            d.soldiers.remove(&id);
        }
    }
    pub(crate) fn vassal_player_override(&mut self, id: EntityId) {
        if self.economy.stewards.automatic || self.vassal_controller(id).is_none() {
            return;
        }
        if self.unit(id).is_some_and(|u| u.kind != UnitKind::Villager) {
            self.economy.vassals.manual.insert(id);
        }
        self.release_vassal_unit(id);
    }
    pub(crate) fn forget_vassal_unit(&mut self, id: EntityId) {
        self.release_vassal_unit(id);
        self.economy.vassals.manual.remove(&id);
    }
    pub(crate) fn advance_vassals(&mut self, changed: &mut Vec<EntityId>) {
        if (self.economy.vassals.domains.is_empty() && self.economy.vassals.returns.is_empty())
            || !self.tick.0.is_multiple_of(20)
        {
            return;
        }
        self.finish_vassal_returns();
        use std::ops::Bound::{Excluded, Unbounded};
        let cursor = self.economy.vassals.cursor.unwrap_or(EntityId(0));
        let ids: Vec<_> = self
            .economy
            .vassals
            .domains
            .range((Excluded(cursor), Unbounded))
            .chain(self.economy.vassals.domains.range(..=cursor))
            .map(|(&id, _)| id)
            .take(8)
            .collect();
        for id in ids {
            self.economy.vassals.cursor = Some(id);
            let d = &self.economy.vassals.domains[&id];
            if !self.building(d.castle).is_some_and(|b| {
                b.hp > 0 && b.garrison.iter().any(|g| g.unit.id == id && g.unit.hp > 0)
            }) {
                self.deactivate_vassal(id);
                continue;
            }
            let (center, player) = (d.center, d.player);
            let stale: Vec<_> = d
                .workers
                .iter()
                .chain(&d.soldiers)
                .copied()
                .filter(|&unit| {
                    self.unit(unit).is_none_or(|u| {
                        u.hp == 0
                            || (u.kind == UnitKind::Villager
                                && !within(center, u.position, VASSAL_RADIUS + 8)
                                && d.status.next != Some(BuildingKind::Road))
                            || self.embarked(unit)
                    })
                })
                .collect();
            for unit in stale {
                self.release_vassal_unit(unit);
            }
            let rect = TileRect::from_xywh(
                center.x - VASSAL_RADIUS,
                center.y - VASSAL_RADIUS,
                2 * VASSAL_RADIUS + 1,
                2 * VASSAL_RADIUS + 1,
            )
            .clamp(self.config.width_tiles, self.config.height_tiles);
            let units = self.query(rect).0;
            for u in units
                .iter()
                .filter(|u| u.player == player && u.hp > 0 && inside(center, u.position))
            {
                if self.vassal_controller(u.id).is_some()
                    || self.general_controller(u.id).is_some()
                    || self.economy.vassals.manual.contains(&u.id)
                    || self.embarked(u.id)
                    || u.moving
                    || u.planning
                    || u.task != UnitTask::Idle
                    || self.economy.garrison_orders.contains_key(&u.id)
                {
                    continue;
                }
                let worker = u.kind == UnitKind::Villager;
                if !worker && super::general::role(u.kind).is_none() {
                    continue;
                }
                // A Steward's villagers and the town's own (the starting
                // villagers by a Town Center) stay with the town.
                if worker
                    && (self.steward_controller(u.id).is_some()
                        || self.by_own_town_center(player, u.position))
                {
                    continue;
                }
                let d = &self.economy.vassals.domains[&id];
                if worker
                    && (!d.development || d.status.developed || d.workers.len() >= VASSAL_LIMIT)
                    || !worker && d.soldiers.len() >= VASSAL_LIMIT
                {
                    continue;
                }
                self.forget_steward_worker(u.id);
                let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
                    continue;
                };
                if worker {
                    d.workers.insert(u.id);
                } else {
                    d.soldiers.insert(u.id);
                }
                self.economy.vassals.controlled.insert(u.id, id);
            }
            self.vassal_defend(id, &units, changed);
            self.vassal_develop(id);
            let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
                continue;
            };
            d.status.villagers = d.workers.len() as u8;
            d.status.military = d.soldiers.len() as u8;
            let (castle, status) = (d.castle, d.status);
            if let Some(g) = self
                .economy
                .buildings
                .get_mut(&castle)
                .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == id))
            {
                g.vassal = Some(status);
            }
        }
    }
}
/// Tiles around a Town Center whose idle villagers belong to the town.
const TOWN_TILES: i32 = 12;
impl GameWorld {
    fn by_own_town_center(&self, player: PlayerId, at: WorldPosition) -> bool {
        self.player_buildings_of(player, BuildingKind::TownCenter)
            .any(|b| {
                let c = TileCoord::new(
                    (b.footprint.min.x + b.footprint.max.x) / 2,
                    (b.footprint.min.y + b.footprint.max.y) / 2,
                );
                within(c, at, TOWN_TILES)
            })
    }
}
fn inside(center: TileCoord, at: WorldPosition) -> bool {
    within(center, at, VASSAL_RADIUS)
}
/// Builders of the wall ring's corners stand just outside the radius.
fn within(center: TileCoord, at: WorldPosition, radius: i32) -> bool {
    super::general::distance(WorldPosition::new(center.x * 1024, center.y * 1024), at)
        <= (i64::from(radius) * 1024).pow(2)
}

impl GameWorld {
    pub(super) fn vassal_land_sources(&self) -> Vec<(PlayerId, TileCoord, i32)> {
        self.economy
            .vassals
            .domains
            .values()
            .map(|d| (d.player, d.center, VASSAL_RADIUS))
            .collect()
    }
}
