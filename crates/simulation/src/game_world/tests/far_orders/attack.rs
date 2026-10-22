//! Far attack orders: edge targets, sealed pockets, and bounded repeat work.
use super::*;
use super::{WIDTH, center, wide_world};
use crate::UnitKind;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, WaterPage, ordered_biome_page_root, ordered_page_root,
    ordered_water_page_root,
};

const AXIS: u16 = 64;
/// The water ring occupies the border of the sample block `RING..=RING + 6`,
/// sealing a land pocket of about 300 x 300 tiles around tile (2144, 2144).
const RING: usize = 30;

/// Flat barren ground with a sealed pocket enclosed by water.
fn walled_world() -> GameWorld {
    let (mut elevations, mut waters, mut biomes) = (Vec::new(), Vec::new(), Vec::new());
    let (mut e_levels, mut w_levels, mut b_levels) = (Vec::new(), Vec::new(), Vec::new());
    let (mut axis, mut level) = (AXIS, 0_u8);
    loop {
        let side = axis as u8;
        let count = usize::from(axis) * usize::from(axis);
        let elevation = ElevationPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            geographic_height_centimeters: vec![0; count],
        };
        let mut ocean = vec![0; count];
        if level == 0 {
            for row in RING..=RING + 6 {
                for column in RING..=RING + 6 {
                    let edge = [row, column].iter().any(|&n| n == RING || n == RING + 6);
                    if edge {
                        ocean[row * usize::from(axis) + column] = 100;
                    }
                }
            }
        }
        let water = WaterPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            ocean_coverage_percent: ocean,
            inland_coverage_percent: vec![0; count],
        };
        let biome = PotentialBiomePage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            potential_biome_class: vec![28; count],
        };
        let level_of = |root| PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: root,
        };
        e_levels.push(level_of(
            ordered_page_root(std::slice::from_ref(&elevation)).expect("elevation root"),
        ));
        w_levels.push(level_of(
            ordered_water_page_root(std::slice::from_ref(&water)).expect("water root"),
        ));
        b_levels.push(level_of(
            ordered_biome_page_root(std::slice::from_ref(&biome)).expect("biome root"),
        ));
        elevations.push(elevation);
        waters.push(water);
        biomes.push(biome);
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
        level += 1;
    }
    let environment = PreparedEnvironment {
        samples_per_axis: AXIS,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid { levels: e_levels },
        water: Some(FieldPyramid { levels: w_levels }),
        vegetation: Some(FieldPyramid { levels: b_levels }),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let generator = MapChunkGenerator::new([0; 32], 0, WIDTH)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            elevations,
        )
        .expect("flat elevation")
        .with_prepared_water(&environment, waters)
        .expect("wall coverage")
        .with_prepared_biomes(&environment, biomes)
        .expect("barren biome");
    let mut world = wide_world();
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    world
}

fn militia(world: &mut GameWorld, player: u16, x: i32, y: i32) -> EntityId {
    world
        .spawn_kind(PlayerId(player), UnitKind::Militia, center(x, y))
        .expect("militia")
}

#[test]
fn a_far_attack_on_a_target_at_the_west_or_north_edge_is_accepted() {
    for (target, from) in [((2, 700), (1_700, 700)), ((700, 1), (700, 1_700))] {
        let mut world = wide_world();
        let attacker = militia(&mut world, 0, from.0, from.1);
        let victim = militia(&mut world, 1, target.0, target.1);
        assert_eq!(world.issue_attack(PlayerId(0), attacker, victim), Ok(()));
        let goal = world.movement_order(attacker).expect("walking").destination;
        let tile = goal.tile_floor();
        assert!(tile.x >= 0 && tile.y >= 0, "{tile:?}");
        assert!((tile.x - target.0).abs() <= 3 && (tile.y - target.1).abs() <= 3);
    }
}

