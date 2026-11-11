//! Exact construction clocks. Only arrival/wake and completion/sight events
//! visit builders; reads project the intervening integer work and earned HP.
use super::*;
use crate::game_world::visibility::TileHasher;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::BuildHasherDefault;

/// Cumulative work counters, excluded from canonical state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConstructionWork {
    pub visits: u64,
    pub slept: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Clock {
    since: u64,
    rate: u64,
    until: u64,
    repairing: bool,
    clearing: bool,
    #[cfg(test)]
    inputs: (u64, u64, u16, u16, u8),
}

#[derive(Debug, Default)]
pub(crate) struct ConstructionTimers {
    clocks: BTreeMap<EntityId, Clock>,
    due: BTreeSet<(u64, EntityId)>,
    tiles: HashMap<TileCoord, BTreeSet<EntityId>, BuildHasherDefault<TileHasher>>,
    pub(crate) tick: u64,
    pub(crate) visits: u64,
    pub(crate) slept: u64,
}

impl ConstructionTimers {
    pub(crate) fn progressing(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.clocks
            .iter()
            .filter(|(_, clock)| !clock.repairing && !clock.clearing)
            .map(|(id, _)| *id)
    }

    pub(crate) fn project<'a>(
        &self,
        b: &'a super::super::GameBuilding,
    ) -> Cow<'a, super::super::GameBuilding> {
        if b.is_complete() {
            return Cow::Borrowed(b);
        }
        let Some(clock) = self.clocks.get(&b.id) else {
            return Cow::Borrowed(b);
        };
        if clock.repairing || clock.clearing || self.tick == clock.since {
            return Cow::Borrowed(b);
        }
        let mut b = b.clone();
        let total = b.work_total.max(1);
        let before = u64::from(b.max_hp) * b.work_done / total;
        b.work_done = b
            .work_done
            .saturating_add(
                clock
                    .rate
                    .saturating_mul(self.tick.saturating_sub(clock.since)),
            )
            .min(b.work_total);
        let earned = u64::from(b.max_hp) * b.work_done / total - before;
        b.hp = (u64::from(b.hp) + earned).clamp(1, u64::from(b.max_hp)) as u16;
        Cow::Owned(b)
    }

    fn subscribe(&mut self, id: EntityId, rect: TileRect, add: bool) {
        for y in rect.min.y - 1..=rect.max.y {
            for x in rect.min.x - 1..=rect.max.x {
                let tile = TileCoord::new(x, y);
                if add {
                    self.tiles.entry(tile).or_default().insert(id);
                } else if let Some(ids) = self.tiles.get_mut(&tile) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.tiles.remove(&tile);
                    }
                }
            }
        }
    }
}

impl GameWorld {
    pub fn construction_work(&self) -> ConstructionWork {
        ConstructionWork {
            visits: self.economy.construction.visits,
            slept: self.economy.construction.slept,
        }
    }

    /// Materialize through the last completed construction phase before any
    /// writer changes a clock input (including combat, cancellation and tech).
    pub(crate) fn wake_construction(&mut self, id: EntityId) {
        self.economy.builders.dirty.remove(&id);
        if let Some(b) = self.economy.buildings.get(&id)
            && let Some(clock) = self.economy.construction.clocks.get(&id).copied()
        {
            let projected = self.economy.construction.project(b).into_owned();
            let rect = b.footprint;
            self.economy.construction.clocks.remove(&id);
            self.economy.construction.due.remove(&(clock.until, id));
            self.economy.construction.subscribe(id, rect, false);
            self.economy.buildings.insert(id, projected);
        }
        self.economy.builders.wake(id);
    }

    pub(crate) fn wake_construction_at(&mut self, tile: TileCoord) {
        let Some(ids) = self.economy.construction.tiles.get(&tile).cloned() else {
            return;
        };
        for id in ids {
            self.wake_construction(id);
        }
    }

