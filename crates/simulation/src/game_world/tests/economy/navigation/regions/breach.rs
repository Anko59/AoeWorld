use super::*;

#[test]
fn attack_move_breaches_an_enemy_wall_and_finishes_the_original_order() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (mut world, _, _) = village();
                world.set_fog_disabled(true);
                let o = TileCoord::new(100, 100);
                let wall = fort(&mut world, o, true)[0];
                world.economy.buildings.get_mut(&wall).unwrap().hp = 32;
                let army = militia(&mut world, 1, &block(103, 111, 20));
                let target = WorldPosition::from_tile_center(TileCoord::new(105, 104)).unwrap();
                let blocked = WorldPosition::from_tile_center(TileCoord::new(105, 108)).unwrap();
                assert!(
                    world
                        .issue_formation_move(
                            PlayerId(1),
                            &army,
                            Formation::Box,
                            blocked,
                            false,
                            true
                        )
                        .is_ok()
                );
                world
                    .issue_formation_move(PlayerId(1), &army, Formation::Box, target, true, true)
                    .unwrap();
                advance(&mut world, 2_400);
                let remaining = world.building(wall).map(|b| b.hp);
                assert_eq!(remaining, None, "the clicked gate survived");
                assert!(
                    army.iter()
                        .filter(|&&id| {
                            let at = world.unit(id).unwrap().position.tile_floor();
                            (101..110).contains(&at.x) && (101..108).contains(&at.y)
                        })
                        .count()
                        >= 5,
                    "the army never followed its queued order into the fort"
                );
                world.canonical_hash()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn attack_move_still_breaches_a_wall_with_an_enemy_visible_behind_it() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let o = TileCoord::new(100, 100);
    let wall = fort(&mut world, o, true)[0];
    world.economy.buildings.get_mut(&wall).unwrap().hp = 32;
    // A defender in the yard is in sight but unreachable until the gate falls.
    militia(&mut world, 0, &[(105, 104)]);
    let army = militia(&mut world, 1, &block(103, 111, 20));
    let target = WorldPosition::from_tile_center(TileCoord::new(105, 104)).unwrap();
    world
        .issue_formation_move(PlayerId(1), &army, Formation::Box, target, true, true)
        .unwrap();
    advance(&mut world, 2_400);
    assert_eq!(world.building(wall).map(|b| b.hp), None, "the gate stood");
}

#[test]
fn attack_move_breaches_a_building_on_a_map_backed_collision_overlay() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    world.set_fog_disabled(true);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let army = militia(&mut world, 0, &[(100, 101), (100, 103), (101, 102)]);
    let wall = world
        .insert_building(
            PlayerId(1),
            BuildingKind::PalisadeWall,
            aoe_core::TileRect::from_xywh(106, 102, 1, 1),
            true,
        )
        .unwrap();
    world.economy.buildings.get_mut(&wall).unwrap().hp = 4;
    let target = at(106, 102);
    assert!(
        !world
            .terrain()
            .passable(target.tile_floor(), world.config())
    );
    assert!(
        world
            .issue_formation_move(PlayerId(0), &army, Formation::Line, target, false, false)
            .is_err()
    );
    world
        .issue_formation_move(PlayerId(0), &army, Formation::Line, target, false, true)
        .unwrap();
    advance(&mut world, 800);
    assert!(world.building(wall).is_none());
    assert!(
        army.iter()
            .all(|&id| world.movement_waypoints(id).is_empty())
    );
}
