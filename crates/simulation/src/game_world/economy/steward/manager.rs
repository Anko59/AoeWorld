use super::{
    STEWARD_RULES, StewardStatus, StewardWait, StewardWork, rules::Budget, snapshot::Snapshot,
};
use crate::{BuildingKind, GameWorld, UnitKind, UnitTask, VILLAGE_RULES};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, WorldPosition};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

/// A point just outside the TownCenter footprint, on the side facing `from`. The
/// TownCenter centre itself lies inside its own footprint and cannot be walked to.
fn arrival_point(center: WorldPosition, from: WorldPosition) -> WorldPosition {
    let step = |d: i32| d.signum() * 3 * FIXED_SUBUNITS_PER_TILE;
    WorldPosition::new(
        center.x + step(from.x - center.x),
        center.y + step(from.y - center.y),
    )
}

/// Squared distance between two world positions, in integer subunits.
fn dist2(a: WorldPosition, b: WorldPosition) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}

impl GameWorld {
    pub(crate) fn advance_stewards(&mut self, changed: &mut Vec<EntityId>) {
        let mut work = StewardWork::default();
        if !self.tick.0.is_multiple_of(STEWARD_RULES.period_ticks) {
            self.economy.stewards.work = work;
            return;
        }
        let active = &self.economy.stewards.active;
        let ids: Vec<_> = if let Some(cursor) = self.economy.stewards.cursor {
            active
                .range((Excluded(cursor), Unbounded))
                .chain(active.range(..=cursor))
                .take(STEWARD_RULES.domains_per_pass)
                .copied()
                .collect()
        } else {
            active
                .iter()
                .take(STEWARD_RULES.domains_per_pass)
                .copied()
                .collect()
        };
        // A departure reserves its slot until arrival, even when the destination
        // is not among this pass's admitted domains.
        let finished: Vec<_> = self
            .economy
            .stewards
            .transfers
            .iter()
            .filter(|(worker, dest)| {
                !self.active_steward(**dest)
                    || self.unit(**worker).is_none_or(|u| {
                        u.hp == 0
                            || (!u.moving
                                && !u.planning
                                && !self.economy.steward_homes[dest].owns(u.position))
                    })
            })
            .map(|(worker, _)| *worker)
            .collect();
        for worker in finished {
            self.economy.stewards.transfers.remove(&worker);
        }
        let mut incoming = BTreeMap::new();
        for dest in self.economy.stewards.transfers.values() {
            *incoming.entry(*dest).or_default() += 1;
        }
        for id in ids {
            self.economy.stewards.cursor = Some(id);
            if !self.active_steward(id) {
                self.set_steward_town_center(id, None);
                continue;
            }
            let workers: Vec<_> = self.economy.steward_homes[&id]
                .workers
                .iter()
                .chain(self.economy.steward_homes[&id].navy.ships.iter())
                .copied()
                .collect();
            for worker in workers {
                if self.steward_controller(worker) != Some(id) {
                    self.release_steward_worker(worker);
                }
            }
            let Some(mut snapshot) = self.steward_snapshot(id, &mut work) else {
                continue;
            };
            work.domains += 1;
            let mut budget = Budget::default();
            self.economy.stewards.ordering = Some(id);
            // Villagers above the owner's cap walk to a Steward with room first.
            let cap = self.economy.steward_homes[&id].cap();
            let excess: Vec<_> = self.economy.steward_homes[&id]
                .workers
                .iter()
                .copied()
                .skip(cap)
                .collect();
            self.steward_send_away(id, &excess, &mut budget, &mut work, &mut incoming, changed);
            self.steward_adopt(&snapshot);
            self.steward_watchdog(&snapshot, changed);
            // Retain discovery progress even when all workers are productive.
            let discover = self.steward_needs_discovery(&snapshot);
            // One bounded sweep also finds coastal work when land workers are busy.
            self.steward_discover(&mut snapshot, &mut budget, &mut work);
            self.steward_navy(&mut snapshot, &mut budget, &mut work, changed);
            let wait = self.steward_train(&mut snapshot, &mut budget, &mut work);
            self.steward_house_for_busy(&mut snapshot, &mut budget, &mut work, changed);
            self.steward_rebalance(&mut snapshot, &mut budget, &mut work, changed);
            self.steward_reseeds(&snapshot, &mut budget, &mut work);
            self.steward_refresh_farms(&mut snapshot, &mut budget, &mut work, changed);
            self.steward_refresh_wood(&mut snapshot, &mut budget, &mut work);
            let unemployed = self.steward_idle(id, &mut snapshot, &mut budget, &mut work, changed);
            while discover && budget.searches < STEWARD_RULES.resource_searches_per_domain {
                if !self.steward_discover(&mut snapshot, &mut budget, &mut work) {
                    break;
                }
            }
            // Idle villagers this plot could not employ go where a Steward has room.
            self.steward_send_away(
                id,
                &unemployed,
                &mut budget,
                &mut work,
                &mut incoming,
                changed,
            );
            self.steward_publish_status(id, snapshot.villagers, wait);
            self.economy.stewards.ordering = None;
        }
        self.economy.stewards.work = work;
    }
    /// Mirror the status into the TownCenter's occupant record, so a changed status
    /// reaches the owner like any other building change.
    fn steward_publish_status(&mut self, id: EntityId, residents: usize, wait: StewardWait) {
        let Some(home) = self.economy.steward_homes.get_mut(&id) else {
            return;
        };
        home.status = StewardStatus {
            settings: home.settings,
            managed: home.workers.len() as u16,
            residents: residents.min(usize::from(u16::MAX)) as u16,
            wait,
        };
        let Some(town_center) = home.town_center else {
            return;
        };
        let status = home.status;
        if let Some(entry) = self
            .economy
            .buildings
            .get_mut(&town_center)
            .and_then(|b| b.garrison.iter_mut().find(|g| g.unit.id == id))
            && entry.steward != Some(status)
        {
            entry.steward = Some(status);
        }
    }
    fn steward_train(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
    ) -> StewardWait {
        if !s.home.settings.training {
            return StewardWait::Paused;
        }
        if !s.complete {
            return StewardWait::Census;
        }
        let mut wait = StewardWait::NoTownCenter;
        for b in &s.buildings {
            if s.villagers + s.queued >= s.home.cap() {
                return StewardWait::Full;
            }
            if b.kind != BuildingKind::TownCenter
                || !b.is_complete()
                || !s.home.covers(b.footprint, 1)
                || b.rally_point.is_some_and(|r| !s.home.contains(r.position))
            {
                continue;
            }
            wait = StewardWait::Growing;
            if b.research.is_some() || !b.training_queue.is_empty() {
                continue;
            }
            let Some(p) = self.player_economy(s.player) else {
                break;
            };
            if p.stockpile.food < VILLAGE_RULES.villager_food_cost {
                wait = StewardWait::Food;
                break;
            }
            if p.population_used as usize + s.global_queued >= p.population_cap as usize {
                wait = StewardWait::Population;
                break;
            }
            if !budget.support_available() || !budget.order(work) {
                break;
            }
            if self.issue_train(s.player, b.id, UnitKind::Villager).is_ok() {
                s.queued += 1;
                s.global_queued += 1;
            }
        }
        if s.villagers + s.queued >= s.home.cap() {
            return StewardWait::Full;
        }
        wait
    }
    /// Moves villagers to the nearest other active Steward of the same player
    /// that has room (including reservations for travelling arrivals). A worker that
    /// cannot be sent stays where it is.
    fn steward_send_away(
        &mut self,
        from: EntityId,
        workers: &[EntityId],
        budget: &mut Budget,
        work: &mut StewardWork,
        incoming: &mut BTreeMap<EntityId, usize>,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(player) = self.economy.steward_homes.get(&from).map(|h| h.player) else {
            return;
        };
        for &worker in workers {
            let Some(at) = self.unit(worker).map(|u| u.position) else {
                continue;
            };
            let Some((dest, center)) = self.steward_with_room(player, from, at, incoming) else {
                break;
            };
            if !budget.order(work) {
                break;
            }
            if !matches!(self.issue_move(worker, center), Ok(true)) {
                continue;
            }
            self.release_steward_worker(worker);
            // Manual: the source domain must not re-adopt it while it walks there.
            self.economy.stewards.manual.insert(worker);
            self.economy.stewards.transfers.insert(worker, dest);
            *incoming.entry(dest).or_default() += 1;
            changed.push(worker);
        }
    }
    fn steward_with_room(
        &self,
        player: PlayerId,
        from: EntityId,
        at: WorldPosition,
        incoming: &BTreeMap<EntityId, usize>,
    ) -> Option<(EntityId, WorldPosition)> {
        self.economy
            .stewards
            .by_player
            .get(&player)?
            .iter()
            .filter(|(id, _)| **id != from)
            .filter_map(|(id, center)| {
                let home = self.economy.steward_homes.get(id)?;
                let load = home.workers.len() + incoming.get(id).copied().unwrap_or(0);
                (load < home.cap()).then_some((dist2(*center, at), *id, *center))
            })
            .min_by_key(|(d, id, _)| (*d, *id))
            .map(|(_, id, center)| (id, arrival_point(center, at)))
    }
    fn steward_reseeds(&mut self, s: &Snapshot, budget: &mut Budget, work: &mut StewardWork) {
        if !s.units.iter().any(|u| {
            self.steward_controller(u.id).is_some()
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
            if !budget.support_available() || !budget.order(work) {
                break;
            }
            let _ = self.issue_farm_reseed(s.player, b.id, true);
        }
    }
    fn steward_refresh_wood(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
    ) {
        if !budget.support_available() {
            return;
        }
        let targets: Vec<_> = s
            .units
            .iter()
            .filter_map(|u| {
                let task = self.economy.gatherers.get(&u.id)?;
                (self.steward_controller(u.id) == Some(s.steward)
                    && task.kind == aoe_map::ResourceKind::Wood
                    && u.carried.is_none())
                .then_some((u.id, task.tile))
            })
            .collect();
        for (worker, tile) in targets {
            if !s.buildings.iter().any(|b| {
                b.kind.accepts(aoe_map::ResourceKind::Wood)
                    && super::super::dropoff::ring_distance(b.footprint, tile) <= 64
            }) && self.steward_build_near(
                worker,
                BuildingKind::LumberCamp,
                tile,
                s,
                budget,
                work,
            ) {
                break;
            }
        }
    }
    fn steward_idle(
        &mut self,
        id: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
        changed: &mut Vec<EntityId>,
    ) -> Vec<EntityId> {
        let mut unemployed = Vec::new();
        let after = s.home.worker_after;
        let split = after.map_or(0, |id| s.units.partition_point(|u| u.id <= id));
        let ids: Vec<_> = s.units[split..]
            .iter()
            .chain(&s.units[..split])
            .map(|u| u.id)
            .collect();
        for worker in ids {
            if budget.orders >= STEWARD_RULES.orders_per_domain {
                break;
            }
            if !self.steward_worker_idle(worker, id) {
                continue;
            }
            if !budget.job() {
                break;
            }
            if let Some(home) = self.economy.steward_homes.get_mut(&id) {
                home.worker_after = Some(worker);
            }
            s.avoid = self.economy.steward_homes[&id].stalls.get(&worker).copied();
            let job = self.steward_job(worker, None, s, budget, work);
            s.avoid = None;
            if let Some(kind) = job {
                self.claim_steward_worker(id, worker, kind);
                changed.push(worker);
            } else if budget.searches < STEWARD_RULES.resource_searches_per_domain {
                // The search ran and found no work for this villager here.
                unemployed.push(worker);
            }
        }
        unemployed
    }
    pub(super) fn steward_worker_idle(&self, worker: EntityId, id: EntityId) -> bool {
        let waiting = self
            .economy
            .gatherers
            .get(&worker)
            .is_some_and(|t| t.phase == super::super::GatherPhase::WaitingDropoff)
            && self.steward_controller(worker) == Some(id);
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
            && self.steward_controller(worker).is_none_or(|v| v == id)
            && (self.economy.steward_homes[&id].workers.contains(&worker)
                || self.economy.steward_homes[&id].workers.len()
                    < self.economy.steward_homes[&id].cap())
    }
}
