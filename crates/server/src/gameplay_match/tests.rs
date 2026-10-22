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

/// Per village, what the standard Arabia-like start holds near its centre.
fn kit_near(world: &GameWorld, scout: aoe_core::EntityId) -> Vec<(String, usize)> {
    let at = world
        .units()
        .find(|unit| unit.id == scout)
        .expect("scout")
        .position
        .tile_floor();
    let near = |tile: TileCoord| (tile.x - at.x).abs().max((tile.y - at.y).abs()) <= 24;
    let mut counts = std::collections::BTreeMap::new();
    for node in world.starting_resources().iter().filter(|n| near(n.tile)) {
        *counts.entry(format!("{:?}", node.kind)).or_insert(0) += 1;
    }
    // Natural nodes of the map count too (the start tops them up, never doubles).
    if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
        for y in at.y - 24..=at.y + 24 {
            for x in at.x - 24..=at.x + 24 {
                let node = generator
                    .object_at_with_cancel(TileCoord::new(x, y), &|| false)
                    .expect("object lookup");
                if let Some(node) = node {
                    *counts.entry(format!("{:?}", node.kind)).or_insert(0) += 1;
                }
            }
        }
    }
    for animal in world.animals().filter(|a| near(a.position.tile_floor())) {
        *counts.entry(format!("{:?}", animal.kind)).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

#[test]
fn every_flat_match_village_gets_the_full_standard_start() {
    for (tiles, seed, villages) in [(500, 3, 4), (500, 5, 4), (300, 1, 2), (900, 2, 9)] {
        let (package, pages) = flat::package(flat::FlatMap {
            tiles_per_side: tiles,
            seed,
        })
        .expect("flat package");
        let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![])
            .expect("flat world");
        let scouts = place_villages(&mut world, villages, &|| false).expect("villages");
        let mut short = Vec::new();
        for (slot, scout) in scouts.into_iter().enumerate() {
            let have: std::collections::BTreeMap<_, _> =
                kit_near(&world, scout).into_iter().collect();
            for (kind, need) in [
                ("Food", 6),
                ("Gold", 5),
                ("Stone", 5),
                ("Wood", 12),
                ("Sheep", 6),
                ("Deer", 2),
                ("Boar", 1),
            ] {
                if have.get(kind).copied().unwrap_or(0) < need {
                    short.push(format!("{tiles}/{seed} slot {slot}: {kind} {have:?}"));
                }
            }
        }
        assert!(short.is_empty(), "{short:#?}");
    }
}
