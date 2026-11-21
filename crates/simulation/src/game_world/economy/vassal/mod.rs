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
mod road_cache;
mod road_demo;
mod road_job;
mod road_search;
mod roads;
mod threat;
mod work;
pub use work::{VassalFocus, VassalJob, VassalJobs};

pub const VASSAL_LIMIT: usize = 20;
/// The domain a Vassal develops, fortifies and defends: a Steward's circle.
pub const VASSAL_RADIUS: i32 = super::steward::STEWARD_RULES.radius_tiles;
const PASS_TICKS: u64 = 20;

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
pub struct VassalSquad {
    pub holding: u8,
    pub moving: u8,
    pub attacking: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VassalStatus {
    pub military: u8,
    /// Infantry, archers, cavalry, siege and scouts, in that order.
    pub squads: [VassalSquad; 5],
    pub development: bool,
    pub developed: bool,
    pub alert: bool,
    pub next: Option<BuildingKind>,
    pub wait: VassalWait,
    pub completed: u16,
    pub total: u16,
    pub responding: u16,
    pub focus: VassalFocus,
    pub current: VassalJobs,
    pub planned: VassalJobs,
    pub finished: VassalJobs,
}
#[derive(Debug)]
pub(crate) struct Domain {
    castle: EntityId,
    player: PlayerId,
    center: TileCoord,
    walking: Option<std::sync::Arc<super::land::walking::Walking>>,
    soldiers: BTreeSet<EntityId>,
    development: bool,
    cursor: usize,
    refused: BTreeMap<TileCoord, u64>,
    progress: Option<(EntityId, u64, u64)>,
    status: VassalStatus,
    /// Cached plan (see `plan::REPLAN_TICKS`), its age and the tick it was made.
    plan: Vec<plan::Site>,
    /// Paid defensive foundations in the territory at the last survey.
    paid: Vec<EntityId>,
    plan_age: crate::Age,
    planned: u64,
    /// Earliest explicit rebuild deadline, independent of road completion.
    replan_at: Option<u64>,
    pending: Option<road_job::Planner>,
    /// Where the latest raider was seen: the threatened side is walled first.
    hot: Option<TileCoord>,
}
impl Domain {
    fn request_replan(&mut self, at: u64) {
        self.replan_at = Some(self.replan_at.map_or(at, |earlier| earlier.min(at)));
    }
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
    plan_cursor: Option<EntityId>,
    walk_second: Option<u64>,
}
impl Manager {
    fn scheduled_domain(&self, tick: u64) -> Option<EntityId> {
        use std::ops::Bound::{Excluded, Unbounded};

        let due = (0..self.domains.len()).take(8).len() as u64;
        let phase = tick % PASS_TICKS;
        // Spend the existing visit budget across the period. As the budget is
        // less than the period, consecutive boundaries differ by at most one.
        if ((phase + 1) * due).div_ceil(PASS_TICKS) == (phase * due).div_ceil(PASS_TICKS) {
            return None;
        }
        let cursor = self.cursor.unwrap_or(EntityId(0));
        self.domains
            .range((Excluded(cursor), Unbounded))
            .next()
            .or_else(|| self.domains.first_key_value())
            .map(|(&id, _)| id)
    }
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
                walking: None,
                soldiers: BTreeSet::new(),
                development: true,
                cursor: 0,
                refused: BTreeMap::new(),
                progress: None,
                plan: Vec::new(),
                paid: Vec::new(),
                plan_age: crate::Age::Dark,
                planned: 0,
                replan_at: None,
                pending: None,
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
    fn owned_vassal(
        &mut self,
        player: PlayerId,
        id: EntityId,
    ) -> Result<&mut Domain, EconomyError> {
        let d = self
            .economy
            .vassals
            .domains
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if d.player != player {
            return Err(EconomyError::NotOwner);
        }
        Ok(d)
    }

