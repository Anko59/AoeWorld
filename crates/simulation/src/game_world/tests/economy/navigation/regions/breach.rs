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

#[test]
fn attack_move_breaks_the_direct_wall_even_when_the_base_has_an_open_entrance() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let o = TileCoord::new(100, 100);
    let gates = fort(&mut world, o, true);
    world.remove_building(gates[1]);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let wall = gates[0];
    world.economy.buildings.get_mut(&wall).unwrap().hp = 4;
    let army = militia(&mut world, 1, &[(105, 113), (105, 114), (105, 115)]);
    let target = at(105, 104);
    let searches = |world: &GameWorld| {
        world
            .order_demand()
            .iter()
            .map(|d| d.route_searches)
            .sum::<u64>()
    };
    assert!(
        world
            .land_reachable(
                PlayerId(1),
                aoe_map::LandBucket::ALL[0],
                TileCoord::new(105, 114),
                target.tile_floor(),
            )
            .unwrap(),
        "the open entrance makes a detour possible"
    );
    let before = searches(&world);
    world
        .issue_formation_move(PlayerId(1), &army, Formation::Line, target, false, true)
        .unwrap();
    assert_eq!(
        searches(&world),
        before,
        "the clear approach needs no search"
    );
    assert!(world.formation_matches(&army, target, true));
    for _ in 0..400 {
        if world.building(wall).is_none() {
            break;
        }
        for &id in &army {
            let tile = world.unit(id).unwrap().position.tile_floor();
            assert!(tile.y > 108, "unit went around the wall through the gate");
        }
        world.advance();
    }
    assert!(world.building(wall).is_none(), "the direct wall survived");
    advance(&mut world, 400);
    assert!(
        army.iter()
            .all(|&id| world.unit(id).unwrap().position.tile_floor().y < 108),
        "the army did not continue through the breach: {:?}",
        army.iter()
            .map(|&id| world.unit(id).unwrap())
            .map(|u| (u.position, u.attack_target))
            .collect::<Vec<_>>()
    );
}

#[test]
fn attack_move_routes_around_own_and_allied_walls() {
    for owner in [PlayerId(0), PlayerId(1)] {
        let (mut world, _, _) = village();
        world.set_fog_disabled(true);
        world
            .initialize_village(PlayerId(1), TileCoord::new(145, 145))
            .unwrap();
        world
            .set_diplomatic_stance(PlayerId(0), PlayerId(1), aoe_core::DiplomaticStance::Ally)
            .unwrap();
        let army = militia(&mut world, 0, &[(100, 102)]);
        let wall = world
            .insert_building(
                owner,
                BuildingKind::PalisadeWall,
                aoe_core::TileRect::from_xywh(106, 102, 1, 1),
                true,
            )
            .unwrap();
        let hp = world.building(wall).unwrap().hp;
        let target = WorldPosition::from_tile_center(TileCoord::new(114, 102)).unwrap();
        world
            .issue_formation_move(PlayerId(0), &army, Formation::Line, target, false, true)
            .unwrap();
        let order = world.movement_order(army[0]).unwrap();
        assert_eq!(order.destination, target, "the {owner:?} wall is no breach");
        for _ in 0..600 {
            world.advance();
            let unit = world.unit(army[0]).unwrap();
            assert_ne!(unit.position.tile_floor(), TileCoord::new(106, 102));
            assert_ne!(unit.attack_target, Some(wall));
        }
        assert_eq!(world.building(wall).unwrap().hp, hp);
    }
}

#[test]
fn attack_move_clears_two_destroyable_buildings_in_route_order() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 102)).unwrap();
    let army = militia(&mut world, 0, &[(84, 102), (85, 102), (86, 102)]);
    let [first, second] = [106, 111].map(|x| {
        let id = world
            .insert_building(
                PlayerId(1),
                BuildingKind::PalisadeWall,
                aoe_core::TileRect::from_xywh(x, 102, 1, 1),
                true,
            )
            .unwrap();
        world.economy.buildings.get_mut(&id).unwrap().hp = 4;
        id
    });
    world
        .issue_formation_move(PlayerId(0), &army, Formation::Line, at(117), false, true)
        .unwrap();
    for _ in 0..800 {
        if world.building(second).is_none() {
            break;
        }
        if world.building(first).is_some() {
            assert!(
                army.iter()
                    .all(|&id| world.unit(id).unwrap().position.x < at(106).x),
                "the march passed its first blocker"
            );
        }
        world.advance();
    }
    assert!(world.building(first).is_none());
    assert!(world.building(second).is_none());
    advance(&mut world, 400);
    assert!(
        army.iter()
            .all(|&id| world.unit(id).unwrap().position.x > at(111).x)
    );
}
