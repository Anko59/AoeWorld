use super::*;

fn snapshot() -> Arc<NavigationSnapshot> {
    let generator = aoe_map::MapChunkGenerator::new([7; 32], 7, 1);
    super::super::prepare(&generator, [7; 32], None, &|| false).unwrap();
    generator.navigation_snapshot().unwrap()
}

#[test]
fn snapshot_store_bounds_bytes_and_uses_lru_without_pinning_evicted_worlds() {
    let (one, two, three) = (snapshot(), snapshot(), snapshot());
    let bytes = one.retained_bytes();
    let mut cache = SnapshotCache::new(bytes * 2);
    let (a, b, c) = (PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c"));
    cache.insert(a.clone(), one.clone());
    cache.insert(b.clone(), two.clone());
    cache.get(&a).unwrap();
    cache.insert(c, three);
    assert!(cache.entries[&a].retained.is_some());
    assert!(
        cache.entries[&b].retained.is_none(),
        "evict the least recently used snapshot"
    );
    assert!(
        Arc::ptr_eq(&cache.get(&b).unwrap(), &two),
        "a live world still shares after eviction"
    );
    assert!(cache.usage().retained_bytes <= bytes * 2);
    let weak = Arc::downgrade(&two);
    drop(two);
    assert!(
        weak.upgrade().is_none(),
        "the weak registry does not retain a retired world"
    );
    assert!(cache.get(&b).is_none());
}

#[test]
fn oversized_snapshots_and_empty_metadata_are_bounded() {
    let snapshot = snapshot();
    let mut cache = SnapshotCache::new(0);
    for at in 0..MAX_ENTRIES + 20 {
        cache.insert(PathBuf::from(at.to_string()), snapshot.clone());
        assert!(cache.entries.len() <= MAX_ENTRIES);
        assert_eq!(cache.usage().retained_bytes, 0);
    }
    drop(snapshot);
    cache.prune();
    assert!(cache.entries.is_empty());
}
