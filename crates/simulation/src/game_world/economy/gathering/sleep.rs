//! Gatherer sleep: a worker at its resource only adds a constant credit each
//! tick until its next harvest. It sleeps until that harvest tick instead of
//! running the whole gather step, and its credits are reconstructed exactly.
//!
//! A sleeper is re-validated on every visit against the state it went to sleep
//! with: its unit record, no movement order, its player's technologies (rate
//! and carry capacity) and its target's remaining amount. Any difference wakes
//! it, so a missed wake event cannot diverge; waking early is always exact.
//! `GameWorld::set_work_sleep(false)` turns sleep off to prove hash equality.
use super::*;
use crate::GameUnit;

/// Why a gatherer may skip its step: everything the pure credit step reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GatherSleep {
    /// The tick whose full step stored `credits`.
    since: u64,
    /// The first tick whose step reaches the harvest threshold.
    until: u64,
    /// Credits each skipped tick would have added.
    rate: u64,
    researched: u64,
    unit: GameUnit,
}

impl GatherSleep {
    pub(super) fn until(&self) -> u64 {
        self.until
    }
}

impl GatherTask {
    /// Credits as the ordinary step would hold them once tick `tick` is done.
    pub(crate) fn credits_at(&self, tick: u64) -> u64 {
        self.sleep.map_or(self.credits, |sleep| {
            self.credits + sleep.rate * tick.saturating_sub(sleep.since)
        })
    }

    /// Reconstruct the credits of the skipped ticks before tick `now` runs.
    fn wake(&mut self, now: u64) {
        if let Some(sleep) = self.sleep.take() {
            self.credits += sleep.rate * now.saturating_sub(sleep.since + 1);
        }
    }
}

impl GameWorld {
    /// On by default. Off, every gatherer runs its full step each tick: the
    /// replay oracle compares both settings hash for hash.
    pub fn set_work_sleep(&mut self, enabled: bool) {
        self.economy.no_sleep = !enabled;
    }

    /// The credit one ordinary gather tick adds for this worker and task.
    pub(super) fn gather_tick_rate(&self, index: usize, task: &GatherTask) -> u64 {
        // Fish rates use current DE naval balance, independent of land technologies.
        match (task.fish, self.units[index].state.kind) {
            (Some(aoe_map::ObjectKind::DeepFish), UnitKind::FishingShip) => 416500,
            (Some(aoe_map::ObjectKind::ShoreFish), UnitKind::FishingShip) => 238000,
            (Some(_), _) => 430000,
            _ if task.farm.is_some() => {
                u64::from(super::super::farm::FARMER_WORK_HUNDREDTHS) * 10_000
            }
            _ => task.animal.and_then(|id| self.animal(id)).map_or_else(
                || {
                    let (numerator, denominator) =
                        self.gather_rate(self.units[index].state.player, task.kind);
                    u64::from(numerator) * 10_000 / u64::from(denominator)
                },
                |a| u64::from(a.kind.rules().gather_hundredths) * 10_000,
            ),
        }
    }

    /// Whether `id`'s visit changes nothing this tick: a sleeper whose
    /// sleep still holds, a drop-off waiter, or a walker under an order. A
    /// sleeper whose sleep no longer holds is woken first: its skipped
    /// credits are restored and it runs the ordinary step.
    pub(super) fn gather_visit_skips(&mut self, id: EntityId) -> bool {
        #[cfg(test)]
        {
            self.economy.tick_work.gather_visits += 1;
        }
        let (Some(&index), Some(task)) = (self.lookup.get(&id), self.economy.gatherers.get(&id))
        else {
            return false;
        };
        let now = self.tick.0;
        let asleep = !self.economy.no_sleep
            && match task.sleep {
                // Waiting for a drop-off reads nothing but the retry tick.
                None => task.phase == GatherPhase::WaitingDropoff && now < task.retry_tick,
                Some(sleep) => now < sleep.until && self.sleep_holds(index, task, &sleep),
            };
        self.count_gather(Count::Visits, 1);
        self.count_gather(Count::Slept, u64::from(asleep));
        if asleep {
            #[cfg(test)]
            {
                self.economy.tick_work.gather_slept += 1;
            }
            // A spurious wake: doze again until the harvest tick.
            if task.sleep.is_some() {
                self.economy.gatherers.doze(id);
            }
            return true;
        }
        if task.sleep.is_none() {
            let walks = self.gatherer_walks(index, task);
            self.count_gather(Count::Walkers, u64::from(walks));
            if walks {
                self.sleep_walking_gatherer(id);
            }
            return walks;
        }
        self.economy
            .gatherers
            .update_progress(&id, |task| task.wake(now));
        let walks = self
            .economy
            .gatherers
            .get(&id)
            .is_some_and(|task| self.gatherer_walks(index, task));
        if walks {
            self.sleep_walking_gatherer(id);
        }
        walks
    }

