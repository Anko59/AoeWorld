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
    assert_eq!(on["hashes"].as_array().unwrap().len(), 2);
    assert_eq!(report["simulation_fog_off"]["ticks"], 4);
    assert!(report["census"]["initial"]["units"].as_u64().unwrap() >= 400);
    assert!(report["terrain_probe"]["cold_point_query_us_per_tile"].is_number());
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
