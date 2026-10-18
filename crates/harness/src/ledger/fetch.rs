//! Filling the ledger from GitHub within a call budget, and finding reverts in
//! dev's own history. `gh` is the only network client; the source is a trait so
//! the incremental logic is tested without it.
use super::record::{Ledger, Pr, RawPr};
use serde::Deserialize;
use std::{path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A merged pull request in the cheap listing.
#[derive(Debug, Deserialize)]
pub(super) struct Listed {
    pub(super) number: u64,
}

pub(super) trait Source {
    /// The newest `window` merged pull requests into dev (one API call).
    fn list(&self, window: usize) -> Result<Vec<Listed>>;
    /// One pull request with its body and commits (one API call).
    fn view(&self, number: u64) -> Result<RawPr>;
}

pub(super) struct Gh<'a>(pub(super) &'a Path);

fn gh(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("gh").args(args).current_dir(root).output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh {}: {}", args.join(" "), err.trim()).into());
    }
    Ok(output.stdout)
}

impl Source for Gh<'_> {
    fn list(&self, window: usize) -> Result<Vec<Listed>> {
        let limit = window.to_string();
        let args = [
            "pr", "list", "--state", "merged", "--base", "dev", "--limit", &limit, "--json",
            "number",
        ];
        Ok(serde_json::from_slice(&gh(self.0, &args)?)?)
    }

    fn view(&self, number: u64) -> Result<RawPr> {
        let number = number.to_string();
        let fields = "number,title,body,mergedAt,headRefName,commits";
        Ok(serde_json::from_slice(&gh(
            self.0,
            &["pr", "view", &number, "--json", fields],
        )?)?)
    }
}

/// What a refresh did.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Refresh {
    pub(super) calls: usize,
    pub(super) fetched: usize,
    /// Listed pull requests still missing from the ledger after the budget ran out.
    pub(super) missing: usize,
    /// Why the refresh stopped early, when it did.
    pub(super) stopped: Option<String>,
}

/// Fetch the pull requests of the newest `window` that the ledger lacks,
/// newest first, using at most `max_calls` API calls (the listing counts one).
/// A failed call stops the refresh; what was fetched stays in the ledger.
pub(super) fn refresh(
    source: &dyn Source,
    ledger: &mut Ledger,
    window: usize,
    max_calls: usize,
) -> Refresh {
    let mut done = Refresh {
        calls: 0,
        fetched: 0,
        missing: 0,
        stopped: None,
    };
    if max_calls == 0 {
        done.stopped = Some("the call budget is 0".to_owned());
        return done;
    }
    done.calls += 1;
    let listed = match source.list(window) {
        Ok(listed) => listed,
        Err(error) => {
            done.stopped = Some(error.to_string());
            return done;
        }
    };
    let mut wanted: Vec<u64> = listed
        .iter()
        .map(|l| l.number)
        .filter(|n| !ledger.knows(*n))
        .collect();
    wanted.sort_unstable_by(|a, b| b.cmp(a));
    for (i, number) in wanted.iter().enumerate() {
        if done.calls >= max_calls {
            done.missing = wanted.len() - i;
            break;
        }
        done.calls += 1;
        match source.view(*number) {
            Ok(raw) => {
                ledger.insert(Pr::from_raw(&raw));
                done.fetched += 1;
            }
            Err(error) => {
                done.missing = wanted.len() - i;
                done.stopped = Some(error.to_string());
                break;
            }
        }
    }
    done
}

/// Subjects of dev's history, for `Revert "…"` commits (no API call).
pub(super) fn dev_subjects(root: &Path) -> Result<String> {
    for branch in ["origin/dev", "dev"] {
        let output = Command::new("git")
            .args(["log", branch, "--format=%s"])
            .current_dir(root)
            .output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    Err("neither origin/dev nor dev exists, so reverts are unknown".into())
}
