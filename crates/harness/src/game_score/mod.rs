//! `game-score`: a real game on a real map, played by the Rust AI that runs in
//! the server, in process, with no socket and no clock (`HeadlessMatch`), and
//! measured. This first scenario is the early game: a fresh start with every
//! seat played by a Rust AI. Each worker count plays it in its own process
//! (the engine's thread pool is fixed per process), and the runs must agree
//! on every hash and every order. See docs/scale/game-score.md.
mod children;
mod judge;
mod measure;
mod play;
mod report;
#[cfg(test)]
mod tests;

use clap::Args;
use judge::Comparison;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) type BoxError = Box<dyn std::error::Error>;

#[derive(Args, Debug, Clone)]
pub struct Options {
    /// The directory that holds the map package (`<hash>.json` and its pages).
    #[arg(long, default_value = ".cache/game-score/packages")]
    package_directory: PathBuf,
    /// The package to play on. Omitted: the only package of the directory.
    #[arg(long)]
    map_hash: Option<String>,
    /// Seats, all played by the Rust AI.
    #[arg(long, default_value_t = 100)]
    seats: usize,
    /// Ticks played; the game runs at ten ticks per game second, so the
    /// default is one game minute (the engine's window of deliveries).
    #[arg(long, default_value_t = 600)]
    ticks: u64,
    /// Seeds the village layout.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Worker counts, comma separated, 0 for this machine's cores. Each count
    /// plays the match in its own process and every run is compared with the
    /// first. Repeating a count compares two runs at that count.
    #[arg(long, value_delimiter = ',', default_value = "1,0")]
    workers: Vec<usize>,
    /// Hash the world every this many ticks (and at the end).
    #[arg(long, default_value_t = 10)]
    hash_every: u64,
    /// Count the census every this many ticks (0 for the end only).
    #[arg(long, default_value_t = 50)]
    sample_every: u64,
    /// Keep no order log: times the run as a live server runs, and leaves the
    /// orders out of the comparison.
    #[arg(long)]
    no_orders: bool,
    #[arg(long, default_value = ".cache/game-score/game-score.json")]
    out: PathBuf,
    /// Internal: play one match and write its measurement here.
    #[arg(long, hide = true)]
    child: Option<PathBuf>,
}

/// One match for one process to play.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Plan {
    pub package_directory: PathBuf,
    pub map_hash: String,
    pub seats: usize,
    pub ticks: u64,
    pub seed: u64,
    pub workers: usize,
    pub hash_every: u64,
    pub sample_every: u64,
    pub record_orders: bool,
}

impl Options {
    fn plan(&self, map_hash: &str, workers: usize) -> Plan {
        Plan {
            package_directory: self.package_directory.clone(),
            map_hash: map_hash.to_owned(),
            seats: self.seats,
            ticks: self.ticks,
            seed: self.seed,
            workers,
            hash_every: self.hash_every,
            sample_every: self.sample_every,
            record_orders: !self.no_orders,
        }
    }
}

impl Plan {
    /// The arguments that make `game-score` play this plan and write `out`.
    pub(super) fn child_args(&self, out: &Path) -> Vec<String> {
        let mut args = vec![
            "game-score".to_owned(),
            "--package-directory".into(),
            self.package_directory.display().to_string(),
            "--map-hash".into(),
            self.map_hash.clone(),
            "--seats".into(),
            self.seats.to_string(),
            "--ticks".into(),
            self.ticks.to_string(),
            "--seed".into(),
            self.seed.to_string(),
            "--workers".into(),
            self.workers.to_string(),
            "--hash-every".into(),
            self.hash_every.to_string(),
            "--sample-every".into(),
            self.sample_every.to_string(),
            "--child".into(),
            out.display().to_string(),
        ];
        if !self.record_orders {
            args.push("--no-orders".into());
        }
        args
    }
}

