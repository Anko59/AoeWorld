//! Dense `EntityId -> unit slot` index. Entity ids are allocated sequentially,
//! so a flat vector replaces the ordered map on the per-mover hot path.
use aoe_core::EntityId;
use std::ops::Index;

const VACANT: usize = usize::MAX;

#[derive(Clone, Debug, Default)]
pub(crate) struct UnitIndex {
    slots: Vec<usize>,
}

impl UnitIndex {
    pub(crate) const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(crate) fn get(&self, id: &EntityId) -> Option<&usize> {
        self.slots
            .get(id.0 as usize)
            .filter(|slot| **slot != VACANT)
    }

    pub(crate) fn contains_key(&self, id: &EntityId) -> bool {
        self.get(id).is_some()
    }

    pub(crate) fn insert(&mut self, id: EntityId, slot: usize) {
        let at = id.0 as usize;
        if at >= self.slots.len() {
            self.slots.resize(at + 1, VACANT);
        }
        self.slots[at] = slot;
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<usize> {
        let slot = self.slots.get_mut(id.0 as usize)?;
        (*slot != VACANT).then(|| std::mem::replace(slot, VACANT))
    }
}

impl Index<&EntityId> for UnitIndex {
    type Output = usize;

    fn index(&self, id: &EntityId) -> &usize {
        // A vacant slot holds `usize::MAX`, which fails the caller's own bounds check.
        &self.slots[id.0 as usize]
    }
}

#[cfg(test)]
mod tests;
