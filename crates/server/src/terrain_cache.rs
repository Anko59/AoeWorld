use aoe_map::CompactChunk;
use serde::Serialize;
use std::collections::BTreeMap;

pub const MAX_GENERATED_TERRAIN_CACHE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Usage {
    pub entries: usize,
    pub resident_bytes: usize,
    pub limit_bytes: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Key {
    pub content_hash: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug)]
struct Entry {
    chunk: CompactChunk,
    resident_bytes: usize,
    last_used: u64,
}

#[derive(Debug)]
pub struct TerrainCache {
    entries: BTreeMap<Key, Entry>,
    resident_bytes: usize,
    clock: u64,
    limit: usize,
}

impl Default for TerrainCache {
    fn default() -> Self {
        Self::with_limit(MAX_GENERATED_TERRAIN_CACHE_BYTES)
    }
}

impl TerrainCache {
    fn with_limit(limit: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            resident_bytes: 0,
            clock: 0,
            limit,
        }
    }

    pub fn get(&mut self, key: &Key) -> Option<CompactChunk> {
        let last_used = self.next_clock();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = last_used;
        Some(entry.chunk.clone())
    }

    pub fn insert(&mut self, key: Key, chunk: CompactChunk) {
        // Charge retained hexadecimal capacity, keys, entries and conservative
        // BTree node pointer slack, including empty payloads.
        let resident_bytes = chunk.payload_hex.capacity()
            + key.content_hash.capacity()
            + size_of::<Key>()
            + size_of::<Entry>()
            + 4 * size_of::<usize>();
        if resident_bytes > self.limit {
            return;
        }
        self.clock = self.next_clock();
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.resident_bytes);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(resident_bytes);
        self.entries.insert(
            key,
            Entry {
                chunk,
                resident_bytes,
                last_used: self.clock,
            },
        );
        while self.resident_bytes > self.limit {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.last_used, *key))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.resident_bytes = self.resident_bytes.saturating_sub(entry.resident_bytes);
            }
        }
    }

    pub fn usage(&self) -> Usage {
        Usage {
            entries: self.entries.len(),
            resident_bytes: self.resident_bytes,
            limit_bytes: self.limit,
        }
    }

    fn next_clock(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }
}

#[path = "terrain_cache/tests.rs"]
#[cfg(test)]
mod tests;
