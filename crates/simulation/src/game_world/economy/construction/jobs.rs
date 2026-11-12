//! Builder membership owns wake events for every job mutation.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

#[derive(Debug, Default)]
pub(crate) struct Builders {
    tasks: BTreeMap<EntityId, BuildTask>,
    pub(super) awake: BTreeSet<EntityId>,
    pub(super) members: BTreeMap<EntityId, BTreeSet<EntityId>>,
    pub(super) dirty: BTreeSet<EntityId>,
    pub(super) sleeping: BTreeMap<EntityId, crate::GameUnit>,
    held: BTreeMap<TileCoord, usize>,
    pub(super) walking: BTreeSet<EntityId>,
    pub(super) retries: BTreeMap<EntityId, u64>,
    retry_due: BTreeSet<(u64, EntityId)>,
}

impl Builders {
    pub(crate) fn insert(&mut self, id: EntityId, task: BuildTask) -> Option<BuildTask> {
        let old = self.remove(&id);
        self.dirty.insert(task.building);
        self.members.entry(task.building).or_default().insert(id);
        self.awake.insert(id);
        self.tasks.insert(id, task);
        old
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<BuildTask> {
        let task = self.tasks.remove(id)?;
        self.dirty.insert(task.building);
        self.awake.remove(id);
        self.unhold(*id);
        self.walking.remove(id);
        if let Some(until) = self.retries.remove(id) {
            self.retry_due.remove(&(until, *id));
        }
        if let Some(ids) = self.members.get_mut(&task.building) {
            ids.remove(id);
            if ids.is_empty() {
                self.members.remove(&task.building);
            }
        }
        Some(task)
    }

    pub(crate) fn get_mut(&mut self, id: &EntityId) -> Option<&mut BuildTask> {
        if let Some(task) = self.tasks.get(id) {
            self.dirty.insert(task.building);
        }
        self.tasks.get_mut(id)
    }

    pub(super) fn wake(&mut self, building: EntityId) {
        if let Some(ids) = self.members.get(&building).cloned() {
            for id in &ids {
                self.unhold(*id);
                self.walking.remove(id);
                if let Some(until) = self.retries.remove(id) {
                    self.retry_due.remove(&(until, *id));
                }
                self.awake.insert(*id);
            }
        }
    }

    pub(super) fn doze_walk(&mut self, id: EntityId) {
        self.awake.remove(&id);
        self.walking.insert(id);
    }

    pub(super) fn holds(&self, tile: TileCoord) -> bool {
        self.held.contains_key(&tile)
    }

    pub(super) fn hold(&mut self, unit: crate::GameUnit) {
        self.unhold(unit.id);
        *self.held.entry(unit.position.tile_floor()).or_default() += 1;
        self.sleeping.insert(unit.id, unit);
    }

    fn unhold(&mut self, id: EntityId) {
        if let Some(unit) = self.sleeping.remove(&id) {
            let tile = unit.position.tile_floor();
            if let Some(count) = self.held.get_mut(&tile) {
                *count -= 1;
                if *count == 0 {
                    self.held.remove(&tile);
                }
            }
        }
    }

    pub(super) fn doze_retry(&mut self, id: EntityId, until: u64) {
        if let Some(old) = self.retries.insert(id, until) {
            self.retry_due.remove(&(old, id));
        }
        self.retry_due.insert((until, id));
        self.awake.remove(&id);
    }

    pub(super) fn wake_retries(&mut self, now: u64) {
        while let Some(&(until, id)) = self.retry_due.first() {
            if until > now {
                break;
            }
            self.retry_due.remove(&(until, id));
            self.retries.remove(&id);
            self.awake.insert(id);
        }
    }
}

impl Deref for Builders {
    type Target = BTreeMap<EntityId, BuildTask>;
    fn deref(&self) -> &Self::Target {
        &self.tasks
    }
}

impl<'a> IntoIterator for &'a Builders {
    type Item = (&'a EntityId, &'a BuildTask);
    type IntoIter = std::collections::btree_map::Iter<'a, EntityId, BuildTask>;
    fn into_iter(self) -> Self::IntoIter {
        self.tasks.iter()
    }
}
