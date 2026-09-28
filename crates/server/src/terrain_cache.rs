use aoe_map::CompactChunk;
use serde::Serialize;
use std::collections::BTreeMap;

pub const MAX_GENERATED_TERRAIN_CACHE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Usage {
    pub entries: usize,
    pub decoded_bytes: usize,
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
    decoded_bytes: usize,
    last_used: u64,
}

#[derive(Debug)]
pub struct TerrainCache {
    entries: BTreeMap<Key, Entry>,
    decoded_bytes: usize,
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
            decoded_bytes: 0,
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
        let decoded_bytes = chunk.decoded_len().unwrap_or(0);
        if decoded_bytes > self.limit {
            return;
        }
        self.clock = self.next_clock();
        if let Some(previous) = self.entries.remove(&key) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(previous.decoded_bytes);
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        self.entries.insert(
            key,
            Entry {
                chunk,
                decoded_bytes,
                last_used: self.clock,
            },
        );
        while self.decoded_bytes > self.limit {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.last_used, *key))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(entry.decoded_bytes);
            }
        }
    }

    pub fn usage(&self) -> Usage {
        Usage {
            entries: self.entries.len(),
            decoded_bytes: self.decoded_bytes,
            limit_bytes: self.limit,
        }
    }

    fn next_clock(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> Key {
        Key {
            content_hash: id.to_owned(),
            x: 0,
            y: 0,
        }
    }

    fn chunk(bytes: usize) -> CompactChunk {
        CompactChunk {
            x: 0,
            y: 0,
            payload_hex: "00".repeat(bytes),
        }
    }

    #[test]
    fn evicts_the_least_recently_used_decoded_chunk() {
        let mut cache = TerrainCache::with_limit(4);
        cache.insert(key("first"), chunk(2));
        cache.insert(key("second"), chunk(2));
        assert_eq!(
            cache.usage(),
            Usage {
                entries: 2,
                decoded_bytes: 4,
                limit_bytes: 4,
            }
        );
        assert!(cache.get(&key("first")).is_some());
        cache.insert(key("third"), chunk(2));
        assert!(cache.get(&key("first")).is_some());
        assert!(cache.get(&key("second")).is_none());
        assert!(cache.get(&key("third")).is_some());
    }
}