    /// Movement reports changed units; a walking builder subscribes only to
    /// first perimeter contact or a stopped/failed order, not intermediate steps.
    fn builder_walk_holds(&self, id: EntityId) -> bool {
        let Some(job) = self.economy.builders.get(&id) else {
            return false;
        };
        let Some(b) = self.building(job.building) else {
            return false;
        };
        self.lookup.get(&id).is_some_and(|&i| {
            let u = &self.units[i];
            !(b.is_complete() && b.hp == b.max_hp)
                && (u.order.is_some() || u.state.planning)
                && !adjacent(b.footprint, u.state.position.tile_floor())
        })
    }

    pub(super) fn wake_before_construction(
        &mut self,
        changed: &[EntityId],
    ) -> BTreeMap<EntityId, u64> {
        let mut repairs = BTreeMap::new();
        for id in std::mem::take(&mut self.economy.builders.dirty) {
            self.wake_construction(id);
        }
        self.economy.builders.wake_retries(self.tick.0);
        if self.economy.no_sleep {
            self.economy.builders.wake_retries(u64::MAX);
            let ids = self
                .economy
                .construction
                .clocks
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in ids {
                self.wake_construction(id);
            }
            self.economy
                .builders
                .awake
                .extend(std::mem::take(&mut self.economy.builders.walking));
        }
        while let Some(&(until, id)) = self.economy.construction.due.first() {
            if until > self.tick.0 {
                break;
            }
            let clock = self.economy.construction.clocks[&id];
            if self
                .economy
                .buildings
                .get(&id)
                .is_some_and(|b| !b.is_complete() || b.hp < b.max_hp)
            {
                // Materialize only the skipped ticks. The site's ordinary
                // step runs below, with its unchanged crew count.
                let building = self
                    .economy
                    .construction
                    .project(&self.economy.buildings[&id])
                    .into_owned();
                #[cfg(test)]
                let inputs = (
                    building.work_done,
                    building.work_total,
                    building.hp,
                    building.max_hp,
                    building.los,
                );
                self.economy.buildings.insert(id, building);
                let since = self.economy.construction.tick;
                if let Some(current) = self.economy.construction.clocks.get_mut(&id) {
                    current.since = since;
                    #[cfg(test)]
                    {
                        current.inputs = inputs;
                    }
                }
                self.economy.construction.due.remove(&(until, id));
                repairs.insert(id, clock.rate - 2);
            } else {
                self.wake_construction(id);
            }
        }
        if self.economy.construction.clocks.is_empty()
            && self.economy.builders.walking.is_empty()
            && self.economy.builders.retries.is_empty()
        {
            return repairs;
        }
        for &id in changed {
            let wake = self.economy.builders.sleeping.contains_key(&id)
                || (self.economy.builders.walking.contains(&id) && !self.builder_walk_holds(id));
            if wake && let Some(job) = self.economy.builders.get(&id) {
                self.wake_construction(job.building);
            }
            if let Some(unit) = self.unit(id) {
                self.wake_construction_at(unit.position.tile_floor());
                self.wake_construction_at(unit.previous_position.tile_floor());
            }
        }
        repairs.retain(|id, _| self.economy.construction.clocks.contains_key(id));
        let slept = (self.economy.builders.sleeping.len()
            + self.economy.builders.walking.len()
            + self.economy.builders.retries.len()) as u64;
        self.economy.construction.slept += slept;
        #[cfg(test)]
        {
            self.economy.tick_work.builder_slept += slept as usize;
            self.assert_construction_dozers();
        }
        repairs
    }

    pub(super) fn sleep_completed_farm(&mut self, id: EntityId, target: EntityId) {
        if self.economy.no_sleep
            || self
                .economy
                .builders
                .get(&id)
                .is_none_or(|job| job.building != target || !job.queue.is_empty())
            || self.economy.buildings.get(&target).is_none_or(|b| {
                b.kind != BuildingKind::Farm || !b.is_complete() || b.hp != b.max_hp
            })
        {
            return;
        }
        let period = u64::from(self.config.tick_hz).max(1);
        let delta = (u64::from(id.0) % period + period - self.tick.0 % period) % period;
        let until = self
            .tick
            .0
            .saturating_add(if delta == 0 { period } else { delta });
        // A failed staffing attempt reinserts its job. Consume that mutation's
        // wake before sleeping, otherwise it would cancel this deadline on
        // the very next tick.
        self.wake_construction(target);
        self.economy.builders.doze_retry(id, until);
    }

