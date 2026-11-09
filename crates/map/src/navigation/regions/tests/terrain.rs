//! Bulk terrain reads must preserve the planner's canonical passability.
use super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use crate::{MapRegionGround, ResourceOverlay, RouteTerrain};

#[test]
fn bulk_ground_matches_planner_tiles_and_edges_across_chunks() {
    let generator = flat_terrain_with_water_wall(96, 48, 17);
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(30, 20, 5, 7))
        .unwrap();
    let ground = MapRegionGround::new(&generator, &overlay, &|| false);
    for y in 0..96 {
        for x in 0..96 {
            let from = TileCoord::new(x, y);
            let expected = generator.route_walkable(&overlay, from, &|| false).unwrap();
            assert_eq!(ground.tile(from) == RegionTile::Open, expected);
            for (dx, dy) in label::NEIGHBOURS {
                let to = TileCoord::new(x + dx, y + dy);
                if to.x < 0 || to.y < 0 || to.x >= 96 || to.y >= 96 {
                    continue;
                }
                assert_eq!(
                    ground.edge_clear(from, to),
                    generator
                        .route_overlay_edge_clear(&overlay, from, to, &|| false)
                        .unwrap(),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }
    assert_eq!(ground.error(), None);
}

#[test]
fn cancelled_bulk_ground_reports_an_error_instead_of_proving_unreachable() {
    let generator = flat_terrain_with_water_wall(64, 32, 17);
    let ground = MapRegionGround::new(&generator, ResourceOverlay::empty(), &|| true);
    let mut regions = Regions::new(64, 64);
    regions.reachable(&ground, 1, TileCoord::new(0, 0), TileCoord::new(63, 63));
    assert_eq!(ground.error(), Some(crate::EnvironmentPageError::Cancelled));
}

#[test]
fn bulk_chunk_topology_equals_point_only_ground() {
    struct Points<'a>(&'a MapRegionGround<'a>);
    impl RegionGround for Points<'_> {
        fn tile(&self, tile: TileCoord) -> RegionTile {
            self.0.tile(tile)
        }
        fn edge_clear(&self, a: TileCoord, b: TileCoord) -> bool {
            self.0.edge_clear(a, b)
        }
        fn diagonal_override(&self, a: TileCoord, b: TileCoord) -> Option<bool> {
            self.0.diagonal_override(a, b)
        }
        fn gate_owner(&self, gate: u32) -> u32 {
            self.0.gate_owner(gate)
        }
    }
    let generator = flat_terrain_with_water_wall(96, 48, -1);
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(30, 20, 5, 7))
        .unwrap();
    let ground = MapRegionGround::new(&generator, &overlay, &|| false);
    let mut fast = Regions::new(96, 96);
    let mut points = Regions::new(96, 96);
    let point_ground = Points(&ground);
    for y in 0..96 {
        for x in 0..96 {
            let tile = TileCoord::new(x, y);
            assert_eq!(
                fast.component(&ground, tile),
                points.component(&point_ground, tile)
            );
        }
    }
    for (from, goal) in [
        (TileCoord::new(10, 20), TileCoord::new(70, 20)),
        (TileCoord::new(70, 20), TileCoord::new(31, 22)),
    ] {
        assert_eq!(
            fast.frontier(&ground, 1, from, goal),
            points.frontier(&point_ground, 1, from, goal)
        );
    }
}
