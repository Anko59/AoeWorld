//! Replay oracle for economy sleep: the same workload with sleep on and off
//! must agree hash for hash, and sleep must actually skip work. The full
//! workload also records interleaved per-phase times in `reports/perf`.
use super::super::*;
use crate::TickPhase;
use std::time::Instant;

/// Phase/tick diagnostics and combat work per tick, for one world.
#[derive(Default)]
struct Times {
    movement: Vec<u64>,
    combat: Vec<u64>,
    gathering: Vec<u64>,
    tick: Vec<u64>,
    combat_work: Vec<crate::CombatStats>,
}

impl Times {
    fn advance(&mut self, world: &mut GameWorld) -> Vec<crate::GameUnit> {
        let start = Instant::now();
        let mut last = start;
        let mut movement = 0;
        let mut combat = 0;
        let mut gathering = 0;
        let changed = world.advance_with_probe(&mut |_| true, &mut |phase| {
            let now = Instant::now();
            let elapsed = (now - last).as_nanos() as u64;
            match phase {
                TickPhase::Movement => movement = elapsed,
                TickPhase::Combat => combat = elapsed,
                TickPhase::Gathering => gathering = elapsed,
                _ => {}
            }
            last = now;
        });
        self.gathering.push(gathering);
        self.tick.push(start.elapsed().as_nanos() as u64);
        self.movement.push(movement);
        self.combat.push(combat);
        self.combat_work.push(world.combat_stats());
        changed
    }
}

/// Mirror the instruction kernel's population and first-1% movement orders.
fn population(scenario: aoe_scenario::Scenario) -> GameWorld {
    let config = aoe_core::WorldConfig {
        width_tiles: scenario.world_size,
        height_tiles: scenario.world_size,
        seed: scenario.seed,
        ..aoe_core::WorldConfig::default()
    };
    let mut world = GameWorld::with_population_in_extent(
        config,
        scenario.entities,
        scenario.hotspot_entities,
        scenario.players,
        scenario.active_extent,
    )
    .unwrap();
    let moving: Vec<_> = world
        .units()
        .take((scenario.entities as usize / 100).min(512))
        .map(|unit| (unit.id, unit.position))
        .collect();
    for (id, at) in moving {
        let _ = world.issue_move(
            id,
            aoe_core::WorldPosition::new(at.x.saturating_add(4_096), at.y.saturating_add(2_048)),
        );
    }
    assert_eq!(world.unit_count(), scenario.entities as usize);
    world
}

#[test]
fn population_instruction_fixtures_replay_with_sleep() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(profile_population_fixtures);
}

fn profile_population_fixtures() {
    let mut report = String::from(
        "Four measured-kernel ticks, one worker, test profile; shared-host times are diagnostic only.\n",
    );
    for scenario in [aoe_scenario::SMOKE, aoe_scenario::TARGET_DISTRIBUTED] {
        let (_, on, off) = run_pair(population(scenario), population(scenario), 4, 1);
        assert_eq!(
            on.combat_work, off.combat_work,
            "{} activity",
            scenario.name
        );
        report.push_str(&format!(
            "{}: {} units; sleep on/off changes and hashes agree each tick\n",
            scenario.name, scenario.entities
        ));
        for (name, times) in [("sleep_on", on), ("sleep_off", off)] {
            report.push_str(&format!(
                "{name}: movement_ns={:?} combat_ns={:?} gathering_ns={:?} tick_ns={:?}\ncombat_work={:?}\n",
                times.movement, times.combat, times.gathering, times.tick, times.combat_work
            ));
        }
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/perf");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("work-sleep-population.txt"), report).unwrap();
}

fn percentile(values: &[u64], percent: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]
}

/// Runs both worlds `ticks` ticks, interleaved, comparing changed units every
/// tick and hashes every `every` ticks. Returns the thousandths of gatherer
/// visits that slept with sleep on, and both worlds' times.
fn run_pair(
    mut on: GameWorld,
    mut off: GameWorld,
    ticks: usize,
    every: usize,
) -> (usize, Times, Times) {
    off.set_work_sleep(false);
    assert_eq!(on.canonical_hash(), off.canonical_hash());
    let (mut on_times, mut off_times) = (Times::default(), Times::default());
    for tick in 1..=ticks {
        let changed = on_times.advance(&mut on);
        assert_eq!(
            changed,
            off_times.advance(&mut off),
            "changed units at tick {tick}"
        );
        if tick % every == 0 {
            assert_eq!(
                on.canonical_hash_hex(),
                off.canonical_hash_hex(),
                "sleep on/off diverged at tick {tick}"
            );
        }
    }
    assert_eq!(off.economy.tick_work.gather_slept, 0);
    let work = &on.economy.tick_work;
    let slept = work.gather_slept * 1000 / work.gather_visits.max(1);
    (slept, on_times, off_times)
}

#[test]
fn work_sleep_replays_hash_for_hash_over_two_game_minutes() {
    let (slept, ..) = run_pair(build(8).unwrap(), build(8).unwrap(), 2_400, 1);
    // Most visits of a gathering village are pure credit ticks.
    assert!(slept >= 300, "only {slept}/1000 gatherer visits slept");
}

#[test]
fn work_sleep_replays_the_full_late_game_workload() {
    let (slept, on, off) = run_pair(
        GameWorld::late_game_workload().unwrap(),
        GameWorld::late_game_workload().unwrap(),
        512,
        16,
    );
    // Walking and returning gatherers still step; 238/1000 measured.
    assert!(slept >= 200, "only {slept}/1000 gatherer visits slept");
    // Steady state after the 384-tick warm-up the profiles also skip.
    let line = |name: &str, times: &Times| {
        let (g, t) = (&times.gathering[384..], &times.tick[384..]);
        format!(
            "{name}: gathering_p50_us={} gathering_p95_us={} tick_p50_us={} tick_p95_us={}\n",
            percentile(g, 50) / 1000,
            percentile(g, 95) / 1000,
            percentile(t, 50) / 1000,
            percentile(t, 95) / 1000,
        )
    };
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/perf");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("work-sleep-late-game.txt"),
        format!(
            "late_game_workload 14000 units, ticks 385..512, test profile (opt 2, debug assertions), on/off interleaved\nslept_visits_per_1000={slept}\n{}{}",
            line("sleep_on", &on),
            line("sleep_off", &off)
        ),
    )
    .unwrap();
}
