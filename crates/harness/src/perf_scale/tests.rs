//! Current-output replay pins for a fixed scenario with movers, gathering,
//! construction and short routes. Cache and index refactors preserve these;
//! intentional gameplay changes regenerate them after behaviour tests pass.
//! Capture with `perf-scale --players 4 --units-per-player 80 --side-tiles 2048
//! --fight-size 4 --skip-attacks --ticks 300 --fog-off-ticks 0 --hash-every 100
//! --server-ticks 0`.
// Combined Castle bundle: u64 research, healers, garrisons, queued builders and
// cargo/embarked maps extend current state. Regenerate the same scenario; all
// independent replay, thread equality and emitted-change assertions remain.
use super::*;

fn smoke_options(out: PathBuf) -> Options {
    Options {
        players: 4,
        units_per_player: 100,
        side_tiles: 2_000,
        ticks: 12,
        fog_off_ticks: 4,
        orders_per_second: 3.0,
        seed: 1,
        fight_size: 4,
        hash_every: 6,
        server_ticks: 3,
        subscribers: 4,
        idle_world: false,
        skip_attacks: false,
        profile_movement: true,
        out,
    }
}

#[test]
fn the_smoke_run_writes_a_valid_report_whose_phases_add_up() {
    let directory = tempfile::tempdir().unwrap();
    let out = directory.path().join("nested/report.json");
    run(smoke_options(out.clone())).unwrap();
    let report: Value = serde_json::from_str(&fs::read_to_string(out).unwrap()).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["workload"]["players"], 4);
    assert_eq!(report["workload"]["tick_hz"], 20);
    let on = &report["simulation_fog_on"];
    assert_eq!(on["ticks"], 12);
    assert_eq!(on["tick_budget_ms"], 50.0);
    for phase in ["unit_spacing", "vassal_economy", "idle_tail"] {
        assert!(on["phases"][phase]["mean_ms"].is_number(), "{phase}");
    }
    assert_eq!(on["hashes"].as_array().unwrap().len(), 2);
    assert_eq!(report["simulation_fog_off"]["ticks"], 4);
    assert!(report["census"]["initial"]["units"].as_u64().unwrap() >= 400);
    assert!(report["terrain_probe"]["cold_point_query_us_per_tile"].is_number());
    assert!(report["simulation_fog_on"]["phases"]["unit_spacing"].is_object());
    assert_eq!(
        report["simulation_fog_on"]["phases"]
            .as_object()
            .unwrap()
            .len(),
        TickPhase::Changed as usize + 2
    );
    assert_eq!(report["movement_profile"]["enabled"], true);
    assert!(
        report["movement_profile"]["fog_on"]["avoidance_query"]["calls"]
            .as_u64()
            .unwrap()
            > 0
    );
    let server = &report["server"];
    assert_eq!(server["with_sessions_fog_on"]["sessions"], 4);
    assert_eq!(server["baseline_no_sessions_tick"]["samples"], 3);
}

#[test]
fn phase_time_sums_to_the_advance_time_and_the_budget_follows_the_tick_rate() {
    let mut built = world::build(&Params {
        players: 2,
        units_per_player: 60,
        side_tiles: 1_000,
        seed: 2,
        fight_pairs_size: 2,
        idle: false,
    })
    .unwrap();
    let mut driver = Driver::new(2, built.players.len(), 3.0, built.world.config().tick_hz);
    let run = run_ticks(&mut built.world, &mut built.players, &mut driver, 6, 0);
    assert_eq!(PHASES[TickPhase::Spacing as usize], "unit_spacing");
    assert_eq!(PHASES[TickPhase::VassalEconomy as usize], "vassal_economy");
    assert_eq!(PHASES[TickPhase::Changed as usize], "result_collation");
    assert_eq!(PHASES.last(), Some(&"idle_tail"));
    assert_eq!(run.phases.len(), TickPhase::Changed as usize + 2);
    let phases: f64 = run.phases.iter().flat_map(|s| s.0.iter()).sum();
    let total: f64 = run.total.0.iter().sum();
    assert!(
        (phases - total).abs() < 1e-6 * total.max(1.0),
        "{phases} vs {total}"
    );
    assert_eq!(
        run.budget_ms,
        1000.0 / f64::from(built.world.config().tick_hz)
    );
    assert_eq!(run.json()["tick_budget_ms"], run.budget_ms);
}

#[test]
fn scale_tail_movers_have_a_bounded_obstruction_budget() {
    let mut built = world::build(&Params {
        players: 4,
        units_per_player: 1000,
        side_tiles: 16_000,
        seed: 1,
        fight_pairs_size: 12,
        idle: false,
    })
    .unwrap();
    let mut driver = Driver::new(1, 4, 3.0, 20);
    run_ticks(&mut built.world, &mut built.players, &mut driver, 600, 0);
    eprintln!("tail census: {}", census(&built.world, &built.players));
    let out =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reports/scale/settling-tail.json");
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    fs::write(out, census(&built.world, &built.players).to_string()).unwrap();
    for unit in built.world.units() {
        assert!(built.world.movement_obstruction_ticks(unit.id).unwrap_or(0) <= 128);
    }
}

