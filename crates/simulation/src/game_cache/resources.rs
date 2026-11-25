//! Candidate blocks are immutable until the starting overlay is changed.
//! Keep a bounded LRU, including empty blocks, rather than the entire walk.
use crate::game_world::ChunkNodes;
use std::collections::{BTreeMap, BTreeSet};

/// Bytes, entry overhead included, so empty blocks are bounded too (about
/// 220,000 of them). There is deliberately no separate entry count: a fixed
/// 8,192-block cap was smaller than the villages' working set from 50k units
/// up (14,890 blocks at 48k, 20,721 at 96k, 4.4 and 6.6 MB), so every
/// replacement search rebuilt evicted blocks from the map generator.
const MAX_BYTES: usize = 32 << 20;
type Key = (i32, i32);

#[derive(Debug)]
struct Entry {
    nodes: ChunkNodes,
    bytes: usize,
    touched: u64,
    /// The stamp `order` files this entry under (at most `touched`).
    filed: u64,
}

#[derive(Debug)]
pub(crate) struct ResourceCache {
    entries: BTreeMap<Key, Entry>,
    order: BTreeSet<(u64, Key)>,
    bytes: usize,
    clock: u64,
    limit: usize,
}

impl Default for ResourceCache {
    fn default() -> Self {
        Self::with_limit(MAX_BYTES)
    }
}

impl ResourceCache {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            order: BTreeSet::new(),
            bytes: 0,
            clock: 0,
            limit,
        }
    }

    /// A hit only restamps its entry: `order` keeps each key at the stamp it
    /// was filed under, and eviction files a stale one again at its current
    /// stamp, so the least recently used entry still leaves first without
    /// two tree edits per hit (#978).
    pub(crate) fn get(&mut self, key: &Key) -> Option<ChunkNodes> {
        let entry = self.entries.get_mut(key)?;
        self.clock += 1;
        entry.touched = self.clock;
        Some(entry.nodes.clone())
    }

    pub(crate) fn insert(&mut self, key: Key, nodes: ChunkNodes) {
        // Include both tree entries, Arc header, vector and conservative node slack.
        let bytes = nodes.capacity() * size_of::<aoe_map::ResourceNode>()
            + size_of::<Entry>()
            + size_of::<Key>()
            + size_of::<Vec<aoe_map::ResourceNode>>()
            + size_of::<(u64, Key)>()
            + 10 * size_of::<usize>();
        if bytes > self.limit {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
            self.order.remove(&(old.filed, key));
        }
        while self.bytes + bytes > self.limit {
            let Some((filed, key)) = self.order.pop_first() else {
                break;
            };
            match self.entries.get_mut(&key) {
                Some(entry) if entry.touched != filed => {
                    entry.filed = entry.touched;
                    self.order.insert((entry.touched, key));
                }
                Some(_) => {
                    if let Some(old) = self.entries.remove(&key) {
                        self.bytes -= old.bytes;
                    }
                }
                None => {}
            }
        }
        self.clock += 1;
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                nodes,
                bytes,
                touched: self.clock,
                filed: self.clock,
            },
        );
        self.order.insert((self.clock, key));
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn usage(&self) -> (usize, usize) {
        (self.len(), self.bytes)
    }
}
