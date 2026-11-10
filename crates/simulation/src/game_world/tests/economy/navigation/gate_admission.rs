use super::*;

/// A map order may settle at its certified nearest endpoint instead of refusing
/// immediately. Inspect every emitted segment, including such partial routes.
fn plan_reaches(world: &GameWorld, player: PlayerId, from: TileCoord, to: TileCoord) -> bool {
    use aoe_map::{RoutePlanner, RoutePlannerPoll};
    let mut planner = RoutePlanner::new(from, to, crate::MAX_ROUTE_WORK_PER_ORDER);
    for _ in 0..128 {
        match world.probe_route_poll(&mut planner, 8192, crate::UnitKind::Villager, player) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                assert!(
                    path.tiles
                        .iter()
                        .all(|&tile| world.route_tile_admitted(player, tile)),
                    "a planned path entered a forbidden gate"
                );
                if path.tiles.last() == Some(&to) {
                    return true;
                }
            }
            RoutePlannerPoll::Unreachable
            | RoutePlannerPoll::InvalidDestination
            | RoutePlannerPoll::Complete => return false,
            other => panic!("unexpected route result: {other:?}"),
        }
    }
    panic!("gate route did not settle within its bounded work");
}

#[test]
fn closed_gate_routes_and_cached_verdicts_respect_the_movers_player() {
    for map in [false, true] {
        for owner_first in [false, true] {
            let (mut world, o) = if map {
                map_fort_site()
            } else {
                (village().0, TileCoord::new(100, 100))
            };
            let gate = fort(&mut world, o, false)[0];
            advance(&mut world, 2);
            let from = WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 20)).unwrap();
            let to = WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 4)).unwrap();
            let own = world.spawn_villager(PlayerId(0), from).unwrap();
            let enemy = world.spawn_villager(PlayerId(1), from).unwrap();
            let ids = if owner_first {
                [own, enemy]
            } else {
                [enemy, own]
            };
            for id in ids {
                let result = world.issue_task_move(id, to);
                if id == own {
                    assert!(
                        result.is_ok(),
                        "owner: map={map}, first={owner_first}: {result:?}"
                    );
                } else {
                    // Map orders can approach a certified nearest reachable tile.
                    // Every emitted waypoint must still obey gate admission.
                    if !map {
                        assert_eq!(result, Err(crate::GameWorldError::Unreachable));
                    }
                    let stored = &world.units[world.lookup[&id]];
                    assert!(
                        stored
                            .route
                            .iter()
                            .all(|at| world.route_tile_admitted(PlayerId(1), at.tile_floor()))
                    );
                    assert!(!plan_reaches(
                        &world,
                        PlayerId(1),
                        from.tile_floor(),
                        TileCoord::new(o.x + 4, o.y + 8)
                    ));
                }
            }
            assert!(!world.building(gate).unwrap().gate_open);
            world
                .spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 10)).unwrap(),
                )
                .unwrap();
            advance(&mut world, 1);
            assert!(world.building(gate).unwrap().gate_open);
            assert!(
                world.issue_task_move(enemy, to).is_ok(),
                "an open gate invalidates the enemy's refusal"
            );
        }
    }
}

#[test]
fn locked_gates_block_plans_even_when_held_open_and_survive_a_save() {
    for map in [false, true] {
        for (wall, kind) in [
            (BuildingKind::StoneWall, BuildingKind::StoneGate),
            (BuildingKind::PalisadeWall, BuildingKind::PalisadeGate),
        ] {
            let (mut world, o) = if map {
                map_fort_site()
            } else {
                (village().0, TileCoord::new(100, 100))
            };
            let gate = fort_kind(&mut world, o, false, wall, kind)[0];
            advance(&mut world, 2);
            let outside = TileCoord::new(o.x + 5, o.y + 20);
            let inside = TileCoord::new(o.x + 5, o.y + 4);
            let at = WorldPosition::from_tile_center(outside).unwrap();
            let own = world.spawn_villager(PlayerId(0), at).unwrap();
            let enemy = world.spawn_villager(PlayerId(1), at).unwrap();
            for bucket in aoe_map::LandBucket::ALL {
                assert!(
                    world
                        .land_reachable(PlayerId(0), bucket, outside, inside)
                        .unwrap()
                );
                assert!(
                    !world
                        .land_reachable(PlayerId(1), bucket, outside, inside)
                        .unwrap()
                );
            }
            let keeper = world
                .spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(o.x + 3, o.y + 7)).unwrap(),
                )
                .unwrap();
            advance(&mut world, 2);
            assert!(world.building(gate).unwrap().gate_open);
            let on_gate = TileCoord::new(o.x + 4, o.y + 8);
            assert_eq!(walk(&mut world, keeper, on_gate, 300), on_gate);
            assert!(
                world.set_gate_locked(PlayerId(1), gate, true).is_err(),
                "only its owner can lock it"
            );
            world.set_gate_locked(PlayerId(0), gate, true).unwrap();
            advance(&mut world, 2);
            assert!(
                world.building(gate).unwrap().gate_open,
                "cannot close through a living unit"
            );
            for player in [PlayerId(0), PlayerId(1)] {
                for bucket in aoe_map::LandBucket::ALL {
                    assert!(
                        !world
                            .land_reachable(player, bucket, outside, inside)
                            .unwrap(),
                        "locked {kind:?}"
                    );
                }
            }
            for unit in [own, enemy] {
                let player = world.unit(unit).unwrap().player;
                assert!(!plan_reaches(&world, player, outside, on_gate));
                let result =
                    world.issue_task_move(unit, WorldPosition::from_tile_center(inside).unwrap());
                if !map {
                    assert_eq!(result, Err(crate::GameWorldError::Unreachable));
                }
            }
            advance(&mut world, 900);
            let yard = aoe_core::TileRect::from_xywh(o.x, o.y, 11, 9);
            assert!(
                [own, enemy]
                    .iter()
                    .all(|&id| !yard.contains(world.unit(id).unwrap().position.tile_floor()))
            );
            assert!(world.building(gate).unwrap().gate_open);
            let snapshot = world.snapshot([0; 32]).unwrap();
            assert!(
                snapshot
                    .buildings
                    .iter()
                    .find(|b| b.id == gate)
                    .unwrap()
                    .gate_locked
            );
            let mut restored = if map {
                map_fort_site().0
            } else {
                GameWorld::new(world.config()).unwrap()
            };
            restored
                .restore_snapshot(&snapshot, [0; 32], &|| false)
                .unwrap();
            assert!(restored.building(gate).unwrap().gate_locked);
            restored.set_gate_locked(PlayerId(0), gate, false).unwrap();
            assert!(
                restored
                    .issue_task_move(own, WorldPosition::from_tile_center(inside).unwrap())
                    .is_ok()
            );
            assert!(
                restored
                    .issue_task_move(enemy, WorldPosition::from_tile_center(inside).unwrap())
                    .is_ok(),
                "unlocked, currently open enemy gate"
            );
        }
    }
}
