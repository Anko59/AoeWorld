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
    pub(crate) base_branch: String,
    pub(crate) merge_base: String,
    pub(crate) changed: Vec<String>,
    pub(crate) gates: Vec<GateResult>,
    pub(crate) verdict: Verdict,
    /// The pull request level, its floor and why (None without a description).
    pub(crate) level: Option<super::levels::Decision>,
    /// The re-check against a moved base just before the push, if one ran.
    pub(crate) recheck: Option<super::recheck::Recheck>,
    pub(crate) started: u64,
    pub(crate) finished: u64,
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

/// Gates at the exact commit, then evidence; a moved HEAD or tree leaves none.
pub(crate) fn judge(root: &Path, options: &super::Options) -> super::Result<Evidence> {
    let subject = git::subject(root)?;
    super::stack::refuse_base(&options.base, &subject.branch)?;
    let (base, merge_base) = git::base(root, &options.base, !options.no_fetch)?;
    let changed = git::changed(root, &merge_base)?;
    let registry = crate::gates::registry::Registry::load(root)?;
    let suites = registry.classify(&changed).suites;
    let plan = registry.plan(crate::gates::registry::Cadence::Preflight, &suites)?;
    // After a review fix, rerun only what the fix can affect: the gates the diff
    // since the newest passing evidence on this branch selects.
    let prior = prior_pass(root, &merge_base, &options.base)?;
    let (run, reused) = match &prior {
        Some(prior) => {
            let since = git::changed_between(root, &prior.head, "HEAD")?;
            let rerun: Vec<String> = registry
                .affected(&registry.classify(&since).suites)
                .into_iter()
                .collect();
            split(&plan.gates, &rerun, prior)
        }
        None => (plan.gates.clone(), Vec::new()),
    };
    let gates: Vec<_> = run
        .iter()
        .filter_map(|id| registry.gates.iter().find(|gate| &gate.id == id))
        .collect();
    let started = super::now();
    eprintln!(
        "ship: {} preflight gate(s) at {} on {}{}",
        gates.len(),
        &subject.head[..12],
        subject.branch,
        prior.as_ref().map_or_else(String::new, |prior| format!(
            "; {} reused from passing evidence at {}",
            reused.len(),
            &prior.head[..12]
        ))
    );
    crate::timeline::mark(root, "gates", "start", serde_json::json!({}));
    let mut results = super::run::run_gates_selecting(root, &gates, &options.slots, &merge_base);
    results.extend(reused);
    results.sort_by_key(|result| plan.gates.iter().position(|id| *id == result.gate));
    crate::timeline::mark(
        root,
        "gates",
        "end",
        serde_json::json!({"verdict":verdict(&results)}),
    );
    if git::subject(root).ok().as_ref() != Some(&subject) {
        return Err("HEAD, the tree or the branch changed while the gates ran; no evidence written. Commit, then ship again".into());
    }
    let evidence = Evidence {
        version: 1,
        cadence: "preflight".into(),
        verdict: verdict(&results),
        head: subject.head,
        tree: subject.tree,
        branch: subject.branch,
        base,
        base_branch: options.base.clone(),
        merge_base,
        changed,
        gates: results,
        level: options.level.as_deref().cloned(),
        recheck: None,
        started,
        finished: super::now(),
    };
    let path = write(root, &evidence)?;
    eprintln!("ship: evidence {}", path.display());
    Ok(evidence)
}

/// How far back on the branch to look for passing evidence.
const REUSE_DEPTH: usize = 50;

/// The newest passing preflight evidence for a strict ancestor of HEAD on this
/// branch (first parents back to the merge base), against the same base branch.
fn prior_pass(root: &Path, merge_base: &str, base_branch: &str) -> Result<Option<Evidence>> {
    let range = format!("{merge_base}..HEAD~1");
    let listed = git::git(root, &["rev-list", "--first-parent", &range]).unwrap_or_default();
    for head in listed.lines().take(REUSE_DEPTH) {
        if let Some(evidence) = read(root, head)?
            && evidence.verdict == Verdict::Pass
            && evidence.cadence == "preflight"
            && evidence.base_branch == base_branch
        {
            return Ok(Some(evidence));
        }
    }
    Ok(None)
}

/// Split the planned gates into those to run and results carried over from
/// `prior`: a gate is reused only when the change since `prior` does not select
/// it and `prior` passed it.
pub(crate) fn split(
    planned: &[String],
    rerun: &[String],
    prior: &Evidence,
) -> (Vec<String>, Vec<GateResult>) {
    let mut run = Vec::new();
    let mut reused = Vec::new();
    for id in planned {
        let passed = prior
            .gates
            .iter()
            .find(|result| &result.gate == id && result.verdict == GateVerdict::Pass);
        match passed {
            Some(result) if !rerun.contains(id) => reused.push(GateResult {
                seconds: 0.0,
                summary: format!("reused from {}: {}", &prior.head[..12], result.summary),
                ..result.clone()
            }),
            _ => run.push(id.clone()),
        }
    }
    (run, reused)
}

#[cfg(test)]
mod tests;
