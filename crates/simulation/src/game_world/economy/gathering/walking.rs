//! Walking jobs subscribe to movement/contact events, not the gathering loop.
use super::*;
use aoe_core::TileRect;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Walk {
    until: u64,
    near: Option<TileCoord>,
}

#[derive(Debug, Default)]
pub(super) struct Walks {
    jobs: BTreeMap<EntityId, Walk>,
    tiles: BTreeMap<TileCoord, BTreeSet<EntityId>>,
    notified: BTreeSet<EntityId>,
}

fn contact_tiles(at: TileCoord) -> impl Iterator<Item = TileCoord> {
    (at.y - 2..=at.y + 2)
        .flat_map(move |y| (at.x - 2..=at.x + 2).map(move |x| TileCoord::new(x, y)))
}

impl Walks {
    fn subscribe(&mut self, id: EntityId, at: TileCoord, add: bool) {
        for tile in contact_tiles(at) {
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

    pub(super) fn remove(&mut self, id: EntityId) -> Option<u64> {
        let walk = self.jobs.remove(&id)?;
        if let Some(at) = walk.near {
            self.subscribe(id, at, false);
        }
        Some(walk.until)
    }

    fn update(&mut self, id: EntityId, walk: Walk) -> Option<u64> {
        let old = self.jobs.insert(id, walk);
        if old.and_then(|w| w.near) != walk.near {
            if let Some(at) = old.and_then(|w| w.near) {
                self.subscribe(id, at, false);
            }
            if let Some(at) = walk.near {
                self.subscribe(id, at, true);
            }
        }
        old.map(|w| w.until)
    }

    pub(super) fn len(&self) -> usize {
        self.jobs.len()
    }
}

impl Gatherers {
    pub(crate) fn wake_walking(&mut self, id: EntityId) {
        if self.walking(id) {
            self.wake(id);
        }
    }
    pub(super) fn walking(&self, id: EntityId) -> bool {
        self.dozes.walking.jobs.contains_key(&id)
    }

    fn doze_walk(&mut self, id: EntityId, walk: Walk) {
        if self.doze_until(id, walk.until) {
            self.dozes.walking.update(id, walk);
        }
    }

    fn retime_walk(&mut self, id: EntityId, walk: Walk) {
        if let Some(old) = self.dozes.walking.update(id, walk) {
            self.retime_doze(id, old, walk.until);
        }
    }

    pub(crate) fn notify_walkers_at(&mut self, tile: TileCoord) {
        if let Some(ids) = self.dozes.walking.tiles.get(&tile) {
            self.dozes.walking.notified.extend(ids);
        }
    }

    pub(crate) fn notify_walker_ground(&mut self, area: TileRect) {
        let walks = &mut self.dozes.walking;
        let count = i64::from(area.width()) * i64::from(area.height());
        if count <= walks.tiles.len() as i64 {
            for y in area.min.y..area.max.y {
                for x in area.min.x..area.max.x {
                    if let Some(ids) = walks.tiles.get(&TileCoord::new(x, y)) {
                        walks.notified.extend(ids);
                    }
                }
            }
        } else {
            for (tile, ids) in &walks.tiles {
                if area.contains(*tile) {
                    walks.notified.extend(ids);
                }
            }
        }
    }
}

impl GameWorld {
    fn walker_refines_nearby(&self, index: usize, task: &GatherTask) -> bool {
        let at = self.units[index].state.position.tile_floor();
        task.phase == GatherPhase::Approaching
            && task.crowded
            && task.farm.is_none()
            && task.animal.is_none()
            && task.fish.is_none()
            && at.x.abs_diff(task.tile.x) <= 8
            && at.y.abs_diff(task.tile.y) <= 8
    }

    fn walk_schedule(&self, id: EntityId, index: usize, task: &GatherTask) -> Walk {
        let period = u64::from(self.config.tick_hz).max(1);
        let until = if self.walker_refines_nearby(index, task) {
            let delta = (period - (self.tick.0 + u64::from(id.0)) % period) % period;
            self.tick
                .0
                .saturating_add(if delta == 0 { period } else { delta })
        } else {
            u64::MAX
        };
        // Only walkers already at geometric work contact need neighbour
        // notifications: a distant body cannot change their arrival predicate.
        let near = self
            .gather_contact_geometry(index, task)
            .then_some(self.units[index].state.position.tile_floor());
        Walk { until, near }
    }

    pub(super) fn sleep_walking_gatherer(&mut self, id: EntityId) {
        if self.economy.no_sleep {
            return;
        }
        if let (Some(&index), Some(task)) = (self.lookup.get(&id), self.economy.gatherers.get(&id))
        {
            let walk = self.walk_schedule(id, index, task);
            self.economy.gatherers.doze_walk(id, walk);
        }
    }

    /// A movement event wakes a walking job only on contact, termination,
    /// failure, or its existing near-site refinement deadline. Intermediate
    /// positions only update the derived contact subscription.
    pub(super) fn wake_gatherer_event(&mut self, id: EntityId) {
        if !self.economy.gatherers.walking(id) {
            self.economy.gatherers.wake(id);
            return;
        }
        let plan = self.lookup.get(&id).and_then(|&index| {
            let task = self.economy.gatherers.get(&id)?;
            let refine = self.walker_refines_nearby(index, task)
                && (self.tick.0 + u64::from(id.0))
                    .is_multiple_of(u64::from(self.config.tick_hz).max(1));
            (self.gatherer_walks(index, task) && !refine)
                .then(|| self.walk_schedule(id, index, task))
        });
        if let Some(walk) = plan {
            self.economy.gatherers.retime_walk(id, walk);
        } else {
            self.economy.gatherers.wake(id);
        }
    }

    pub(super) fn wake_walker_contacts(&mut self) {
        for id in std::mem::take(&mut self.economy.gatherers.dozes.walking.notified) {
            self.wake_gatherer_event(id);
        }
    }

    #[cfg(test)]
    pub(super) fn assert_walking_dozers(&self) {
        for id in self.economy.gatherers.dozes.walking.jobs.keys() {
            let task = &self.economy.gatherers[id];
            assert!(
                self.lookup
                    .get(id)
                    .is_some_and(|&index| self.gatherer_walks(index, task)),
                "missed wake: walking gatherer {id:?} at tick {}",
                self.tick.0
            );
        }
    }
}
