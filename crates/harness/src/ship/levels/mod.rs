//! Pull request levels (docs/shipping.md#the-pull-request-description): the
//! limits of each level come from `gates/levels.json`, the lowest level a
//! change may use (the floor) is computed from the change, and `make
//! level-stats` compares the levels of merged pull requests with the target.
pub(crate) mod config;
pub(crate) mod stats;
#[cfg(test)]
mod tests;

use super::{describe::Level, git};
use crate::agents::is_test_path;
use crate::gates::registry::Registry;
pub(crate) use config::{Levels, RowSet};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What the floor rules read about a change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Change {
    /// Every changed line: the sum of the parts.
    pub(crate) lines: u64,
    /// Each changed file with its own lines, so a rule counts only its paths.
    pub(crate) parts: Vec<Part>,
    /// How many of the `parts` are uncommitted (staged, unstaged or untracked
    /// and not ignored); always 0 for the committed change.
    pub(crate) uncommitted: usize,
}

/// Where a change shows on screen: gameplay, rendering, maps, assets and the UI.
const VISIBLE: &[&str] = &[
    "crates/client/",
    "crates/rendering/",
    "crates/simulation/",
    "crates/map/",
    "crates/assets/",
    "crates/scenario/",
    "web/",
    "browser/",
];

impl Change {
    /// Whether a player would see the change: a non-test file under [`VISIBLE`]. Only then
    /// does a PR need a showcase video; harness, protocol, server and tooling changes do not.
    pub(crate) fn visible(&self) -> bool {
        self.parts.iter().any(|part| {
            VISIBLE.iter().any(|prefix| part.path.starts_with(prefix))
                && !crate::agents::is_test_path(&part.path.to_ascii_lowercase())
        })
    }
}

/// One changed file: its lines (added plus removed) and the suites it selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    pub(crate) path: String,
    pub(crate) lines: u64,
    pub(crate) suites: BTreeSet<String>,
    /// Behaviour changes: not a test and not documentation or configuration
    /// (`Levels::is_inert`). Only these count towards a `behaviour` rule.
    pub(crate) behaviour: bool,
}

