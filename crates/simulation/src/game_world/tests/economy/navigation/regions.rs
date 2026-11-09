//! Phase 3 (regions and reachability) scenarios: orders start in the army's
//! own region and end at the nearest reachable point, open gates admit
//! everyone, and toggling a gate relabels nothing.
use super::gate_routes::fort;
use super::*;
use aoe_core::Formation;

fn militia(world: &mut GameWorld, player: u16, tiles: &[(i32, i32)]) -> Vec<EntityId> {
    tiles
        .iter()
        .map(|&(x, y)| {
            world
                .spawn_militia(
                    PlayerId(player),
                    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap(),
                )
                .unwrap()
        })
        .collect()
}

fn block(x: i32, y: i32, count: i32) -> Vec<(i32, i32)> {
    (0..count).map(|n| (x + n % 5, y + n / 5)).collect()
}

fn enemy_enters_open_gate() -> [u8; 32] {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let o = TileCoord::new(100, 100);
    let gate = fort(&mut world, o, false)[0];
    advance(&mut world, 2);
    let outside = TileCoord::new(o.x + 5, o.y + 20);
    let inside = TileCoord::new(o.x + 5, o.y + 4);
    let destination = WorldPosition::from_tile_center(inside).unwrap();
    let army = militia(
        &mut world,
        1,
        &[(outside.x, outside.y), (outside.x + 1, outside.y)],
    );
    for &id in &army {
        world.units[world.lookup[&id]].state.stance = aoe_core::CombatStance::NoAttack;
    }
    let yard = aoe_core::TileRect::from_xywh(o.x, o.y, 11, 9);
    let check_regions = |world: &GameWorld, open| {
        for bucket in aoe_map::LandBucket::ALL {
            assert_eq!(
                world
                    .land_reachable(PlayerId(1), bucket, outside, inside)
                    .unwrap(),
                open,
                "enemy reachability, {bucket:?}, open={open}"
            );
            let frontier = world
                .land_frontier(PlayerId(1), bucket, outside, inside)
                .unwrap()
                .unwrap();
            assert_eq!(yard.contains(frontier), open);
            if open {
                assert_eq!(frontier, inside);
            }
            let from = world
                .land_player_region(PlayerId(1), bucket, outside)
                .unwrap()
                .unwrap();
            let to = world
                .land_player_region(PlayerId(1), bucket, inside)
                .unwrap()
                .unwrap();
            assert_eq!(from == to, open, "enemy components, {bucket:?}");
            assert!(
                world
                    .land_reachable(PlayerId(0), bucket, outside, inside)
                    .unwrap()
            );
        }
    };
    check_regions(&world, false);
    world
        .issue_formation_move(
            PlayerId(1),
            &army,
            Formation::Line,
            destination,
            false,
            false,
        )
        .unwrap();
    assert!(!world.formation_matches(&army, destination));
    assert!(
        army.iter()
            .all(|&id| !yard.contains(world.formation_destination_slot(id).unwrap().tile_floor()))
    );
    let before = world.region_stats();
    // A defender beside the only entrance holds it physically open.
    let defender = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(o.x + 3, o.y + 7)).unwrap(),
        )
        .unwrap();
    world.units[world.lookup[&defender]].state.stance = aoe_core::CombatStance::NoAttack;
    advance(&mut world, 2);
    assert!(world.building(gate).unwrap().gate_open);
    check_regions(&world, true);
    world
        .issue_formation_move(
            PlayerId(1),
            &army,
            Formation::Line,
            destination,
            false,
            false,
        )
        .unwrap();
    assert!(world.formation_matches(&army, destination));
    advance(&mut world, 300);
    assert!(
        army.iter()
            .all(|&id| yard.contains(world.unit(id).unwrap().position.tile_floor()))
    );
    assert!(world.building(gate).unwrap().gate_open);
    // Closing it must discard the enemy's cached joined components too.
    world
        .issue_move(
            defender,
            WorldPosition::from_tile_center(TileCoord::new(o.x + 2, o.y + 2)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 100);
    assert!(!world.building(gate).unwrap().gate_open);
    check_regions(&world, false);
    assert_eq!(world.region_stats().relabels, before.relabels);
    assert_eq!(world.region_stats().global_rebuilds, before.global_rebuilds);
    world.canonical_hash()
}

#[test]
fn an_enemy_formation_enters_a_gate_held_open_by_a_defender() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(enemy_enters_open_gate)
    };
    assert_eq!(run(1), run(4));
}

/// The enemy army of the walled-base scenario after 30 s: members inside the
/// fort, members still moving (pressers), and the state hash.
fn enemy_into_walled_base() -> (usize, usize, [u8; 32]) {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let o = TileCoord::new(100, 100);
    fort(&mut world, o, true);
    advance(&mut world, 2);
    let army = militia(&mut world, 1, &block(o.x + 3, o.y + 30, 20));
    let inside = WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 4)).unwrap();
    world
        .issue_formation_move(PlayerId(1), &army, Formation::Line, inside, false, false)
        .unwrap();
    advance(&mut world, 600);
    let rect = aoe_core::TileRect::from_xywh(o.x, o.y, 11, 9);
    let units: Vec<_> = army.iter().map(|&id| world.unit(id).unwrap()).collect();
    for u in units.iter().filter(|u| u.moving) {
        let goal = world.units[world.lookup[&u.id]].formation;
        eprintln!(
            "DEBUG presser {:?} at {:?} group {:?} slot {:?} anchor {:?} obstruction {:?} order {:?}",
            u.id,
            u.position,
            goal,
            world.formation_destination_slot(u.id),
            world.formation_anchor(u.id),
            world.movement_obstruction_ticks(u.id),
            world.movement_order(u.id)
        );
    }
    (
        units
            .iter()
            .filter(|u| rect.contains(u.position.tile_floor()))
            .count(),
        units.iter().filter(|u| u.moving).count(),
        world.canonical_hash(),
    )
}

