//! `scale-match`: the real game server at its promised scale, end to end.
//!
//! Starts a release `aoe-server`, hosts a match (`POST /match`: one human
//! slot, the rest Python AIs, optional start units), runs
//! the bots in sibling `aoeworld/sdk-python` containers, samples the server's
//! own per-tick records (`GET /match/stats`) with process and container CPU
//! and memory, and optionally drives the human's browser for a frame-rate
//! measurement. Everything is written to `reports/scale/match/match-*.json` and a
//! summary block in `docs/scale/match.md`. Make only starts the container.
mod browser;
mod hosts;
mod http;
mod procs;
mod report;
mod run;
#[cfg(test)]
mod tests;

use super::BoxError;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum MapSource {
    /// A France-centred 1:30 request with no source layers (procedural terrain).
    Procedural,
    /// A package already held by the server's package directory (`--map-hash`).
    Package,
    /// The flat synthetic ground the AI tests use.
    Flat,
}

#[derive(Args, Debug, Clone)]
pub struct Options {
    /// Match seats; the first `--humans` are people, the rest Python AIs.
    #[arg(long, default_value_t = 100)]
    pub players: usize,
    #[arg(long, default_value_t = 1)]
    pub humans: usize,
    /// Map side in two-metre tiles (ratio 30:1 for `procedural`).
    #[arg(long, default_value_t = 16_000)]
    pub tiles: u32,
    /// Villagers every player starts with besides the standard kit (seeded, not played).
    #[arg(long, default_value_t = 0)]
    pub start_villagers: u32,
    /// Militia every player starts with besides the standard kit (seeded, not played).
    #[arg(long, default_value_t = 0)]
    pub start_militia: u32,
    /// Wall-clock minutes the bots play after they are connected.
    #[arg(long, default_value_t = 20.0)]
    pub minutes: f64,
    /// Python host processes; the bots are split evenly among them.
    #[arg(long, default_value_t = 4)]
    pub hosts: usize,
    #[arg(long, value_enum, default_value_t = MapSource::Procedural)]
    pub map: MapSource,
    /// The package to open for `--map package` (64 hex digits).
    #[arg(long)]
    pub map_hash: Option<String>,
    #[arg(long, default_value = "local-assets/map-packages")]
    pub package_directory: PathBuf,
    #[arg(long, default_value_t = 9_100)]
    pub port: u16,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Original-art pack (a `local-assets/packs/<hash>` directory); also `AOE_ASSET_PACK`.
    #[arg(long)]
    pub asset_pack: Option<PathBuf>,
    /// Also drive the human's view in Chromium and record its frame rate.
    #[arg(long)]
    pub browser: bool,
    /// Save the browser's video alongside its report for visual inspection.
    #[arg(long, requires = "browser")]
    pub record_video: bool,
    /// Minutes of browser measurement (half near the village, half over busy ground).
    #[arg(long, default_value_t = 4.0)]
    pub browser_minutes: f64,
    /// Seconds into the run before the browser starts measuring.
    #[arg(long, default_value_t = 120)]
    pub browser_delay: u64,
    /// Where the JSON report goes; default `reports/scale/match/match-<label>-<time>.json`.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Summary file whose generated block is rewritten.
    #[arg(long, default_value = "docs/scale/match.md")]
    pub summary: PathBuf,
    /// Leave the summary file alone (smoke runs).
    #[arg(long)]
    pub no_summary: bool,
    /// Names the run in the report and its file.
    #[arg(long, default_value = "full")]
    pub label: String,
    #[arg(long, default_value = "aoeworld/sdk-python:1.15.0")]
    pub bot_image: String,
    #[arg(long, default_value = "aoeworld/browser-tools:1.64.0")]
    pub browser_image: String,
}

pub fn run(options: Options) -> Result<(), BoxError> {
    run::run(&options)
}
