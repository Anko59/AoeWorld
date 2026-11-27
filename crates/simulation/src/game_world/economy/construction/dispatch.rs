//! Shared automatic construction for placed foundations.
use super::super::gathering::WorkTarget;
use super::{super::buildings::BUILD_WORK_PER_BASE_TICK, *};
use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Unbounded},
};

// Castle wall rings can extend more than 64 tiles from the village.
const REACH: i32 = 128;
const RESIDENTS_PER_BUILDER: usize = 2;
const RESIDENTS_PER_BORROW: usize = 4;
// A player's own gatherers answer a foundation only from nearby.
const MANUAL_REACH: i32 = 64;

#[derive(Debug, Default)]
pub(crate) struct ConstructionDispatch {
    pub(crate) sites: BTreeMap<EntityId, Option<EntityId>>,
    pub(crate) cursor: Option<EntityId>,
    /// Manual gatherers lent to construction and the job each one returns to.
    /// Like Steward held jobs this is transient: a restore drops it.
    pub(crate) lent: BTreeMap<EntityId, (aoe_map::ResourceKind, WorkTarget)>,
}

impl GameWorld {
    pub(in super::super) fn placement_builders(
        &self,
        player: PlayerId,
        builders: &[EntityId],
        trap: bool,
    ) -> Result<Vec<EntityId>, EconomyError> {
        if builders.is_empty() && !trap {
            return Ok(Vec::new());
        }
        self.validate_builders(player, builders, trap)
    }

    pub(in super::super) fn queue_foundations(
        &mut self,
        ids: &[EntityId],
        vassal: Option<EntityId>,
    ) {
        self.economy
            .dispatch
            .sites
            .extend(ids.iter().map(|&id| (id, vassal)));
    }

    pub(in super::super) fn dispatch_construction(&mut self, changed: &mut Vec<EntityId>) {
        let dispatch = &self.economy.dispatch;
        let period = self.config.ticks(4);
        if dispatch.sites.is_empty() || !self.tick.0.is_multiple_of(period) {
            return;
        }
        // Spread a sweep of the queued sites across roughly one game second.
        // Dispatch throughput grows with the queue instead of capping the world.
        let passes_per_second = u64::from(self.config.tick_hz).div_ceil(period) as usize;
        let sites_this_pass = dispatch.sites.len().div_ceil(passes_per_second);
        let cursor = dispatch.cursor.unwrap_or(EntityId(0));
        let sites: Vec<_> = dispatch
            .sites
            .range((Excluded(cursor), Unbounded))
            .chain(dispatch.sites.range(..=cursor))
            .map(|(&id, _)| id)
            .take(sites_this_pass)
            .collect();
        let census = self.unit_census();
        let mut builders = BTreeMap::<PlayerId, (usize, usize)>::new();
        for (&worker, task) in &self.economy.builders {
            if let Some(building) = self.economy.buildings.get(&task.building)
                && building.kind != BuildingKind::FishTrap
            {
                let (active, borrowed) = builders.entry(building.player).or_default();
                *borrowed += usize::from(
                    self.steward_has_held_job(worker) || dispatch.lent.contains_key(&worker),
                );
                // A Steward's seeder is starting a food job. Far-view Farms
                // still use a shared construction slot.
                let steward_seeding = building.kind == BuildingKind::Farm
                    && !dispatch.sites.contains_key(&task.building)
                    && self.steward_controller(worker).is_some();
                *active += usize::from(!steward_seeding);
            }
        }
        if !self.economy.dispatch.lent.is_empty() {
            let working = &self.economy.builders;
            self.economy
                .dispatch
                .lent
                .retain(|id, _| working.contains_key(id));
        }
        let automatic = std::mem::replace(&mut self.economy.stewards.automatic, true);
        for id in sites {
            self.economy.dispatch.cursor = Some(id);
            let Some(player) = self.economy.buildings.get(&id).map(|b| b.player) else {
                self.economy.dispatch.sites.remove(&id);
                continue;
            };
            let villagers = census
                .get(&player)
                .map_or(0, |row| row[UnitKind::Villager as usize] as usize);
            let limit = (villagers / RESIDENTS_PER_BUILDER).max(1);
            let borrow_limit = (villagers / RESIDENTS_PER_BORROW).max(1);
            let (active, borrowed) = builders.entry(player).or_default();
            let (assigned, taken) = self.dispatch_foundation(
                id,
                limit.saturating_sub(*active),
                borrow_limit.saturating_sub(*borrowed),
                changed,
            );
            *active += assigned;
            *borrowed += taken;
        }
        self.economy.stewards.automatic = automatic;
    }

