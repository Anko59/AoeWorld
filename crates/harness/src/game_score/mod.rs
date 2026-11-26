//! `game-score`: real games on a real map, played by the Rust AI that runs in
//! the server, in process, with no socket and no clock (`HeadlessMatch`), and
//! measured. Each scenario (`scenarios`) names how its match starts, how long
//! it is played and which windows of it are reported; each worker count plays
//! it in a child process of its own (the engine's thread pool is fixed per
//! process), and the runs must agree on every hash and every order. The
//! `counters` read the world after each tick; `windows` turns what they kept
//! into numbers; `report` writes the table and the JSON.
//! See docs/scale/game-score.md.
mod children;
mod counters;
mod judge;
mod measure;
mod plan;
mod play;
mod report;
mod scenarios;
#[cfg(test)]
mod tests;
mod windows;

use clap::Args;
use plan::Plan;
use report::Played;
use scenarios::Scenario;
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
    /// The scenario to play: `natural` or `late`. Omitted: the pull request set.
    #[arg(long)]
    scenario: Option<String>,
    /// Seats, all played by the Rust AI.
    #[arg(long, default_value_t = 100)]
    seats: usize,
    /// Ticks played (ten ticks are one game second). Omitted: the scenario's
    /// own length; a window that does not fit in fewer ticks is left out.
    #[arg(long)]
    ticks: Option<u64>,
    /// Seeds the village layout.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// The scale of the late-game recipe in percent (0 keeps the owner's
    /// shapes). Omitted: the scenario's own.
    #[arg(long)]
    late_percent: Option<u32>,
    /// Worker counts, comma separated, 0 for this machine's cores. Each count
    /// plays the scenario in its own process and every run is compared with
    /// the first, which gives the numbers. Repeating a count compares two
    /// runs at that count.
    #[arg(long, value_delimiter = ',', default_value = "1,0")]
    workers: Vec<usize>,
    /// Hash the world every this many ticks (and at the end). Omitted: the
    /// scenario's own.
    #[arg(long)]
    hash_every: Option<u64>,
    /// Take the census every this many ticks. Omitted: the scenario's own.
    #[arg(long)]
    sample_every: Option<u64>,
    /// Keep no order log: times the run as a live server runs, and leaves the
    /// orders out of the comparison.
    #[arg(long)]
    no_orders: bool,
    #[arg(long, default_value = ".cache/game-score/game-score.json")]
    out: PathBuf,
    /// Internal: play one scenario once and write its measurement here.
    #[arg(long, hide = true)]
    child: Option<PathBuf>,
}

impl Options {
    fn plan(&self, scenario: &Scenario, map_hash: &str, workers: usize) -> Plan {
        Plan {
            package_directory: self.package_directory.clone(),
            map_hash: map_hash.to_owned(),
            scenario: scenario.id.to_owned(),
            seats: self.seats,
            ticks: self.ticks.unwrap_or(scenario.ticks),
            seed: self.seed,
            late_percent: self.late_percent,
            workers,
            hash_every: self.hash_every.unwrap_or(scenario.hash_every),
            sample_every: self.sample_every.unwrap_or(scenario.sample_every),
            record_orders: !self.no_orders,
        }
    }

    /// The scenarios to play: the one named, or the pull request set.
    fn scenarios(&self) -> Result<Vec<&'static Scenario>, BoxError> {
        match &self.scenario {
            Some(id) => Ok(vec![scenarios::find(id)?]),
            None => Ok(scenarios::pull_request_set()),
        }
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

/// The worker counts one scenario is played at: a scenario that is not the
/// same game at every count is played twice at its own.
fn counts_for(scenario: &Scenario, requested: &[usize]) -> Vec<usize> {
    match scenario.fixed_workers {
        Some(count) => vec![count, count],
        None => requested.to_vec(),
    }
}

fn revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// Plays `scenario` at each of `counts` and compares the runs.
fn play_scenario(
    options: &Options,
    scenario: &'static Scenario,
    hash: &str,
    counts: &[usize],
    directory: &Path,
) -> Result<Played, BoxError> {
    let plan = options.plan(scenario, hash, counts.first().copied().unwrap_or(1));
    let mut runs = Vec::new();
    for (index, &workers) in counts.iter().enumerate() {
        eprintln!(
            "game score: {}, {} seats, {} ticks, {workers} workers, map {}",
            scenario.id,
            plan.seats,
            plan.ticks,
            &hash[..8.min(hash.len())]
        );
        let out = directory.join(format!(
            "{}-run-{index}-workers-{workers}.json",
            scenario.id
        ));
        let run = children::run(
            &Plan {
                workers,
                ..plan.clone()
            },
            &out,
        )?;
        eprintln!(
            "game score: {} at {workers} workers took {:.1} s (start {:.1} s)",
            scenario.id, run.native_seconds, run.setup_seconds
        );
        runs.push(run);
    }
    let mut verdict = judge::judge(&runs);
    if let Some(failed) = verdict
        .against
        .iter()
        .find(|comparison| !comparison.holds())
    {
        let prefix = format!("{}-", scenario.id);
        verdict.diagnosis = Some(children::diagnose(
            &plan.replay(failed),
            failed.workers,
            directory,
            &prefix,
        )?);
    }
    Ok(Played {
        scenario,
        plan,
        runs,
        verdict,
    })
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
        let id = options
            .scenario
            .as_deref()
            .ok_or("game-score: a child plays one named scenario")?;
        let hash = resolve_hash(&options.package_directory, options.map_hash.as_deref())?;
        return play::run(&options.plan(scenarios::find(id)?, &hash, workers), out);
    }
    let selected = options.scenarios()?;
    let hash = resolve_hash(&options.package_directory, options.map_hash.as_deref())?;
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    let counts = worker_counts(&options.workers, cores)?;
    let directory = options
        .out
        .parent()
        .unwrap_or(Path::new("."))
        .join("children");
    fs::create_dir_all(&directory)?;
    let mut played = Vec::new();
    for scenario in selected {
        let counts = counts_for(scenario, &counts);
        played.push(play_scenario(
            &options, scenario, &hash, &counts, &directory,
        )?);
    }
    let title = format!(
        "game score: {} seats, seed {}, map {}",
        options.seats,
        options.seed,
        &hash[..8.min(hash.len())]
    );
    let table = report::table(&title, &played);
    let header = json!({
        "revision": revision(),
        "command": std::env::args().collect::<Vec<_>>(),
        "map_hash": hash,
        "package_directory": options.package_directory,
        "orders_recorded": !options.no_orders,
        "cores": cores,
        // Wall times on a shared machine mean little without its load.
        "load": fs::read_to_string("/proc/loadavg").unwrap_or_default().trim(),
    });
    let report = report::json(header, &played, &table);
    fs::write(&options.out, serde_json::to_string_pretty(&report)? + "\n")?;
    eprintln!("{table}");
    eprintln!("game score: wrote {}", options.out.display());
    if played.iter().all(|scenario| scenario.verdict.holds) {
        Ok(())
    } else {
        Err("game score: runs at different worker counts played different games".into())
    }
}
