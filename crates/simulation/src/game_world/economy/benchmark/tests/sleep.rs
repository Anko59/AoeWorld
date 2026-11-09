//! Replay oracle for economy sleep: the same workload with sleep on and off
//! must agree hash for hash, and sleep must actually skip work. The full
//! workload also records interleaved per-phase times in `reports/perf`.
use super::super::*;
use crate::TickPhase;
use std::time::Instant;

/// Gathering-phase and whole-tick nanoseconds per tick, for one world.
#[derive(Default)]
struct Times {
    gathering: Vec<u64>,
    tick: Vec<u64>,
}

impl Times {
    fn advance(&mut self, world: &mut GameWorld) -> Vec<crate::GameUnit> {
        let start = Instant::now();
        let mut last = start;
        let mut gathering = 0;
        let changed = world.advance_with_probe(&mut |_| true, &mut |phase| {
            let now = Instant::now();
            if phase == TickPhase::Gathering {
                gathering = (now - last).as_nanos() as u64;
            }
            last = now;
        });
        self.gathering.push(gathering);
        self.tick.push(start.elapsed().as_nanos() as u64);
        changed
    }
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