    pub(super) fn sleep_construction(&mut self, id: EntityId, count: u64, clearing: bool) {
        if self.economy.no_sleep {
            return;
        }
        let Some(b) = self.economy.buildings.get(&id) else {
            return;
        };
        let repairing = b.is_complete();
        if repairing && b.hp == b.max_hp {
            self.wake_construction(id);
            return;
        }
        if !repairing && !clearing && (b.work_done == 0 || b.work_done >= b.work_total) {
            return;
        }
        let Some(ids) = self.economy.builders.members.get(&id) else {
            return;
        };
        let workers = ids
            .iter()
            .filter(|id| !self.economy.builders.walking.contains(id))
            .copied()
            .collect::<Vec<_>>();
        if workers.len() as u64 != count {
            return;
        }
        let rate = count + 2;
        let total = b.work_total.max(1);
        let los = u64::from(b.los);
        let radius = (los * b.work_done).div_ceil(total);
        let sight_work = (radius * total)
            .checked_div(los)
            .map_or(total, |work| work + 1);
        let hp_work =
            (u64::from(b.max_hp.saturating_sub(1)) * total).div_ceil(u64::from(b.max_hp).max(1));
        let next = sight_work
            .min(total)
            .min(if b.hp < b.max_hp && hp_work > b.work_done {
                hp_work
            } else {
                total
            });
        let until = self.tick.0 + next.saturating_sub(b.work_done).div_ceil(rate).max(1);
        let until = if repairing || clearing {
            self.tick.0 + 1
        } else {
            until
        };
        let rect = b.footprint;
        let units = workers
            .iter()
            .filter_map(|id| self.unit(*id))
            .collect::<Vec<_>>();
        for unit in units {
            self.economy.builders.awake.remove(&unit.id);
            self.economy.builders.hold(unit);
        }
        #[cfg(test)]
        let inputs = {
            let b = &self.economy.buildings[&id];
            (b.work_done, b.work_total, b.hp, b.max_hp, b.los)
        };
        self.economy.construction.clocks.insert(
            id,
            Clock {
                since: self.tick.0,
                rate,
                until,
                repairing,
                clearing,
                #[cfg(test)]
                inputs,
            },
        );
        self.economy.construction.due.insert((until, id));
        self.economy.construction.subscribe(id, rect, true);
    }

    #[cfg(test)]
    fn assert_construction_dozers(&self) {
        for (id, until) in &self.economy.builders.retries {
            let job = &self.economy.builders[id];
            let holds = *until > self.tick.0
                && job.queue.is_empty()
                && self.economy.buildings.get(&job.building).is_some_and(|b| {
                    b.kind == BuildingKind::Farm && b.is_complete() && b.hp == b.max_hp
                });
            assert!(
                holds,
                "missed wake: farm builder {id:?} at tick {}",
                self.tick.0
            );
        }
        for id in &self.economy.builders.walking {
            assert!(
                self.builder_walk_holds(*id),
                "missed wake: walking builder {id:?} at tick {}",
                self.tick.0
            );
        }
        for (id, expected) in &self.economy.builders.sleeping {
            let task = &self.economy.builders[id];
            let b = &self.economy.buildings[&task.building];
            let valid = self.unit(*id) == Some(*expected)
                && self
                    .economy
                    .construction
                    .clocks
                    .contains_key(&task.building)
                && self.economy.construction.clocks[&task.building].inputs
                    == (b.work_done, b.work_total, b.hp, b.max_hp, b.los)
                && self.lookup.get(id).is_some_and(|&i| {
                    self.units[i].order.is_none() && !self.units[i].state.planning
                })
                && (self.economy.construction.clocks[&task.building].clearing
                    || b.is_complete()
                    || !self.foundation_has_blocker(task.building));
            assert!(valid, "missed wake: builder {id:?} at tick {}", self.tick.0);
        }
    }
}
