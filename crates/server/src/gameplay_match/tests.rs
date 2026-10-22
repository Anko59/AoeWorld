use super::*;

#[test]
fn ideal_starts_spread_over_the_whole_map() {
    let two = ideal_starts(400, 2);
    let gap = (two[0].x - two[1].x).abs().max((two[0].y - two[1].y).abs());
    assert!(gap > 150, "two players start on opposite sides: {gap}");
    let many = ideal_starts(800, 100);
    assert_eq!(many.len(), 100);
    let distinct: std::collections::BTreeSet<_> = many.iter().map(|t| (t.x, t.y)).collect();
    assert_eq!(distinct.len(), 100);
    assert!(
        many.iter()
            .all(|t| t.x > 0 && t.x < 800 && t.y > 0 && t.y < 800)
    );
    let (min_x, max_x) = (
        many.iter().map(|t| t.x).min().unwrap(),
        many.iter().map(|t| t.x).max().unwrap(),
    );
    assert!(max_x - min_x > 500, "grid spans the map");
}

#[test]
fn ring_search_covers_every_second_tile_once() {
    let ring = ring_tiles(TileCoord::new(50, 50), 4);
    let distinct: std::collections::BTreeSet<_> = ring.iter().map(|t| (t.x, t.y)).collect();
    assert_eq!(ring.len(), distinct.len());
    assert!(
        ring.iter()
            .all(|t| (t.x - 50).abs().max((t.y - 50).abs()) == 4)
    );
}
