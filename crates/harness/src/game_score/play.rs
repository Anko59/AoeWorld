//! The child process: one match at one worker count. It starts the match
//! through the server's headless seam, ticks it, hashes it on the way and
//! writes its `Measurement` where the parent asked.
use super::measure::{self, Measurement};
use super::{BoxError, Plan};
use crate::perf_scale::stats::{Series, memory};
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
    let measurement = runtime.block_on(play(plan, out))?;
    fs::write(out, serde_json::to_string_pretty(&measurement)? + "\n")?;
    Ok(())
}

/// Where the order log of the run written to `out` goes.
pub(super) fn orders_path(out: &Path) -> std::path::PathBuf {
    out.with_extension("orders.txt")
}

async fn play(plan: &Plan, out: &Path) -> Result<Measurement, BoxError> {
    let started = Instant::now();
    let game = HeadlessMatch::start(&HeadlessPlan {
        package_directory: plan.package_directory.clone(),
        map_hash: plan.map_hash.clone(),
        seats: plan.seats,
        seed: plan.seed,
        record_orders: plan.record_orders,
    })
    .await?;
    let setup_seconds = started.elapsed().as_secs_f64();
    let gameplay = game.gameplay();
    let hash_every = plan.hash_every.max(1);
    let mut digests = vec![gameplay.inspect_scale_world(measure::digest).await];
    let mut samples = Vec::new();
    let mut tick_ms = Series::default();
    let mut tick_time = Duration::ZERO;
    let mut hits = 0_u64;
    for n in 1..=plan.ticks {
        let begun = Instant::now();
        game.tick().await;
        let spent = begun.elapsed();
        tick_time += spent;
        tick_ms.push_ms(spent.as_nanos());
        // The engine overwrites a tick's hits at the next tick: sum them here.
        hits += u64::from(
            gameplay
                .inspect_scale_world(|world| world.combat_stats().hits)
                .await,
        );
        if n % hash_every == 0 || n == plan.ticks {
            digests.push(gameplay.inspect_scale_world(measure::digest).await);
        }
        if plan.sample_every > 0 && n % plan.sample_every == 0 {
            let seats = plan.seats;
            samples.push(
                gameplay
                    .inspect_scale_world(|world| measure::sample(world, seats))
                    .await,
            );
        }
        if n % 100 == 0 {
            eprintln!(
                "game score: {} workers, tick {n} of {}, {:.1} ms per tick so far",
                plan.workers,
                plan.ticks,
                tick_time.as_secs_f64() * 1e3 / n as f64
            );
        }
    }
    let seats = plan.seats;
    let counters = gameplay
        .inspect_scale_world(|world| measure::counters(world, seats, hits, &samples))
        .await;
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
    Ok(Measurement {
        workers: plan.workers,
        ticks: plan.ticks,
        seats: plan.seats,
        setup_seconds,
        tick_seconds,
        ticks_per_second: plan.ticks as f64 / tick_seconds.max(1e-9),
        slowest_tick: slowest_tick(&tick_ms.0),
        tick_ms: tick_ms.summary(),
        peak_memory_mib: memory()["peak_rss_mib"].as_u64().unwrap_or(0),
        counters,
        samples,
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
