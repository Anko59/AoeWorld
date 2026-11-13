use super::Economy;
use aoe_core::TileCoord;
use std::collections::BTreeSet;

#[test]
fn blocked_membership_deduplicates_and_releases_empty_chunks() {
    let mut tiles = Economy::default().blocked_tiles;
    let points = [
        TileCoord::new(-1, -1),
        TileCoord::new(0, 0),
        TileCoord::new(31, 31),
        TileCoord::new(32, 32),
    ];
    tiles.extend(points);
    tiles.extend(points);
    assert_eq!(tiles.len(), 4);
    assert_eq!(tiles.resident_chunks(), 3);
    assert!(!tiles.insert(points[0]));
    assert!(tiles.remove(&points[1]));
    assert_eq!(tiles.resident_chunks(), 3);
    assert!(tiles.remove(&points[2]));
    assert_eq!(tiles.resident_chunks(), 2);
    assert!(!tiles.remove(&points[2]));
    for point in [points[0], points[3]] {
        assert!(tiles.remove(&point));
    }
    assert!(tiles.is_empty());
    assert_eq!(tiles.resident_chunks(), 0);
}

#[test]
fn blocked_membership_matches_an_independent_set_through_edits() {
    let mut actual = Economy::default().blocked_tiles;
    let mut expected = BTreeSet::new();
    let mut seed = 17_u64;
    for index in 0..4000 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let tile = TileCoord::new(
            ((seed >> 16) & 127) as i32 - 64,
            ((seed >> 32) & 127) as i32 - 64,
        );
        if seed & 3 == 0 {
            assert_eq!(actual.remove(&tile), expected.remove(&tile));
        } else {
            assert_eq!(actual.insert(tile), expected.insert(tile));
        }
        assert_eq!(actual.contains(&tile), expected.contains(&tile));
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual.is_empty(), expected.is_empty());
        if index % 127 == 0 {
            for y in (-70..70).step_by(7) {
                for x in (-70..70).step_by(5) {
                    let at = TileCoord::new(x, y);
                    assert_eq!(actual.contains(&at), expected.contains(&at), "{at:?}");
                }
            }
            let regions: BTreeSet<_> = expected
                .iter()
                .map(|p| (p.x.div_euclid(32), p.y.div_euclid(32)))
                .collect();
            assert_eq!(actual.resident_chunks(), regions.len());
        }
    }
    for tile in &expected {
        assert!(actual.remove(tile));
    }
    assert!(actual.is_empty());
    assert_eq!(actual.resident_chunks(), 0);
}

#[test]
fn transient_blockers_do_not_retain_every_region_they_visited() {
    let mut tiles = Economy::default().blocked_tiles;
    for index in 0..5000 {
        let tile = TileCoord::new(index * 32, -32);
        assert!(tiles.insert(tile));
        assert!(tiles.contains(&tile));
        assert!(tiles.remove(&tile));
        assert!(!tiles.contains(&tile));
        assert_eq!(tiles.len(), 0);
        assert_eq!(tiles.resident_chunks(), 0);
    }
}

#[test]
fn blocked_membership_handles_full_chunks_and_integer_edges() {
    let mut tiles = Economy::default().blocked_tiles;
    for y in 0..32 {
        for x in 0..32 {
            assert!(tiles.insert(TileCoord::new(x, y)));
        }
    }
    assert_eq!(tiles.len(), 1024);
    assert_eq!(tiles.resident_chunks(), 1);
    for y in (0..32).rev() {
        for x in (0..32).rev() {
            assert!(tiles.contains(&TileCoord::new(x, y)));
            assert!(tiles.remove(&TileCoord::new(x, y)));
        }
    }
    for tile in [
        TileCoord::new(i32::MIN, i32::MIN),
        TileCoord::new(i32::MAX, i32::MAX),
        TileCoord::new(i32::MIN, i32::MAX),
        TileCoord::new(i32::MAX, i32::MIN),
    ] {
        assert!(tiles.insert(tile));
        assert!(tiles.contains(&tile));
        assert!(tiles.remove(&tile));
    }
    assert!(tiles.is_empty());
    assert_eq!(tiles.resident_chunks(), 0);
}
