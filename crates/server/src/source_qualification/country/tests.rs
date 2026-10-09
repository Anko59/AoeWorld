use super::diagnostics::{TileCounts, blockers, nearest_source, straight_destinations};
use super::*;

#[test]
fn bounded_start_outcomes_remain_distinct_and_unqualified() {
    for (outcome, label) in [
        (StartSearchResult::Unavailable, "unavailable"),
        (StartSearchResult::LimitReached, "limit_reached"),
        (StartSearchResult::Cancelled, "cancelled"),
    ] {
        assert_eq!(start_result(outcome), (label, None));
    }
    assert_eq!(
        start_result(StartSearchResult::Found(TileCoord::new(12, 34))),
        ("found", Some([12, 34]))
    );
    let report = SourceCountryProbe {
        policy: "test-only",
        content_hash: "test-only".into(),
        source_lock_count: 0,
        indexed_pages: 0,
        tiles_per_side: 20_000,
        typed_hydrology: false,
        hydrology_index: None,
        ordinary_start: "limit_reached",
        start: None,
        start_neighbourhood: Vec::new(),
        local_component: None,
        native_local_orders: Vec::new(),
        routes: Vec::new(),
        live_activation: false,
        hardware_qualified: false,
    };
    let value = serde_json::to_value(report).expect("diagnostic report");
    assert_eq!(value["live_activation"], false);
    assert_eq!(value["hardware_qualified"], false);
    assert!(value["start"].is_null());
    assert_eq!(value["routes"], serde_json::json!([]));
    assert_eq!(value["start_neighbourhood"], serde_json::json!([]));
}

#[test]
fn procedural_or_legacy_package_cannot_be_country_source_evidence() {
    let mut request = aoe_map::MapRequest {
        requested_side_meters: 1_200_000,
        compression: aoe_map::Ratio::new(30, 1).expect("compression"),
        ..aoe_map::MapRequest::default()
    };
    for profile in [DetailProfile::StandardV1, DetailProfile::LandscapeV2] {
        request.detail_profile = profile;
        let package = MapPackage::new(9, request, Vec::new()).expect("procedural package");
        assert!(!supported_country(&package));
    }
}

#[test]
fn synthetic_fixed_cardinal_samples_are_bounded_not_real_source_evidence() {
    let origin = TileCoord::new(9999, 9999);
    let mut count = 0;
    for (direction, endpoint) in
        DIRECTIONS
            .into_iter()
            .zip([[10127, 9999], [9999, 10127], [9871, 9999], [9999, 9871]])
    {
        let samples: Vec<_> = straight_destinations(origin, direction).collect();
        assert_eq!(samples.len(), 128);
        let mut previous = origin;
        for (step, tile) in &samples {
            assert!((1..=128).contains(step));
            assert_eq!(previous.x.abs_diff(tile.x) + previous.y.abs_diff(tile.y), 1);
            previous = *tile;
        }
        assert_eq!([previous.x, previous.y], endpoint);
        count += samples.len();
    }
    assert_eq!(count, 512);
    assert_eq!(nearest_source(origin, 128, 20_000), [63, 63]);
    assert_eq!(
        nearest_source(TileCoord::new(19_999, 0), 1024, 20_000),
        [1023, 0]
    );
    assert_eq!(
        nearest_source(TileCoord::new(10_000, 10_000), 128, 20_000),
        [64, 64]
    );
}

fn synthetic_tile() -> TileObservation {
    let raw_physical = aoe_map::MapChunkGenerator::new([0; 32], 7, 20_000)
        .tile_at(TileCoord::new(9999, 9999))
        .expect("synthetic tile");
    TileObservation {
        coordinate: [9999, 9999],
        raw_physical,
        effective_passable: true,
        resource: None,
        appearance: None,
        clearing_reservations: super::diagnostics::ClearingReservations {
            route: false,
            start: false,
            resource_approach: false,
        },
        overview_water: None,
        source_hydrology_coordinate: None,
        source_hydrology: None,
        water_model: None,
    }
}

