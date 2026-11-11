//! True sleep for gatherers: a dozing worker is not visited at all until its
//! harvest tick or a wake event. Wake events come from every writer of what
//! the pure step reads: the task map, the unit (the tick's changed ids and
//! removals), its player's technologies and its target's amount or crowd.
//! Tests also re-check every dozer each tick and fail on a missed wake.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(crate) struct Dozes {
    /// Gatherers visited every tick: every task that is not dozing.
    awake: BTreeSet<EntityId>,
    /// `(harvest tick, id)` of every dozing worker.
    until: BTreeSet<(u64, EntityId)>,
    pub(super) walking: super::walking::Walks,
    by_target: BTreeMap<WorkTarget, BTreeSet<EntityId>>,
    /// While the gathering loop runs: ids whose job began during it. The
    /// ordinary loop visits only the jobs present when it started.
    fresh: Option<BTreeSet<EntityId>>,
}

impl Gatherers {
    pub(super) fn on_insert(&mut self, id: EntityId, old: Option<&GatherTask>) {
        match old {
            Some(old) if !self.dozes.awake.contains(&id) => self.undoze(id, old.clone()),
            Some(_) => {}
            None => {
                if let Some(fresh) = &mut self.dozes.fresh {
                    fresh.insert(id);
                }
            }
        }
        self.dozes.awake.insert(id);
    }

    pub(super) fn on_remove(&mut self, id: EntityId, task: &GatherTask) {
        if !self.dozes.awake.remove(&id) {
            self.undoze(id, task.clone());
        }
    }

    fn undoze(&mut self, id: EntityId, task: GatherTask) {
        if let Some(until) = self.dozes.walking.remove(id) {
            self.dozes.until.remove(&(until, id));
        }
        if let Some(sleep) = task.sleep {
            self.dozes.until.remove(&(sleep.until(), id));
        }
        let target = task.target();
        if let Some(ids) = self.dozes.by_target.get_mut(&target) {
            ids.remove(&id);
            if ids.is_empty() {
                self.dozes.by_target.remove(&target);
            }
        }
    }

    /// Stop visiting `id` until its harvest tick or a wake event.
    pub(super) fn doze(&mut self, id: EntityId) {
        let Some(until) = self
            .get(&id)
            .and_then(|task| task.sleep.map(|sleep| sleep.until()))
        else {
            return;
        };
        self.doze_until(id, until);
    }

    pub(super) fn doze_until(&mut self, id: EntityId, until: u64) -> bool {
        if self.dozes.awake.remove(&id) {
            self.dozes.until.insert((until, id));
            if let Some(target) = self.get(&id).map(GatherTask::target) {
                self.dozes.by_target.entry(target).or_default().insert(id);
            }
            return true;
        }
        false
    }

    pub(super) fn retime_doze(&mut self, id: EntityId, old: u64, until: u64) {
        if old != until {
            self.dozes.until.remove(&(old, id));
            self.dozes.until.insert((until, id));
        }
    }

    pub(crate) fn wake(&mut self, id: EntityId) {
        if self.dozes.awake.contains(&id) {
            return;
        }
        let Some(task) = self.get(&id).cloned() else {
            return;
        };
        self.undoze(id, task);
        self.dozes.awake.insert(id);
    }

    pub(crate) fn wake_target(&mut self, target: WorkTarget) {
        let ids = self
            .dozes
            .by_target
            .get(&target)
            .cloned()
            .unwrap_or_default();
        for id in ids {
            self.wake(id);
        }
    }

    /// A target just got crowded: dozers not yet marked must observe it.
    pub(super) fn wake_uncrowded(&mut self, target: WorkTarget) {
        let ids = self
            .dozes
            .by_target
            .get(&target)
            .cloned()
            .unwrap_or_default();
        for id in ids {
            if self.get(&id).is_some_and(|task| !task.crowded) {
                self.wake(id);
            }
        }
    }

    pub(super) fn wake_due(&mut self, now: u64) {
        while let Some(&(until, id)) = self.dozes.until.first() {
            if until > now {
                break;
            }
            self.wake(id);
        }
    }

    pub(super) fn dozing_count(&self) -> usize {
        self.dozes.until.len()
    }