#[test]
fn a_far_attack_from_a_sealed_pocket_is_rejected_not_walked() {
    let mut world = walled_world();
    let attacker = militia(&mut world, 0, 2_144, 2_144);
    let victim = militia(&mut world, 1, 2_500, 2_144);
    let error = world
        .issue_attack(PlayerId(0), attacker, victim)
        .expect_err("the portal graph proves the pocket sealed off");
    assert_eq!(error, EconomyError::NoAttackPosition);
    assert!(world.movement_order(attacker).is_none());
    // A target inside the same pocket is reachable and accepted.
    let mut open = walled_world();
    let near = militia(&mut open, 0, 2_144, 2_144);
    let friend_foe = militia(&mut open, 1, 2_144, 2_000);
    assert_eq!(open.issue_attack(PlayerId(0), near, friend_foe), Ok(()));
}

fn far_attackers(count: i32) -> (GameWorld, Vec<EntityId>) {
    let mut world = wide_world();
    let mut ids = Vec::new();
    for row in 0..count {
        let attacker = militia(&mut world, 0, 100, 100 + 3 * row);
        let victim = militia(&mut world, 1, 1_700, 100 + 3 * row);
        world
            .issue_attack(PlayerId(0), attacker, victim)
            .expect("far attack");
        ids.push(attacker);
    }
    (world, ids)
}

#[test]
fn a_far_firing_tile_is_found_once_and_reused_while_the_target_stays_put() {
    let (mut world, ids) = far_attackers(3);
    for _ in 0..20 {
        world.advance();
    }
    assert!(
        ids.iter()
            .all(|id| world.combat.attackers[id].far.is_some())
    );
    let orders = ids
        .iter()
        .map(|id| world.movement_order(*id))
        .collect::<Vec<_>>();
    for _ in 0..40 {
        world.advance();
        assert_eq!(
            world.combat.far_searches,
            crate::game_world::combat::MAX_FAR_SEARCHES_PER_TICK,
            "a reused firing tile costs no search budget"
        );
    }
    for (id, order) in ids.iter().zip(orders) {
        assert_eq!(
            world.movement_order(*id).map(|o| o.destination),
            order.map(|o| o.destination)
        );
    }
}

#[test]
fn repeated_far_searches_are_capped_per_tick_and_deferred_ones_retry_next_tick() {
    let (mut world, ids) = far_attackers(10);
    let stale = TileCoord::new(0, 0);
    for id in &ids {
        let entry = world.combat.attackers.get_mut(id).expect("attacker");
        entry.far = Some(crate::game_world::combat::FarAttack {
            centre: stale,
            destination: center(0, 0),
        });
        entry.repath_tick = 0;
    }
    world.advance();
    let now = world.tick.0;
    let deferred = |world: &GameWorld| {
        ids.iter()
            .filter(|id| world.combat.attackers[*id].repath_tick == now + 1)
            .count()
    };
    assert_eq!(deferred(&world), 6, "only four searches fit in one tick");
    let retry = ids
        .iter()
        .copied()
        .filter(|id| world.combat.attackers[id].repath_tick == now + 1)
        .collect::<Vec<_>>();
    let searched = |world: &GameWorld, id: &EntityId| {
        world.combat.attackers[id]
            .far
            .is_some_and(|far| far.centre != stale)
    };
    assert!(
        retry.iter().all(|id| !searched(&world, id)),
        "a deferred attacker was not searched yet"
    );
    world.advance();
    // The four free searches of the next tick go to deferred attackers.
    assert_eq!(
        retry.iter().filter(|id| searched(&world, id)).count(),
        4,
        "deferred attackers are searched on the next tick"
    );
}

#[test]
fn first_far_searches_count_against_the_per_tick_budget() {
    let (mut world, ids) = far_attackers(10);
    for id in &ids {
        let entry = world.combat.attackers.get_mut(id).expect("attacker");
        entry.far = None;
        entry.repath_tick = 0;
    }
    world.advance();
    let searched = ids
        .iter()
        .filter(|id| world.combat.attackers[*id].far.is_some())
        .count();
    assert_eq!(searched, 4, "first searches are capped like repeated ones");
    world.advance();
    let searched = ids
        .iter()
        .filter(|id| world.combat.attackers[*id].far.is_some())
        .count();
    assert_eq!(searched, 8, "the deferred ones follow on the next tick");
}
