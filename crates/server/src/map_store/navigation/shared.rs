//! One verified immutable snapshot per package, shared by preparation and
//! replacement worlds. LRU retention is bounded; weak handles never pin worlds.
use aoe_map::NavigationSnapshot;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, Weak},
};

const MAX_BYTES: usize = 1 << 30;
const MAX_ENTRIES: usize = 256;
static CACHE: LazyLock<Mutex<SnapshotCache>> =
    LazyLock::new(|| Mutex::new(SnapshotCache::new(MAX_BYTES)));

struct Entry {
    data: Weak<NavigationSnapshot>,
    retained: Option<Arc<NavigationSnapshot>>,
    touched: u64,
}

struct SnapshotCache {
    entries: BTreeMap<PathBuf, Entry>,
    bytes: usize,
    clock: u64,
    limit: usize,
}

#[derive(Serialize)]
pub(crate) struct Usage {
    pub retained_bytes: usize,
    pub limit_bytes: usize,
    pub cached_snapshots: usize,
    pub live_snapshots: usize,
    pub live_bytes: usize,
}

impl SnapshotCache {
    fn new(limit: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            bytes: 0,
            clock: 0,
            limit,
        }
    }

    fn prune(&mut self) {
        self.entries
            .retain(|_, entry| entry.data.strong_count() > 0);
    }

    fn get(&mut self, key: &Path) -> Option<Arc<NavigationSnapshot>> {
        self.prune();
        let entry = self.entries.get_mut(key)?;
        let data = entry.data.upgrade()?;
        self.clock += 1;
        entry.touched = self.clock;
        Some(data)
    }

    fn insert(&mut self, key: PathBuf, data: Arc<NavigationSnapshot>) {
        self.prune();
        if let Some(old) = self.entries.remove(&key).and_then(|entry| entry.retained) {
            self.bytes -= old.retained_bytes();
        }
        self.clock += 1;
        let retained = (data.retained_bytes() <= self.limit).then(|| {
            self.bytes += data.retained_bytes();
            data.clone()
        });
        self.entries.insert(
            key,
            Entry {
                data: Arc::downgrade(&data),
                retained,
                touched: self.clock,
            },
        );
        while self.bytes > self.limit {
            let Some(key) = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.retained.is_some())
                .min_by_key(|(key, entry)| (entry.touched, *key))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(data) = self
                .entries
                .get_mut(&key)
                .and_then(|entry| entry.retained.take())
            {
                self.bytes -= data.retained_bytes();
            }
        }
        while self.entries.len() > MAX_ENTRIES {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.touched, *key))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(data) = self.entries.remove(&key).and_then(|entry| entry.retained) {
                self.bytes -= data.retained_bytes();
            }
        }
    }

    fn usage(&self) -> Usage {
        let mut usage = Usage {
            retained_bytes: self.bytes,
            limit_bytes: self.limit,
            cached_snapshots: 0,
            live_snapshots: 0,
            live_bytes: 0,
        };
        for entry in self.entries.values() {
            usage.cached_snapshots += usize::from(entry.retained.is_some());
            if let Some(data) = entry.data.upgrade() {
                usage.live_snapshots += 1;
                usage.live_bytes += data.retained_bytes();
            }
        }
        usage
    }
}

pub(super) fn get(key: &Path) -> Option<Arc<NavigationSnapshot>> {
    CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(key)
}

pub(super) fn insert(key: PathBuf, data: Arc<NavigationSnapshot>) {
    CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key, data);
}

pub(crate) fn usage() -> Usage {
    CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .usage()
}

#[cfg(test)]
mod tests;
