//! Perf-scale report shape, phase budgets and a run-twice determinism property
//! for a fixed scenario with movers, gathering, construction and short routes.
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
        assert!(
            built.world.movement_obstruction_ticks(unit.id).unwrap_or(0) <= 128,
            "blocked tail: {unit:?}, obstruction={:?}, formation={:?}, destination={:?}, gather={:?}",
            built.world.movement_obstruction_ticks(unit.id),
            built.world.formation(unit.id),
            built.world.movement_order(unit.id),
            built.world.gather_target(unit.id),
        );
    }
}

fn small_scale_hashes() -> Vec<Value> {
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
    run_ticks(&mut world, &mut players, &mut driver, 300, 100).hashes
}

#[test]
fn small_scale_scenario_replays_to_identical_hashes() {
    let first = small_scale_hashes();
    assert_eq!(first.len(), 3);
    assert_eq!(first, small_scale_hashes());
}
