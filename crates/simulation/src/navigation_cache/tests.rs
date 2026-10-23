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