#[test]
fn an_enemy_army_ordered_into_a_walled_base_settles_outside_with_no_pressers() {
    let (inside, pressers, hash) = enemy_into_walled_base();
    eprintln!("walled base after 30 s: inside={inside} pressers={pressers}");
    assert_eq!(inside, 0, "nobody entered through the closed gates");
    assert_eq!(pressers, 0, "the army settled at its frontier");
    assert_eq!(hash, enemy_into_walled_base().2, "deterministic");
}

#[test]
fn a_spread_selection_centred_in_a_sealed_pocket_still_marches() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    // A sealed 5 x 5 yard ringed by walls at (126..131, 156..161).
    for i in 0..7 {
        for (x, y) in [
            (125 + i, 155),
            (125 + i, 161),
            (125, 155 + i),
            (131, 155 + i),
        ] {
            world
                .insert_building(
                    PlayerId(0),
                    BuildingKind::StoneWall,
                    aoe_core::TileRect::from_xywh(x, y, 1, 1),
                    true,
                )
                .unwrap();
        }
    }
    let mut tiles = block(98, 157, 10);
    tiles.extend(block(154, 157, 10));
    let army = militia(&mut world, 0, &tiles);
    let probe = world
        .probe_group(PlayerId(0), &army, Formation::Line)
        .unwrap();
    let yard = aoe_core::TileRect::from_xywh(125, 155, 7, 7);
    assert!(
        !yard.contains(probe.origin.tile_floor()),
        "the order starts in the army's region, not the yard: {:?}",
        probe.origin
    );
    let goal = WorldPosition::from_tile_center(TileCoord::new(128, 220)).unwrap();
    world
        .issue_formation_move(PlayerId(0), &army, Formation::Line, goal, false, false)
        .unwrap();
    let distance = |world: &GameWorld| {
        army.iter()
            .map(|&id| i64::from((world.unit(id).unwrap().position.y - goal.y).abs()))
            .sum::<i64>()
            / army.len() as i64
    };
    let before = distance(&world);
    advance(&mut world, 400);
    let after = distance(&world);
    eprintln!("spread selection: mean distance {before} -> {after} subunits");
    assert!(
        after + 10 * 1_024 < before,
        "the army marched: {before} -> {after}"
    );
}

#[test]
fn a_thousand_owner_gate_toggles_relabel_no_region() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let o = TileCoord::new(100, 100);
    let gates = fort(&mut world, o, true);
    advance(&mut world, 2);
    let own = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 20)).unwrap(),
        )
        .unwrap();
    let inside = TileCoord::new(o.x + 5, o.y + 4);
    let east = TileCoord::new(o.x + 22, o.y + 4);
    for bucket in aoe_map::LandBucket::ALL {
        assert!(
            world
                .land_reachable(
                    PlayerId(0),
                    bucket,
                    TileCoord::new(o.x + 5, o.y + 20),
                    inside
                )
                .unwrap()
        );
        assert!(
            !world
                .land_reachable(
                    PlayerId(1),
                    bucket,
                    TileCoord::new(o.x + 5, o.y + 20),
                    inside
                )
                .unwrap()
        );
    }
    let before = world.region_stats();
    let mut toggles = 0;
    let mut open: Vec<bool> = gates
        .iter()
        .map(|g| world.building(*g).unwrap().gate_open)
        .collect();
    for to in [inside, east, inside, east].into_iter().cycle().take(1_000) {
        world
            .issue_move(own, WorldPosition::from_tile_center(to).unwrap())
            .unwrap();
        for _ in 0..900 {
            world.advance();
            for (state, gate) in open.iter_mut().zip(&gates) {
                let now = world.building(*gate).unwrap().gate_open;
                toggles += usize::from(now != *state);
                *state = now;
            }
            // Every toggle is followed by a query for each radius bucket.
            for bucket in aoe_map::LandBucket::ALL {
                assert!(
                    world
                        .land_reachable(PlayerId(0), bucket, east, inside)
                        .unwrap()
                );
            }
            if world.unit(own).unwrap().position.tile_floor() == to {
                break;
            }
        }
        if toggles >= 1_000 {
            break;
        }
    }
    let after = world.region_stats();
    eprintln!(
        "gate toggles={toggles} relabels={} unchanged={}",
        after.relabels - before.relabels,
        after.unchanged - before.unchanged
    );
    assert!(toggles >= 1_000, "the gates opened and closed: {toggles}");
    assert_eq!(after.relabels, before.relabels);
    assert_eq!(after.global_rebuilds, before.global_rebuilds);
}

#[test]
fn region_orders_replay_identically_with_one_and_four_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world =
                    GameWorld::new(WorldConfig::new(512, 512, Seed(73)).unwrap()).unwrap();
                world.set_fog_disabled(true);
                for n in 0..4 {
                    let o = TileCoord::new(100 + n % 2 * 200, 100 + n / 2 * 200);
                    fort(&mut world, o, true);
                    let army = militia(&mut world, 1, &block(o.x + 3, o.y + 30, 80));
                    let target =
                        WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 4)).unwrap();
                    world
                        .issue_formation_move(
                            PlayerId(1),
                            &army,
                            Formation::Line,
                            target,
                            false,
                            false,
                        )
                        .unwrap();
                }
                // 320 members cross the 256-member parallel proposal threshold.
                let mut hashes = Vec::new();
                for _ in 0..600 {
                    world.advance();
                    hashes.push(world.canonical_hash());
                }
                hashes
            })
    };
    assert_eq!(run(1), run(4));
}
