use super::*;
use crate::game_world::economy::placement::perimeter;

#[test]
fn gathering_exhausted_natural_resources_release_placement_and_cached_navigation() {
    for kind in [
        ResourceKind::Wood,
        ResourceKind::Food,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let (mut world, node, builder) = resource_world(kind);
        // Exercise immutable prepared navigation as well as the live overlay.
        world.prepare_navigation(&|| false).unwrap();
        world
            .initialize_village(PlayerId(0), TileCoord::new(180, 180))
            .unwrap();
        world.advance();
        let build = |world: &mut GameWorld| {
            world.issue_build(
                PlayerId(0),
                &[builder],
                BuildingKind::PalisadeWall,
                node.tile,
                false,
            )
        };
        assert_eq!(
            build(&mut world),
            Err(EconomyError::InvalidFootprint),
            "live {kind:?}"
        );
        let target = WorldPosition::from_tile_center(node.tile).unwrap();
        assert!(world.issue_move(builder, target).is_err());
        world
            .deplete_resource(node.id, node.initial_amount - 1)
            .unwrap();
        assert_eq!(
            build(&mut world),
            Err(EconomyError::InvalidFootprint),
            "partial {kind:?}"
        );
        world.deplete_resource(node.id, 1).unwrap();
        world.issue_move(builder, target).unwrap();
        advance(&mut world, 100);
        assert_eq!(
            world.unit(builder).unwrap().position,
            target,
            "opened {kind:?}"
        );
        let free = perimeter(TileRect::from_xywh(node.tile.x, node.tile.y, 1, 1))
            .into_iter()
            .find(|tile| world.tile_free(*tile))
            .unwrap();
        world
            .issue_move(builder, WorldPosition::from_tile_center(free).unwrap())
            .unwrap();
        advance(&mut world, 100);
        let wall =
            build(&mut world).unwrap_or_else(|error| panic!("exhausted {kind:?}: {error:?}"));
        assert_eq!(world.building(wall).unwrap().footprint.min, node.tile);
    }
}

#[test]
fn every_gatherable_blocks_orders_routes_and_steering_until_empty() {
    for kind in [
        ResourceKind::Wood,
        ResourceKind::Food,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let (mut world, node, id) = resource_world(kind);
        let target = WorldPosition::from_tile_center(node.tile).unwrap();
        assert!(!world.tile_passable(node.tile), "{kind:?}");
        assert!(
            world.issue_move(id, target).is_err(),
            "{kind:?} destination"
        );
        let start = world.unit(id).unwrap().position.tile_floor();
        let goals = [
            TileCoord::new(node.tile.x + 3, node.tile.y),
            TileCoord::new(node.tile.x - 3, node.tile.y),
        ];
        let goal = goals
            .into_iter()
            .find(|tile| world.tile_passable(*tile) && *tile != start)
            .unwrap();
        world
            .issue_move(id, WorldPosition::from_tile_center(goal).unwrap())
            .unwrap();
        for _ in 0..1000 {
            world.advance();
            let unit = world.unit(id).unwrap();
            assert!(
                world.tile_passable(unit.position.tile_floor()),
                "{kind:?}: {:?}",
                unit.position
            );
            if !unit.moving && !unit.planning {
                break;
            }
        }
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        for _ in 0..1000 {
            world.advance();
            let unit = world.unit(id).unwrap();
            assert_ne!(unit.position.tile_floor(), node.tile, "{kind:?}");
            if unit.task == UnitTask::Gathering {
                assert!(unit.position.tile_floor().x.abs_diff(node.tile.x) <= 1);
                assert!(unit.position.tile_floor().y.abs_diff(node.tile.y) <= 1);
                break;
            }
        }
        assert_eq!(world.unit(id).unwrap().task, UnitTask::Gathering);
        world
            .deplete_resource(node.id, node.initial_amount)
            .unwrap();
        assert_eq!(world.resource_remaining(node.id), Some(0));
        assert!(world.tile_passable(node.tile));
        world.issue_move(id, target).unwrap();
        for _ in 0..1000 {
            world.advance();
        }
        assert_eq!(world.unit(id).unwrap().position, target);
    }
}

#[test]
fn a_footprint_with_a_narrow_exit_spawns_villagers_on_connected_nearby_ground() {
    let mut world = world();
    let rect = TileRect::new(TileCoord::new(20, 20), TileCoord::new(24, 24));
    for y in 0..world.config().height_tiles {
        for x in 0..world.config().width_tiles {
            let tile = TileCoord::new(x, y);
            let open = rect.contains(tile) || (y == 21 && (24..30).contains(&x));
            if !open {
                world.economy.blocked_tiles.insert(tile);
            }
        }
    }
    world
        .initialize_village(PlayerId(0), TileCoord::new(22, 22))
        .unwrap();
    assert_eq!(world.unit_count(), 4);
    let center = world.buildings().next().unwrap();
    assert_eq!(center.footprint, rect);
    for unit in world.units() {
        assert!((24..30).contains(&unit.position.tile_floor().x));
        assert_eq!(unit.position.tile_floor().y, 21);
        assert!(world.tile_passable(unit.position.tile_floor()));
    }
}
