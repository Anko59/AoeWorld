use super::*;
use aoe_map::Path;

const AREA: TileRect = TileRect::new(TileCoord::new(0, 0), TileCoord::new(4, 4));

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
    cache.insert(TileCoord::new(0, 0), TileCoord::new(1, 0), 32, AREA, first);
    cache.insert(
        TileCoord::new(0, 0),
        TileCoord::new(2, 0),
        32,
        AREA,
        path(2),
    );
    assert_eq!(
        cache.usage(),
        NavigationCacheUsage {
            entries: 2,
            retained_bytes: entry_bytes * 2,
            limit_bytes: entry_bytes * 2,
        }
    );
    assert!(
        cache
            .get(TileCoord::new(0, 0), TileCoord::new(1, 0), 32)
            .is_some()
    );

    cache.insert(
        TileCoord::new(0, 0),
        TileCoord::new(3, 0),
        32,
        AREA,
        path(3),
    );

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

#[test]
fn expansion_exhaustion_is_not_cached() {
    let mut cache = NavigationCache::default();
    let origin = TileCoord::new(1, 2);
    let destination = TileCoord::new(5, 4);
    cache.insert(
        origin,
        destination,
        4_096,
        AREA,
        MovementOutcome::BudgetExceeded,
    );

    assert_eq!(cache.entry_count(), 0);
    assert_eq!(cache.get(origin, destination, 4_096), None);
}

#[test]
fn full_cache_eviction_does_not_scan_thousands_of_unrelated_routes() {
    let mut cache = NavigationCache::default();
    let origin = TileCoord::new(0, 0);
    for x in 0..MAX_NAVIGATION_CACHE_ENTRIES as i32 + 32 {
        cache.insert(origin, TileCoord::new(x, 0), 32, AREA, path(x));
    }
    assert_eq!(cache.entry_count(), MAX_NAVIGATION_CACHE_ENTRIES);
    assert!(cache.get(origin, TileCoord::new(0, 0), 32).is_none());
    assert!(cache.get(origin, TileCoord::new(32, 0), 32).is_some());
    assert!(
        cache.eviction_candidates <= 32,
        "eviction inspected {} entries for 32 replacements",
        cache.eviction_candidates
    );
}

#[test]
fn unaffordable_hits_replacements_invalidation_and_clear_keep_lru_order() {
    let origin = TileCoord::new(0, 0);
    let key = |x| TileCoord::new(x, 0);
    let mut cache = NavigationCache::with_limit(outcome_bytes(&path(1)) * 2);
    cache.insert(origin, key(1), 32, AREA, path(1));
    cache.insert(origin, key(2), 32, AREA, path(2));
    assert!(cache.get(origin, key(1), 31).is_none());
    cache.insert(origin, key(3), 32, AREA, path(3));
    assert!(
        cache.get(origin, key(1), 32).is_none(),
        "an unaffordable lookup must not refresh LRU"
    );
    cache.insert(origin, key(2), 32, AREA, path(20));
    cache.insert(origin, key(4), 32, AREA, path(4));
    assert_eq!(cache.get(origin, key(2), 32), Some((path(20), 32)));
    assert!(cache.get(origin, key(3), 32).is_none());
    cache.invalidate(AREA);
    assert_eq!(cache.usage().retained_bytes, 0);
    cache.insert(origin, key(5), 32, AREA, path(5));
    cache.clear();
    cache.insert(origin, key(6), 32, AREA, path(6));
    cache.insert(origin, key(7), 32, AREA, path(7));
    cache.insert(origin, key(8), 32, AREA, path(8));
    assert!(cache.get(origin, key(6), 32).is_none());
    assert!(cache.get(origin, key(7), 32).is_some());
    assert!(cache.get(origin, key(8), 32).is_some());
}

#[test]
fn saturated_clock_evicts_by_key_when_timestamps_tie() {
    let origin = TileCoord::new(0, 0);
    let mut cache = NavigationCache::with_limit(outcome_bytes(&path(1)) * 2);
    cache.clock = u64::MAX;
    for x in [2, 1, 3] {
        cache.insert(origin, TileCoord::new(x, 0), 32, AREA, path(x));
    }
    assert!(cache.get(origin, TileCoord::new(1, 0), 32).is_none());
    assert!(cache.get(origin, TileCoord::new(2, 0), 32).is_some());
    assert!(cache.get(origin, TileCoord::new(3, 0), 32).is_some());
}