/// The replay that finds the exact first tick where `failed` differs: the
/// base run's plan, hashed at every tick up to the coarse difference.
fn replay_plan(options: &Options, hash: &str, failed: &Comparison) -> Plan {
    Plan {
        ticks: failed.first_differing_tick.unwrap_or(options.ticks),
        hash_every: 1,
        ..options.plan(hash, failed.base_workers)
    }
}

/// The hash of the one package in `directory`, unless one is given.
fn resolve_hash(directory: &Path, given: Option<&str>) -> Result<String, BoxError> {
    if let Some(hash) = given {
        return Ok(hash.to_owned());
    }
    let mut hashes = Vec::new();
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("package directory {}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry?.path();
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && let Some(stem) = stem
            && stem.len() == 64
            && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            hashes.push(stem.to_owned());
        }
    }
    hashes.sort();
    match hashes.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "no map package in {}: pass --package-directory",
            directory.display()
        )
        .into()),
        many => Err(format!(
            "{} map packages in {}: pass --map-hash, one of {many:?}",
            many.len(),
            directory.display()
        )
        .into()),
    }
}

/// The worker counts to play: 0 stands for this machine's cores.
fn worker_counts(requested: &[usize], cores: usize) -> Result<Vec<usize>, BoxError> {
    if requested.is_empty() {
        return Err("game-score: --workers names no worker count".into());
    }
    Ok(requested
        .iter()
        .map(|&count| if count == 0 { cores.max(1) } else { count })
        .collect())
}

fn revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub fn run(options: Options) -> Result<(), BoxError> {
    if let Some(out) = &options.child {
        let workers = *options
            .workers
            .first()
            .ok_or("game-score: no worker count")?;
        if workers == 0 {
            return Err("game-score: a child plays at a given worker count".into());
        }
        let hash = resolve_hash(&options.package_directory, options.map_hash.as_deref())?;
        return play::run(&options.plan(&hash, workers), out);
    }
    let hash = resolve_hash(&options.package_directory, options.map_hash.as_deref())?;
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    let counts = worker_counts(&options.workers, cores)?;
    let directory = options
        .out
        .parent()
        .unwrap_or(Path::new("."))
        .join("children");
    fs::create_dir_all(&directory)?;
    let mut runs = Vec::new();
    for (index, &workers) in counts.iter().enumerate() {
        eprintln!(
            "game score: {} seats, {} ticks, {workers} workers, map {}",
            options.seats,
            options.ticks,
            &hash[..8.min(hash.len())]
        );
        let out = directory.join(format!("run-{index}-workers-{workers}.json"));
        runs.push(children::run(&options.plan(&hash, workers), &out)?);
    }
    let mut verdict = judge::judge(&runs);
    if let Some(failed) = verdict
        .against
        .iter()
        .find(|comparison| !comparison.holds())
    {
        let replay = replay_plan(&options, &hash, failed);
        verdict.diagnosis = Some(children::diagnose(&replay, failed.workers, &directory)?);
    }
    let title = format!(
        "game score, early game: {} seats, {} ticks, seed {}, map {}",
        options.seats,
        options.ticks,
        options.seed,
        &hash[..8.min(hash.len())]
    );
    let table = report::table(&title, &runs, &verdict);
    let header = json!({
        "scenario": "early",
        "revision": revision(),
        "command": std::env::args().collect::<Vec<_>>(),
        "map_hash": hash,
        "package_directory": options.package_directory,
        "seats": options.seats,
        "ticks": options.ticks,
        "seed": options.seed,
        "hash_every": options.hash_every,
        "orders_recorded": !options.no_orders,
        "cores": cores,
        // Wall times on a shared machine mean little without its load.
        "load": fs::read_to_string("/proc/loadavg").unwrap_or_default().trim(),
    });
    let report = report::json(header, &runs, &verdict, &table);
    fs::write(&options.out, serde_json::to_string_pretty(&report)? + "\n")?;
    eprintln!("{table}");
    eprintln!("game score: wrote {}", options.out.display());
    if verdict.holds {
        Ok(())
    } else {
        Err("game score: runs at different worker counts played different games".into())
    }
}
