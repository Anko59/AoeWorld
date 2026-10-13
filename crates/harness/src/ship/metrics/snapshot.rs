//! `make metrics`: the metrics of one commit as a JSON file in the git common
//! directory (`aoe-ship/metrics/<sha>.json`), printed too. Static values are
//! computed from the commit's blobs; measured values are only read.
use super::{Metrics, measured::Measured, repository};
use crate::ship::{evidence, git};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::ship) struct Snapshot {
    pub(in crate::ship) version: u16,
    pub(in crate::ship) revision: String,
    #[serde(rename = "static")]
    pub(in crate::ship) statics: Metrics,
    pub(in crate::ship) measured: Measured,
}

pub(in crate::ship) fn path(root: &Path, sha: &str) -> Result<PathBuf, String> {
    Ok(evidence::directory(root)?
        .with_file_name("metrics")
        .join(format!("{sha}.json")))
}

fn stored(root: &Path, sha: &str) -> Option<Snapshot> {
    serde_json::from_slice(&fs::read(path(root, sha).ok()?).ok()?).ok()
}

/// Metrics of `revision`. Measured values come from the reports that exist now
/// for that commit, else from an earlier snapshot of the same commit.
pub(in crate::ship) fn compute(root: &Path, revision: &str) -> Result<Snapshot, String> {
    let sha = git::git(
        root,
        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )?;
    let measured = super::measured::read(root, &sha);
    Ok(Snapshot {
        version: 1,
        measured: match stored(root, &sha) {
            Some(earlier) => measured.or(earlier.measured),
            None => measured,
        },
        statics: repository(root, &sha)?,
        revision: sha,
    })
}

pub(in crate::ship) fn write(root: &Path, snapshot: &Snapshot) -> Result<PathBuf, String> {
    let target = path(root, &snapshot.revision)?;
    let dir = target.parent().ok_or("no metrics directory")?;
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let json = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(&target, json).map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(target)
}

/// `make metrics`; `METRICS_REV` names another commit than HEAD.
pub(in crate::ship) fn command(root: &Path) -> Result<(), String> {
    let revision = std::env::var("METRICS_REV")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "HEAD".to_owned());
    if revision.starts_with('-') {
        return Err(format!("METRICS_REV={revision}: not a revision"));
    }
    let snapshot = compute(root, &revision)?;
    let target = write(root, &snapshot)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string())?
    );
    eprintln!("metrics: wrote {}", target.display());
    Ok(())
}
