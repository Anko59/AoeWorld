use super::*;

#[test]
fn villagers_close_to_melee_wolves_instead_of_shooting() {
    let mut w = world();
    let villager = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let wolf = w.spawn_animal(AnimalKind::Wolf, at(104, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[villager], wolf).unwrap();
    w.advance();
    assert_eq!(w.animal(wolf).unwrap().hp, AnimalKind::Wolf.rules().hp);
    assert_ne!(w.unit(villager).unwrap().task, UnitTask::Hunting);
    run(&mut w, 120);
    assert!(w.animal(wolf).unwrap().hp < AnimalKind::Wolf.rules().hp);
}

#[test]
fn wolf_walks_around_a_house_and_only_attacks_in_reach() {
    fn replay(threads: usize) -> [u8; 32] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let mut w = world();
                let villager = w.spawn_villager(PlayerId(0), at(105, 100)).unwrap();
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::House,
                    aoe_core::TileRect::from_xywh(102, 99, 2, 2),
                    true,
                )
                .unwrap();
                let wolf = w.spawn_animal(AnimalKind::Wolf, at(100, 100)).unwrap();
                w.advance();
                assert_ne!(w.animal(wolf).unwrap().task, AnimalTask::Attacking);
                run(&mut w, 240);
                assert!(w.unit(villager).unwrap().hp < w.unit(villager).unwrap().max_hp);
                w.canonical_hash()
            })
    }
    assert_eq!(replay(1), replay(4));
}

#[test]
fn animals_cannot_spawn_in_an_enclosed_tree_clearing() {
    let mut w = world();
    for (x, y) in [
        (99, 99),
        (100, 99),
        (101, 99),
        (99, 100),
        (101, 100),
        (99, 101),
        (100, 101),
        (101, 101),
    ] {
        w.insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            aoe_core::TileRect::from_xywh(x, y, 1, 1),
            true,
        )
        .unwrap();
    }
    assert!(w.spawn_animal(AnimalKind::Wolf, at(100, 100)).is_err());
}

#[test]
fn wolf_kit_skips_a_large_disconnected_pocket_even_with_local_walking_room() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    for kind in [
        AnimalKind::Wolf,
        AnimalKind::Sheep,
        AnimalKind::Boar,
        AnimalKind::Deer,
    ] {
        w.remove_animals(kind);
    }
    let tc = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .footprint;
    let center = TileCoord::new(tc.min.x + 2, tc.min.y + 2);
    let pocket = TileCoord::new(center.x + 16, center.y);
    for dy in -6_i32..=6 {
        for dx in -6_i32..=6 {
            if dx.abs().max(dy.abs()) == 6 {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::PalisadeWall,
                    aoe_core::TileRect::from_xywh(pocket.x + dx, pocket.y + dy, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    let root = w
        .units()
        .find(|u| u.kind == UnitKind::Scout)
        .unwrap()
        .position
        .tile_floor();
    w.initialize_wildlife(center, root);
    assert!(
        !w.animals()
            .any(|a| a.position.tile_floor().x.abs_diff(pocket.x) < 6
                && a.position.tile_floor().y.abs_diff(pocket.y) < 6),
        "no start animal may occupy a pocket disconnected from the village"
    );
}

#[test]
fn a_passable_hole_surrounded_by_real_trees_is_not_a_wolf_spawn() {
    let (package, pages, water) = crate::game_world::terrain_fixtures::flat_fixture(false);
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let offsets = [
        (0, -1),
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
    ];
    let center;
    if let crate::Terrain::Map { generator, overlay } = &mut w.terrain {
        center = (16..220)
            .flat_map(|y| (16..220).map(move |x| TileCoord::new(x, y)))
            .find(|at| {
                (-3..=3).all(|dy| {
                    (-3..=3).all(|dx| {
                        generator
                            .tile_and_node_with_cancel(
                                TileCoord::new(at.x + dx, at.y + dy),
                                &|| false,
                            )
                            .unwrap()
                            .is_some_and(|(tile, node)| tile.passable && node.is_none())
                    })
                })
            })
            .expect("a bare patch for the tree enclosure");
        for (dx, dy) in offsets {
            let node = aoe_map::ResourceNode::starting(
                TileCoord::new(center.x + dx, center.y + dy),
                aoe_map::ResourceKind::Wood,
            )
            .unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
        }
    } else {
        panic!("prepared map")
    }
    w.resource_collision_changed();
    assert!(w.tile_passable(center), "the hole itself is legal ground");
    assert!(
        w.spawn_animal(
            AnimalKind::Wolf,
            WorldPosition::from_tile_center(center).unwrap()
        )
        .is_err()
    );
}
