use super::*;

pub(super) fn cleared_work_area(kind: ResourceKind) -> (GameWorld, ResourceNode) {
    let (mut world, original) = resource_mainland();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    overlay
        .deplete(generator, original.id, original.initial_amount)
        .unwrap();
    let tile = (40..210)
        .flat_map(|y| (40..210).map(move |x| TileCoord::new(x, y)))
        .find(|at| {
            (-10..=4).all(|dx| {
                (-3..=3).all(|dy| {
                    let tile = TileCoord::new(at.x + dx, at.y + dy);
                    generator.object_at(tile).is_none()
                        && generator.tile_at(tile).is_some_and(|t| t.passable)
                })
            })
        })
        .expect("clear deterministic work area");
    let node = ResourceNode::starting(tile, kind).unwrap();
    overlay.insert_starting_resource(generator, node).unwrap();
    (world, node)
}

#[test]
fn exhausted_workers_choose_from_the_used_dropoff_for_wood_gold_and_stone() {
    for kind in [ResourceKind::Wood, ResourceKind::Gold, ResourceKind::Stone] {
        let (mut world, old) = cleared_work_area(kind);
        let camp_tile = TileCoord::new(old.tile.x - 9, old.tile.y - 2);
        let camp = world.place_town_center(PlayerId(0), camp_tile).unwrap();
        let near_camp = TileCoord::new(old.tile.x - 3, old.tile.y);
        let near_old = TileCoord::new(old.tile.x + 1, old.tile.y);
        let Terrain::Map { generator, overlay } = &mut world.terrain else {
            unreachable!()
        };
        let mut ids = Vec::new();
        for tile in [near_camp, near_old] {
            let node = ResourceNode::starting(tile, kind).unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            ids.push(node.id);
        }
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
        let worker = world
            .spawn_villager(
                PlayerId(0),
                position(TileCoord::new(old.tile.x - 5, old.tile.y)).unwrap(),
            )
            .unwrap();
        let mut task = fresh_task();
        task.node = old.id;
        task.tile = old.tile;
        task.kind = kind;
        task.dropoff = Some(camp);
        assert!(world.resume_resource(worker, &mut task));
        assert_eq!(
            task.node, ids[0],
            "{kind:?}: rank from the drop-off, not the exhausted node"
        );
    }
}

#[test]
fn replacement_skips_a_sealed_node_and_is_deterministic() {
    let run = || {
        let (mut world, old) = cleared_work_area(ResourceKind::Wood);
        let near = TileCoord::new(old.tile.x - 4, old.tile.y);
        let far = TileCoord::new(old.tile.x + 2, old.tile.y);
        let Terrain::Map { generator, overlay } = &mut world.terrain else {
            unreachable!()
        };
        let mut ids = Vec::new();
        for tile in [near, far] {
            let node = ResourceNode::starting(tile, ResourceKind::Wood).unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            ids.push(node.id);
        }
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
        for dy in -1..=1 {
            for dx in -1..=1 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(near.x + dx, near.y + dy));
            }
        }
        let worker = world
            .spawn_villager(
                PlayerId(0),
                position(TileCoord::new(old.tile.x - 7, old.tile.y)).unwrap(),
            )
            .unwrap();
        let mut task = fresh_task();
        task.node = old.id;
        task.tile = old.tile;
        assert!(world.resume_resource(worker, &mut task));
        assert_eq!(task.node, ids[1]);
        (task.node, world.canonical_hash())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_long_dropoff_trip_still_replaces_a_tree_in_the_reachable_work_area() {
    let (mut world, old) = cleared_work_area(ResourceKind::Wood);
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    let camp_tile = (old.tile.y..old.tile.y + 20)
        .flat_map(|y| (old.tile.x + 40..old.tile.x + 60).map(move |x| TileCoord::new(x, y)))
        .find(|at| {
            (0..4).all(|dy| {
                (0..4).all(|dx| {
                    generator
                        .object_at(TileCoord::new(at.x + dx, at.y + dy))
                        .is_none()
                })
            })
        })
        .expect("clear distant Town Center");
    let next = ResourceNode::starting(
        TileCoord::new(old.tile.x + 1, old.tile.y),
        ResourceKind::Wood,
    )
    .unwrap();
    overlay.insert_starting_resource(generator, next).unwrap();
    overlay
        .deplete(generator, old.id, old.initial_amount)
        .unwrap();
    let camp = world.place_town_center(PlayerId(0), camp_tile).unwrap();
    let worker = world
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(camp_tile.x - 1, camp_tile.y)).unwrap(),
        )
        .unwrap();
    let mut task = fresh_task();
    task.node = old.id;
    task.tile = old.tile;
    task.dropoff = Some(camp);
    task.work_tile = Some(TileCoord::new(old.tile.x - 1, old.tile.y));
    assert!(
        world.resume_resource(worker, &mut task),
        "the search bound is local to the forest, not the long return trip"
    );
    assert_eq!(task.node, next.id);
}
