//! Shared automatic construction for placed foundations.
use super::{super::buildings::BUILD_WORK_PER_BASE_TICK, *};
use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Unbounded},
};

// Castle wall rings can extend more than 64 tiles from the village.
const REACH: i32 = 128;
const RESIDENTS_PER_BUILDER: usize = 2;
const RESIDENTS_PER_BORROW: usize = 4;

#[derive(Debug, Default)]
pub(crate) struct ConstructionDispatch {
    pub(crate) sites: BTreeMap<EntityId, Option<EntityId>>,
    pub(crate) cursor: Option<EntityId>,
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
                *borrowed += usize::from(self.steward_has_held_job(worker));
                // A Steward's seeder is starting a food job. Far-view Farms
                // still use a shared construction slot.
                let steward_seeding = building.kind == BuildingKind::Farm
                    && !dispatch.sites.contains_key(&task.building)
                    && self.steward_controller(worker).is_some();
                *active += usize::from(!steward_seeding);
            }
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

    pub(super) fn dispatch_builder_limit(&self, b: &crate::GameBuilding) -> usize {
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
        let need = self.dispatch_builder_limit(&b);
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
                || self.economy.stewards.manual.contains(&u.id)
                || self.economy.stewards.transfers.contains_key(&u.id)
                || self.economy.garrison_orders.contains_key(&u.id)
                || self.steward_has_held_job(u.id)
            {
                continue;
            }
            let idle = u.task == UnitTask::Idle
                && !u.moving
                && !u.planning
                && !self.economy.gatherers.contains_key(&u.id);
            let steward = self.steward_controller(u.id);
            if let Some(steward) = steward
                && !self.economy.steward_homes[&steward].covers(footprint, 1)
            {
                continue;
            }
            let steward_gatherer = steward.is_some() && self.economy.gatherers.contains_key(&u.id);
            if idle || steward_gatherer {
                candidates.push((!idle, distance, u.id));
            }
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
        for (from_gathering, _, worker) in candidates {
            if from_gathering && borrowed >= borrow_capacity {
                break;
            }
            if self.issue_construct(player, &[worker], id).is_ok() {
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
