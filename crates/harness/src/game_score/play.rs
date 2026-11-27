//! The child process: one scenario at one worker count. It starts the match
//! through the server's headless seam, ticks it, lets the recorder read the
//! world after every tick and writes its `Measurement` where the parent asked.
use super::counters::Recorder;
use super::measure::{self, Measurement};
use super::scenarios::{self, Scenario};
use super::windows::{self, Record};
use super::{BoxError, Plan};
use crate::perf_scale::stats::memory;
use aoe_server::{HeadlessMatch, HeadlessPlan};
use std::{fs, path::Path, time::Duration, time::Instant};

pub(super) fn run(plan: &Plan, out: &Path) -> Result<(), BoxError> {
    // The tick runs on blocking threads: only the global pool reaches them.
    rayon::ThreadPoolBuilder::new()
        .num_threads(plan.workers)
        .build_global()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(plan.workers.max(2))
        .enable_all()
        .build()?;
    let scenario = scenarios::find(&plan.scenario)?;
    let measurement = runtime.block_on(play(plan, scenario, out))?;
    fs::write(out, serde_json::to_string_pretty(&measurement)? + "\n")?;
    Ok(())
}

/// Where the order log of the run written to `out` goes.
pub(super) fn orders_path(out: &Path) -> std::path::PathBuf {
    out.with_extension("orders.txt")
}

async fn play(plan: &Plan, scenario: &Scenario, out: &Path) -> Result<Measurement, BoxError> {
    let started = Instant::now();
    let game = HeadlessMatch::start(&HeadlessPlan {
        package_directory: plan.package_directory.clone(),
        map_hash: plan.map_hash.clone(),
        seats: plan.seats,
        seed: plan.seed,
        late_game_percent: scenario.late_percent(plan.late_percent),
        record_orders: plan.record_orders,
    })
    .await?;
    let setup_seconds = started.elapsed().as_secs_f64();
    let gameplay = game.gameplay();
    let tick_hz = gameplay
        .inspect_scale_world(|world| world.config().tick_hz)
        .await;
    let mut recorder = Recorder::new(plan.seats, tick_hz);
    let sampled = scenario.sample_ticks(plan.ticks, plan.sample_every);
    let hash_every = plan.hash_every.max(1);
    let mut digests = Vec::new();
    gameplay
        .inspect_scale_world(|world| {
            recorder.start(world);
            digests.push(measure::digest(world));
        })
        .await;
    let mut tick_time = Duration::ZERO;
    for n in 1..=plan.ticks {
        let begun = Instant::now();
        game.tick().await;
        let spent = begun.elapsed();
        tick_time += spent;
        let hash = n % hash_every == 0 || n == plan.ticks;
        let sample = sampled.contains(&n);
        let digest = gameplay
            .inspect_scale_world(|world| {
                recorder.tick(world, spent);
                if sample {
                    recorder.sample(world);
                }
                hash.then(|| measure::digest(world))
            })
            .await;
        digests.extend(digest);
        if n % 500 == 0 {
            eprintln!(
                "game score: {} {} workers, tick {n} of {}, {:.1} ms per tick so far",
                scenario.id,
                plan.workers,
                plan.ticks,
                tick_time.as_secs_f64() * 1e3 / n as f64
            );
        }
    }
    let record = Record {
        rows: &recorder.rows,
        samples: &recorder.samples,
        events: &recorder.events,
    };
    let windows = windows::windows(record, &scenario.spans_for(plan.ticks))?;
    let series = windows::series(record, plan.ticks)?;
    let (order_count, order_log_hash, order_log_file) = if plan.record_orders {
        let lines = game.order_log();
        let file = orders_path(out);
        fs::write(&file, lines.join("\n") + "\n")?;
        let hash = blake3::hash(lines.join("\n").as_bytes())
            .to_hex()
            .to_string();
        (Some(lines.len() as u64), Some(hash), Some(file))
    } else {
        (None, None, None)
    };
    let tick_seconds = tick_time.as_secs_f64();
    let milliseconds: Vec<f64> = recorder
        .rows
        .iter()
        .map(|row| row.nanos as f64 / 1e6)
        .collect();
    Ok(Measurement {
        scenario: scenario.id.to_owned(),
        workers: plan.workers,
        ticks: plan.ticks,
        seats: plan.seats,
        setup_seconds,
        tick_seconds,
        native_seconds: started.elapsed().as_secs_f64(),
        ticks_per_second: plan.ticks as f64 / tick_seconds.max(1e-9),
        slowest_tick: slowest_tick(&milliseconds),
        peak_memory_mib: memory()["peak_rss_mib"].as_u64().unwrap_or(0),
        windows,
        series,
        digests,
        order_count,
        order_log_hash,
        order_log_file,
    })
}

/// The position of the longest tick, counting from 1; 0 for no tick.
pub(super) fn slowest_tick(milliseconds: &[f64]) -> u64 {
    milliseconds
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map_or(0, |(index, _)| index as u64 + 1)
}
