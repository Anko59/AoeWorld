//! Long natural matches on a stored map, using the live server's Rust seats.
mod gold;
mod recorder;
#[cfg(test)]
mod tests;

use super::{BoxError, resolve_hash, revision};
use aoe_server::{HeadlessMatch, HeadlessPlan, MAX_MATCH_SLOTS};
use aoe_simulation::TECH_RULES;
use clap::Args;
use recorder::Recorder;
use serde::Serialize;
use serde_json::json;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Args, Debug)]
pub struct Options {
    /// Stored map packages and their terrain pages. Defaults to the game-score directory.
    #[arg(long)]
    package_directory: Option<PathBuf>,
    /// Package hash. Required when the directory holds more than one map.
    #[arg(long)]
    map_hash: Option<String>,
    /// Seat counts, played in sequence. Each size starts from Dark Age.
    #[arg(long, value_delimiter = ',', default_value = "8,100")]
    seats: Vec<usize>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Worker threads for the simulation and Rust AI.
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=64))]
    workers: u32,
    /// Game minutes to play at most per match.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u32).range(1..=360))]
    game_minutes: u32,
    /// Wall minutes allowed per match, including setup.
    #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u32).range(1..=220))]
    wall_minutes: u32,
    /// Directory for the JSON Lines streams. Existing streams are preserved.
    #[arg(long, default_value = ".cache/balance/run")]
    out: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stop {
    TickTarget,
    WallLimit,
}

fn stop(tick: u64, target: u64, elapsed: Duration, bound: Duration) -> Option<Stop> {
    if tick == target {
        Some(Stop::TickTarget)
    } else if elapsed >= bound {
        Some(Stop::WallLimit)
    } else {
        None
    }
}

pub(crate) fn run(options: Options) -> Result<(), BoxError> {
    if options.seats.is_empty()
        || options
            .seats
            .iter()
            .any(|&size| !(2..=MAX_MATCH_SLOTS).contains(&size))
        || options
            .seats
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != options.seats.len()
    {
        return Err(
            format!("balance-match: use distinct seat counts from 2 to {MAX_MATCH_SLOTS}").into(),
        );
    }
    let directory = options
        .package_directory
        .clone()
        .unwrap_or_else(super::pr::home::default_packages);
    let hash = resolve_hash(&directory, options.map_hash.as_deref())?;
    fs::create_dir_all(&options.out)?;
    if options.seats.len() == 1 {
        let workers = options.workers as usize;
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build_global()?;
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers.max(2))
            .enable_all()
            .build()?
            .block_on(play(&options, directory, hash, options.seats[0]));
    }
    // The headless server retains its in-process seats and their map state for
    // the process lifetime. Release all of it before starting the next match.
    for size in &options.seats {
        let status = Command::new(std::env::current_exe()?)
            .arg("balance-match")
            .arg("--package-directory")
            .arg(&directory)
            .arg("--map-hash")
            .arg(&hash)
            .arg("--out")
            .arg(&options.out)
            .args(["--seats", &size.to_string()])
            .args(["--seed", &options.seed.to_string()])
            .args(["--workers", &options.workers.to_string()])
            .args(["--game-minutes", &options.game_minutes.to_string()])
            .args(["--wall-minutes", &options.wall_minutes.to_string()])
            .status()?;
        if !status.success() {
            return Err(format!("balance-match: {size}-seat child {status}").into());
        }
    }
    Ok(())
}

fn write(stream: &mut BufWriter<File>, line: &impl Serialize) -> Result<(), BoxError> {
    serde_json::to_writer(&mut *stream, line)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

async fn play(
    options: &Options,
    directory: PathBuf,
    hash: String,
    seats: usize,
) -> Result<(), BoxError> {
    let started = Instant::now();
    let wall_bound = Duration::from_secs(60 * u64::from(options.wall_minutes));
    let path = options.out.join(format!("{seats}-seats.jsonl"));
    let mut stream = BufWriter::new(File::create_new(&path)?);
    let plan = HeadlessPlan {
        package_directory: directory,
        map_hash: hash,
        seats,
        seed: options.seed,
        late_game_percent: None,
        record_orders: false,
    };
    eprintln!(
        "balance-match: setting up {seats} seats on {}",
        plan.map_hash
    );
    let game = tokio::time::timeout(wall_bound, HeadlessMatch::start(&plan)).await??;
    let service = game.gameplay();
    let (tick_hz, partial_kits, dimensions) = service
        .inspect_scale_world(|w| {
            let c = w.config();
            (
                c.tick_hz,
                w.partial_starting_kits(),
                [c.width_tiles, c.height_tiles],
            )
        })
        .await;
    let minute = u64::from(tick_hz) * 60;
    let target = minute * u64::from(options.game_minutes);
    let techs: Vec<_> = TECH_RULES.iter().map(|rules| {
        json!({"tech": rules.tech, "price": rules.cost, "research_seconds": rules.seconds})
    }).collect();
    write(
        &mut stream,
        &json!({
            "kind": "match",
            "revision": revision(),
            "map_hash": plan.map_hash,
            "dimensions": dimensions,
            "rust_ai": true,
            "seats": seats,
            "seed": options.seed,
            "workers": options.workers,
            "tick_hz": tick_hz,
            "target_ticks": target,
            "wall_limit_seconds": wall_bound.as_secs(),
            "setup_seconds": started.elapsed().as_secs_f64(),
            "partial_starting_kits": partial_kits,
            "resource_order": ["food", "wood", "gold", "stone"],
            "technologies": techs,
        }),
    )?;

    let mut recorder = service
        .inspect_scale_world(|w| Recorder::new(w, seats))
        .await?;
    for tick in 0..=target {
        let ending = stop(tick, target, started.elapsed(), wall_bound);
        let scheduled = tick.is_multiple_of(minute) || ending.is_some();
        if let Some(sample) = service
            .inspect_scale_world(|w| recorder.observe(w, scheduled))
            .await?
        {
            write(&mut stream, &json!({"kind": "sample", "data": sample}))?;
        }
        if tick.is_multiple_of(5 * minute) {
            eprintln!(
                "balance-match: {seats} seats, {} game min, {:.1} wall s",
                tick / minute,
                started.elapsed().as_secs_f64()
            );
        }
        if let Some(reason) = ending {
            write(
                &mut stream,
                &json!({
                    "kind": "end",
                    "reason": reason,
                    "tick": tick,
                    "wall_seconds": started.elapsed().as_secs_f64(),
                }),
            )?;
            eprintln!(
                "balance-match: {reason:?} at tick {tick}, wrote {}",
                path.display()
            );
            break;
        }
        game.tick().await;
    }
    Ok(())
}
