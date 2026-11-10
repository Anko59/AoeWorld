use super::*;
use aoe_map::{NavigationPhase, NavigationProbe, NavigationSpan};
use std::sync::atomic::AtomicUsize;

#[derive(Debug, Default)]
struct Probe {
    portals: AtomicUsize,
    hierarchy: AtomicUsize,
}
struct Span;
impl NavigationSpan for Span {}
impl NavigationProbe for Probe {
    fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
        if phase == NavigationPhase::Portals {
            self.portals.fetch_add(1, Ordering::Relaxed);
        }
        if phase == NavigationPhase::Hierarchy {
            self.hierarchy.fetch_add(1, Ordering::Relaxed);
        }
        Box::new(Span)
    }
}

#[test]
fn a_region_certified_far_trip_needs_no_second_connectivity_search() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain_size(256);
    world.prepare_navigation(&|| false).unwrap();
    let probe = Arc::new(Probe::default());
    if let Terrain::Map { generator, .. } = &world.terrain {
        generator.set_navigation_probe(probe.clone());
    }
    let from = TileCoord::new(10, 10);
    let target = TileCoord::new(220, 10);
    assert!(
        world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, target)
            .unwrap()
    );
    let id = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(from).unwrap())
        .unwrap();
    world
        .issue_task_move(id, WorldPosition::from_tile_center(target).unwrap())
        .unwrap();
    for _ in 0..8000 {
        world.advance();
        if world.movement_order(id).is_none() {
            break;
        }
    }
    assert_eq!(
        world.unit(id).unwrap().position,
        WorldPosition::from_tile_center(target).unwrap()
    );
    assert_eq!(world.movement_failure(id), None);
    assert_eq!(world.route_accounting_error(), None);
    assert_eq!(
        probe.portals.load(Ordering::Relaxed),
        0,
        "a current region witness already certifies connectivity; local legs still supply the path"
    );
}

#[test]
fn a_certified_near_endpoint_uses_its_region_witness_and_keeps_terminal_unreachable() {
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain();
    if let Terrain::Map { overlay, .. } = &mut world.terrain {
        for rect in [
            TileRect::from_xywh(25, 25, 5, 1),
            TileRect::from_xywh(25, 29, 5, 1),
            TileRect::from_xywh(25, 26, 1, 3),
            TileRect::from_xywh(29, 26, 1, 3),
        ] {
            overlay.add_footprint(rect).unwrap();
        }
    }
    let from = TileCoord::new(20, 24);
    let target = TileCoord::new(27, 27);
    let mut planner = aoe_map::RoutePlanner::new(from, target, MAX_ROUTE_WORK_PER_ORDER);
    for _ in 0..MAX_ROUTE_WORK_PER_ORDER {
        assert_eq!(
            world.poll_world_route_planner(&mut planner, 1, &|| false, UnitKind::Villager),
            RoutePlannerPoll::Pending
        );
        if planner.certified_destination().is_some() {
            break;
        }
    }
    let certified = planner.certified_destination().unwrap();
    assert_ne!(certified, target);
    let opening = TileRect::from_xywh(25, 27, 1, 1);
    assert!(planner.opening_invalidates_proof(opening));
    if let Terrain::Map { generator, overlay } = &world.terrain {
        let ground = aoe_map::MapRegionGround::new(generator, overlay, &|| false);
        let mut regions = aoe_map::Regions::new(64, 64, aoe_map::LandBucket::Small);
        assert_eq!(
            regions.reachable_bounded(&ground, 0, from, certified, 64),
            aoe_map::RegionReachability::Straight
        );
    }
    let probe = Arc::new(Probe::default());
    if let Terrain::Map { generator, .. } = &world.terrain {
        generator.set_navigation_probe(probe.clone());
    }
    let mut last = from;
    let mut terminal = false;
    for _ in 0..1024 {
        world.reuse_region_endpoint_proof(&mut planner, UnitKind::Villager, PlayerId(0));
        assert!(
            planner.opening_invalidates_proof(opening),
            "the original sealed goal must still be retried if its entrance opens"
        );
        match world.poll_world_route_planner(&mut planner, 1024, &|| false, UnitKind::Villager) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => last = *path.tiles.last().unwrap(),
            RoutePlannerPoll::Unreachable => {
                terminal = true;
                break;
            }
            other => panic!("unexpected certified route outcome: {other:?}"),
        }
    }
    assert!(terminal);
    assert_eq!(last, certified);
    assert_eq!(
        probe.hierarchy.load(Ordering::Relaxed),
        0,
        "the straight path to a region-certified endpoint needs no second hierarchy search"
    );
}
