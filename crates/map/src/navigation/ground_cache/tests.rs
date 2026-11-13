use super::RouteGroundCache;
use aoe_core::{TileCoord, TileRect};

#[test]
fn cache_keeps_negative_answers_and_invalidates_only_changed_chunks() {
    let mut cache = RouteGroundCache::default();
    let water = TileCoord::new(31, 31);
    let land = TileCoord::new(32, 32);
    cache.insert(water, true);
    cache.insert(land, false);
    assert_eq!(cache.get(water), Some(true));
    assert_eq!(cache.get(land), Some(false));
    cache.invalidate(TileRect::from_xywh(31, 31, 2, 2));
    assert_eq!(cache.get(water), None);
    assert_eq!(cache.get(land), None);
    cache.insert(water, true);
    cache.invalidate(TileRect::from_xywh(96, 96, 1, 1));
    assert_eq!(cache.get(water), Some(true));
    cache.clear();
    assert_eq!(cache.get(water), None);
}

#[test]
fn visited_ground_survives_queries_in_more_than_4096_other_regions() {
    let mut cache = RouteGroundCache::default();
    for x in 0..20_000 {
        let tile = TileCoord::new(x * 32, -32);
        cache.insert(tile, x % 2 == 0);
        cache.insert_edge(tile, TileCoord::new(tile.x + 1, tile.y), x % 2 == 0);
        assert_eq!(cache.get(tile), Some(x % 2 == 0));
    }
    for x in 0..20_000 {
        assert_eq!(cache.get(TileCoord::new(x * 32, -32)), Some(x % 2 == 0));
        let from = TileCoord::new(x * 32, -32);
        if let Some(clear) = cache.get_edge(from, TileCoord::new(from.x + 1, from.y)) {
            assert_eq!(clear, x % 2 == 0);
        }
    }
    cache.invalidate(TileRect::from_xywh(
        -1_000_000, -1_000_000, 2_000_000, 2_000_000,
    ));
    assert_eq!(cache.get(TileCoord::new(19_999 * 32, -32)), None);
}

#[test]
fn an_edge_depends_on_the_target_and_diagonal_corners_across_chunks() {
    let mut cache = RouteGroundCache::default();
    let from = TileCoord::new(31, 31);
    let to = TileCoord::new(32, 32);
    cache.insert_edge(from, to, true);
    cache.insert_edge(to, from, false);
    assert_eq!(cache.get_edge(from, to), Some(true));
    assert_eq!(cache.get_edge(to, from), Some(false));
    cache.invalidate(TileRect::from_xywh(32, 31, 1, 1));
    assert_eq!(cache.get_edge(from, to), None);
    assert_eq!(cache.get_edge(to, from), None);
}
