use super::*;

#[test]
fn gatherers_choose_distinct_available_work_tiles_around_one_tree() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let node = (0..8)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .flat_map(|(x, y)| generator.chunk(x, y).unwrap().resources)
        .find(|node| {
            node.kind == ResourceKind::Wood
                && crate::game_world::economy::placement::perimeter(TileRect::new(
                    node.tile,
                    TileCoord::new(node.tile.x + 1, node.tile.y + 1),
                ))
                .into_iter()
                .filter(|&t| w.tile_passable(t))
                .count()
                    >= 4
        })
        .expect("tree with at least four accessible work spots");
    let initial = crate::game_world::economy::placement::perimeter(TileRect::new(
        node.tile,
        TileCoord::new(node.tile.x + 1, node.tile.y + 1),
    ))
    .into_iter()
    .find(|&t| w.tile_passable(t))
    .unwrap();
    let first = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(initial).unwrap(),
        )
        .unwrap();
    let available = crate::game_world::economy::placement::perimeter(TileRect::new(
        node.tile,
        TileCoord::new(node.tile.x + 1, node.tile.y + 1),
    ))
    .into_iter()
    .filter(|&t| w.tile_passable(t))
    .collect::<Vec<_>>();
    assert!(available.len() >= 2);
    let mut ids = vec![first];
    for _ in 1..available.len().min(6) {
        ids.push(
            w.spawn_villager(PlayerId(0), w.unit(first).unwrap().position)
                .unwrap(),
        );
    }
    for &id in &ids {
        w.issue_gather(PlayerId(0), id, node.id).unwrap();
    }
    let goals = ids
        .iter()
        .map(|&id| {
            w.movement_order(id)
                .map_or(w.unit(id).unwrap().position.tile_floor(), |o| o.target_tile)
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        goals.len(),
        ids.len(),
        "each available work spot is reserved once"
    );
    advance(&mut w, 250);
    let tiles = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position.tile_floor())
        .collect::<BTreeSet<_>>();
    assert_eq!(tiles.len(), ids.len());
    assert!(
        tiles
            .iter()
            .all(|t| t.x.abs_diff(node.tile.x) <= 1 && t.y.abs_diff(node.tile.y) <= 1)
    );
    assert!(ids.iter().all(|&id| w.unit(id).unwrap().carried.is_some()));
}
