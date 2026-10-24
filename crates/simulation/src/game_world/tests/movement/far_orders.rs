//! Orders hundreds or thousands of tiles away must start on the tick they are
//! issued and keep moving: planning is never a precondition for the first step.
#[path = "../far_orders/attack.rs"]
mod attack;
use super::*;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, WaterPage, ordered_biome_page_root, ordered_page_root,
    ordered_water_page_root,
};

pub(super) const WIDTH: i32 = 4_096;
/// Barren ground: no generated trees or other blocking resources.
const BARREN: u8 = 28;

#[test]
fn a_prepared_near_order_waits_for_endpoint_refinement_before_approaching() {
    let mut world = wide_world();
    world.prepare_navigation(&|| false).unwrap();
    let unit = world
        .spawn_kind(PlayerId(0), UnitKind::Scout, center(10, 20))
        .unwrap();
    world.issue_move(unit, center(20, 20)).unwrap();
    // A newly closed accepted destination must refine its endpoint; a new
    // order directly onto an already blocked tile is rejected at issue time.
    if let Terrain::Map { overlay, .. } = &mut world.terrain {
        overlay
            .add_footprint(TileRect::from_xywh(20, 20, 1, 1))
            .unwrap();
    }
    world.collision_closed(TileRect::from_xywh(20, 20, 1, 1));
    for _ in 0..200 {
        world.advance();
        if world.movement_order(unit).is_none() {
            break;
        }
    }
    assert_eq!(
        world.unit(unit).unwrap().position.tile_floor(),
        TileCoord::new(20, 19)
    );
    assert_eq!(
        world.movement_failure(unit),
        Some(GameWorldError::Unreachable)
    );
}

fn wide_barren_terrain() -> Terrain {
    prepared_terrain(vec![0; 4], vec![BARREN; 4])
}

pub(super) fn prepared_terrain(water_samples: Vec<u8>, biome_samples: Vec<u8>) -> Terrain {
    let axis = water_samples.len().isqrt() as u8;
    assert_eq!(usize::from(axis).pow(2), water_samples.len());
    assert_eq!(water_samples.len(), biome_samples.len());
    let mut elevations = Vec::new();
    let mut waters = Vec::new();
    let mut biomes = Vec::new();
    let (mut side, mut level) = (axis, 0);
    loop {
        let count = usize::from(side).pow(2);
        elevations.push(ElevationPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            geographic_height_centimeters: vec![0; count],
        });
        waters.push(WaterPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            ocean_coverage_percent: if level == 0 {
                water_samples.clone()
            } else {
                vec![water_samples.iter().copied().max().unwrap(); count]
            },
            inland_coverage_percent: vec![0; count],
        });
        biomes.push(PotentialBiomePage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            potential_biome_class: if level == 0 {
                biome_samples.clone()
            } else {
                vec![biome_samples[0]; count]
            },
        });
        if side == 1 {
            break;
        }
        side = side.div_ceil(2);
        level += 1;
    }
    let levels = |roots: Vec<[u8; 32]>| FieldPyramid {
        levels: roots
            .into_iter()
            .enumerate()
            .map(|(level, ordered_page_root)| PyramidLevel {
                samples_per_axis: u16::from(axis).div_ceil(1 << level),
                ordered_page_root,
            })
            .collect(),
    };
    let environment = PreparedEnvironment {
        samples_per_axis: u16::from(axis),
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: levels(
            elevations
                .iter()
                .map(|p| ordered_page_root(std::slice::from_ref(p)).unwrap())
                .collect(),
        ),
        water: Some(levels(
            waters
                .iter()
                .map(|p| ordered_water_page_root(std::slice::from_ref(p)).unwrap())
                .collect(),
        )),
        vegetation: Some(levels(
            biomes
                .iter()
                .map(|p| ordered_biome_page_root(std::slice::from_ref(p)).unwrap())
                .collect(),
        )),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let generator = MapChunkGenerator::new([0; 32], 0, WIDTH)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            elevations,
        )
        .expect("elevation")
        .with_prepared_water(&environment, waters)
        .expect("water")
        .with_prepared_biomes(&environment, biomes)
        .expect("biome");
    Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    }
}

pub(super) fn wide_world() -> GameWorld {
    let config = WorldConfig::new(WIDTH, WIDTH, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = wide_barren_terrain();
    world
}

pub(super) fn center(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).expect("tile center")
}

