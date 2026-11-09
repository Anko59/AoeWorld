use super::build::Shape;
use super::{divergence, run};

#[test]
fn side_keeps_the_owner_density() {
    let shape = |seats, scale| Shape {
        seats,
        scale,
        side_tiles: 0,
        seed: 1,
    };
    assert_eq!(shape(100, 1.0).side(), 2_000);
    assert_eq!(shape(100, 4.0).side(), 4_000);
    assert_eq!(shape(1_000, 1.0).side(), 6_324);
    assert_eq!(shape(4, 0.01).side(), 256);
    let fixed = Shape {
        side_tiles: 900,
        ..shape(100, 1.0)
    };
    assert_eq!(fixed.side(), 900);
}

#[test]
fn divergence_names_the_first_differing_tick() {
    let a = vec![
        (0, "x".to_owned()),
        (10, "y".to_owned()),
        (20, "z".to_owned()),
    ];
    let mut b = a.clone();
    assert_eq!(divergence(&a, &b), None);
    b[2].1 = "w".to_owned();
    assert_eq!(
        divergence(&a, &b),
        Some((20, "z".to_owned(), "w".to_owned()))
    );
}

/// A small closed-loop match replays to the same hashes on 1, 2 and 4 workers.
#[test]
fn small_match_is_deterministic_across_workers() {
    let plan = |threads| run::Plan {
        shape: Shape {
            seats: 4,
            scale: 0.2,
            side_tiles: 400,
            seed: 3,
        },
        threads,
        ticks: 60,
        hash_every: 20,
        sample_every: 0,
    };
    let one = run::run(&plan(1)).expect("one worker");
    let two = run::run(&plan(2)).expect("two workers");
    let four = run::run(&plan(4)).expect("four workers");
    assert_eq!(one.hashes.len(), 4);
    assert_eq!(one.hashes, two.hashes);
    assert_eq!(one.hashes, four.hashes);
    assert!(one.report["setup"]["units"].as_u64().unwrap_or(0) > 20);
    for report in [&one.report, &two.report] {
        for phase in ["index_collision", "index_chunks"] {
            let summary = &report["phases"][phase];
            assert_eq!(summary["samples"], 60, "missing per-tick {phase} timings");
            assert!(summary["p50_ms"].as_f64().is_some_and(f64::is_finite));
        }
    }
}

#[test]
fn unequal_hash_lengths_are_not_deterministic_in_either_order() {
    use serde_json::json;
    let short = json!({"threads": 1, "hashes": [[0, "a"], [100, "b"]]});
    let long = json!({"threads": 2, "hashes": [[0, "a"], [100, "b"], [200, "c"]]});
    for runs in [[short.clone(), long.clone()], [long, short]] {
        let verdict = super::determinism(&runs);
        assert_eq!(verdict["all_equal"], false, "{verdict}");
        assert_eq!(verdict["against"][0]["first_divergent_tick"], 200);
    }
}

#[test]
fn two_seat_ai_wave_targets_the_opponent() {
    use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};
    use aoe_simulation::{GameWorld, UnitKind};
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    // Seat 1's first wave is tick 1060 (offset 140). Spawn after advancing
    // so the soldiers are idle when the timer fires.
    for _ in 0..1060 {
        world.advance();
    }
    let soldiers: Vec<_> = (0..10)
        .map(|n| {
            world
                .spawn_kind(
                    PlayerId(1),
                    UnitKind::Militia,
                    WorldPosition::from_tile_center(TileCoord::new(100 + n, 100)).unwrap(),
                )
                .unwrap()
        })
        .collect();
    let seats: Vec<_> = [TileCoord::new(50, 50), TileCoord::new(100, 100)]
        .into_iter()
        .enumerate()
        .map(|(n, center)| super::build::Seat {
            id: PlayerId(n as u16),
            center,
            town_center: None,
            target: 0,
        })
        .collect();
    let mut counts = super::bots::BotCounts::default();
    super::bots::step(&mut world, &seats, &[], &mut counts);
    assert_eq!(counts.waves_ok, 1);
    assert_eq!(
        world.move_destinations(soldiers[0]),
        vec![WorldPosition::from_tile_center(seats[0].center).unwrap()]
    );
}

