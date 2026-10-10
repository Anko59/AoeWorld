//! Evidence for exactly one commit, kept in the Git common directory
//! (`aoe-ship/evidence/<sha>.json`) where the agent policy refuses every write.
//! A new commit has no evidence, so stale evidence cannot stand in for it.
use super::{
    git::{self, Result},
    run::{GateResult, GateVerdict},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Verdict {
    Pass,
    Fail,
    Incomplete,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub(crate) version: u16,
    /// The registry cadence that ran: `preflight` (CI re-runs the `pr`/`ci` gates).
    pub(crate) cadence: String,
    pub(crate) head: String,
    pub(crate) tree: String,
    pub(crate) branch: String,
    /// `origin/<base_branch>` at the run: `dev`, or a stacked parent branch.
    pub(crate) base: String,
    #[serde(default = "dev")]
    pub(crate) base_branch: String,
    pub(crate) merge_base: String,
    pub(crate) changed: Vec<String>,
    pub(crate) gates: Vec<GateResult>,
    pub(crate) verdict: Verdict,
    pub(crate) started: u64,
    pub(crate) finished: u64,
}

fn dev() -> String {
    crate::review::base::DEV.to_owned()
}

pub(crate) fn verdict(results: &[GateResult]) -> Verdict {
    if results.iter().any(|r| r.verdict == GateVerdict::Fail) {
        Verdict::Fail
    } else if results.is_empty()
        || results
            .iter()
            .any(|r| r.verdict == GateVerdict::Unavailable)
    {
        Verdict::Incomplete
    } else {
        Verdict::Pass
    }
}

pub(crate) fn directory(root: &Path) -> Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/evidence"))
}

fn path(root: &Path, head: &str) -> Result<PathBuf> {
    if head.len() < 40 || !head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("not a full commit id: {head}"));
    }
    Ok(directory(root)?.join(format!("{head}.json")))
}

pub(crate) fn write(root: &Path, evidence: &Evidence) -> Result<PathBuf> {
    let path = path(root, &evidence.head)?;
    fs::create_dir_all(path.parent().ok_or("evidence has no directory")?)
        .map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(evidence).map_err(|e| e.to_string())?;
    fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Evidence recorded for exactly `head`, if any.
pub(crate) fn read(root: &Path, head: &str) -> Result<Option<Evidence>> {
    let path = path(root, head)?;
    match fs::read(&path) {
        Ok(bytes) => {
            let evidence: Evidence =
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            if evidence.head != head {
                return Err(format!("{} names another commit", path.display()));
            }
            Ok(Some(evidence))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
