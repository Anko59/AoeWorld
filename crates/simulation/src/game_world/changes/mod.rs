//! Sparse mutation journals. Immutable access never marks a unit; mutable
//! access conservatively marks it, including orders between simulation ticks.
use super::{GameWorld, StoredUnit};
use aoe_core::EntityId;
use std::collections::{BTreeMap, BTreeSet, btree_map};
use std::ops::{Deref, Index, IndexMut};
use std::slice::SliceIndex;

#[derive(Debug, Default)]
pub(crate) struct Units {
    #[cfg(not(test))]
    values: Vec<StoredUnit>,
    #[cfg(test)]
    values: super::unit_access::UnitStore,
    dirty: BTreeSet<EntityId>,
}

impl Deref for Units {
    type Target = [StoredUnit];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<I: SliceIndex<[StoredUnit]>> Index<I> for Units {
    type Output = I::Output;
    fn index(&self, at: I) -> &Self::Output {
        &self.values[at]
    }
}
impl IndexMut<usize> for Units {
    fn index_mut(&mut self, at: usize) -> &mut StoredUnit {
        let unit = &mut self.values[at];
        self.dirty.insert(unit.state.id);
        unit
    }
}
impl<'a> IntoIterator for &'a Units {
    type Item = &'a StoredUnit;
    type IntoIter = std::slice::Iter<'a, StoredUnit>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<'a> IntoIterator for &'a mut Units {
    type Item = &'a mut StoredUnit;
    type IntoIter = std::slice::IterMut<'a, StoredUnit>;
    fn into_iter(self) -> Self::IntoIter {
        self.mark_all();
        self.values.iter_mut()
    }
}
impl Units {
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    fn mark_all(&mut self) {
        // The probe counts the caller's exposed slots once, independently of
        // the journal's internal reads of those same slots.
        #[cfg(test)]
        let values = self.values.oracle_slice();
        #[cfg(not(test))]
        let values = &self.values;
        self.dirty.extend(values.iter().map(|u| u.state.id));
    }
    pub(crate) fn push(&mut self, unit: StoredUnit) {
        self.dirty.insert(unit.state.id);
        self.values.push(unit);
    }
    pub(crate) fn swap_remove(&mut self, at: usize) -> StoredUnit {
        let removed = self.values.swap_remove(at);
        self.dirty.insert(removed.state.id);
        if let Some(moved) = self.values.get(at) {
            self.dirty.insert(moved.state.id);
        }
        removed
    }
}

/// Report metadata has the same invalidation rule as unit storage. There is
/// deliberately no DerefMut: writes must pass through a journaling method.
#[derive(Debug)]
pub(crate) struct Reports<T> {
    values: BTreeMap<EntityId, T>,
    dirty: BTreeSet<EntityId>,
}
impl<T> Default for Reports<T> {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            dirty: BTreeSet::new(),
        }
    }
}
impl<T> Deref for Reports<T> {
    type Target = BTreeMap<EntityId, T>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<'a, T> IntoIterator for &'a Reports<T> {
    type Item = (&'a EntityId, &'a T);
    type IntoIter = btree_map::Iter<'a, EntityId, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<T> Reports<T> {
    pub(crate) fn insert(&mut self, id: EntityId, value: T) -> Option<T> {
        self.dirty.insert(id);
        self.values.insert(id, value)
    }
    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<T> {
        let value = self.values.remove(id)?;
        self.dirty.insert(*id);
        Some(value)
    }
    pub(crate) fn get_mut(&mut self, id: &EntityId) -> Option<&mut T> {
        let value = self.values.get_mut(id)?;
        self.dirty.insert(*id);
        Some(value)
    }
    pub(crate) fn entry(&mut self, id: EntityId) -> btree_map::Entry<'_, EntityId, T> {
        self.dirty.insert(id);
        self.values.entry(id)
    }
    pub(crate) fn mark(&mut self, ids: impl IntoIterator<Item = EntityId>) {
        self.dirty.extend(ids);
    }
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.dirty.extend(self.values.keys().copied());
        self.values.clear();
    }
    pub(crate) fn take_changes(&mut self) -> BTreeSet<EntityId> {
        std::mem::take(&mut self.dirty)
    }
}

impl GameWorld {
    /// End-of-tick ids whose state, report or storage membership may differ.
    /// Removed ids remain present so adapters can invalidate retained indexes.
    pub fn unit_changes(&self) -> &[EntityId] {
        &self.unit_changes
    }

    /// Storage order is observable when a bounded owner list overflows.
    pub fn unit_storage_slot(&self, id: EntityId) -> Option<usize> {
        self.lookup.get(&id).copied()
    }

    pub(crate) fn publish_unit_changes(&mut self, changed: &[super::GameUnit]) {
        let mut ids = std::mem::take(&mut self.units.dirty);
        ids.extend(changed.iter().map(|u| u.id));
        ids.extend(self.economy.gatherers.take_changes());
        ids.extend(self.wildlife.take_unit_changes());
        ids.extend(self.economy.cargo.take_changes());
        ids.extend(self.economy.generals.armies.take_changes());
        ids.extend(self.economy.generals.controlled.take_changes());
        ids.extend(self.economy.stewards.controlled.take_changes());
        // A member's movement, attack or HP changes its General's squad report.
        let generals = ids
            .iter()
            .filter_map(|id| self.general_controller(*id))
            .collect::<Vec<_>>();
        ids.extend(generals);
        self.unit_changes = ids.into_iter().collect();
    }
}

#[cfg(test)]
mod tests;
