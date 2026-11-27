//! Current observer of every unit, building and animal, indexed by entity id.
//! Ids are allocated sequentially, so a flat vector replaces the ordered map
//! on the per-mover path. Herd recruits use sparse identities and slots.
use super::Observer;
use crate::game_world::wildlife::HERD_RECRUIT_ID_START;
use aoe_core::EntityId;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(super) struct Observers {
    slots: Vec<Option<Observer>>,
    herd_recruits: BTreeMap<EntityId, Observer>,
}

impl Observers {
    pub(super) fn get(&self, id: &EntityId) -> Option<Observer> {
        if id.0 < HERD_RECRUIT_ID_START {
            self.slots.get(id.0 as usize).copied().flatten()
        } else {
            self.herd_recruits.get(id).copied()
        }
    }

    pub(super) fn insert(&mut self, id: EntityId, observer: Observer) {
        if id.0 >= HERD_RECRUIT_ID_START {
            self.herd_recruits.insert(id, observer);
            return;
        }
        let at = id.0 as usize;
        if at >= self.slots.len() {
            self.slots.resize(at + 1, None);
        }
        self.slots[at] = Some(observer);
    }

    pub(super) fn remove(&mut self, id: &EntityId) -> Option<Observer> {
        if id.0 < HERD_RECRUIT_ID_START {
            self.slots.get_mut(id.0 as usize)?.take()
        } else {
            self.herd_recruits.remove(id)
        }
    }

    pub(super) fn clear(&mut self) {
        self.slots.clear();
        self.herd_recruits.clear();
    }
}