    pub(super) fn automatic_builder_limit(&self, b: &crate::GameBuilding) -> usize {
        let remaining = b.work_total.saturating_sub(b.work_done);
        let work_per_builder = 20 * u64::from(self.config.tick_hz) * BUILD_WORK_PER_BASE_TICK;
        let footprint = b.footprint;
        let edges = 2 * (footprint.width() + footprint.height()) as usize;
        (remaining.div_ceil(work_per_builder) as usize).clamp(1, edges)
    }

    fn dispatch_foundation(
        &mut self,
        id: EntityId,
        capacity: usize,
        borrow_capacity: usize,
        changed: &mut Vec<EntityId>,
    ) -> (usize, usize) {
        let Some(b) = self.building(id).filter(|b| b.hp > 0 && !b.is_complete()) else {
            self.economy.dispatch.sites.remove(&id);
            return (0, 0);
        };
        let (player, footprint) = (b.player, b.footprint);
        let tile = footprint.min;
        let need = self.automatic_builder_limit(&b);
        let assigned = self.economy.builders.assigned_count(id);
        if assigned >= need || capacity == 0 {
            return (0, 0);
        }
        let area =
            TileRect::from_xywh(tile.x - REACH, tile.y - REACH, 2 * REACH + 1, 2 * REACH + 1)
                .clamp(self.config.width_tiles, self.config.height_tiles);
        let mut candidates = Vec::new();
        for u in self.query(area).0 {
            let distance = distance_squared(tile, u.position.tile_floor());
            if u.player != player
                || u.kind != UnitKind::Villager
                || u.hp == 0
                || distance > (REACH as u64).pow(2)
                || self.embarked(u.id)
            {
                continue;
            }
            if self.economy.builders.contains_key(&u.id) {
                continue;
            }
            if u.carried.is_some()
                || u.attack_target.is_some()
                || self.economy.stewards.transfers.contains_key(&u.id)
                || self.economy.garrison_orders.contains_key(&u.id)
                || self.steward_has_held_job(u.id)
            {
                continue;
            }
            let gathering = self.economy.gatherers.contains_key(&u.id);
            let manual = self.economy.stewards.manual.contains(&u.id);
            let idle =
                !manual && u.task == UnitTask::Idle && !u.moving && !u.planning && !gathering;
            let steward = self.steward_controller(u.id);
            if let Some(steward) = steward
                && !self.economy.steward_homes[&steward].covers(footprint, 1)
            {
                continue;
            }
            // Idle first, then Steward-managed gatherers, then the player's own.
            let tier = if idle {
                0
            } else if gathering && steward.is_some() {
                1
            } else if gathering && distance <= (MANUAL_REACH as u64).pow(2) {
                2
            } else {
                continue;
            };
            candidates.push((tier, distance, u.id));
        }
        let wanted = (need - assigned).min(capacity);
        let tries = wanted + 2;
        if candidates.len() > tries {
            candidates.select_nth_unstable(tries);
            candidates.truncate(tries);
        }
        candidates.sort_unstable();
        let mut admitted = 0;
        let mut borrowed = 0;
        for (tier, _, worker) in candidates {
            let from_gathering = tier > 0;
            if from_gathering && borrowed >= borrow_capacity {
                break;
            }
            let job = (tier == 2)
                .then(|| self.economy.gatherers.get(&worker))
                .flatten()
                .map(|task| {
                    let target = task
                        .animal
                        .map(WorkTarget::Animal)
                        .or_else(|| task.farm.map(WorkTarget::Farm))
                        .unwrap_or(WorkTarget::Node(task.node));
                    (task.kind, target)
                });
            if self.issue_construct(player, &[worker], id).is_ok() {
                if let Some(job) = job {
                    self.economy.dispatch.lent.insert(worker, job);
                }
                changed.push(worker);
                borrowed += usize::from(from_gathering);
                admitted += 1;
                if admitted == wanted {
                    break;
                }
            }
        }
        (admitted, borrowed)
    }
}