#[test]
fn far_order_moves_on_the_tick_it_is_issued_and_never_waits_for_planning() {
    let mut world = wide_world();
    let mut replay = wide_world();
    let origin = center(1, 10);
    let target = center(1_500, 700);
    let unit = world.spawn_unit(PlayerId(0), origin).expect("unit");
    let twin = replay.spawn_unit(PlayerId(0), origin).expect("twin");

    assert!(world.issue_move(unit, target).expect("far order"));
    assert!(replay.issue_move(twin, target).expect("far replay order"));
    let issued = world.unit(unit).expect("unit");
    assert!(issued.moving, "a far order starts without a planning wait");
    assert!(!issued.planning);

    world.advance();
    replay.advance();
    assert_ne!(world.unit(unit).expect("unit").position, origin);

    let mut arrived_after = None;
    for tick in 2..=20_000_u32 {
        world.advance();
        replay.advance();
        let state = world.unit(unit).expect("unit");
        assert!(!state.planning, "planning pause at tick {tick}");
        assert_eq!(world.movement_failure(unit), None);
        if tick % 512 == 0 {
            assert_eq!(world.canonical_hash(), replay.canonical_hash());
        }
        if !state.moving {
            arrived_after = Some(tick);
            break;
        }
    }
    assert!(arrived_after.is_some(), "the far order never arrived");
    assert_eq!(world.unit(unit).expect("unit").position, target);
    assert_eq!(world.canonical_hash(), replay.canonical_hash());
    assert_eq!(world.active_planner_count, 0);
    assert_eq!(world.active_route_searches, 0);
}

#[test]
fn simultaneous_far_orders_all_move_on_the_first_tick() {
    let mut world = wide_world();
    let units = (0..12)
        .map(|index| {
            let row = 10 + 3 * index;
            let unit = world.spawn_unit(PlayerId(0), center(1, row)).expect("unit");
            (unit, center(1, row), center(3_000, row + 400))
        })
        .collect::<Vec<_>>();
    for (unit, _, target) in &units {
        assert!(world.issue_move(*unit, *target).expect("far order"));
    }
    world.advance();
    for (unit, origin, _) in &units {
        let state = world.unit(*unit).expect("unit");
        assert!(state.moving, "unit {unit:?} waits for a planner");
        assert!(!state.planning);
        assert_ne!(state.position, *origin);
    }
    for tick in 0..1_024 {
        world.advance();
        for (unit, _, _) in &units {
            let state = world.unit(*unit).expect("unit");
            assert!(
                state.moving && !state.planning,
                "unit {unit:?} paused at tick {tick}"
            );
        }
    }
    // All twelve left their first spatial chunk together; each must still be
    // indexed exactly once (a stale bucket slot used to panic here).
    let (indexed, _) = world.query(TileRect::from_xywh(0, 0, WIDTH, WIDTH));
    assert_eq!(indexed.len(), units.len());
}

#[test]
fn redirected_far_order_moves_toward_the_new_destination_at_once() {
    let mut world = wide_world();
    let origin = center(2_000, 2_000);
    let unit = world.spawn_unit(PlayerId(0), origin).expect("unit");
    assert!(world.issue_move(unit, center(4_000, 2_000)).expect("east"));
    for _ in 0..200 {
        world.advance();
    }
    let turned_at = world.unit(unit).expect("unit").position;
    assert!(turned_at.x > origin.x);
    assert!(world.issue_move(unit, center(10, 2_000)).expect("west"));
    let state = world.unit(unit).expect("unit");
    assert!(state.moving && !state.planning);
    for _ in 0..200 {
        world.advance();
        let state = world.unit(unit).expect("unit");
        assert!(state.moving && !state.planning);
    }
    assert!(world.unit(unit).expect("unit").position.x < turned_at.x);
}

#[test]
fn a_1600_tile_attack_order_succeeds_and_the_attacker_walks_toward_the_target() {
    let mut world = wide_world();
    let attacker = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, center(100, 100))
        .expect("attacker");
    let target = world
        .spawn_kind(PlayerId(1), crate::UnitKind::Militia, center(1_700, 100))
        .expect("target");
    world
        .issue_attack(PlayerId(0), attacker, target)
        .expect("a far attack needs no local firing-position proof");
    let order = world.movement_order(attacker).expect("walking order");
    let destination = order.destination.tile_floor();
    assert!(
        (1_697..=1_700).contains(&destination.x) && (99..=101).contains(&destination.y),
        "{destination:?}"
    );
    let before = world.unit(attacker).expect("attacker").position;
    for _ in 0..40 {
        world.advance();
    }
    let after = world.unit(attacker).expect("attacker");
    assert!(after.position.x > before.x && after.attack_target == Some(target));
    // Same inputs, same order: the fallback is deterministic.
    let mut twin = wide_world();
    let a = twin
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, center(100, 100))
        .expect("attacker");
    let t = twin
        .spawn_kind(PlayerId(1), crate::UnitKind::Militia, center(1_700, 100))
        .expect("target");
    twin.issue_attack(PlayerId(0), a, t).expect("twin attack");
    assert_eq!(twin.movement_order(a), Some(order));
}

