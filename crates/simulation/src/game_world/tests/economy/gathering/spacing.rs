use super::*;

#[test]
fn gatherers_choose_distinct_available_work_tiles_in_a_deposit_area() {
    // Trees now take at most two workers; the six-worker case belongs on a
    // larger deposit. The separate occupancy tests require lumberjack fan-out.
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
            node.kind == ResourceKind::Gold
                && crate::game_world::economy::placement::perimeter(TileRect::new(
                    node.tile,
                    TileCoord::new(node.tile.x + 1, node.tile.y + 1),
                ))
                .into_iter()
                .filter(|&t| w.tile_passable(t))
                .count()
                    >= 4
        })
        .expect("deposit with at least four accessible work spots");
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
    assert_eq!(
        tiles.len(),
        ids.len(),
        "workers: {:?}",
        ids.iter()
            .map(|id| (w.unit(*id), w.movement_order(*id), w.gather_target(*id)))
            .collect::<Vec<_>>()
    );
    for id in &ids {
        let target = generator
            .resource_by_id(w.gather_target(*id).expect("every worker keeps a job"))
            .unwrap();
        let tile = w.unit(*id).unwrap().position.tile_floor();
        assert!(tile.x.abs_diff(target.tile.x) <= 1 && tile.y.abs_diff(target.tile.y) <= 1);
        assert_eq!(target.kind, ResourceKind::Gold);
    }
    assert!(
        ids.iter().all(|&id| w.unit(id).unwrap().carried.is_some()),
        "{:?}",
        ids.iter()
            .map(|id| (
                w.unit(*id),
                w.movement_order(*id),
                w.units[w.lookup[id]].last_movement_error,
                w.units[w.lookup[id]].obstruction
            ))
            .collect::<Vec<_>>()
    );
}
