//! Lookup-only scratch data for one immutable road-planning call.
use crate::game_world::visibility::TileHasher;
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{BuildHasherDefault, Hash},
};

pub(super) type Lookup<K, V> = HashMap<K, V, BuildHasherDefault<TileHasher>>;
// Values occupy a fixed arena. Cache misses always return the live answer,
// including when the arena has no storage for another remembered value.
struct Store<K> {
    indices: Lookup<K, usize>,
    values: Box<[bool]>,
}

pub(super) struct Memo<K>(RefCell<Store<K>>);

impl<K> Default for Memo<K> {
    fn default() -> Self {
        Self(RefCell::new(Store {
            indices: Lookup::default(),
            values: vec![false; 65_536].into_boxed_slice(),
        }))
    }
}

impl<K: Copy + Eq + Hash> Memo<K> {
    pub(super) fn get(&self, key: K, read: impl FnOnce() -> bool) -> bool {
        let known = {
            let store = self.0.borrow();
            store.indices.get(&key).map(|&at| store.values[at])
        };
        if let Some(known) = known {
            return known;
        }
        let value = read();
        let mut store = self.0.borrow_mut();
        let at = store.indices.len();
        if let Some(slot) = store.values.get_mut(at) {
            *slot = value;
            store.indices.insert(key, at);
        }
        value
    }
}

#[cfg(test)]
#[path = "../../tests/economy/vassal/road_cache.rs"]
mod tests;
