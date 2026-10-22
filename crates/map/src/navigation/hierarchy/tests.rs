use super::{HierarchicalOutcome, hierarchical_route};
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use aoe_core::TileCoord;

fn lower_bound(a: TileCoord, b: TileCoord) -> u64 {
    u64::from((a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs())) * 1_024
}

#[test]
fn tiles_in_one_chunk_need_no_waypoints() {
    let generator = flat_terrain_with_water_wall(64, -1, -1);
    let outcome = hierarchical_route(&generator, TileCoord::new(2, 2), TileCoord::new(10, 2), 64);
    assert_eq!(
        outcome,
        Ok(HierarchicalOutcome::Path {
            waypoints: Vec::new(),
            cost: 8 * 1_024
        })
    );
}

#[test]
fn a_wall_detour_goes_through_the_only_gateway() {
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let outcome = hierarchical_route(
        &generator,
        TileCoord::new(1, 32),
        TileCoord::new(62, 32),
        256,
    )
    .expect("route");
    let HierarchicalOutcome::Path { waypoints, cost } = outcome else {
        panic!("expected a path, got {outcome:?}");
    };
    assert!(
        waypoints.contains(&TileCoord::new(31, 5)) || waypoints.contains(&TileCoord::new(32, 5))
    );
    assert!(cost > lower_bound(TileCoord::new(1, 32), TileCoord::new(62, 32)));
}

#[test]
fn a_wall_without_a_gateway_is_proven_disconnected() {
    let generator = flat_terrain_with_water_wall(64, 32, -1);
    assert_eq!(
        hierarchical_route(
            &generator,
            TileCoord::new(1, 32),
            TileCoord::new(62, 32),
            256
        ),
        Ok(HierarchicalOutcome::Unreachable)
    );
}

#[test]
fn a_1600_tile_route_is_found_over_chunks_deterministically_and_within_bound() {
    let generator = flat_terrain_with_water_wall(4_096, -1, -1);
    let (origin, destination) = (TileCoord::new(100, 200), TileCoord::new(1_700, 900));
    let first = hierarchical_route(&generator, origin, destination, 2_048).expect("route");
    let again = hierarchical_route(
        &flat_terrain_with_water_wall(4_096, -1, -1),
        origin,
        destination,
        2_048,
    )
    .expect("route");
    assert_eq!(first, again, "a cold cache must give the same answer");
    let HierarchicalOutcome::Path { waypoints, cost } = first else {
        panic!("expected a path, got {first:?}");
    };
    assert!(waypoints.len() >= 2 * 49 && waypoints.len() % 2 == 0);
    assert!(cost >= lower_bound(origin, destination));
    for entrance in waypoints.chunks(2) {
        let (a, b) = (entrance[0], entrance[1]);
        assert_eq!((a.x - b.x).abs() + (a.y - b.y).abs(), 1);
        assert_ne!(
            (a.x.div_euclid(32), a.y.div_euclid(32)),
            (b.x.div_euclid(32), b.y.div_euclid(32))
        );
    }
    assert_eq!(
        hierarchical_route(&generator, origin, destination, 3),
        Ok(HierarchicalOutcome::Bounded)
    );
}

#[test]
fn blocked_endpoints_are_invalid() {
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    assert_eq!(
        hierarchical_route(
            &generator,
            TileCoord::new(32, 20),
            TileCoord::new(62, 32),
            16
        ),
        Ok(HierarchicalOutcome::Invalid)
    );
}
