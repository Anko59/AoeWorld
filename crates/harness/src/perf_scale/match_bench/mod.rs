//! `match-bench`: the closed-loop deterministic in-process match (movement
//! rework phase 0, rung 0). The owner's late game of 2026-11-06 seeded at
//! `--seats` and `--scale` (1x is about 12,000 units at 100 seats), played by
//! minimal Rust bots (`bots.rs`) without a server or Python hosts. Reports
//! ticks/s and per-phase time per scale and worker count, the route queue
//! over time, the forbidden-pass counters, and whether the canonical hashes
//! of every worker count agree. `--callgrind` instead re-runs itself under
//! Callgrind and splits the counted ticks into stages (`counted.rs`).
mod bots;
mod build;
mod counted;
mod run;
#[cfg(test)]
mod tests;

use super::world::BoxError;
use build::Shape;
use clap::Args;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Args, Debug, Clone)]
pub struct Options {
    #[arg(long, default_value_t = 100)]
    seats: u16,
    /// Late-game load factors, comma separated (1 = the owner's shape).
    #[arg(long, value_delimiter = ',', default_value = "1")]
    scale: Vec<f64>,
    /// Map side in tiles; 0 keeps the owner's density (2,000 at 100 seats, 1x).
    #[arg(long, default_value_t = 0)]
    side_tiles: u32,
    #[arg(long, default_value_t = 2_000)]
    ticks: u64,
    /// Rayon worker counts, comma separated; every count replays the same match.
    #[arg(long, value_delimiter = ',', default_value = "6")]
    threads: Vec<usize>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 100)]
    hash_every: u64,
    #[arg(long, default_value_t = 100)]
    sample_every: u64,
    #[arg(long, default_value = ".cache/match-bench/match-bench.json")]
    out: PathBuf,
    /// Count instructions per stage with Callgrind instead of timing.
    #[arg(long)]
    callgrind: bool,
    /// Ticks played before the counted window (`--callgrind`).
    #[arg(long, default_value_t = 200)]
    warmup: u64,
    /// Ticks counted (`--callgrind`).
    #[arg(long, default_value_t = 5)]
    counted: u64,
    /// Internal: the process Callgrind runs.
    #[arg(long, hide = true)]
    counted_child: Option<PathBuf>,
    /// Compare the hash sequences of earlier reports instead of running.
    #[arg(long, value_delimiter = ',')]
    compare: Vec<PathBuf>,
    /// `phase0`: rung 0 (100 seats, 1x/5x/10x, at most 1,000 ticks), worker determinism (1x at
    /// 1/2/6/12), 1,000 seats for `--ticks`, and Callgrind at 1x/5x/10x, one
    /// report each beside `--out`. Other flags give the shared seed and ticks.
    #[arg(long)]
    suite: Option<String>,
    /// Ticks of the suite's determinism replay.
    #[arg(long, default_value_t = 3_000)]
    determinism_ticks: u64,
}

fn suite(options: &Options) -> Result<(), BoxError> {
    let dir = options
        .out
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let base = Options {
        suite: None,
        ..options.clone()
    };
    let studies = [
        Options {
            seats: 100,
            scale: vec![1.0, 5.0, 10.0],
            threads: vec![6],
            ticks: options.ticks.min(1_000),
            out: dir.join("rung0.json"),
            ..base.clone()
        },
        Options {
            seats: 100,
            scale: vec![1.0],
            threads: vec![1, 2, 6, 12],
            ticks: options.determinism_ticks,
            out: dir.join("determinism.json"),
            ..base.clone()
        },
        Options {
            seats: 1_000,
            scale: vec![1.0],
            threads: vec![6],
            out: dir.join("seats1000.json"),
            ..base.clone()
        },
        Options {
            seats: 100,
            scale: vec![1.0, 5.0, 10.0],
            callgrind: true,
            out: dir.join("stages.json"),
            ..base
        },
    ];
    let mut failed = Vec::new();
    for study in studies {
        let name = study.out.display().to_string();
        eprintln!("match-bench suite: {name}");
        if let Err(error) = run(study) {
            eprintln!("match-bench suite: {name} failed: {error}");
            failed.push(name);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("match-bench suite: failed {failed:?}").into())
    }
}

fn shape(options: &Options, scale: f64) -> Shape {
    Shape {
        seats: options.seats,
        scale,
        side_tiles: options.side_tiles,
        seed: options.seed,
    }
}

/// The first `(tick, a, b)` where two hash sequences differ, `None` when every
/// common tick agrees.
pub(super) fn divergence(
    a: &[(u64, String)],
    b: &[(u64, String)],
) -> Option<(u64, String, String)> {
    a.iter()
        .zip(b)
        .find(|(x, y)| x != y)
        .map(|(x, y)| (x.0.min(y.0), x.1.clone(), y.1.clone()))
}

