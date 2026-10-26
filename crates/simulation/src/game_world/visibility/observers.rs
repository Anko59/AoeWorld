//! Current observer of every unit, building and animal, indexed by entity id.
//! Ids are allocated sequentially, so a flat vector replaces the ordered map
//! on the per-mover path. Nothing iterates it.
use super::Observer;
use aoe_core::EntityId;

#[derive(Debug, Default)]
pub(super) struct Observers {
    slots: Vec<Option<Observer>>,
}

impl Observers {
    pub(super) fn get(&self, id: &EntityId) -> Option<Observer> {
        self.slots.get(id.0 as usize).copied().flatten()
    }

    pub(super) fn insert(&mut self, id: EntityId, observer: Observer) {
        let at = id.0 as usize;
        if at >= self.slots.len() {
            self.slots.resize(at + 1, None);
        }
        self.slots[at] = Some(observer);
    }

    pub(super) fn remove(&mut self, id: &EntityId) -> Option<Observer> {
        self.slots.get_mut(id.0 as usize)?.take()
    }

    pub(super) fn clear(&mut self) {
        self.slots.clear();
    }
}
