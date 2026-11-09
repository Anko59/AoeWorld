//! One closed-loop run: the bots act after every tick, the tick is timed per
//! phase, the canonical hash is kept every `hash_every` ticks and the route
//! queue and population every `sample_every`.
use super::super::stats::{Series, memory};
use super::super::{PHASES, world::BoxError};
use super::bots::{self, BotCounts};
use super::build::{self, Shape};
use aoe_simulation::{GameWorld, TickPhase};
use serde_json::{Value, json};
use std::time::Instant;

pub(super) struct Plan {
    pub shape: Shape,
    pub threads: usize,
    pub ticks: u64,
    pub hash_every: u64,
    pub sample_every: u64,
}

pub(super) struct Outcome {
    pub report: Value,
    /// `(tick, hash)` every `hash_every` ticks and at the end.
    pub hashes: Vec<(u64, String)>,
}

pub(super) fn hex(world: &GameWorld) -> String {
    world
        .canonical_hash()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

/// A tick Callgrind counts (collection toggled around it by client requests;
/// the linker may fold it with `plain_tick`, so it is not matched by name).
#[inline(never)]
pub(super) fn counted_tick(world: &mut GameWorld) {
    std::hint::black_box(world.advance());
}

/// The same tick outside the counted window.
#[inline(never)]
pub(super) fn plain_tick(world: &mut GameWorld) {
    std::hint::black_box(world.advance());
}

fn sample(world: &GameWorld) -> Value {
    let routes = world.route_scheduler_stats();
    json!({
        "tick": world.tick().0,
        "units": world.unit_count(),
        "movers": world.active_mover_count(),
        "waiting_units": routes.waiting_units,
        "waiting_without_planner": routes.waiting_without_planner,
        "oldest_wait_ticks": routes.oldest_wait_ticks,
        "players_at_search_cap": routes.players_at_search_cap,
        "world_capacity_exhausted": routes.world_capacity_exhausted,
    })
}

pub(super) fn run(plan: &Plan) -> Result<Outcome, BoxError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(plan.threads)
        .build()?;
    Ok(pool.install(|| run_in_pool(plan).map_err(|error| error.to_string()))?)
}

fn run_in_pool(plan: &Plan) -> Result<Outcome, BoxError> {
    let mut built = build::build(&plan.shape)?;
    let world = &mut built.world;
    let start_units = world.unit_count();
    let start_buildings = world.buildings().count();
    let start_hash = hex(world);
    let mut counts = BotCounts::default();
    let mut tick_ms = Series::default();
    let mut bots_ms = Series::default();
    let mut phases = vec![Series::default(); PHASES.len()];
    let mut samples = vec![sample(world)];
    let mut hashes = vec![(world.tick().0, start_hash.clone())];
    let mut hash_ms = Series::default();
    let begin = Instant::now();
    for n in 1..=plan.ticks {
        let started = Instant::now();
        let mut last = started;
        let mut spent = [0u128; PHASES.len()];
        world.advance_with_probe(&mut |_| true, &mut |phase: TickPhase| {
            let now = Instant::now();
            spent[phase as usize] += (now - last).as_nanos();
            last = now;
        });
        let ended = Instant::now();
        spent[PHASES.len() - 1] += (ended - last).as_nanos();
        tick_ms.push_ms((ended - started).as_nanos());
        for (series, nanos) in phases.iter_mut().zip(spent) {
            series.push_ms(nanos);
        }
        bots::step(world, &built.seats, &built.generals, &mut counts);
        bots_ms.push_ms(ended.elapsed().as_nanos());
        if plan.hash_every > 0 && (n % plan.hash_every == 0 || n == plan.ticks) {
            let hashed = Instant::now();
            hashes.push((world.tick().0, hex(world)));
            hash_ms.push_ms(hashed.elapsed().as_nanos());
        }
        if plan.sample_every > 0 && n % plan.sample_every == 0 {
            samples.push(sample(world));
        }
    }
    let wall_s = begin.elapsed().as_secs_f64();
    let tick_s: f64 = tick_ms.0.iter().sum::<f64>() / 1000.0;
    let phase_summary: serde_json::Map<String, Value> = PHASES
        .iter()
        .zip(&phases)
        .map(|(name, series)| ((*name).to_owned(), series.summary()))
        .collect();
    let passes: serde_json::Map<String, Value> = world
        .pass_counts()
        .into_iter()
        .map(|(name, count)| {
            (
                name.to_owned(),
                json!({ "calls": count.calls, "visited": count.visited }),
            )
        })
        .collect();
    let report = json!({
        "seats": plan.shape.seats,
        "scale": plan.shape.scale,
        "side_tiles": built.side,
        "seed": plan.shape.seed,
        "threads": plan.threads,
        "ticks": plan.ticks,
        "setup": {
            "map_s": built.map_s,
            "setup_s": built.setup_s,
            "villages_failed": built.villages_failed,
            "stewards_garrisoned": built.stewards,
            "generals": built.generals.len(),
            "units": start_units,
            "buildings": start_buildings,
            "hash": start_hash,
        },
        "ticks_per_s": plan.ticks as f64 / tick_s.max(1e-9),
        "ticks_per_s_with_bots": plan.ticks as f64 / wall_s.max(1e-9),
        "tick": tick_ms.summary(),
        "bots": bots_ms.summary(),
        "hash_cost": hash_ms.summary(),
        "phases": phase_summary,
        "bot_orders": counts,
        "samples": samples,
        "end": sample(world),
        "passes": passes,
        "memory": memory(),
        "final_hash": hashes.last().map(|(_, h)| h.clone()),
    });
    Ok(Outcome { report, hashes })
}

/// Builds the world, plays `warmup` ticks, then `counted` ticks inside
/// `counted_tick` (one rayon worker, for Callgrind). Prints the movers.
pub(super) fn counted_child(shape: &Shape, warmup: u64, counted: u64) -> Result<Value, BoxError> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build()?;
    let counted_run = || -> Result<Value, BoxError> {
        let mut built = build::build(shape)?;
        let world = &mut built.world;
        let mut counts = BotCounts::default();
        for _ in 0..warmup {
            plain_tick(world);
            bots::step(world, &built.seats, &built.generals, &mut counts);
        }
        let (mut movers, mut units) = (0u64, 0u64);
        gungraun::client_requests::callgrind::start_instrumentation();
        for _ in 0..counted {
            movers += world.active_mover_count() as u64;
            units += world.unit_count() as u64;
            // Collect only the tick: the bots between ticks stay out.
            gungraun::client_requests::callgrind::toggle_collect();
            counted_tick(world);
            gungraun::client_requests::callgrind::toggle_collect();
            bots::step(world, &built.seats, &built.generals, &mut counts);
        }
        gungraun::client_requests::callgrind::stop_instrumentation();
        let per = |v: u64| v / counted.max(1);
        Ok(json!({
            "seats": shape.seats, "scale": shape.scale, "side_tiles": built.side,
            "warmup": warmup, "counted": counted,
            "movers_per_tick": per(movers), "units_per_tick": per(units),
            "routes": sample(world), "bot_orders": counts,
        }))
    };
    Ok(pool.install(|| counted_run().map_err(|error| error.to_string()))?)
}
