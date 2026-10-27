use super::*;

#[test]
fn arbitrary_angle_bridges_cross_one_to_three_tile_streams_and_collapse_disconnects_them() {
    for width in 1..=3 {
        for rise in [-3, -1, 1, 2, 3] {
            let mut world = river_width(true, 30..30 + width);
            clear_nodes(&mut world, TileRect::from_xywh(24, 12, 18, 18));
            world
                .place_town_center(PlayerId(0), TileCoord::new(10, 10))
                .unwrap();
            let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
            player.age = crate::Age::Feudal;
            player.stockpile = Stockpile {
                wood: 2_000,
                stone: 2_000,
                ..Default::default()
            };
            fixture::survey(&mut world, PlayerId(0), TileCoord::new(30, 20), 12);
            let from = TileCoord::new(29, 20);
            let to = TileCoord::new(30 + width as i32, 20 + rise);
            let start = TileCoord::new(28, 20);
            let goal = TileCoord::new(to.x + 2, to.y);
            let worker = world
                .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(start).unwrap())
                .unwrap();
            world.prepare_navigation(&|| false).unwrap();
            assert!(!matches!(
                world.terrain.route_outcome(start, goal),
                Some(MovementOutcome::Path(_))
            ));
            let bridge = world
                .issue_build_bridge(PlayerId(0), &[worker], from, to, false)
                .unwrap_or_else(|e| panic!("width={width}, rise={rise}: {e:?}"));
            let beside = TileCoord::new(from.x, to.y);
            assert!(
                !world.economy.blocked_tiles.contains(&beside),
                "reserve only raster cells"
            );
            assert!(world.tile_passable(beside));
            advance(&mut world, 2_000);
            assert!(world.building(bridge).unwrap().is_complete());
            assert!(
                matches!(
                    world.terrain.route_outcome(start, goal),
                    Some(MovementOutcome::Path(_))
                ),
                "width={width}, rise={rise}"
            );
            world
                .issue_move(worker, WorldPosition::from_tile_center(goal).unwrap())
                .unwrap();
            advance(&mut world, 500);
            assert_eq!(world.unit(worker).unwrap().position.tile_floor(), goal);
            world.remove_building(bridge);
            assert!(!matches!(
                world.terrain.route_outcome(start, goal),
                Some(MovementOutcome::Path(_))
            ));
        }
    }
}

#[test]
fn angled_bridge_never_replaces_or_removes_an_unrelated_buildings_spatial_index() {
    let (mut world, worker) = setup();
    clear_nodes(&mut world, TileRect::from_xywh(26, 16, 12, 12));
    let beside = TileCoord::new(29, 23);
    let wall = world
        .insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            TileRect::from_xywh(beside.x, beside.y, 1, 1),
            true,
        )
        .unwrap();
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 23),
            false,
        )
        .unwrap();
    assert_eq!(world.building_at(beside), Some(wall));
    world.rebuild_building_sight_index();
    assert_eq!(world.building_at(beside), Some(wall));
    world.remove_building(bridge);
    assert_eq!(world.building_at(beside), Some(wall));
}