    pub fn issue_vassal_development(
        &mut self,
        player: PlayerId,
        id: EntityId,
        enabled: bool,
    ) -> Result<(), EconomyError> {
        let d = self.owned_vassal(player, id)?;
        d.development = enabled;
        d.status.development = enabled;
        Ok(())
    }
    pub(crate) fn deactivate_vassal(&mut self, id: EntityId) {
        if let Some(d) = self.economy.vassals.domains.remove(&id) {
            if !self
                .economy
                .vassals
                .domains
                .values()
                .any(|other| other.player == d.player)
            {
                self.economy.walking.vassals.remove(&d.player);
            }
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
            d.soldiers.remove(&id);
        }
    }
    pub(crate) fn vassal_player_override(&mut self, id: EntityId) {
        if self.economy.stewards.automatic || self.vassal_controller(id).is_none() {
            return;
        }
        self.economy.vassals.manual.insert(id);
        self.release_vassal_unit(id);
    }
    pub(crate) fn forget_vassal_unit(&mut self, id: EntityId) {
        self.release_vassal_unit(id);
        self.economy.vassals.manual.remove(&id);
    }
    pub(crate) fn advance_vassals(&mut self, changed: &mut Vec<EntityId>) {
        self.refresh_vassal_walk();
        self.advance_vassal_plans();
        if self.tick.0.is_multiple_of(PASS_TICKS) && !self.economy.vassals.returns.is_empty() {
            self.finish_vassal_returns();
        }
        if let Some(id) = self.economy.vassals.scheduled_domain(self.tick.0) {
            self.economy.vassals.cursor = Some(id);
            let d = &self.economy.vassals.domains[&id];
            if !self.building(d.castle).is_some_and(|b| {
                b.hp > 0 && b.garrison.iter().any(|g| g.unit.id == id && g.unit.hp > 0)
            }) {
                self.deactivate_vassal(id);
                return;
            }
            let (center, player) = (d.center, d.player);
            let walking = d.walking.clone();
            let stale: Vec<_> = d
                .soldiers
                .iter()
                .copied()
                .filter(|&unit| {
                    self.unit(unit)
                        .is_none_or(|u| u.hp == 0 || self.embarked(unit))
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
            for u in units.iter().filter(|u| {
                u.player == player && u.hp > 0 && owns(walking.as_deref(), id, center, u.position)
            }) {
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
                if u.kind == UnitKind::Villager || super::general::role(u.kind).is_none() {
                    continue;
                }
                let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
                    break;
                };
                if d.soldiers.len() >= VASSAL_LIMIT {
                    break;
                }
                d.soldiers.insert(u.id);
                self.economy.vassals.controlled.insert(u.id, id);
            }
            self.vassal_defend(id, &units, changed);
            self.vassal_develop(id);
            let squads = self.vassal_squads(id);
            let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
                return;
            };
            d.status.military = d.soldiers.len() as u8;
            d.status.squads = squads;
            let (castle, status) = (d.castle, d.status);
            if let Some(g) = self
                .economy
                .buildings
                .get_mut(&castle)
                .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == id))
                && g.vassal.as_deref() != Some(&status)
            {
                g.vassal = Some(std::sync::Arc::new(status));
                self.mark_building_changed(castle);
            }
        }
    }
}
fn owns(
    walking: Option<&super::land::walking::Walking>,
    id: EntityId,
    center: TileCoord,
    at: WorldPosition,
) -> bool {
    walking.map_or_else(
        || within(center, at, VASSAL_RADIUS),
        |walking| walking.owns(id, at.tile_floor()),
    )
}
fn within(center: TileCoord, at: WorldPosition, radius: i32) -> bool {
    super::general::distance(WorldPosition::new(center.x * 1024, center.y * 1024), at)
        <= (i64::from(radius) * 1024).pow(2)
}

impl GameWorld {
    fn vassal_owns(&self, id: EntityId, at: WorldPosition) -> bool {
        self.economy
            .vassals
            .domains
            .get(&id)
            .is_some_and(|d| owns(d.walking.as_deref(), id, d.center, at))
    }

    fn refresh_vassal_walk(&mut self) {
        if self.economy.vassals.domains.is_empty() {
            return;
        }
        let second = self.tick.0 / u64::from(self.config.tick_hz);
        if self.economy.vassals.walk_second == Some(second) {
            return;
        }
        self.economy.vassals.walk_second = Some(second);
        let mut owners = BTreeMap::<PlayerId, Vec<(EntityId, TileRect)>>::new();
        for (&id, domain) in &self.economy.vassals.domains {
            if let Some(castle) = self.building(domain.castle) {
                owners
                    .entry(domain.player)
                    .or_default()
                    .push((id, castle.footprint));
            }
        }
        let previous = std::mem::take(&mut self.economy.walking.vassals);
        for (player, sources) in owners {
            let walking = self.walking_group(sources.clone(), previous.get(&player));
            for (id, _) in sources {
                if let Some(domain) = self.economy.vassals.domains.get_mut(&id) {
                    domain.walking = Some(walking.clone());
                }
            }
            self.economy.walking.vassals.insert(player, walking);
        }
    }

    pub(super) fn vassal_land_sources(&self) -> Vec<(PlayerId, TileCoord, i32)> {
        self.economy
            .vassals
            .domains
            .values()
            .map(|d| (d.player, d.center, VASSAL_RADIUS))
            .collect()
    }
}
