use super::{
    VASSAL_RULES, VassalStatus, VassalWait, VassalWork, rules::Budget, snapshot::Snapshot,
};
use crate::{BuildingKind, GameWorld, UnitKind, UnitTask, VILLAGE_RULES};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, WorldPosition};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

/// A point just outside the Castle footprint, on the side facing `from`. The
/// Castle centre itself lies inside its own footprint and cannot be walked to.
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
        // A departure reserves its slot until arrival, even when the destination
        // is not among this pass's admitted domains.
        let finished: Vec<_> = self
            .economy
            .vassals
            .transfers
            .iter()
            .filter(|(worker, dest)| {
                !self.active_vassal(**dest)
                    || self.unit(**worker).is_none_or(|u| {
                        u.hp == 0
                            || (!u.moving
                                && !u.planning
                                && !self.economy.vassal_homes[dest].owns(u.position))
                    })
            })
            .map(|(worker, _)| *worker)
            .collect();
        for worker in finished {
            self.economy.vassals.transfers.remove(&worker);
        }
        let mut incoming = BTreeMap::new();
        for dest in self.economy.vassals.transfers.values() {
            *incoming.entry(*dest).or_default() += 1;
        }
        for id in ids {
            self.economy.vassals.cursor = Some(id);
            if !self.active_vassal(id) {
                self.set_vassal_castle(id, None);
                continue;
            }
            let workers: Vec<_> = self.economy.vassal_homes[&id]
                .workers
                .iter()
                .chain(self.economy.vassal_homes[&id].navy.ships.iter())
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
            self.economy.vassals.ordering = Some(id);
            // Villagers above the owner's cap walk to a Vassal with room first.
            let cap = self.economy.vassal_homes[&id].cap();
            let excess: Vec<_> = self.economy.vassal_homes[&id]
                .workers
                .iter()
                .copied()
                .skip(cap)
                .collect();
            self.vassal_send_away(id, &excess, &mut budget, &mut work, &mut incoming, changed);
            self.vassal_adopt(&snapshot);
            self.vassal_watchdog(&snapshot, changed);
            // Retain discovery progress even when all workers are productive.
            let discover = self.vassal_needs_discovery(&snapshot);
            // One bounded sweep also finds coastal work when land workers are busy.
            self.vassal_discover(&mut snapshot, &mut budget, &mut work);
            self.vassal_navy(&mut snapshot, &mut budget, &mut work, changed);
            let wait = self.vassal_train(&mut snapshot, &mut budget, &mut work);
            self.vassal_house_for_busy(&mut snapshot, &mut budget, &mut work, changed);
            self.vassal_rebalance(&mut snapshot, &mut budget, &mut work, changed);
            self.vassal_reseeds(&snapshot, &mut budget, &mut work);
            self.vassal_refresh_farms(&mut snapshot, &mut budget, &mut work, changed);
            self.vassal_refresh_wood(&mut snapshot, &mut budget, &mut work);
            let unemployed = self.vassal_idle(id, &mut snapshot, &mut budget, &mut work, changed);
            while discover && budget.searches < VASSAL_RULES.resource_searches_per_domain {
                if !self.vassal_discover(&mut snapshot, &mut budget, &mut work) {
                    break;
                }
            }
            // Idle villagers this plot could not employ go where a Vassal has room.
            self.vassal_send_away(
                id,
                &unemployed,
                &mut budget,
                &mut work,
                &mut incoming,
                changed,
            );
            self.vassal_publish_status(id, snapshot.villagers, wait);
            self.economy.vassals.ordering = None;
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
            settings: home.settings,
            managed: home.workers.len() as u16,
            residents: residents.min(usize::from(u16::MAX)) as u16,
            wait,
        };
        let Some(castle) = home.castle else { return };
        let status = home.status;
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
        if !s.home.settings.training {
            return VassalWait::Paused;
        }
        if !s.complete {
            return VassalWait::Census;
        }
        let mut wait = VassalWait::NoTownCenter;
        for b in &s.buildings {
            if s.villagers + s.queued >= s.home.cap() {
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
            if p.population_used as usize + s.global_queued >= p.population_cap as usize {
                wait = VassalWait::Population;
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
            return VassalWait::Full;
        }
        wait
    }
    /// Moves villagers to the nearest other active Vassal of the same player
    /// that has room (including reservations for travelling arrivals). A worker that
    /// cannot be sent stays where it is.
    fn vassal_send_away(
        &mut self,
        from: EntityId,
        workers: &[EntityId],
        budget: &mut Budget,
        work: &mut VassalWork,
        incoming: &mut BTreeMap<EntityId, usize>,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(player) = self.economy.vassal_homes.get(&from).map(|h| h.player) else {
            return;
        };
        for &worker in workers {
            let Some(at) = self.unit(worker).map(|u| u.position) else {
                continue;
            };
            let Some((dest, center)) = self.vassal_with_room(player, from, at, incoming) else {
                break;
            };
            if !budget.order(work) {
                break;
            }
            if !matches!(self.issue_move(worker, center), Ok(true)) {
                continue;
            }
            self.release_vassal_worker(worker);
            // Manual: the source domain must not re-adopt it while it walks there.
            self.economy.vassals.manual.insert(worker);
            self.economy.vassals.transfers.insert(worker, dest);
            *incoming.entry(dest).or_default() += 1;
            changed.push(worker);
        }
    }
    fn vassal_with_room(
        &self,
        player: PlayerId,
        from: EntityId,
        at: WorldPosition,
        incoming: &BTreeMap<EntityId, usize>,
    ) -> Option<(EntityId, WorldPosition)> {
        self.economy
            .vassals
            .by_player
            .get(&player)?
            .iter()
            .filter(|(id, _)| **id != from)
            .filter_map(|(id, center)| {
                let home = self.economy.vassal_homes.get(id)?;
                let load = home.workers.len() + incoming.get(id).copied().unwrap_or(0);
                (load < home.cap()).then_some((dist2(*center, at), *id, *center))
            })
            .min_by_key(|(d, id, _)| (*d, *id))
            .map(|(_, id, center)| (id, arrival_point(center, at)))
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
            if !budget.support_available() || !budget.order(work) {
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
        if !budget.support_available() {
            return;
        }
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
            if budget.orders >= VASSAL_RULES.orders_per_domain {
                break;
            }
            if !self.vassal_worker_idle(worker, id) {
                continue;
            }
            if !budget.job() {
                break;
            }
            if let Some(home) = self.economy.vassal_homes.get_mut(&id) {
                home.worker_after = Some(worker);
            }
            s.avoid = self.economy.vassal_homes[&id].stalls.get(&worker).copied();
            let job = self.vassal_job(worker, None, s, budget, work);
            s.avoid = None;
            if let Some(kind) = job {
                self.claim_vassal_worker(id, worker, kind);
                changed.push(worker);
            } else if budget.searches < VASSAL_RULES.resource_searches_per_domain {
                // The search ran and found no work for this villager here.
                unemployed.push(worker);
            }
        }
        unemployed
    }
    pub(super) fn vassal_worker_idle(&self, worker: EntityId, id: EntityId) -> bool {
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
                || self.economy.vassal_homes[&id].workers.len()
                    < self.economy.vassal_homes[&id].cap())
    }
}