/// Re-pinned when route searches became a per-player fair pass run before
/// units move (see `route_scheduler`): legs are found on different ticks.
/// Re-pinned again when gatherers began to step up against their node and
/// animals to move inside their tile on map terrain. The merge with the
/// seeded mainland placement and the route planners surviving ground changes
/// moved them once more; the values are the combined tree's.
/// Unit spacing (distinct destination and work slots, idle separation,
/// yielding followers) is a further intended cause of the values below, as is facing the work target on working ticks.
/// Re-pinned for the combined follow-ups: exact explored placement/foundation
/// sight, retained route guidance and endpoint proof state, bounded idle
/// rotation and active follower sampling. These intentional state/timing
/// changes, including deferred fast attempts and within-player far reserves,
/// preserve the three checkpoints and deterministic scenario checks.
/// Static-navigation integration adds the shared per-tick preparation allowance
/// and the budgeted reverse-proof state to authoritative hashes. Re-pin for
/// that intentional schema/pacing change, keeping seed, workload and thresholds.
/// Re-pin for the original DAT TC annex collision, retained detour searches
/// and removal of the obsolete unproven-approach flag from planner hashes.
/// The merged drop-off work state, prepaid reseeds and auto-scout scheduling
/// extend that same schema. Regenerate these current pins for the combination;
/// the scenario, checkpoints and limits remain unchanged.
/// Castle Age widens research bits, adds the ordered healer index and refines
/// gather credits. Regenerate current hashes for those deliberate state changes.
/// Unit obstruction, bounded local refinement and reachable arrival slots
/// intentionally change paths, work timing and neighbour-index order. Regenerate
/// these current checkpoints with the same workload, seed and assertions.
/// Combined with perimeter-contact drop-off, these timings also change worker
/// stockpiles; bounded worker corner escapes also change retained routes.
/// Regenerate all three current pins from `make test-native-focused
/// AOE_NATIVE_TEST_PACKAGE=aoe-harness AOE_NATIVE_TEST_FILTER='test(perf_scale::)'`.
/// Keep the same 300-tick workload and checkpoints.
/// Builders now use exact-position corner exits and start on first free
/// perimeter contact, deliberately changing construction timing and routes.
/// Regenerate only the current output hashes.
// The current Vassal home/control maps and retained manager cursors extend
// canonical state even in a world without Vassals. Re-pin the same workload,
// seed, checkpoints and independent replay/thread assertions for that schema.
// Bundled gatherer/builder reservations, the Vassal economy phase and walking
// proposals change work assignment, canonical state and route timing. Regenerate
// with the command above after their behaviour and replay/thread checks pass.
// Rebasing these reservations onto avoidance, arrival slots and contact
// drop-off combines both behaviours; regenerate with the command above.
const PINNED_HASHES: [(u64, &str); 3] = [
    (
        100,
        "833472100cd475f676fe2274a68ad8245971092ff8873f4d9225e8a833d4d2a8",
    ),
    (
        200,
        "46e447b7ec5e78f954f205e3f903fb0ec9688ba42b73108dcab38105f7ff8467",
    ),
    (
        300,
        "17aef54e943664f1f7874a9dab57f64e8971f910224e7044ebe84fe307fbf48a",
    ),
];

#[test]
fn small_scale_scenario_replays_to_the_pinned_hashes() {
    // Regenerated for the current construction state: builders now hash their
    // ordered foundation queues, and completion can choose another own site.
    let params = Params {
        players: 4,
        units_per_player: 80,
        side_tiles: 2048,
        seed: 1,
        fight_pairs_size: 4,
        idle: false,
    };
    let Built {
        mut world,
        mut players,
        ..
    } = world::build(&params).expect("scenario builds");
    let mut driver = Driver::new(1, players.len(), 3.0, world.config().tick_hz);
    driver.skip_attacks = true;
    let run = run_ticks(&mut world, &mut players, &mut driver, 300, 100);
    let hashes: Vec<(u64, String)> = run
        .hashes
        .iter()
        .map(|entry| {
            (
                entry["tick"].as_u64().expect("tick"),
                entry["blake3"].as_str().expect("hash").to_owned(),
            )
        })
        .collect();
    let pinned: Vec<(u64, String)> = PINNED_HASHES
        .iter()
        .map(|(tick, hash)| (*tick, (*hash).to_owned()))
        .collect();
    assert_eq!(hashes, pinned);
}