#[test]
fn missing_stewards_cannot_silently_change_the_workload() {
    // Small worlds exercise the same blocked spawn as the 1,000-seat setup.
    let shape = Shape {
        seats: 4,
        scale: 0.2,
        side_tiles: 400,
        seed: 1,
    };
    let mut built = super::build::build(&shape).expect("all four required Stewards");
    assert_eq!(built.stewards, 4);
    // Successful orders must actually enter the Town Center on the first tick,
    // rather than depending on an unreachable route from a fixed offset.
    built.world.advance();
    for seat in &built.seats {
        let town_center = built.world.building(seat.town_center.unwrap()).unwrap();
        assert_eq!(
            town_center
                .garrison
                .iter()
                .filter(|g| { g.unit.kind == aoe_simulation::UnitKind::Steward })
                .count(),
            1,
            "seat {:?}",
            seat.id
        );
    }
}

fn options() -> super::Options {
    super::Options {
        seats: 4,
        scale: vec![0.2],
        side_tiles: 400,
        ticks: 200,
        threads: vec![1],
        seed: 1,
        hash_every: 100,
        sample_every: 100,
        out: ".cache/test.json".into(),
        callgrind: true,
        warmup: 100,
        counted: 3,
        counted_child: None,
        compare: vec![],
        suite: None,
        determinism_ticks: 3000,
        no_work_sleep: false,
    }
}

#[test]
fn callgrind_cache_binds_every_option_and_executable_contents() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("harness");
    std::fs::write(&exe, "executable revision one").unwrap();
    let base = options();
    let key = super::counted::cache_key(&base, 0.2, &exe).unwrap();
    assert_eq!(key, super::counted::cache_key(&base, 0.2, &exe).unwrap());
    macro_rules! changed {
        ($field:ident, $value:expr) => {{
            let other = super::Options {
                $field: $value,
                ..base.clone()
            };
            assert_ne!(
                key,
                super::counted::cache_key(&other, 0.2, &exe).unwrap(),
                "cache ignored {}",
                stringify!($field)
            );
        }};
    }
    changed!(seats, 2);
    changed!(scale, vec![0.4]);
    changed!(side_tiles, 500);
    changed!(ticks, 201);
    changed!(threads, vec![2]);
    changed!(seed, 2);
    changed!(hash_every, 50);
    changed!(sample_every, 50);
    changed!(out, "other.json".into());
    changed!(callgrind, false);
    changed!(warmup, 200);
    changed!(counted, 5);
    changed!(counted_child, Some("child.json".into()));
    changed!(compare, vec!["report.json".into()]);
    changed!(suite, Some("phase0".into()));
    changed!(determinism_ticks, 4000);
    changed!(no_work_sleep, true);
    assert_ne!(key, super::counted::cache_key(&base, 0.4, &exe).unwrap());
    std::fs::write(&exe, "executable revision two").unwrap();
    assert_ne!(key, super::counted::cache_key(&base, 0.2, &exe).unwrap());
}

/// The economy-sleep oracle on the closed-loop match: bots, farms, fights
/// and Generals with sleep on and off agree on every tenth tick's hash.
#[test]
fn small_match_hashes_alike_with_work_sleep_on_and_off() {
    let plan = run::Plan {
        shape: Shape {
            seats: 4,
            scale: 0.2,
            side_tiles: 400,
            seed: 3,
        },
        threads: 1,
        ticks: 400,
        hash_every: 10,
        sample_every: 0,
    };
    let on = run::run(&plan).expect("sleep on");
    super::WORK_SLEEP.store(false, std::sync::atomic::Ordering::Relaxed);
    let off = run::run(&plan).expect("sleep off");
    super::WORK_SLEEP.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(on.hashes.len(), 41);
    assert_eq!(divergence(&on.hashes, &off.hashes), None);
}