impl Change {
    #[cfg(test)]
    pub(crate) fn lines(lines: u64) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }

    /// The change of `(path, lines)` pairs classified by `registry`.
    pub(crate) fn from_parts(
        levels: &Levels,
        registry: &Registry,
        counted: Vec<(String, u64)>,
    ) -> Self {
        let parts: Vec<Part> = counted
            .into_iter()
            .map(|(path, lines)| {
                let behaviour = !is_test_path(&path) && !levels.is_inert(&path);
                let suites = registry.classify(std::slice::from_ref(&path)).suites;
                Part {
                    path,
                    lines,
                    suites,
                    behaviour,
                }
            })
            .collect();
        Self {
            lines: parts.iter().map(|part| part.lines).sum(),
            parts,
            uncommitted: 0,
        }
    }

    /// The change from the merge base with `origin/<base>` to HEAD.
    pub(crate) fn collect(root: &Path, levels: &Levels, base: &str, fetch: bool) -> Result<Self> {
        let (_, merge_base) = git::base(root, base, fetch)?;
        let numstat = git::git_raw(
            root,
            &[
                "diff",
                "--numstat",
                "-z",
                "--no-renames",
                &merge_base,
                "HEAD",
            ],
        )?;
        Self::classified(
            root,
            levels,
            numstat_parts(&numstat, levels.binary_file_lines),
        )
    }

    /// The change as `make ship` will see it once committed: the working tree
    /// against the merge base, with the untracked files that are not ignored.
    /// An untracked entry that Git would commit as a symlink counts as one
    /// line; one that cannot be counted (a nested repository, a directory, an
    /// unreadable file) is left out with a note, never an error.
    pub(crate) fn collect_working(
        root: &Path,
        levels: &Levels,
        base: &str,
        fetch: bool,
    ) -> Result<Self> {
        // Description validation precedes the ship's catch-up rebase, including
        // for an unpublished local parent. Count the child's own change here.
        let (_, merge_base) = git::ship_base(root, base, fetch)?;
        let numstat = git::git_raw(
            root,
            &["diff", "--numstat", "-z", "--no-renames", &merge_base],
        )?;
        let mut counted = numstat_parts(&numstat, levels.binary_file_lines);
        // The files that differ from HEAD: the uncommitted part of the change.
        let mut uncommitted: BTreeSet<String> = nul_list(&git::git_raw(
            root,
            &["diff", "--name-only", "-z", "--no-renames", "HEAD"],
        )?)
        .map(str::to_owned)
        .collect();
        for path in untracked(root)? {
            match untracked_lines(&root.join(&path), levels.binary_file_lines) {
                Ok(lines) => {
                    uncommitted.insert(path.clone());
                    counted.push((path, lines));
                }
                Err(reason) => {
                    eprintln!("ship: untracked {path} is left out of the floor ({reason})");
                }
            }
        }
        let mut change = Self::classified(root, levels, counted)?;
        change.uncommitted = change
            .parts
            .iter()
            .filter(|part| uncommitted.contains(&part.path))
            .count();
        Ok(change)
    }

    fn classified(root: &Path, levels: &Levels, counted: Vec<(String, u64)>) -> Result<Self> {
        // The registry of origin/dev, like the review floor: a branch cannot
        // change which suites its own files select.
        let registry =
            Registry::parse(crate::review::trusted(root, "gates/registry.json")?.as_bytes())?;
        levels.check_suites(registry.suites.iter().map(|s| s.id.as_str()))?;
        Ok(Self::from_parts(levels, &registry, counted))
    }
}

/// The records of a NUL-terminated Git list.
fn nul_list(text: &str) -> impl Iterator<Item = &str> {
    text.split('\0').filter(|record| !record.is_empty())
}

/// `(path, lines added plus removed)` of each record of `git diff --numstat
/// -z` (`added<TAB>removed<TAB>path<NUL>`): the path is raw, so a name with a
/// quote, a tab or a non-ASCII byte is matched as it is. A binary file (`-`)
/// counts as `binary` lines.
fn numstat_parts(numstat: &str, binary: u64) -> Vec<(String, u64)> {
    nul_list(numstat)
        .filter_map(|record| {
            let mut columns = record.splitn(3, '\t');
            let added = columns.next()?;
            let removed = columns.next()?;
            let path = columns.next()?;
            let lines = match (added.parse::<u64>(), removed.parse::<u64>()) {
                (Ok(added), Ok(removed)) => added + removed,
                _ => binary,
            };
            Some((path.to_owned(), lines))
        })
        .collect()
}

/// The untracked files that Git does not ignore.
fn untracked(root: &Path) -> Result<Vec<String>> {
    let listed = git::git_raw(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    Ok(nul_list(&listed).map(str::to_owned).collect())
}

/// Lines of an untracked entry, counted as Git counts them when it is added:
/// a symlink is its target path (one line), a regular file its own bytes, and
/// a file with a NUL byte is binary. Anything else (a directory such as a
/// nested repository, a FIFO, an unreadable file) is an `Err` with the reason.
fn untracked_lines(path: &Path, binary: u64) -> std::result::Result<u64, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Ok(1);
    }
    let bytes = if meta.is_file() {
        std::fs::read(path).map_err(|e| e.to_string())?
    } else {
        return Err("not a regular file or symlink, such as a nested repository".to_owned());
    };
    if bytes.contains(&0) {
        return Ok(binary);
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
    Ok(newlines + u64::from(bytes.last().is_some_and(|byte| *byte != b'\n')))
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
    /// The metrics row set of the chosen level (`ship/metrics/table.rs`).
    pub(crate) metrics: RowSet,
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
