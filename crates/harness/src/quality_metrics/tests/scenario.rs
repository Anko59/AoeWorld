use crate::quality_metrics::march::{self, OrderPath, prepare};

#[test]
fn general_cohort_is_actually_adopted_and_orders_are_not_synthetic_labels() {
    let scene = prepare("river", OrderPath::General).unwrap();
    assert_eq!(scene.ids.len(), 40);
    assert!(
        scene
            .world
            .general_status(scene.controller.unwrap())
            .unwrap()
            .squads
            .iter()
            .flatten()
            .count()
            >= 2,
        "the General case must exercise multiple independently routed squads"
    );
    assert!(
        scene
            .ids
            .iter()
            .all(|id| scene.world.general_controller(*id).is_some())
    );
}

#[test]
fn single_and_group_share_terrain_and_requested_goal() {
    let single = prepare("river", OrderPath::Single).unwrap();
    let group = prepare("river", OrderPath::Group).unwrap();
    assert_eq!(single.goal, group.goal);
    assert_eq!(single.ids.len(), 1);
    assert_eq!(group.ids.len(), 40);
}

#[test]
fn combined_march_contains_a_real_ford_material_and_completed_bridges() {
    use aoe_core::TileCoord;
    let scene = prepare("quality-march", OrderPath::Single).unwrap();
    assert_eq!(
        scene
            .world
            .buildings()
            .filter(|b| b.kind == aoe_simulation::BuildingKind::Bridge)
            .count(),
        2
    );
    let aoe_simulation::Terrain::Map { generator, .. } = scene.world.terrain() else {
        panic!("prepared map")
    };
    assert!((20..=180).any(|y| (135..165).any(|x| {
        generator
            .tile_at(TileCoord::new(x, y))
            .is_some_and(|t| t.material == aoe_map::GroundMaterial::Ford)
    })));
}

#[test]
fn steward_transfer_cohort_is_adopted_before_cap_order() {
    let mut scene = prepare("river", OrderPath::Steward).unwrap();
    assert!(
        scene
            .ids
            .iter()
            .all(|id| scene.world.steward_controller(*id) == scene.controller)
    );
    scene.issue().unwrap();
    for _ in 0..40 {
        scene.world.advance();
    }
    assert!(scene.ids.iter().any(|id| {
        scene
            .world
            .unit(*id)
            .is_some_and(|u| u.moving || u.planning)
    }));
}

#[test]
fn complete_river_metrics_repeat_across_worker_counts() {
    let measure = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                prepare("river", OrderPath::Group)
                    .unwrap()
                    .measure("river", None)
                    .unwrap()
            })
    };
    let single = measure(1);
    assert_eq!(single, measure(2));
    assert_eq!(single["movement"]["ordered"], 40);
    assert!(single["movement"]["arrived"].as_u64().unwrap() > 0);
}

#[test]
fn source_crossing_setup_adds_a_bridge_without_demo_actors() {
    let mut world = aoe_simulation::terrain_fixtures::measurement_world("river");
    let (start, end) = crate::quality_metrics::source::crossing(&mut world).unwrap();
    assert_ne!(start, end);
    assert_eq!(world.units().count(), 0);
    assert!(
        world
            .buildings()
            .any(|b| b.kind == aoe_simulation::BuildingKind::Bridge)
    );
}

#[test]
fn reaching_a_tactical_combat_slot_is_not_arriving_at_the_requested_objective() {
    use aoe_core::{PlayerId, Seed, WorldConfig};
    use aoe_simulation::{GameWorld, UnitKind};
    let world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let mut scene = crate::quality_metrics::march::populate(
        world,
        (35, 100),
        (140, 100),
        OrderPath::General,
        UnitKind::Militia,
        80,
    )
    .unwrap();
    scene.attack_move = true;
    scene
        .world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Militia,
            crate::quality_metrics::march::at(42, 100).unwrap(),
        )
        .unwrap();
    let report = scene.measure("near-contact-far-objective", None).unwrap();
    assert_eq!(
        report["movement"]["arrived"], 0,
        "eighty ticks cannot cover the hundred-tile march"
    );
}

#[test]
fn invalid_inputs_missing_cohort_actor_returns_an_error() {
    let mut scene = prepare("river", OrderPath::Single).unwrap();
    scene.ids[0] = aoe_core::EntityId(u32::MAX);
    assert!(scene.measure("missing-actor", None).is_err());
}

#[test]
fn straight_line_time_bound_stops_at_the_two_tile_arrival_boundary() {
    use aoe_core::{PlayerId, Seed, WorldConfig};
    use aoe_simulation::{GameWorld, UnitKind};
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let start = march::at(35, 100).unwrap();
    let goal = march::at(45, 100).unwrap();
    let id = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, start)
        .unwrap();
    let (speed, denominator) = world.movement_speed(UnitKind::Militia, PlayerId(0));
    let expected = (8 * 1024 * denominator).div_ceil(speed);
    let scene = march::Scene {
        world,
        ids: vec![id],
        goal,
        path: OrderPath::Single,
        controller: None,
        attack_move: false,
        limit: 1000,
    };
    let report = scene.measure("straight-line-arrival-bound", None).unwrap();
    assert_eq!(report["straight_line_lower_bound_ticks_min"], expected);
    assert!(report["movement"]["ratio_ppm_p50"].as_u64().unwrap() >= 1_000_000);
}
