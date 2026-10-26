use super::*;

fn retained_bytes(cache: &TerrainCache) -> usize {
    cache
        .entries
        .iter()
        .map(|(key, entry)| {
            key.content_hash.capacity()
                + entry.chunk.payload_hex.capacity()
                + std::mem::size_of::<Key>()
                + std::mem::size_of::<Entry>()
        })
        .sum()
}

#[test]
fn terrain_budget_charges_the_retained_hex_strings_and_entry_storage() {
    let mut cache = TerrainCache::with_limit(4096);
    for index in 0..100 {
        cache.insert(key(&format!("chunk-{index}")), chunk(900));
        assert!(
            retained_bytes(&cache) <= 4096,
            "encoded chunks exceeded the real residency budget"
        );
    }
    assert!(cache.get(&key("chunk-0")).is_none());
    assert!(cache.get(&key("chunk-99")).is_some());
}

#[test]
fn empty_payloads_still_pay_for_keys_and_entries() {
    let mut cache = TerrainCache::with_limit(1024);
    for index in 0..100 {
        cache.insert(key(&format!("empty-{index}")), chunk(0));
    }
    assert!(retained_bytes(&cache) <= 1024);
    assert!(cache.entries.len() < 100);
}

#[test]
fn oversized_capacity_is_not_admitted_and_replacements_release_the_old_charge() {
    let mut cache = TerrainCache::with_limit(4096);
    let mut oversized = chunk(0);
    oversized.payload_hex.reserve(8192);
    cache.insert(key("oversized"), oversized);
    assert_eq!(cache.usage().entries, 0);
    cache.insert(key("resident"), chunk(900));
    let before = cache.usage().resident_bytes;
    cache.insert(key("resident"), chunk(1));
    assert_eq!(cache.usage().entries, 1);
    assert!(cache.usage().resident_bytes < before);
    assert_eq!(cache.get(&key("resident")).unwrap().payload_hex, "00");
}

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
    let cost = retained_cost("first", 2);
    let mut cache = TerrainCache::with_limit(cost * 2 + 2);
    cache.insert(key("first"), chunk(2));
    cache.insert(key("second"), chunk(2));
    assert_eq!(
        cache.usage(),
        Usage {
            entries: 2,
            resident_bytes: cost * 2 + 1,
            limit_bytes: cost * 2 + 2,
        }
    );
    assert!(cache.get(&key("first")).is_some());
    cache.insert(key("third"), chunk(2));
    assert!(cache.get(&key("first")).is_some());
    assert!(cache.get(&key("second")).is_none());
    assert!(cache.get(&key("third")).is_some());
}

fn retained_cost(id: &str, bytes: usize) -> usize {
    id.len() + bytes * 2 + size_of::<Key>() + size_of::<Entry>() + 4 * size_of::<usize>()
}
