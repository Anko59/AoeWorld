//! Differential reachability: the reference walks tiles and knows no portals.
use super::super::*;
use crate::RouteTerrain;
use std::collections::{BTreeSet, VecDeque};

fn reachable(
    generator: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
) -> BTreeSet<TileCoord> {
    let open = |tile| generator.route_walkable(overlay, tile, &|| false).unwrap();
    let edge = |from, to| generator.route_edge_clear(from, to, &|| false).unwrap();
    let mut seen = BTreeSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some(from) = queue.pop_front() {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let to = TileCoord::new(from.x + dx, from.y + dy);
                if (dx == 0 && dy == 0) || seen.contains(&to) || !open(to) || !edge(from, to) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    let (a, b) = (TileCoord::new(to.x, from.y), TileCoord::new(from.x, to.y));
                    if !open(a) || !open(b) || !edge(from, a) || !edge(from, b) {
                        continue;
                    }
                }
                seen.insert(to);
                queue.push_back(to);
            }
        }
    }
    seen
}

#[test]
fn prepared_nearest_points_match_a_full_tile_flood_on_seeded_landscapes() {
    for seed in 0..8_u64 {
        let generator = MapChunkGenerator::new([seed as u8; 32], seed, 64);
        let overlay = ResourceOverlay::default();
        generator.prepare_navigation(&overlay, &|| false).unwrap();
        let origins = (0..64)
            .flat_map(|y| (0..64).map(move |x| TileCoord::new(x, y)))
            .filter(|tile| {
                generator
                    .route_walkable(&overlay, *tile, &|| false)
                    .unwrap()
            })
            .step_by(257)
            .take(4)
            .collect::<Vec<_>>();
        assert!(!origins.is_empty(), "seed {seed} has no reference origin");
        for origin in origins {
            let tiles = reachable(&generator, &overlay, origin);
            for target in [
                TileCoord::new(1, 1),
                TileCoord::new(62, 1),
                TileCoord::new(31, 31),
                TileCoord::new(1, 62),
                TileCoord::new(62, 62),
            ] {
                let expected = tiles.iter().copied().min_by_key(|tile| {
                    let (dx, dy) = (i64::from(tile.x - target.x), i64::from(tile.y - target.y));
                    (dx * dx + dy * dy, tile.y, tile.x)
                });
                assert_eq!(
                    generator
                        .navigation_destination(&overlay, origin, target, &|| false)
                        .unwrap(),
                    expected,
                    "seed {seed}, {origin:?}->{target:?}"
                );
            }
        }
    }
}