fn hashes_of(report: &Value) -> Vec<(u64, String)> {
    report["hashes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| Some((h[0].as_u64()?, h[1].as_str()?.to_owned())))
        .collect()
}

/// Determinism verdict over runs of one scale: every run against the first.
fn determinism(runs: &[Value]) -> Value {
    let Some(first) = runs.first() else {
        return json!(null);
    };
    let base = hashes_of(first);
    let rows: Vec<Value> = runs
        .iter()
        .skip(1)
        .map(|run| {
            let other = hashes_of(run);
            match divergence(&base, &other) {
                None => json!({ "threads": run["threads"], "equal": true,
                    "compared": base.len().min(other.len()) }),
                Some((tick, a, b)) => json!({ "threads": run["threads"], "equal": false,
                    "first_divergent_tick": tick, "base": a, "other": b }),
            }
        })
        .collect();
    json!({ "base_threads": first["threads"], "against": rows,
        "all_equal": rows.iter().all(|r| r["equal"] == json!(true)) })
}

fn table(runs: &[Value]) -> String {
    let mut text = String::from(
        "scale threads  units  movers  ticks/s  p50 ms  p95 ms  movement  spacing  combat  gathering  steward  memory  changed  waiting\n",
    );
    for r in runs {
        let phase = |name: &str| r["phases"][name]["p50_ms"].as_f64().unwrap_or(0.0);
        text.push_str(&format!(
            "{:>5} {:>7} {:>6} {:>7} {:>8.2} {:>7.1} {:>7.1} {:>9.1} {:>8.1} {:>7.1} {:>10.1} {:>8.1} {:>7.1} {:>8.1} {:>8}\n",
            r["scale"], r["threads"], r["end"]["units"], r["end"]["movers"],
            r["ticks_per_s"].as_f64().unwrap_or(0.0),
            r["tick"]["p50_ms"].as_f64().unwrap_or(0.0),
            r["tick"]["p95_ms"].as_f64().unwrap_or(0.0),
            phase("movement_and_path_planning"), phase("unit_spacing"), phase("combat"),
            phase("gathering_and_economy"), phase("steward_economy"),
            phase("building_memory_per_player"), phase("result_collation"),
            r["end"]["waiting_units"],
        ));
    }
    text
}

fn write(path: &PathBuf, value: &Value) -> Result<(), BoxError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

fn compare(paths: &[PathBuf]) -> Result<(), BoxError> {
    let mut runs = Vec::new();
    for path in paths {
        let report: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        for run in report["runs"].as_array().into_iter().flatten() {
            runs.push(run.clone());
        }
    }
    let verdict = determinism(&runs);
    println!("{}", serde_json::to_string_pretty(&verdict)?);
    if verdict["all_equal"] != json!(true) {
        return Err("match-bench: hash sequences differ".into());
    }
    Ok(())
}

pub fn run(options: Options) -> Result<(), BoxError> {
    if !options.compare.is_empty() {
        return compare(&options.compare);
    }
    match options.suite.as_deref() {
        Some("phase0") => return suite(&options),
        Some(other) => return Err(format!("match-bench: unknown suite {other}").into()),
        None => {}
    }
    if let Some(out) = &options.counted_child {
        let scale = options.scale.first().copied().unwrap_or(1.0);
        let value = run::counted_child(&shape(&options, scale), options.warmup, options.counted)?;
        return write(out, &value);
    }
    if options.callgrind {
        return counted::run(&options);
    }
    let mut scales = Vec::new();
    let mut all = Vec::new();
    for &scale in &options.scale {
        let mut runs = Vec::new();
        for &threads in &options.threads {
            let plan = run::Plan {
                shape: shape(&options, scale),
                threads,
                ticks: options.ticks,
                hash_every: options.hash_every,
                sample_every: options.sample_every,
            };
            let outcome = run::run(&plan)?;
            let mut report = outcome.report;
            report["hashes"] = json!(outcome.hashes);
            eprintln!("{}", table(std::slice::from_ref(&report)));
            runs.push(report);
        }
        scales.push(json!({ "scale": scale, "determinism": determinism(&runs) }));
        all.extend(runs);
    }
    eprint!("{}", table(&all));
    let report = json!({
        "version": 1,
        "command": std::env::args().collect::<Vec<_>>(),
        "revision": super::git(&["rev-parse", "HEAD"]),
        "load": fs::read_to_string("/proc/loadavg").unwrap_or_default().trim(),
        "note": "closed-loop in-process match, Rust bots; wall time on a shared machine (load recorded), not pinned cores",
        "determinism": scales,
        "runs": all,
    });
    write(&options.out, &report)?;
    eprintln!("match-bench: wrote {}", options.out.display());
    for scale in &scales {
        if scale["determinism"]["all_equal"] == json!(false) {
            eprintln!(
                "match-bench: hashes differ between worker counts at scale {}",
                scale["scale"]
            );
        }
    }
    Ok(())
}