    /// A gatherer walking under a movement order, with no failed route,
    /// changes nothing in `gather_step`: it skips the task-map round trip.
    pub(super) fn gatherer_walks(&self, index: usize, task: &GatherTask) -> bool {
        task.phase != GatherPhase::WaitingDropoff
            && self.units[index].order.is_some()
            && !self.gather_contact(index, task)
            && !(task.phase == GatherPhase::Returning
                && self.at_dropoff(self.units[index].state, task))
            && !(task.phase == GatherPhase::Approaching
                && self.units[index].last_movement_error.is_some())
    }

    pub(super) fn sleep_holds(&self, index: usize, task: &GatherTask, sleep: &GatherSleep) -> bool {
        let stored = &self.units[index];
        if stored.state != sleep.unit || stored.order.is_some() {
            return false;
        }
        if self
            .economy
            .players
            .get(&stored.state.player)
            .map_or(0, |p| p.researched)
            != sleep.researched
        {
            return false;
        }
        if let Some(animal) = task.animal {
            // A carcass does not move; other hunters only lower its food.
            return self
                .animal(animal)
                .is_some_and(|a| a.hp == 0 && a.food_remaining > 0);
        }
        match task.farm {
            Some(farm) => self.farm_remaining(farm).is_some_and(|food| food > 0),
            // An untouched node keeps the positive amount it had at sleep time.
            None => self
                .changed_resource_amount(task.node)
                .is_none_or(|left| left > 0),
        }
    }

    fn changed_resource_amount(&self, node: u64) -> Option<u16> {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return Some(0);
        };
        overlay.changed_amount(node)
    }

    /// After an ordinary step that left `task` harvesting at its resource,
    /// sleep through the ticks that only add credits.
    pub(super) fn try_sleep_gatherer(&self, id: EntityId, task: &mut GatherTask) {
        task.sleep = None;
        if self.economy.no_sleep
            || task.phase != GatherPhase::Gathering
            || task.retry_tick > self.tick.0
            // Only a carcass stays put; a live animal moves.
            || task
                .animal
                .is_some_and(|a| self.animal(a).is_none_or(|a| a.hp > 0))
        {
            return;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let stored = &self.units[index];
        let unit = stored.state;
        if stored.order.is_some() || unit.moving || unit.planning || stored.planner.is_some() {
            return;
        }
        if !self.task_remaining(task).is_some_and(|left| left > 0)
            || unit
                .carried
                .is_some_and(|load| load.amount >= self.gather_task_capacity(&unit, task))
        {
            return;
        }
        let rate = self.gather_tick_rate(index, task);
        let threshold = u64::from(self.config.tick_hz) * 1_000_000;
        if rate == 0 || task.credits >= threshold {
            return;
        }
        let ticks = (threshold - task.credits).div_ceil(rate);
        if ticks < 2 {
            return;
        }
        task.sleep = Some(GatherSleep {
            since: self.tick.0,
            until: self.tick.0 + ticks,
            rate,
            researched: self.player_economy(unit.player).map_or(0, |p| p.researched),
            unit,
        });
    }
}
