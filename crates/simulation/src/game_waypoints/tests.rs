use super::*;
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig};

fn centered(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).expect("tile center")
}

#[test]
fn waypoint_orders_require_tile_centers_and_walkable_edges() {
    let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
    let (mut world, _) = GameWorld::with_cavalry(config).expect("world");
    let unit = world
        .spawn_unit(PlayerId(0), centered(TileCoord::new(10, 10)))
        .expect("unit");
    let off_center = WorldPosition::new(11_000, 10_512);
    assert!(matches!(
        world.issue_move_waypoints(unit, &[off_center, centered(TileCoord::new(12, 10))]),
        Err(GameWorldError::InvalidPosition)
    ));
    assert!(matches!(
        world.issue_move_waypoints(
            unit,
            &[
                centered(TileCoord::new(11, 10)),
                centered(TileCoord::new(11, 10))
            ],
        ),
        Err(GameWorldError::InvalidPosition)
    ));
    assert!(world.movement_order(unit).is_none());
}

#[test]
fn one_order_retains_the_final_destination_and_remaining_route() {
    let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
    let (mut world, _) = GameWorld::with_cavalry(config).expect("world");
    let unit = world
        .spawn_unit(PlayerId(0), centered(TileCoord::new(10, 10)))
        .expect("unit");
    let waypoints = [
        centered(TileCoord::new(12, 10)),
        centered(TileCoord::new(10, 10)),
        centered(TileCoord::new(12, 10)),
    ];

    assert!(
        world
            .issue_move_waypoints(unit, &waypoints)
            .expect("waypoint route")
    );
    let index = world.lookup[&unit];
    let order = world.movement_order(unit).expect("order");
    assert_eq!(order.waypoint, waypoints[0]);
    assert_eq!(order.destination, waypoints[2]);
    assert_eq!(
        world.units[index].route,
        VecDeque::from([waypoints[1], waypoints[2]])
    );
    assert_eq!(world.active_mover_count(), 1);
}
