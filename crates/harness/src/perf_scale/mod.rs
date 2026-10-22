//! `perf-scale`: headless authoritative-tick benchmark at the promised scale
//! (100 players, about 100,000 units, a 16,000-tile France-sized map).
mod driver;
mod server;
mod stats;
mod world;

use aoe_simulation::{GameWorld, TickPhase};
use clap::Args;
use driver::{Driver, ORDER_KINDS};
use serde_json::{Value, json};
use stats::{Series, memory, round};
use std::{fs, path::PathBuf, process::Command, time::Instant};
use world::{BoxError, Built, Params, Player};

const PHASES: [&str; 9] = [
    "movement_and_path_planning",
    "wildlife",
    "combat",
    "construction_and_gates",
    "gathering_and_economy",
    "research_and_training",
    "building_memory_per_player",
    "result_collation",
    "idle_tail",
];

#[derive(Args, Debug, Clone)]
pub struct Options {
    #[arg(long, default_value_t = 100)]
    players: u16,
    #[arg(long, default_value_t = 1000)]
    units_per_player: u32,
    /// Map side in two-metre tiles; the map is France-centred at ratio 30:1.
    #[arg(long, default_value_t = 16_000)]
    side_tiles: u32,
    /// Ticks measured with fog of war enabled (600 = 30 s at 20 Hz).
    #[arg(long, default_value_t = 600)]
    ticks: u32,
    /// Further ticks with fog disabled, to isolate visibility cost.
    #[arg(long, default_value_t = 100)]
    fog_off_ticks: u32,
    /// Orders per player per second.
    #[arg(long, default_value_t = 3.0)]
    orders_per_second: f64,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Fighting militia per side at each neighbour pair midpoint.
    #[arg(long, default_value_t = 12)]
    fight_size: u32,
    /// Measure the canonical replay hash every this many ticks.
    #[arg(long, default_value_t = 100)]
    hash_every: u32,
    /// Server ticks per mode (fog on and fog off); 0 skips the server part.
    #[arg(long, default_value_t = 40)]
    server_ticks: u32,
    /// Subscribed sessions in the server part.
    #[arg(long, default_value_t = 100)]
    subscribers: u16,
    /// Leave all units idle, so server fan-out can be measured without simulation load.
    #[arg(long)]
    idle_world: bool,
    #[arg(long, default_value = "reports/scale/perf-scale.json")]
    out: PathBuf,
}

struct PhaseRun {
    total: Series,
    with_orders: Series,
    orders: Series,
    phases: Vec<Series>,
    changed: Vec<f64>,
    hashes: Vec<Value>,
    hash_ms: Series,
    start_tick: u64,
    budget_ms: f64,
}

fn run_ticks(
    world: &mut GameWorld,
    players: &mut [Player],
    driver: &mut Driver,
    ticks: u32,
    hash_every: u32,
) -> PhaseRun {
    let budget_ms = 1000.0 / f64::from(world.config().tick_hz);
    let mut run = PhaseRun {
        total: Series::default(),
        with_orders: Series::default(),
        orders: Series::default(),
        phases: vec![Series::default(); PHASES.len()],
        changed: Vec::new(),
        hashes: Vec::new(),
        hash_ms: Series::default(),
        start_tick: world.tick().0,
        budget_ms,
    };
    for index in 0..ticks {
        let begin = Instant::now();
        driver.issue(world, players);
        let ordered = Instant::now();
        let mut last = ordered;
        let mut spent = [0u128; PHASES.len()];
        let changed = world.advance_with_probe(&mut |_| true, &mut |phase: TickPhase| {
            let now = Instant::now();
            spent[phase as usize] += (now - last).as_nanos();
            last = now;
        });
        let end = Instant::now();
        spent[8] += (end - last).as_nanos();
        run.orders.push_ms((ordered - begin).as_nanos());
        run.total.push_ms((end - ordered).as_nanos());
        run.with_orders.push_ms((end - begin).as_nanos());
        for (series, nanos) in run.phases.iter_mut().zip(spent) {
            series.push_ms(nanos);
        }
        run.changed.push(changed.len() as f64);
        if hash_every > 0 && (index + 1) % hash_every == 0 {
            let hashed = Instant::now();
            let hash = world.canonical_hash();
            run.hash_ms.push_ms(hashed.elapsed().as_nanos());
            run.hashes
                .push(json!({ "tick": world.tick().0, "blake3": hex(&hash) }));
        }
    }
    run
}

