//! Telemetry follows resolved damage and survives entity cleanup.
use super::*;

#[test]
fn melee_telemetry_retains_kills_after_cleanup_and_expires_an_idle_minute() {
    let (mut world, attacker, victim) = duel();
    world.units[world.lookup[&victim]].state.hp = 1;
    world.issue_attack(PlayerId(0), attacker, victim).unwrap();
    world.advance();
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (1, 0, 1)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
    assert_eq!(world.player_combat_totals(PlayerId(99)), (0, 0));
    advance(&mut world, 650);
    assert!(
        world.unit(victim).is_none(),
        "corpse cleanup must precede the minute boundary"
    );
    assert_eq!(world.combat_telemetry().deaths, 1);
    advance(&mut world, 600);
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (0, 0, 0)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
    assert_eq!(world.player_projection_totals(PlayerId(1)).1, 1);
}

#[test]
fn ranged_telemetry_counts_impact_once_and_never_counts_a_missed_arrow() {
    let mut world = world();
    let archer = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Archer, at(100, 100))
        .unwrap();
    let victim = world.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    world.units[world.lookup[&victim]].state.hp = 1;
    world.issue_attack(PlayerId(0), archer, victim).unwrap();
    world.advance();
    assert_eq!(world.projectiles().count(), 1);
    assert_eq!(
        world.combat_telemetry().ranged_hits,
        0,
        "launch is not a hit"
    );
    advance(&mut world, 12);
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (0, 1, 1)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    let other = world.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    world.issue_attack(PlayerId(0), archer, other).unwrap();
    ticks_until(&mut world, 100, |w| w.projectiles().count() == 1);
    world.remove_unit(other);
    advance(&mut world, 20);
    assert_eq!(
        world.combat_telemetry().ranged_hits,
        1,
        "an absent target takes no damage"
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
}

#[test]
fn telemetry_counts_destroyed_buildings_but_excludes_wildlife() {
    let mut world = world();
    let attacker = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let building = world
        .place_town_center(PlayerId(1), TileCoord::new(101, 100))
        .unwrap();
    world.building_mut(building).unwrap().hp = 1;
    world.issue_attack(PlayerId(0), attacker, building).unwrap();
    world.advance();
    assert!(world.building(building).is_none());
    assert_eq!(world.combat_telemetry().deaths, 1);
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    let (buildings, unit_losses, last) = world.player_projection_totals(PlayerId(0));
    assert_eq!((buildings, unit_losses), (1, 0));
    let (target, killer, tile, _) = last.unwrap();
    assert_eq!(
        (target, killer, tile),
        (building, attacker, TileCoord::new(101, 100))
    );
    assert_eq!(
        world.player_projection_totals(PlayerId(1)).1,
        0,
        "a building is not a lost unit"
    );
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
    let before = world.combat_telemetry();
    let sheep = world
        .spawn_animal(crate::AnimalKind::Sheep, at(99, 100))
        .unwrap();
    world.issue_attack(PlayerId(0), attacker, sheep).unwrap();
    ticks_until(&mut world, 100, |w| w.animal(sheep).unwrap().hp == 0);
    let after = world.combat_telemetry();
    assert_eq!(
        (after.melee_hits, after.ranged_hits, after.deaths),
        (before.melee_hits, before.ranged_hits, before.deaths)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
}

#[test]
fn wildlife_killing_a_player_unit_counts_hit_death_and_loss_without_player_kill_credit() {
    for kind in [crate::AnimalKind::Wolf, crate::AnimalKind::Boar] {
        let mut world = world();
        let victim = world.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        world.units[world.lookup[&victim]].state.hp = 1;
        let animal = world.spawn_animal(kind, at(101, 100)).unwrap();
        if kind == crate::AnimalKind::Boar {
            world.issue_hunt(PlayerId(0), &[victim], animal).unwrap();
        }
        ticks_until(&mut world, 500, |w| w.unit(victim).unwrap().hp == 0);
        let counts = world.combat_telemetry();
        assert_eq!(
            (counts.melee_hits, counts.ranged_hits, counts.deaths),
            (1, 0, 1),
            "{kind:?} dealt one lethal hit to a player unit"
        );
        assert_eq!(world.player_combat_totals(PlayerId(0)), (0, 1));
        assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 0));
        advance(&mut world, 650);
        assert!(world.unit(victim).is_none());
        assert_eq!(world.player_combat_totals(PlayerId(0)), (0, 1));
    }
}

/// Measure authoritative circles, not overlapping sprite rectangles. Counts
/// include only living pairs whose two members currently hold attack jobs.
#[derive(Debug, Default)]
struct FightOverlap {
    samples: u64,
    overlaps: u64,
    deepest_subunits: u64,
    minimum_distance: Option<u64>,
}

fn sample_fighting_pairs(
    world: &GameWorld,
    friendly: bool,
    stationary: bool,
    out: &mut FightOverlap,
) {
    let units = world
        .units()
        .filter(|u| {
            u.hp > 0
                && u.attack_target.is_some()
                && (!stationary || (u.task == UnitTask::Attacking && !u.moving))
        })
        .collect::<Vec<_>>();
    for (i, a) in units.iter().enumerate() {
        for b in &units[i + 1..] {
            if (a.player == b.player) != friendly {
                continue;
            }
            let dx = i64::from(a.position.x) - i64::from(b.position.x);
            let dy = i64::from(a.position.y) - i64::from(b.position.y);
            let squared = (dx * dx + dy * dy) as u64;
            let distance = squared.isqrt();
            let radii = (a.kind.collision_radius() + b.kind.collision_radius()) as u64;
            out.samples += 1;
            out.minimum_distance = Some(out.minimum_distance.map_or(distance, |d| d.min(distance)));
            if squared < radii * radii {
                out.overlaps += 1;
                out.deepest_subunits = out.deepest_subunits.max(radii - distance);
            }
        }
    }
}

