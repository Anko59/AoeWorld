use super::*;
use aoe_map::{MapPackage, MapRequest};

#[test]
fn dropped_mutation_and_failed_restore_preserve_world_then_commit_roundtrips() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
    let hash = package.content_hash;
    let node = (0..16)
        .find_map(|y| {
            (0..16).find_map(|x| {
                package
                    .generator()
                    .chunk(x, y)
                    .ok()?
                    .resources
                    .into_iter()
                    .next()
            })
        })
        .expect("fixture resource");
    let mut world = GameWorld::from_map(package.clone()).unwrap();
    let before = world.canonical_hash();
    {
        let transaction = world.prepare_resource_depletion(node.id, 1).unwrap();
        assert_eq!(transaction.snapshot(hash).revision, 1);
    }
    assert_eq!(world.canonical_hash(), before);
    let change = world
        .prepare_resource_depletion(node.id, 1)
        .unwrap()
        .commit();
    assert_eq!(change.removed, 1);
    let snapshot = world.resource_snapshot(hash).unwrap();
    let mut restored = GameWorld::from_map(package).unwrap();
    assert!(
        restored
            .restore_resources(&snapshot, [9; 32], &|| false)
            .is_err()
    );
    assert_eq!(restored.canonical_hash(), before);
    restored
        .restore_resources(&snapshot, hash, &|| false)
        .unwrap();
    assert_eq!(restored.canonical_hash(), world.canonical_hash());
}

struct Ring;
impl aoe_map::RouteTerrain for Ring {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: aoe_core::TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, aoe_map::EnvironmentPageError> {
        Ok((0..256).contains(&tile.x)
            && (0..256).contains(&tile.y)
            && (tile.x - 200).abs().max((tile.y - 200).abs()) != 2)
    }
    fn route_edge_clear(
        &self,
        _: aoe_core::TileCoord,
        _: aoe_core::TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, aoe_map::EnvironmentPageError> {
        Ok(true)
    }
    fn route_step_cost(
        &self,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, aoe_map::EnvironmentPageError> {
        Ok(if from.x == to.x || from.y == to.y {
            10
        } else {
            14
        })
    }
}

fn check_collision_oracle(opened: bool) {
    use aoe_core::{PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
    use aoe_map::{RoutePlanner, RoutePlannerPoll};

    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let origin = TileCoord::new(200, 197);
    let destination = TileCoord::new(200, 200);
    let id = w
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(origin).unwrap(),
        )
        .unwrap();
    let mut planner = RoutePlanner::new(origin, destination, 1_000_000);
    assert_eq!(
        planner.poll(&Ring, &ResourceOverlay::default(), 1024, &|| false),
        RoutePlannerPoll::Pending
    );
    assert_eq!(
        planner.poll(&Ring, &ResourceOverlay::default(), 16, &|| false),
        RoutePlannerPoll::Pending
    );
    let area = if opened {
        let entrance = TileRect::from_xywh(198, 200, 1, 1);
        assert!(planner.opening_invalidates_proof(entrance));
        assert!(!planner.touches(entrance));
        entrance
    } else {
        let searched = TileRect::from_xywh(200, 200, 1, 1);
        assert!(planner.touches(searched));
        searched
    };
    let index = w.lookup[&id];
    w.store_planner(index, Some(planner));
    // Inject a missed invalidation: the planner is still subscribed. The
    // full-scan oracle must catch this even though registration is exact.
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            w.assert_collision_subscribers_complete(area, opened);
        }))
        .is_err(),
        "oracle accepted an invalidated planner (opened={opened}, {area:?})"
    );
    if opened {
        w.collision_opened(area);
    } else {
        w.collision_closed(area);
    }
    assert!(w.units[index].planner.is_none());
    assert!(!w.collision_subscribers.planners.contains(&id));
}

#[test]
fn collision_oracle_rejects_a_subscribed_planner_after_a_closing() {
    check_collision_oracle(false);
}

#[test]
fn collision_oracle_rejects_a_subscribed_planner_after_an_opening() {
    check_collision_oracle(true);
}
