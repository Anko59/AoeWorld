//! One run for one process to play, and how the parent hands it to a child.
use super::judge::Comparison;
use std::path::{Path, PathBuf};

/// One scenario at one worker count: everything a child needs to play it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Plan {
    pub package_directory: PathBuf,
    pub map_hash: String,
    /// The id of the scenario (`scenarios`).
    pub scenario: String,
    pub seats: usize,
    /// Ticks played, the scenario's own length unless a flag changed it.
    pub ticks: u64,
    pub seed: u64,
    /// The late-game scale a flag asked for, over the scenario's own.
    pub late_percent: Option<u32>,
    pub workers: usize,
    pub hash_every: u64,
    pub sample_every: u64,
    pub record_orders: bool,
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
            "--scenario".into(),
            self.scenario.clone(),
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
        if let Some(percent) = self.late_percent {
            args.extend(["--late-percent".to_owned(), percent.to_string()]);
        }
        if !self.record_orders {
            args.push("--no-orders".into());
        }
        args
    }

    /// The replay that finds the exact first tick where `failed` differs:
    /// this plan at the base run's workers, hashed at every tick up to the
    /// coarse difference. One tick more than that tick: an order logged at
    /// tick T is applied by the call that makes tick T + 1, and its effect
    /// shows only then.
    pub(super) fn replay(&self, failed: &Comparison) -> Self {
        Self {
            ticks: failed
                .first_differing_tick
                .map_or(self.ticks, |tick| (tick + 1).min(self.ticks)),
            hash_every: 1,
            workers: failed.base_workers,
            ..self.clone()
        }
    }
}