fn measured_duel(kind: crate::UnitKind) -> (Vec<u64>, [u8; 32]) {
    let mut world = world();
    world.set_fog_disabled(true);
    let a = world.spawn_kind(PlayerId(0), kind, at(100, 100)).unwrap();
    let b = world.spawn_kind(PlayerId(1), kind, at(101, 100)).unwrap();
    for id in [a, b] {
        world.units[world.lookup[&id]].state.hp = 10_000;
        if kind == crate::UnitKind::General {
            let u = world.unit(id).unwrap();
            world.issue_general_move(u.player, id, u.position).unwrap();
        }
    }
    world.issue_attack(PlayerId(0), a, b).unwrap();
    world.issue_attack(PlayerId(1), b, a).unwrap();
    let mut due = 0;
    let mut hits = Vec::new();
    for _ in 0..400 {
        world.advance();
        let next = world.units[world.lookup[&a]].next_hit_tick;
        if next > due {
            hits.push(world.tick().0);
            due = next;
        }
    }
    (hits, world.canonical_hash())
}

#[test]
fn melee_fight_audit_measures_each_kind_without_changing_combat_rules() {
    let mut report = String::from(
        "20 Hz; duel=400 ticks; crowd=800 ticks, 8 versus 8, HP=10000.\n\
         Pair samples require both living members to hold attack jobs; circle overlap\n\
         is distance_squared < (radius_a+radius_b)^2; depth uses floor sqrt.\n\
         Generals use the ordinary MoveAlone order before their attack orders.\n",
    );
    for stats in UNIT_COMBAT
        .iter()
        .filter(|s| s.attack_kind == DamageKind::Melee && s.range == 1 && s.attack > 0)
    {
        let kind = stats.kind;
        let (hits, hash) = measured_duel(kind);
        assert_eq!((hits.clone(), hash), measured_duel(kind), "{kind:?}");
        assert!(hits.len() >= 3, "{kind:?}: duel never sustained combat");
        let intervals = hits.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
        let expected = u64::from(stats.attack_period_centiseconds) * 20 / 100;
        assert!(
            intervals.iter().all(|&n| n == expected),
            "{kind:?}: {hits:?}"
        );
        let mut world = world();
        world.set_fog_disabled(true);
        let mut teams = [Vec::new(), Vec::new()];
        for (p, team) in teams.iter_mut().enumerate() {
            for n in 0..8 {
                let pos = at(96 + p as i32 * 8 + n / 4, 98 + n % 4);
                let id = world.spawn_kind(PlayerId(p as u16), kind, pos).unwrap();
                world.units[world.lookup[&id]].state.hp = 10_000;
                if kind == crate::UnitKind::General {
                    world
                        .issue_general_move(PlayerId(p as u16), id, pos)
                        .unwrap();
                }
                team.push(id);
            }
        }
        for p in 0..2 {
            for (n, &id) in teams[p].iter().enumerate() {
                world
                    .issue_attack(PlayerId(p as u16), id, teams[1 - p][n])
                    .unwrap();
            }
        }
        let (mut friends, mut enemies) = (FightOverlap::default(), FightOverlap::default());
        let (mut standing_friends, mut standing_enemies) =
            (FightOverlap::default(), FightOverlap::default());
        let mut hit_ticks = 0;
        for _ in 0..800 {
            world.advance();
            hit_ticks += u32::from(world.combat_stats().hits > 0);
            sample_fighting_pairs(&world, true, false, &mut friends);
            sample_fighting_pairs(&world, false, false, &mut enemies);
            sample_fighting_pairs(&world, true, true, &mut standing_friends);
            sample_fighting_pairs(&world, false, true, &mut standing_enemies);
        }
        assert!(hit_ticks > 0, "{kind:?}: crowd never fought");
        report.push_str(&format!(
            "{kind:?}: period_ticks={expected} duel_hits={} radius={} crowd_hit_ticks={hit_ticks} friendly={friends:?} enemy={enemies:?} standing_friendly={standing_friends:?} standing_enemy={standing_enemies:?}\n",
            hits.len(), kind.collision_radius()
        ));
    }
    eprintln!("{report}");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/movement");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("fight-audit.txt"), report).unwrap();
}

#[test]
fn projection_totals_and_gameplay_replay_alike_on_one_and_four_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut w = world();
                let attacker = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
                let victim = w.spawn_villager(PlayerId(1), at(99, 100)).unwrap();
                w.units[w.lookup[&victim]].state.hp = 1;
                w.issue_attack(PlayerId(0), attacker, victim).unwrap();
                w.advance();
                let target = w
                    .place_town_center(PlayerId(1), TileCoord::new(101, 100))
                    .unwrap();
                w.building_mut(target).unwrap().hp = 1;
                w.issue_attack(PlayerId(0), attacker, target).unwrap();
                advance(&mut w, 100);
                let winner = w.player_projection_totals(PlayerId(0));
                let loser = w.player_projection_totals(PlayerId(1));
                assert_eq!((winner.0, loser.1), (1, 1));
                (w.canonical_hash(), winner, loser)
            })
    };
    assert_eq!(run(1), run(4));
}
