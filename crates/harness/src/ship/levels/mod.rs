//! Pull request levels (docs/shipping.md#the-pull-request-description): the
//! limits of each level come from `gates/levels.json`, the lowest level a
//! change may use (the floor) is computed from the change, and `make
//! level-stats` compares the levels of merged pull requests with the target.
pub(crate) mod config;
pub(crate) mod stats;
#[cfg(test)]
mod tests;

use super::{describe::Level, git};
use crate::gates::registry::Registry;
pub(crate) use config::Levels;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What the floor rules read about a change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Change {
    pub(crate) lines: u64,
    pub(crate) files: Vec<String>,
    pub(crate) suites: BTreeSet<String>,
}

impl Change {
    #[cfg(test)]
    pub(crate) fn lines(lines: u64) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }

    /// The change from the merge base with `origin/<base>` to HEAD.
    pub(crate) fn collect(root: &Path, levels: &Levels, base: &str, fetch: bool) -> Result<Self> {
        let (_, merge_base) = git::base(root, base, fetch)?;
        let files = git::changed(root, &merge_base)?;
        let numstat = git::git(root, &["diff", "--numstat", &merge_base, "HEAD"])?;
        // The registry of origin/dev, like the review floor: a branch cannot
        // change which suites its own files select.
        let registry =
            Registry::parse(crate::review::trusted(root, "gates/registry.json")?.as_bytes())?;
        levels.check_suites(registry.suites.iter().map(|s| s.id.as_str()))?;
        let suites = registry.classify(&files).suites;
        Ok(Self {
            lines: numstat_lines(&numstat, levels.binary_file_lines),
            files,
            suites,
        })
    }
}

/// Lines added plus removed; a binary file (`-` in numstat) counts as
/// `binary` lines per side.
fn numstat_lines(numstat: &str, binary: u64) -> u64 {
    numstat
        .lines()
        .map(|l| {
            l.split('\t')
                .take(2)
                .map(|n| n.parse::<u64>().unwrap_or(binary))
                .sum::<u64>()
        })
        .sum()
}

/// The lowest level a change may use and the rule that demands it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Floor {
    pub(crate) level: Level,
    pub(crate) reason: String,
}

/// The level a pull request chose, the floor and why, recorded in the ship
/// evidence and shown in the "For AI" part.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Decision {
    pub(crate) chosen: Level,
    pub(crate) floor: Level,
    pub(crate) reason: String,
    /// The metrics row set of the chosen level (selected by `ship/metrics.rs`).
    pub(crate) metrics: String,
}

/// The floor of HEAD against the current base, from the fetched origin ref.
pub(crate) fn current_floor(root: &Path) -> Result<(Floor, Change)> {
    let levels = Levels::load(root)?;
    let change = Change::collect(root, &levels, &crate::review::base::current(), false)?;
    Ok((levels.floor(&change), change))
}

/// `make level-floor`: the lowest level the change allows, then why.
pub(crate) fn print_floor(root: &Path) -> Result<()> {
    let (floor, change) = current_floor(root)?;
    println!("{}", floor.level.name());
    eprintln!("{} ({} changed lines)", floor.reason, change.lines);
    Ok(())
}

/// `make level-stats`: the levels of the last merged pull requests on dev.
pub(crate) fn print_stats(root: &Path) -> Result<()> {
    let levels = Levels::load(root)?;
    println!("{}", stats::read(root, &levels)?.table(&levels));
    Ok(())
}