#[test]
fn synthetic_counts_overlap_without_water_or_forest_cause_attribution() {
    let mut sample = synthetic_tile();
    sample.raw_physical.water = aoe_map::WaterKind::Lake;
    sample.raw_physical.material = aoe_map::GroundMaterial::ForestFloor;
    sample.raw_physical.surface.kind = aoe_map::SurfaceKind::Cliff;
    sample.raw_physical.passable = false;
    sample.effective_passable = false;
    sample.resource = Some(aoe_map::ResourceNode {
        id: 1,
        tile: TileCoord::new(9999, 9999),
        kind: aoe_map::ResourceKind::Wood,
        object: aoe_map::ObjectKind::Tree,
        initial_amount: 1,
        visual_variant: 0,
    });
    let mut counts = TileCounts::default();
    counts.observe(&sample);
    let value = serde_json::to_value(&counts).expect("synthetic counts");
    for field in [
        "sampled_tiles",
        "water_present",
        "nonwalkable_surface",
        "raw_impassable",
        "effective_impassable",
        "resource_present",
        "forest_floor",
    ] {
        assert_eq!(value[field], 1);
    }
    assert_eq!(value["canopy_present"], 0);
    let observation = serde_json::to_value(&sample).expect("synthetic schema");
    assert_eq!(observation["raw_physical"]["water"], "Lake");
    assert_eq!(observation["raw_physical"]["surface"]["kind"], "Cliff");
    assert!(observation["water_model"].is_null());
    assert!(observation["source_hydrology"].is_null());
    assert!(observation.get("root_cause").is_none());
    let facts = serde_json::to_value(blockers(&sample, &sample)).expect("blocker facts");
    assert_eq!(facts["to_raw_impassable"], true);
    assert_eq!(facts["to_nonwalkable_surface"], true);
    assert_eq!(facts["to_resource_present"], true);
    assert_eq!(facts["game_height_step_exceeds_one"], false);
    assert!(facts.get("water_caused_failure").is_none());
}

#[test]
fn synthetic_component_caps_are_not_route_or_hardware_qualification() {
    let terrain = aoe_simulation::Terrain::uniform(1);
    let config =
        aoe_core::WorldConfig::new(20_000, 20_000, aoe_core::Seed(1)).expect("synthetic config");
    let report = component::observe(&terrain, config, TileCoord::new(9999, 9999))
        .expect("bounded synthetic component");
    assert_eq!(report.visited_tiles, 4096);
    assert!(report.probe_work <= 32768);
    assert!(report.truncated);
    assert!(!report.proves_complete_component);
    assert!(!report.proves_planner_route);
    let value = serde_json::to_value(report).expect("component diagnostic schema");
    assert!(value.get("hardware_qualified").is_none());
    assert!(value.get("root_cause").is_none());
}

#[test]
fn synthetic_small_component_completeness_does_not_prove_planner_orders() {
    let terrain = aoe_simulation::Terrain::uniform(1);
    let config =
        aoe_core::WorldConfig::new(3, 3, aoe_core::Seed(1)).expect("synthetic small config");
    let report = component::observe(&terrain, config, TileCoord::new(1, 1))
        .expect("synthetic small component");
    assert_eq!(report.visited_tiles, 9);
    assert_eq!(report.bounds, Some([0, 0, 2, 2]));
    assert!(!report.truncated);
    assert!(report.proves_complete_component);
    assert!(!report.proves_planner_route);
    let blocked =
        component::observe(&terrain, config, TileCoord::new(-1, -1)).expect("out of bounds origin");
    assert_eq!(blocked.visited_tiles, 0);
    assert_eq!(blocked.bounds, None);
    assert!(!blocked.proves_complete_component);
}

#[test]
fn synthetic_local_order_advances_real_fixed_point_simulation_not_hardware() {
    let config =
        aoe_core::WorldConfig::new(128, 128, aoe_core::Seed(1)).expect("synthetic movement config");
    let world = aoe_simulation::GameWorld::new(config).expect("synthetic uniform world");
    let origin = TileCoord::new(64, 64);
    let destination = TileCoord::new(96, 64);
    let report = movement::advance_order(world, origin, destination, movement::OrderCase::Nearby32)
        .expect("synthetic native move");
    assert_eq!(report.outcome, "arrived");
    assert!(report.ticks_advanced > 0 && report.ticks_advanced <= 2048);
    assert_eq!(report.requested_distance_tiles, 32);
    assert_eq!(report.requested_game_meters, 64);
    assert_eq!(report.maximum_ticks, 2048);
    assert_eq!(
        report.policy,
        "native-country-four-fixed-32-tile-orders-2048-ticks-v1"
    );
    assert!(report.failure.is_none());
    assert!(!report.hardware_qualified);
    let target = aoe_core::WorldPosition::from_tile_center(destination).expect("destination");
    assert_eq!(report.final_position_subunits, [target.x, target.y]);
}