    pub(crate) fn dozing(&self) -> Vec<EntityId> {
        self.dozes.until.iter().map(|&(_, id)| id).collect()
    }

    pub(super) fn begin_loop(&mut self) {
        self.dozes.fresh = Some(BTreeSet::new());
    }

    pub(super) fn end_loop(&mut self) {
        self.dozes.fresh = None;
    }

    /// The next awake job after `cursor` that existed when the loop began.
    pub(super) fn next_awake(&self, cursor: Option<EntityId>) -> Option<EntityId> {
        use std::ops::Bound::{Excluded, Unbounded};
        let from = cursor.map_or(Unbounded, Excluded);
        self.dozes
            .awake
            .range((from, Unbounded))
            .copied()
            .find(|id| self.dozes.fresh.as_ref().is_none_or(|f| !f.contains(id)))
    }

    #[cfg(test)]
    pub(super) fn clear_dozes(&mut self) {
        self.dozes = Dozes::default();
    }
}

impl GameWorld {
    pub(crate) fn wake_gatherer(&mut self, id: EntityId) {
        self.economy.gatherers.wake(id);
    }

    /// A technology changed `player`'s rates or carry capacity.
    pub(crate) fn wake_player_gatherers(&mut self, player: aoe_core::PlayerId) {
        for id in self.economy.gatherers.dozing() {
            if self.unit(id).is_none_or(|u| u.player == player) {
                self.economy.gatherers.wake(id);
            }
        }
    }

    /// Before the loop: harvest ticks that came due, and every unit changed
    /// since the last loop (this tick's earlier phases, the last tick's later
    /// phases and the commands between ticks).
    pub(super) fn wake_before_gathering(&mut self, changed: &[EntityId]) {
        if self.economy.no_sleep {
            for id in self.economy.gatherers.dozing() {
                self.economy.gatherers.wake(id);
            }
        }
        self.economy.gatherers.wake_due(self.tick.0);
        // Count the remaining dozers as visits their sleep skipped.
        let dozing = self.economy.gatherers.dozing_count() as u64;
        self.count_gather(Count::Visits, dozing);
        self.count_gather(Count::Slept, dozing);
        self.count_gather(
            Count::Walkers,
            self.economy.gatherers.dozes.walking.len() as u64,
        );
        #[cfg(test)]
        {
            self.economy.tick_work.gather_visits += dozing as usize;
            self.economy.tick_work.gather_slept += dozing as usize;
        }
        for &id in changed {
            self.wake_gatherer_event(id);
            if let Some(unit) = self.unit(id) {
                self.economy
                    .gatherers
                    .notify_walkers_at(unit.position.tile_floor());
                self.economy
                    .gatherers
                    .notify_walkers_at(unit.previous_position.tile_floor());
            }
        }
        for id in std::mem::take(&mut self.economy.late_changed) {
            self.wake_gatherer_event(id);
            if let Some(unit) = self.unit(id) {
                self.economy
                    .gatherers
                    .notify_walkers_at(unit.position.tile_floor());
                self.economy
                    .gatherers
                    .notify_walkers_at(unit.previous_position.tile_floor());
            }
        }
        self.wake_walker_contacts();
        #[cfg(test)]
        self.assert_dozers_hold();
    }

    /// Every dozer still holds its sleep; a failure is a missed wake event.
    #[cfg(test)]
    pub(super) fn assert_dozers_hold(&self) {
        self.assert_walking_dozers();
        for id in self.economy.gatherers.dozing() {
            if self.economy.gatherers.walking(id) {
                continue;
            }
            let task = &self.economy.gatherers[&id];
            let sleep = task.sleep.expect("a dozer sleeps");
            let holds = self
                .lookup
                .get(&id)
                .is_some_and(|&index| self.sleep_holds(index, task, &sleep));
            assert!(
                holds,
                "missed wake: gatherer {} dozes at tick {} but its sleep no longer holds: {:?} now {:?} order {:?} animal {:?}",
                id.0,
                self.tick.0,
                sleep,
                self.unit(id),
                self.lookup.get(&id).map(|&i| self.units[i].order.is_some()),
                task.animal.and_then(|a| self.animal(a)),
            );
        }
    }
}