/// Cold cost of one tile: a point query against a whole 32x32 chunk, per tile.
fn terrain_probe(side_tiles: u32, seed: u64) -> Result<Value, BoxError> {
    let generator = world::package(side_tiles, seed)?.generator();
    let side = i64::from(side_tiles) - 64;
    let mut random = seed | 1;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        (random % side as u64) as i32
    };
    let points = 2_000;
    let started = Instant::now();
    for _ in 0..points {
        let _ = generator
            .tile_and_node_with_cancel(aoe_core::TileCoord::new(next(), next()), &|| false);
    }
    let point_us = started.elapsed().as_secs_f64() * 1e6 / f64::from(points);
    let chunks = 40;
    let started = Instant::now();
    for _ in 0..chunks {
        let _ = generator.chunk(next() / 32, next() / 32);
    }
    let chunk_tile_us = started.elapsed().as_secs_f64() * 1e6 / f64::from(chunks * 1024);
    Ok(json!({
        "cold_point_query_us_per_tile": round(point_us),
        "cold_chunk_us_per_tile": round(chunk_tile_us),
        "note": "procedural fallback terrain; source-backed packages read pages instead",
    }))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl PhaseRun {
    fn json(&self) -> Value {
        let phases: serde_json::Map<String, Value> = PHASES
            .iter()
            .zip(&self.phases)
            .map(|(name, series)| ((*name).to_owned(), series.summary()))
            .collect();
        let mean_changed = self.changed.iter().sum::<f64>() / self.changed.len().max(1) as f64;
        let budget_ms = self.budget_ms;
        let over = self
            .with_orders
            .0
            .iter()
            .filter(|ms| **ms > budget_ms)
            .count();
        json!({
            "ticks": self.total.0.len(),
            "first_tick": self.start_tick,
            "tick_advance": self.total.summary(),
            "orders_issue": self.orders.summary(),
            "tick_with_orders": self.with_orders.summary(),
            "tick_budget_ms": budget_ms,
            "ticks_over_budget": over,
            "phases": phases,
            "mean_changed_units_per_tick": round(mean_changed),
            "canonical_hash_ms": self.hash_ms.summary(),
            "hashes": self.hashes,
        })
    }
}

fn census(world: &GameWorld, built_players: &[Player]) -> Value {
    let mut tasks = std::collections::BTreeMap::<String, u64>::new();
    let mut moving = 0u64;
    let mut kinds = std::collections::BTreeMap::<String, u64>::new();
    for unit in world.units() {
        *tasks.entry(format!("{:?}", unit.task)).or_default() += 1;
        *kinds.entry(format!("{:?}", unit.kind)).or_default() += 1;
        moving += u64::from(unit.moving || unit.planning);
    }
    let tile_work: u64 = built_players
        .iter()
        .map(|p| world.vision_storage(p.id).2)
        .sum();
    let visible: usize = built_players
        .iter()
        .map(|p| world.vision_storage(p.id).1)
        .sum();
    let nav = world.navigation_cache_usage();
    json!({
        "tick": world.tick().0,
        "units": world.unit_count(),
        "buildings": world.buildings().count(),
        "units_by_task": tasks,
        "units_by_kind": kinds,
        "units_moving_or_planning": moving,
        "active_movers": world.active_mover_count(),
        "occupied_spatial_chunks": world.occupied_chunk_count(),
        "combat": format!("{:?}", world.combat_stats()),
        "vision_tile_touches_total": tile_work,
        "vision_visible_tiles_total": visible,
        "navigation_cache": { "entries": nav.entries, "retained_bytes": nav.retained_bytes },
        "memory": memory(),
    })
}