#[test]
fn synthetic_rejected_local_order_does_not_become_an_arrival() {
    let config =
        aoe_core::WorldConfig::new(64, 64, aoe_core::Seed(1)).expect("synthetic movement config");
    let world = aoe_simulation::GameWorld::new(config).expect("synthetic uniform world");
    // issue_move canonicalizes positions; an out-of-range target is clamped,
    // so exercise an origin error instead of fabricating an invalid verdict.
    assert!(
        movement::advance_order(
            world,
            TileCoord::new(-1, -1),
            TileCoord::new(32, 0),
            movement::OrderCase::Nearby32,
        )
        .is_err()
    );
}

#[test]
fn native_order_case_labels_and_original_endpoints_are_fixed_not_adaptive() {
    let origin = TileCoord::new(9999, 9999);
    let cases = movement::ORDER_CASES;
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].distance_tiles(), 32);
    assert_eq!(cases[0].maximum_ticks(), 2048);
    assert_eq!(cases[1].distance_tiles(), LOCAL_DISTANCE);
    assert_eq!(cases[1].maximum_ticks(), 8192);
    assert_ne!(cases[0].policy(), cases[1].policy());
    assert_eq!(
        cases[1].policy(),
        "native-country-original-four-fixed-128-tile-orders-8192-ticks-v1"
    );
    for (direction, endpoint) in
        DIRECTIONS
            .into_iter()
            .zip([[10127, 9999], [9999, 10127], [9871, 9999], [9999, 9871]])
    {
        let native = cases[1].destination(origin, direction);
        assert_eq!([native.x, native.y], endpoint);
        let (_, original_direct) = straight_destinations(origin, direction)
            .last()
            .expect("original fixed endpoint");
        assert_eq!(native, original_direct);
    }
}

#[test]
fn synthetic_original_endpoint_native_order_retains_256m_and_8192_tick_labels() {
    let config = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1))
        .expect("synthetic larger movement config");
    let world = aoe_simulation::GameWorld::new(config).expect("synthetic uniform world");
    let origin = TileCoord::new(256, 256);
    let case = movement::OrderCase::Original128;
    let destination = case.destination(origin, DIRECTIONS[0]);
    let report = movement::advance_order(world, origin, destination, case)
        .expect("synthetic original-distance move");
    assert_eq!(report.outcome, "arrived");
    assert_eq!(report.requested_distance_tiles, 128);
    assert_eq!(report.requested_game_meters, 256);
    assert_eq!(report.maximum_ticks, 8192);
    assert_eq!(report.policy, case.policy());
    assert!(report.ticks_advanced > 0 && report.ticks_advanced <= 8192);
    assert!(report.failure.is_none());
    assert!(!report.hardware_qualified);
    let target = aoe_core::WorldPosition::from_tile_center(destination).expect("destination");
    assert_eq!(report.final_position_subunits, [target.x, target.y]);
}

#[test]
fn synthetic_slow_native_orders_stop_at_each_case_tick_cap_without_false_arrival() {
    // Slower synthetic uniform fixture proves each observer stops. Production
    // source worlds retain their ordinary configuration and all route budgets.
    for case in movement::ORDER_CASES {
        let config = aoe_core::WorldConfig {
            width_tiles: 512,
            height_tiles: 512,
            move_speed_subunits_per_tick: 1,
            ..aoe_core::WorldConfig::default()
        };
        let world = aoe_simulation::GameWorld::new(config).expect("slow synthetic world");
        let origin = TileCoord::new(256, 256);
        let destination = case.destination(origin, DIRECTIONS[0]);
        let report = movement::advance_order(world, origin, destination, case)
            .expect("bounded synthetic slow move");
        assert_eq!(report.outcome, "tick_limit");
        assert_eq!(report.maximum_ticks, case.maximum_ticks());
        assert_eq!(report.ticks_advanced, case.maximum_ticks());
        assert_eq!(report.policy, case.policy());
        assert_eq!(
            report.requested_distance_tiles,
            case.distance_tiles() as u32
        );
        assert_eq!(
            report.requested_game_meters,
            case.distance_tiles() as u32 * 2
        );
        assert!(report.failure.is_none());
        assert!(!report.hardware_qualified);
        let target = aoe_core::WorldPosition::from_tile_center(destination).expect("target");
        assert_ne!(report.final_position_subunits, [target.x, target.y]);
    }
}
