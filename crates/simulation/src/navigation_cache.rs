use aoe_core::{TileCoord, TileRect};
use aoe_map::MovementOutcome;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

pub(crate) const MAX_NAVIGATION_CACHE_BYTES: usize = 128 * 1024 * 1024;
/// Entries retained at once: a ground change scans them all, so the count,
/// not only the bytes, is bounded.
pub(crate) const MAX_NAVIGATION_CACHE_ENTRIES: usize = 8_192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationCacheUsage {
    pub entries: usize,
    /// Logical bytes charged for cached outcomes. This includes the modeled
    /// entry, recency key and path payload but excludes allocator/tree overhead.
    pub retained_bytes: usize,
    /// Logical payload budget; this is not a hard resident-process cap.
    pub limit_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    origin: TileCoord,
    destination: TileCoord,
}

#[derive(Debug)]
struct Entry {
    outcome: MovementOutcome,
    /// Planner work that produced the outcome from a fresh search.
    work: u32,
    /// Ground the outcome depends on (`RoutePlanner::explored_area`).
    area: TileRect,
    bytes: usize,
    last_used: u64,
}

/// Bounded, non-canonical cache of already-planned map segments. Its byte
/// limit accounts for logical entry, recency key and path payload bytes;
/// allocator and tree metadata remain outside the reported budget. An entry records the
/// planner work its outcome cost and answers only a caller that could afford
/// that work; the caller charges it, so a hit and a fresh search leave the
/// same canonical planning budget. A ground change drops only the entries
/// whose explored area it touches: outside that area the search would read
/// the same ground and prove the same outcome.
#[derive(Debug)]
pub(crate) struct NavigationCache {
    #[cfg(test)]
    eviction_candidates: usize,
    entries: BTreeMap<Key, Entry>,
    /// Same ordering as the old eviction scan, including saturated-clock ties.
    recency: BTreeSet<(u64, Key)>,
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
            #[cfg(test)]
            eviction_candidates: 0,
            entries: BTreeMap::new(),
            recency: BTreeSet::new(),
            retained_bytes: 0,
            clock: 0,
            limit_bytes,
        }
    }

    pub(crate) fn get(
        &mut self,
        origin: TileCoord,
        destination: TileCoord,
        budget: u32,
    ) -> Option<(MovementOutcome, u32)> {
        self.clock = self.clock.saturating_add(1);
        self.entries
            .get_mut(&Key {
                origin,
                destination,
            })
            .filter(|entry| entry.work <= budget)
            .map(|entry| {
                let key = Key {
                    origin,
                    destination,
                };
                self.recency.remove(&(entry.last_used, key));
                entry.last_used = self.clock;
                self.recency.insert((entry.last_used, key));
                (entry.outcome.clone(), entry.work)
            })
    }

    pub(crate) fn insert(
        &mut self,
        origin: TileCoord,
        destination: TileCoord,
        work: u32,
        area: TileRect,
        outcome: MovementOutcome,
    ) {
        if matches!(outcome, MovementOutcome::BudgetExceeded) {
            return;
        }
        let key = Key {
            origin,
            destination,
        };
        let bytes = outcome_bytes(&outcome);
        if bytes > self.limit_bytes {
            return;
        }
        self.clock = self.clock.saturating_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.recency.remove(&(previous.last_used, key));
            self.retained_bytes = self.retained_bytes.saturating_sub(previous.bytes);
        }
        while self.retained_bytes.saturating_add(bytes) > self.limit_bytes
            || self.entries.len() >= MAX_NAVIGATION_CACHE_ENTRIES
        {
            #[cfg(test)]
            {
                self.eviction_candidates += 1;
            }
            let Some(eviction) = self.recency.pop_first().map(|(_, key)| key) else {
                break;
            };
            if let Some(removed) = self.entries.remove(&eviction) {
                self.retained_bytes = self.retained_bytes.saturating_sub(removed.bytes);
            }
        }
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.recency.insert((self.clock, key));
        self.entries.insert(
            key,
            Entry {
                outcome,
                work,
                area,
                bytes,
                last_used: self.clock,
            },
        );
    }

    /// Drops the outcomes that depend on ground inside `changed`.
    pub(crate) fn invalidate(&mut self, changed: TileRect) {
        let overlaps = |area: &TileRect| {
            area.min.x < changed.max.x
                && changed.min.x < area.max.x
                && area.min.y < changed.max.y
                && changed.min.y < area.max.y
        };
        let mut freed = 0;
        self.entries.retain(|key, entry| {
            let keep = !overlaps(&entry.area);
            if !keep {
                freed += entry.bytes;
                self.recency.remove(&(entry.last_used, *key));
            }
            keep
        });
        self.retained_bytes = self.retained_bytes.saturating_sub(freed);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.retained_bytes = 0;
    }

    pub(crate) fn usage(&self) -> NavigationCacheUsage {
        NavigationCacheUsage {
            entries: self.entries.len(),
            retained_bytes: self.retained_bytes,
            limit_bytes: self.limit_bytes,
        }
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
    size_of::<Entry>()
        .saturating_add(size_of::<(u64, Key)>())
        .saturating_add(path_bytes)
}

#[cfg(test)]
mod tests;
