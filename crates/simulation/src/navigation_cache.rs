use aoe_core::TileCoord;
use aoe_map::MovementOutcome;
use std::{collections::BTreeMap, mem::size_of};

pub(crate) const MAX_NAVIGATION_CACHE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
}

#[derive(Debug)]
struct Entry {
    outcome: MovementOutcome,
    bytes: usize,
    last_used: u64,
}

/// Bounded, non-canonical cache of already-planned map segments. The caller
/// still accounts for its planning budget before consulting the cache.
#[derive(Debug)]
pub(crate) struct NavigationCache {
    entries: BTreeMap<Key, Entry>,
    retained_bytes: usize,
    clock: u64,
    limit_bytes: usize,
}

impl Default for NavigationCache {
    fn default() -> Self {
        Self::with_limit(MAX_NAVIGATION_CACHE_BYTES)
    }
}

impl NavigationCache {
    fn with_limit(limit_bytes: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            retained_bytes: 0,
            clock: 0,
            limit_bytes,
        }
    }

    pub(crate) fn get(
        &mut self,
        origin: TileCoord,
        destination: TileCoord,
        max_expansions: u32,
    ) -> Option<MovementOutcome> {
        self.clock = self.clock.saturating_add(1);
        self.entries
            .get_mut(&Key {
                origin,
                destination,
                max_expansions,
            })
            .map(|entry| {
                entry.last_used = self.clock;
                entry.outcome.clone()
            })
    }

    pub(crate) fn insert(
        &mut self,
        origin: TileCoord,
        destination: TileCoord,
        max_expansions: u32,
        outcome: MovementOutcome,
    ) {
        let key = Key {
            origin,
            destination,
            max_expansions,
        };
        let bytes = outcome_bytes(&outcome);
        if bytes > self.limit_bytes {
            return;
        }
        self.clock = self.clock.saturating_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.retained_bytes = self.retained_bytes.saturating_sub(previous.bytes);
        }
        while self.retained_bytes.saturating_add(bytes) > self.limit_bytes {
            let Some(eviction) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.last_used, **key))
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&eviction) {
                self.retained_bytes = self.retained_bytes.saturating_sub(removed.bytes);
            }
        }
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            Entry {
                outcome,
                bytes,
                last_used: self.clock,
            },
        );
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
    }

    #[cfg(test)]
    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

fn outcome_bytes(outcome: &MovementOutcome) -> usize {
    let path_bytes = match outcome {
        MovementOutcome::Path(path) => path.tiles.capacity().saturating_mul(size_of::<TileCoord>()),
        MovementOutcome::InvalidDestination
        | MovementOutcome::Unreachable
        | MovementOutcome::BudgetExceeded => 0,
    };
    size_of::<Entry>().saturating_add(path_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::Path;

    fn path(x: i32) -> MovementOutcome {
        MovementOutcome::Path(Path {
            tiles: vec![TileCoord::new(x, 0)],
            cost: 0,
        })
    }

    #[test]
    fn evicts_the_least_recently_used_segment() {
        let first = path(1);
        let entry_bytes = outcome_bytes(&first);
        let mut cache = NavigationCache::with_limit(entry_bytes * 2);
        cache.insert(TileCoord::new(0, 0), TileCoord::new(1, 0), 32, first);
        cache.insert(TileCoord::new(0, 0), TileCoord::new(2, 0), 32, path(2));
        assert!(
            cache
                .get(TileCoord::new(0, 0), TileCoord::new(1, 0), 32)
                .is_some()
        );

        cache.insert(TileCoord::new(0, 0), TileCoord::new(3, 0), 32, path(3));

        assert!(
            cache
                .get(TileCoord::new(0, 0), TileCoord::new(1, 0), 32)
                .is_some()
        );
        assert!(
            cache
                .get(TileCoord::new(0, 0), TileCoord::new(2, 0), 32)
                .is_none()
        );
    }
}
