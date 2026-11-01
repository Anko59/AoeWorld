use super::{
    VASSAL_RULES, VassalStatus, VassalWait, VassalWork, rules::Budget, snapshot::Snapshot,
};
use crate::{BuildingKind, GameWorld, UnitKind, UnitTask, VILLAGE_RULES};
use aoe_core::EntityId;
use std::ops::Bound::{Excluded, Unbounded};

impl GameWorld {
    pub(crate) fn advance_vassals(&mut self, changed: &mut Vec<EntityId>) {
        let mut work = VassalWork::default();
        if !self.tick.0.is_multiple_of(VASSAL_RULES.period_ticks) {
            self.economy.vassals.work = work;
            return;
        }
        let active = &self.economy.vassals.active;
        let ids: Vec<_> = if let Some(cursor) = self.economy.vassals.cursor {
            active
                .range((Excluded(cursor), Unbounded))
                .chain(active.range(..=cursor))
                .take(VASSAL_RULES.domains_per_pass)
                .copied()
                .collect()
        } else {
            active
                .iter()
                .take(VASSAL_RULES.domains_per_pass)
                .copied()
                .collect()
        };
        for id in ids {
            self.economy.vassals.cursor = Some(id);
            if !self.active_vassal(id) {
                self.set_vassal_active(id, false);
                continue;
            }
            let workers: Vec<_> = self.economy.vassal_homes[&id]
                .workers
                .iter()
                .copied()
                .collect();
            for worker in workers {
                if self.vassal_controller(worker) != Some(id) {
                    self.release_vassal_worker(worker);
                }
            }
            let Some(mut snapshot) = self.vassal_snapshot(id, &mut work) else {
                continue;
            };
            work.domains += 1;
            let mut budget = Budget::default();
            self.vassal_adopt(&snapshot);
            self.vassal_watchdog(&snapshot, changed);
            let wait = self.vassal_train(&mut snapshot, &mut budget, &mut work);
            self.vassal_reseeds(&snapshot, &mut budget, &mut work);
            self.vassal_refresh_wood(&mut snapshot, &mut budget, &mut work);
            self.vassal_idle(id, &mut snapshot, &mut budget, &mut work, changed);
            self.vassal_publish_status(id, snapshot.villagers, wait);
        }
        self.economy.vassals.work = work;
    }
    /// Mirror the status into the Castle's occupant record, so a changed status
    /// reaches the owner like any other building change.
    fn vassal_publish_status(&mut self, id: EntityId, residents: usize, wait: VassalWait) {
        let Some(home) = self.economy.vassal_homes.get_mut(&id) else {
            return;
        };
        home.status = VassalStatus {
            managed: home.workers.len() as u16,
            residents: residents.min(usize::from(u16::MAX)) as u16,
            wait,
        };
        let (castle, status) = (home.castle, home.status);
        if let Some(entry) = self
            .economy
            .buildings
            .get_mut(&castle)
            .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == id))
            && entry.vassal != Some(status)
        {
            entry.vassal = Some(status);
        }
    }
    fn vassal_train(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> VassalWait {
        if !s.complete {
            return VassalWait::Census;
        }
        let mut wait = VassalWait::NoTownCenter;
        for b in &s.buildings {
            if s.villagers + s.queued >= VASSAL_RULES.max_villagers {
                return VassalWait::Full;
            }
            if b.kind != BuildingKind::TownCenter
                || !b.is_complete()
                || !s.home.covers(b.footprint, 1)
                || b.rally_point.is_some_and(|r| !s.home.contains(r.position))
            {
                continue;
            }
            wait = VassalWait::Growing;
            if b.research.is_some() || !b.training_queue.is_empty() {
                continue;
            }
            let Some(p) = self.player_economy(s.player) else {
                break;
            };
            if p.stockpile.food < VILLAGE_RULES.villager_food_cost {
                wait = VassalWait::Food;
                break;
            }
            if !budget.order(work) {
                break;
            }
            if self.issue_train(s.player, b.id, UnitKind::Villager).is_ok() {
                s.queued += 1;
            }
        }
        if s.villagers + s.queued >= VASSAL_RULES.max_villagers {
            return VassalWait::Full;
        }
        wait
    }
    fn vassal_reseeds(&mut self, s: &Snapshot, budget: &mut Budget, work: &mut VassalWork) {
        if !s.units.iter().any(|u| {
            self.vassal_controller(u.id).is_some()
                && self
                    .economy
                    .gatherers
                    .get(&u.id)
                    .is_some_and(|t| t.farm.is_some())
        }) {
            return;
        }
        for b in &s.buildings {
            if b.kind != BuildingKind::Mill || !b.is_complete() || b.farm_reseeds >= 2 {
                continue;
            }
            if self
                .player_economy(s.player)
                .is_none_or(|p| p.stockpile.wood < BuildingKind::Farm.rules().cost.wood)
            {
                break;
            }
            if !budget.order(work) {
                break;
            }
            let _ = self.issue_farm_reseed(s.player, b.id, true);
        }
    }
    fn vassal_refresh_wood(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) {
        let targets: Vec<_> = s
            .units
            .iter()
            .filter_map(|u| {
                let task = self.economy.gatherers.get(&u.id)?;
                (self.vassal_controller(u.id) == Some(s.vassal)
                    && task.kind == aoe_map::ResourceKind::Wood
                    && u.carried.is_none())
                .then_some((u.id, task.tile))
            })
            .collect();
        for (worker, tile) in targets {
            if !s.buildings.iter().any(|b| {
                b.kind.accepts(aoe_map::ResourceKind::Wood)
                    && super::super::dropoff::ring_distance(b.footprint, tile) <= 64
            }) && self.vassal_build_near(worker, BuildingKind::LumberCamp, tile, s, budget, work)
            {
                break;
            }
        }
    }
    fn vassal_idle(
        &mut self,
        id: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        let after = s.home.worker_after;
        let split = after.map_or(0, |id| s.units.partition_point(|u| u.id <= id));
        let ids: Vec<_> = s.units[split..]
            .iter()
            .chain(&s.units[..split])
            .map(|u| u.id)
            .collect();
        for worker in ids {
            if budget.orders >= VASSAL_RULES.orders_per_domain
                || budget.searches >= VASSAL_RULES.resource_searches_per_domain
                || budget.sites >= VASSAL_RULES.site_checks_per_domain
            {
                break;
            }
            if !self.vassal_worker_idle(worker, id) {
                continue;
            }
            if let Some(home) = self.economy.vassal_homes.get_mut(&id) {
                home.worker_after = Some(worker);
            }
            s.avoid = self.economy.vassal_homes[&id].stalls.get(&worker).copied();
            let job = self.vassal_job(worker, s, budget, work);
            s.avoid = None;
            if let Some(kind) = job {
                self.claim_vassal_worker(id, worker, kind);
                s.allocated[kind as usize] += 1;
                changed.push(worker);
            }
        }
    }
    fn vassal_worker_idle(&self, worker: EntityId, id: EntityId) -> bool {
        let waiting = self
            .economy
            .gatherers
            .get(&worker)
            .is_some_and(|t| t.phase == super::super::GatherPhase::WaitingDropoff)
            && self.vassal_controller(worker) == Some(id);
        self.unit(worker).is_some_and(|u| {
            u.kind == UnitKind::Villager
                && u.hp > 0
                && !u.moving
                && !u.planning
                && matches!(u.task, UnitTask::Idle | UnitTask::Carrying)
                && !self.embarked(worker)
        }) && (!self.economy.gatherers.contains_key(&worker) || waiting)
            && !self.economy.builders.contains_key(&worker)
            && !self.economy.garrison_orders.contains_key(&worker)
            && self.vassal_controller(worker).is_none_or(|v| v == id)
            && (self.economy.vassal_homes[&id].workers.contains(&worker)
                || self.economy.vassal_homes[&id].workers.len() < VASSAL_RULES.max_villagers)
    }
}