#[cfg(test)]
mod tests;

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub fn run(options: Options) -> Result<(), BoxError> {
    let params = Params {
        players: options.players,
        units_per_player: options.units_per_player,
        side_tiles: options.side_tiles,
        seed: options.seed,
        fight_pairs_size: options.fight_size,
        idle: options.idle_world,
    };
    eprintln!("perf-scale: building {params:?}");
    let before_build = memory();
    let started = Instant::now();
    let Built {
        mut world,
        mut players,
        mut timings,
        notes,
    } = world::build(&params)?;
    timings.insert("build_total_s", started.elapsed().as_secs_f64());
    let after_build = memory();
    let initial_census = census(&world, &players);
    let initial_hash = {
        let hashed = Instant::now();
        let hash = world.canonical_hash();
        json!({ "blake3": hex(&hash), "ms": round(hashed.elapsed().as_secs_f64() * 1e3) })
    };
    eprintln!("perf-scale: built, {initial_census}");

    let tick_hz = world.config().tick_hz;
    let mut driver = Driver::new(
        options.seed,
        players.len(),
        options.orders_per_second,
        tick_hz,
    );
    let fog_on = run_ticks(
        &mut world,
        &mut players,
        &mut driver,
        options.ticks,
        options.hash_every,
    );
    let census_after = census(&world, &players);
    eprintln!("perf-scale: fog-on done, {}", fog_on.with_orders.summary());

    world.set_fog_disabled(true);
    let fog_off = run_ticks(
        &mut world,
        &mut players,
        &mut driver,
        options.fog_off_ticks,
        options.hash_every,
    );
    eprintln!(
        "perf-scale: fog-off done, {}",
        fog_off.with_orders.summary()
    );
    let after_fog_off = census(&world, &players);

    let server = if options.server_ticks > 0 {
        world.set_fog_disabled(false);
        Some(server::measure(
            world,
            &players,
            options.server_ticks,
            options.subscribers,
        )?)
    } else {
        None
    };

    let rejected: Value = driver
        .outcomes
        .rejected
        .iter()
        .map(|(k, v)| ((*k).to_owned(), json!(v)))
        .collect::<serde_json::Map<_, _>>()
        .into();
    let report = json!({
        "version": 1,
        "revision": git(&["rev-parse", "HEAD"]),
        "dirty": !git(&["status", "--porcelain"]).is_empty(),
        "machine": { "logical_cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0) },
        "workload": {
            "players": options.players,
            "units_per_player": options.units_per_player,
            "side_tiles": options.side_tiles,
            "tick_hz": tick_hz,
            "tick_budget_ms": 1000.0 / f64::from(tick_hz),
            "orders_per_player_per_second": options.orders_per_second,
            "map": "procedural MapPackage (no source layers), France-centred, ratio 30:1, 2 m tiles",
            "limits_raised_from_shipped_caps": true,
            "orders_issued": driver.issued,
            "orders_ok": driver.outcomes.ok,
            "orders_rejected": rejected,
            "order_kinds": ORDER_KINDS,
        },
        "setup": { "timings_s": timings, "notes": notes },
        "memory": { "before_build": before_build, "after_build": after_build },
        "census": { "initial": initial_census, "after_fog_on": census_after, "after_fog_off": after_fog_off },
        "terrain_probe": terrain_probe(options.side_tiles, options.seed)?,
        "initial_hash": initial_hash,
        "simulation_fog_on": fog_on.json(),
        "simulation_fog_off": fog_off.json(),
        "server": server,
    });
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&options.out, serde_json::to_string(&report)? + "\n")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report["simulation_fog_on"]["tick_with_orders"])?
    );
    println!("report: {}", options.out.display());
    Ok(())
}
