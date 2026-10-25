//! Short straight-line orders can need country-scale detours. Search work
//! limits are scheduling boundaries, never evidence that a goal is sealed.
use super::super::far_orders::{center, wide_world};
use super::*;
use crate::{Terrain, UnitKind};
use aoe_map::{ResourceKind, ResourceNode};

fn forest(world: &mut GameWorld, tiles: impl Iterator<Item = TileCoord>) {
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        panic!("map fixture")
    };
    for tile in tiles {
        overlay
            .insert_starting_resource(
                generator,
                ResourceNode::starting(tile, ResourceKind::Wood).unwrap(),
            )
            .unwrap();
    }
    world.prepare_navigation(&|| false).unwrap();
}

fn arrive(world: &mut GameWorld, origin: TileCoord, destination: TileCoord) -> Vec<TileCoord> {
    let id = world
        .spawn_kind(PlayerId(0), UnitKind::Scout, center(origin.x, origin.y))
        .unwrap();
    world
        .issue_move(id, center(destination.x, destination.y))
        .unwrap();
    let mut visited = vec![origin];
    let mut started = false;
    let mut pause = 0;
    let mut previous_position = world.unit(id).unwrap().position;
    for _ in 0..40_000 {
        let before = world.route_metrics.clone();
        world.advance();
        assert!(
            world.route_metrics.issue_work - before.issue_work
                <= u64::from(crate::game_movement::MAX_FAST_ROUTE_WORK_PER_TICK)
        );
        assert!(
            world.route_metrics.fair_work + world.route_metrics.lookahead_work
                - before.fair_work
                - before.lookahead_work
                <= u64::from(MAX_ROUTE_WORK_PER_TICK)
        );
        assert_eq!(
            world.movement_failure(id),
            None,
            "stopped before arrival: {:?}",
            world.unit(id)
        );
        let tile = world.unit(id).unwrap().position.tile_floor();
        let position = world.unit(id).unwrap().position;
        started |= position != previous_position;
        pause = if position == previous_position {
            pause + 1
        } else {
            0
        };
        assert!(
            !started || pause <= 20,
            "the moving order paused mid-detour at {tile:?}"
        );
        previous_position = position;
        if visited.last() != Some(&tile) {
            visited.push(tile);
        }
        assert_eq!(world.route_accounting_error(), None);
        if world.movement_order(id).is_none() {
            break;
        }
    }
    assert_eq!(
        world.unit(id).unwrap().position,
        center(destination.x, destination.y)
    );
    assert_eq!(world.route_scheduler_stats().waiting_units, 0);
    visited
}

#[test]
fn a_near_order_crosses_a_long_forest_wall_at_its_distant_gap() {
    let mut world = wide_world();
    forest(
        &mut world,
        (0..4_096)
            .filter(|y| *y != 64)
            .map(|y| TileCoord::new(512, y)),
    );
    let visited = arrive(
        &mut world,
        TileCoord::new(510, 400),
        TileCoord::new(514, 400),
    );
    assert!(visited.contains(&TileCoord::new(512, 64)));
}

#[test]
fn a_far_order_crosses_a_long_forest_wall_at_its_distant_gap() {
    let mut world = wide_world();
    forest(
        &mut world,
        (0..4_096)
            .filter(|y| *y != 64)
            .map(|y| TileCoord::new(512, y)),
    );
    let visited = arrive(
        &mut world,
        TileCoord::new(462, 400),
        TileCoord::new(562, 400),
    );
    assert!(visited.contains(&TileCoord::new(512, 64)));
}

#[test]
fn a_near_order_leaves_a_u_shaped_forest_pocket_away_from_its_goal() {
    let mut world = wide_world();
    let sides = (100..=700).flat_map(|y| [TileCoord::new(500, y), TileCoord::new(560, y)]);
    let bottom = (501..560).map(|x| TileCoord::new(x, 700));
    forest(&mut world, sides.chain(bottom));
    let visited = arrive(
        &mut world,
        TileCoord::new(530, 650),
        TileCoord::new(570, 650),
    );
    assert!(visited.iter().any(|tile| tile.y < 100));
}

#[test]
fn a_sealed_forest_destination_is_approached_to_the_nearest_reachable_tile() {
    let mut world = wide_world();
    let ring = (510..=514).flat_map(|y| {
        (510..=514).filter_map(move |x| {
            (x == 510 || x == 514 || y == 510 || y == 514).then_some(TileCoord::new(x, y))
        })
    });
    forest(&mut world, ring);
    let id = world.spawn_unit(PlayerId(0), center(508, 512)).unwrap();
    assert_eq!(world.issue_move(id, center(512, 512)), Ok(true));
    for _ in 0..1000 {
        world.advance();
        if world.movement_order(id).is_none() {
            break;
        }
    }
    // All four cardinal approaches tie in squared distance; canonical y/x
    // ordering picks the north tile, even though the unit starts west.
    assert_eq!(world.unit(id).unwrap().position, center(512, 509));
    assert_eq!(
        world.movement_failure(id),
        Some(GameWorldError::Unreachable)
    );
    assert_eq!(world.route_scheduler_stats().waiting_units, 0);
}
