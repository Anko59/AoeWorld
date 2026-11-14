//! The latest passing `dev` instruction measurement, the second reference of
//! the instruction gate. `make perf-dev-reference` (CI, pull requests) writes
//! `reports/perf/dev-reference.json` from the same dev run the performance
//! comment compares against; the gate (`perf-ci`, no network) only reads it.
//! An unusable reference never passes silently: the gate says why and keeps
//! the committed-baseline checks.
use aoe_scenario::{LATE_GAME, SMOKE, TARGET_DISTRIBUTED};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

pub const PATH: &str = "reports/perf/dev-reference.json";

/// One dev run's instruction counts and the workload identity they were
/// measured with. Counts of different workloads are not comparable.
#[derive(Debug, Deserialize, Serialize)]
pub struct DevReference {
    pub run: u64,
    pub revision: String,
    pub workloads: BTreeMap<String, String>,
    /// Kernel name without the `instructions::` prefix to Ir.
    pub instructions: BTreeMap<String, u64>,
}

/// What the fetch step left for the gate.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DevFile {
    Unavailable { unavailable: String },
    Measured(DevReference),
}

/// Late-game ticks the instruction kernels count after the warm-up; part of
/// the identity, since an eight-tick count is not comparable with one tick.
pub const LATE_GAME_TICKS: usize = 8;

/// The workload hashes the instruction kernels run, recorded in every
/// `ci.json` so a later run can tell whether its counts are comparable.
pub fn identity() -> BTreeMap<String, String> {
    [
        ("smoke", SMOKE.workload_hash()),
        ("late_game", LATE_GAME.workload_hash()),
        ("late_game_ticks", LATE_GAME_TICKS.to_string()),
        ("target_distributed", TARGET_DISTRIBUTED.workload_hash()),
    ]
    .into_iter()
    .map(|(name, hash)| (name.to_owned(), hash))
    .collect()
}

/// The reference to gate against, or the reason there is none.
pub fn load(path: &Path) -> Result<DevReference, String> {
    let bytes = fs::read(path).map_err(|_| {
        format!(
            "{} is missing: not a pull request run, or the fetch step did not run",
            path.display()
        )
    })?;
    match serde_json::from_slice::<DevFile>(&bytes) {
        Ok(DevFile::Unavailable { unavailable }) => Err(unavailable),
        Ok(DevFile::Measured(reference)) => usable(reference),
        Err(error) => Err(format!("{} is unreadable: {error}", path.display())),
    }
}

fn usable(reference: DevReference) -> Result<DevReference, String> {
    if reference.workloads != identity() {
        return Err(format!(
            "dev run {} measured different workloads (or predates the identity record)",
            reference.run
        ));
    }
    Ok(reference)
}