#[test]
fn a_1600_tile_attack_on_a_town_center_succeeds() {
    let mut world = wide_world();
    world
        .initialize_village(PlayerId(1), TileCoord::new(1_700, 200))
        .expect("enemy village");
    let town_center = world
        .buildings()
        .find(|building| {
            building.player == PlayerId(1) && building.kind == crate::BuildingKind::TownCenter
        })
        .map(|building| building.id)
        .expect("town center");
    let attacker = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, center(100, 200))
        .expect("attacker");
    let result = world.issue_attack(PlayerId(0), attacker, town_center);
    assert_eq!(result, Ok(()));
    assert!(world.movement_order(attacker).is_some());
}

#[test]
fn a_unit_crosses_3900_tiles_and_takes_the_distant_river_gateway() {
    let mut water = vec![0; 64 * 64];
    for y in 0..64 {
        if y != 5 {
            water[y * 64 + 32] = 100;
        }
    }
    let mut world = wide_world();
    world.terrain = prepared_terrain(water, vec![BARREN; 64 * 64]);
    if let Terrain::Map { overlay, .. } = &mut world.terrain {
        overlay
            .add_footprint(TileRect::from_xywh(700, 1_200, 3, 500))
            .unwrap();
    }
    world.prepare_navigation(&|| false).unwrap();
    let unit = world
        .spawn_kind(PlayerId(0), UnitKind::Scout, center(100, 2_000))
        .unwrap();
    let target = center(4_000, 2_000);
    world.issue_move(unit, target).unwrap();
    let mut used_gateway = false;
    let mut pause = 0;
    let mut longest_pause = 0;
    let mut blocked_while_moving = false;
    let mut previous_position = world.unit(unit).unwrap().position;
    for tick in 0..100_000 {
        world.advance();
        if tick >= 1_000 && !blocked_while_moving {
            let index = *world.lookup.get(&unit).unwrap();
            if let Some(tile) = world.units[index].route.front().copied()
                && tile != world.unit(unit).unwrap().position.tile_floor()
            {
                let footprint = TileRect::from_xywh(tile.x, tile.y, 1, 1);
                if let Terrain::Map { overlay, .. } = &mut world.terrain {
                    overlay.add_footprint(footprint).unwrap();
                }
                world.collision_closed(footprint);
                blocked_while_moving = true;
            }
        }
        let state = world.unit(unit).unwrap();
        pause = if state.position == previous_position {
            pause + 1
        } else {
            0
        };
        previous_position = state.position;
        longest_pause = longest_pause.max(pause);
        assert!(
            longest_pause <= 20,
            "movement pause exceeded 20 ticks at {tick}: {longest_pause}"
        );
        used_gateway |= state.position.tile_floor().y < 400;
        assert_eq!(world.movement_failure(unit), None, "stopped on tick {tick}");
        if world.movement_order(unit).is_none() {
            break;
        }
    }
    assert_eq!(world.unit(unit).unwrap().position, target);
    assert!(used_gateway);
    assert!(blocked_while_moving);
    // Close the river gateway after the first trip. The same warm map must
    // update its affected chunks and certify the exact closest endpoint.
    // Span the map so bilinear shore transitions cannot leave a crossing
    // just outside the nominal source-water sample's gateway rectangle.
    let closed = TileRect::from_xywh(2_080, 0, 1, WIDTH);
    if let Terrain::Map { overlay, .. } = &mut world.terrain {
        overlay.add_footprint(closed).unwrap();
    }
    world.collision_closed(closed);
    let unreachable = TileCoord::new(100, 2_000);
    let Terrain::Map { generator, overlay } = &world.terrain else {
        unreachable!()
    };
    let nearest = generator
        .navigation_destination(overlay, target.tile_floor(), unreachable, &|| false)
        .unwrap()
        .unwrap();
    assert_ne!(nearest, unreachable);
    world
        .issue_move(unit, center(unreachable.x, unreachable.y))
        .unwrap();
    pause = 0;
    previous_position = world.unit(unit).unwrap().position;
    for tick in 0..100_000 {
        world.advance();
        let state = world.unit(unit).unwrap();
        pause = if state.position == previous_position {
            pause + 1
        } else {
            0
        };
        previous_position = state.position;
        let index = *world.lookup.get(&unit).unwrap();
        assert!(
            pause <= 20,
            "nearest-point trip paused at {tick}: position={:?}, nearest={nearest:?}, order={:?}, route_len={}, planner={:?}, scheduler={:?}",
            state.position,
            world.movement_order(unit),
            world.units[index].route.len(),
            world.units[index].planner.as_ref().map(|p| (
                p.work(),
                p.leg_work(),
                p.certified_destination(),
                p.is_terminal(),
                p.requires_search_slot(),
                p.wants_lookahead()
            )),
            world.route_scheduler_stats()
        );
        if world.movement_order(unit).is_none() {
            break;
        }
    }
    assert_eq!(world.unit(unit).unwrap().position.tile_floor(), nearest);
    assert_eq!(
        world.movement_failure(unit),
        Some(GameWorldError::Unreachable)
    );
    assert_eq!(world.route_scheduler_stats().waiting_units, 0);
    assert_eq!(world.route_accounting_error(), None);
}
