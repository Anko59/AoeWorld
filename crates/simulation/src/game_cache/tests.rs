use super::*;
use aoe_core::TileCoord;
use aoe_core::{Seed, WorldConfig};
use aoe_map::{ResourceKind, ResourceNode};
use std::sync::Arc;

#[test]
fn resource_candidates_do_not_retain_every_visited_block() {
    let world = GameWorld::new(WorldConfig::new(64, 64, Seed(1)).unwrap()).unwrap();
    let mut cache = world.resource_chunks.lock().unwrap();
    for x in 0..10_000 {
        cache.insert((x, 0), Arc::new(Vec::new()));
    }
    assert!(
        cache.len() <= 8_192,
        "empty candidate blocks also need eviction"
    );
}

#[test]
fn candidate_cache_obeys_bytes_lru_replacement_and_clear() {
    let nodes = Arc::new(vec![
        ResourceNode::starting(
            TileCoord::new(1, 1),
            ResourceKind::Wood
        )
        .unwrap();
        64
    ]);
    let mut cache = ResourceCache::with_limit(4_000);
    cache.insert((0, 0), nodes.clone());
    cache.insert((1, 0), nodes.clone());
    assert_eq!(cache.len(), 2);
    assert!(cache.get(&(0, 0)).is_some());
    cache.insert((2, 0), nodes.clone());
    assert!(
        cache.get(&(1, 0)).is_none(),
        "least recently used block is evicted"
    );
    assert!(cache.get(&(0, 0)).is_some());
    assert!(cache.usage().1 <= 4_000);
    cache.insert((0, 0), nodes.clone());
    assert_eq!(cache.len(), 2, "replacement is charged only once");
    cache.insert((3, 0), Arc::new(vec![nodes[0]; 1_000]));
    assert!(
        cache.get(&(3, 0)).is_none(),
        "oversized blocks are not retained"
    );
    cache.clear();
    assert_eq!(cache.usage(), (0, 0));
}
