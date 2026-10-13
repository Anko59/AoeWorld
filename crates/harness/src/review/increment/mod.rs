//! Commits added after a passing review (docs/review.md). The approved code is
//! never reviewed again: the next review judges only the diff from the
//! approved state to HEAD, and a finding outside that diff cannot block.
//! The approved state is the newest passing full or closing review of the
//! branch whose commit is in HEAD's history, or whose patch (by the patch
//! identity of reuse) is still in it after a rebase; when a rebase changed
//! files of the approved patch, those files are the increment; when no
//! approval can be told, nothing is approved and the review is a full one.
use super::{
    Report, Tier, base,
    closing::{self, Next, Plan},
    protocol::Reported,
};
use crate::ship::git;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

/// Changed lines up to which an increment gets a closing-style review.
pub(crate) const SMALL: usize = 200;
/// Commits of a rebased branch searched for the approved patch.
const SEARCHED: usize = 100;

/// What the reviewers judge: the commits since an approved one, or, when a
/// rebase changed the approved patch, the files that differ from it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) enum Scope {
    Since(String),
    Files(Vec<String>),
}

/// A review of the part of a branch that no passing review has seen.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Increment {
    /// The commit whose review passed.
    pub(crate) approved: String,
    pub(crate) grade: u8,
    pub(crate) tier: String,
    pub(crate) scope: Scope,
    /// Lines added plus removed by the increment.
    pub(crate) lines: usize,
    /// Closing-style (small, floor within the passed tier) or a full review.
    pub(crate) closing: bool,
    /// Findings outside the increment: reported, never blocking.
    #[serde(default)]
    pub(crate) outside: Vec<Reported>,
}

impl Increment {
    /// "closing review of 12 lines since `abc`, approved 9/10".
    pub(crate) fn sentence(&self) -> String {
        let kind = if self.closing { "closing" } else { "full" };
        let head = &self.approved[..12.min(self.approved.len())];
        match &self.scope {
            Scope::Since(since) => format!(
                "{kind} review of {} lines since `{}`, approved {}/10",
                self.lines,
                &since[..12.min(since.len())],
                self.grade
            ),
            Scope::Files(files) => format!(
                "{kind} review of {} lines in {} file(s) that differ from the approved patch of `{head}`, approved {}/10",
                self.lines,
                files.len(),
                self.grade
            ),
        }
    }
}

/// What `make ship` does next: `closing::next`, except that a branch with an
/// approved state is not started over (its next review is an increment).
pub(crate) fn next(root: &Path, history: &[Report], head: &str, now: u64) -> Next {
    let legacy = closing::next(history, head, now);
    if !matches!(legacy, Next::Review(Plan::Full)) {
        return legacy;
    }
    match approved(root, history, head) {
        Some(increment) => Next::Review(Plan::Increment(increment)),
        None => legacy,
    }
}

/// The increment after the newest approval that can be placed, if any.
fn approved(root: &Path, history: &[Report], head: &str) -> Option<Increment> {
    let (_, merge_base) = git::base(root, &base::current(), false).ok()?;
    let current = git::change_identity(root, &base::current(), "HEAD").ok()?.1;
    history
        .iter()
        .rev()
        .filter(|r| r.complete() && r.passes() && r.reused_from.is_none() && r.head != head)
        .find_map(|report| {
            let scope = place(root, report, &merge_base, &current)?;
            let (files, diff) = view(root, &merge_base, &scope).ok()?;
            let lines = changed_lines(&diff);
            let passed = <Tier as clap::ValueEnum>::from_str(&report.tier, true).ok()?;
            let floor = super::floor_of(root, &files).ok()?;
            Some(Increment {
                approved: report.head.clone(),
                grade: report.grade,
                tier: report.tier.clone(),
                scope,
                lines,
                // Small follow-ups add up: closing-style only while everything
                // added since the last full review stays small in total.
                closing: lines + since_full(history.iter().map(slice)) <= SMALL && floor <= passed,
                outside: Vec::new(),
            })
        })
}

/// How a stored report counts towards `since_full`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    /// A passing full review, of the whole diff or of an increment.
    Full,
    /// A passing closing-style increment review of this many lines.
    Small(usize),
    /// A failed review, or a closing review of fixes: approves no new lines.
    Other,
}

fn slice(report: &Report) -> Step {
    if !(report.complete() && report.passes()) {
        return Step::Other;
    }
    match &report.increment {
        Some(increment) if increment.closing => Step::Small(increment.lines),
        _ if report.closing => Step::Other,
        _ => Step::Full,
    }
}

/// Lines approved by closing-style increment reviews since the last passing
/// full review. Only a full review ends the count.
pub(crate) fn since_full(history: impl DoubleEndedIterator<Item = Step>) -> usize {
    let mut total = 0;
    for step in history.rev() {
        match step {
            Step::Other => {}
            Step::Small(lines) => total += lines,
            Step::Full => break,
        }
    }
    total
}

/// Where the approved patch of `report` stands in the branch now.
fn place(root: &Path, report: &Report, merge_base: &str, current: &[u8]) -> Option<Scope> {
    if git::git(root, &["merge-base", "--is-ancestor", &report.head, "HEAD"]).is_ok() {
        return Some(Scope::Since(report.head.clone()));
    }
    let approved = super::reuse::source_identity(root, report).ok()?;
    let range = format!("{merge_base}..HEAD");
    let commits = git::git(root, &["rev-list", "-n", &SEARCHED.to_string(), &range]).ok()?;
    for commit in commits.lines().skip(1) {
        let identity = git::change_identity(root, &base::current(), commit).ok()?.1;
        if identity == approved {
            return Some(Scope::Since(commit.to_owned()));
        }
    }
    differing(&approved, current).map(Scope::Files)
}

/// Files of the current patch that the approved patch does not have as is.
/// `None` when nothing differs or a path cannot be named exactly.
fn differing(approved: &[u8], current: &[u8]) -> Option<Vec<String>> {
    let files = |bytes: &[u8]| -> Option<BTreeMap<Vec<u8>, serde_json::Value>> {
        let list: Vec<serde_json::Value> = serde_json::from_slice(bytes).ok()?;
        list.into_iter()
            .map(|file| {
                let path = file.get("path")?.as_array()?;
                let path = path
                    .iter()
                    .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                    .collect::<Option<Vec<u8>>>()?;
                Some((path, file))
            })
            .collect()
    };
    let (approved, current) = (files(approved)?, files(current)?);
    let names = current
        .iter()
        .filter(|(path, file)| approved.get(*path) != Some(file))
        .map(|(path, _)| String::from_utf8(path.clone()).ok())
        .collect::<Option<Vec<String>>>()?;
    (!names.is_empty()).then_some(names)
}

/// The branch's own files changed since `since`: a rebase must not bring dev's
/// changes in, and a fix may have reverted a file to dev.
pub(super) fn since_files(
    root: &Path,
    merge_base: &str,
    since: &str,
) -> Result<Vec<String>, String> {
    let mut files = git::changed(root, merge_base)?;
    let since_base = git::git(root, &["merge-base", merge_base, since])?;
    files.extend(git::changed_between(root, &since_base, since)?);
    files.sort();
    files.dedup();
    Ok(files)
}

/// The files and the diff of an increment, taken literally.
pub(super) fn view(
    root: &Path,
    merge_base: &str,
    scope: &Scope,
) -> Result<(Vec<String>, String), String> {
    let (from, files) = match scope {
        Scope::Since(since) => {
            let own = since_files(root, merge_base, since)?;
            let moved = git::changed_between(root, since, "HEAD")?;
            (
                since.as_str(),
                moved.into_iter().filter(|f| own.contains(f)).collect(),
            )
        }
        Scope::Files(files) => (merge_base, files.clone()),
    };
    let mut args = vec!["--literal-pathspecs", "diff", from, "HEAD", "--"];
    args.extend(files.iter().map(String::as_str));
    Ok((files.clone(), git::git(root, &args)?))
}

/// `git diff --stat` of an increment.
pub(super) fn stat(root: &Path, from: &str, files: &[String]) -> Result<String, String> {
    let mut args = vec!["--literal-pathspecs", "diff", "--stat", from, "HEAD", "--"];
    args.extend(files.iter().map(String::as_str));
    git::git(root, &args)
}

/// The diff `since..HEAD` limited to the branch's own files.
pub(super) fn since_diff(root: &Path, merge_base: &str, since: &str) -> Result<String, String> {
    let files = since_files(root, merge_base, since)?;
    let mut args = vec!["--literal-pathspecs", "diff", since, "HEAD", "--"];
    args.extend(files.iter().map(String::as_str));
    git::git(root, &args)
}

/// Lines added plus removed; a binary change is never small.
fn changed_lines(diff: &str) -> usize {
    diff.lines()
        .map(|line| match line {
            l if l.starts_with("+++") || l.starts_with("---") => 0,
            l if l.starts_with('+') || l.starts_with('-') => 1,
            l if l.starts_with("Binary files") || l == "GIT binary patch" => SMALL + 1,
            _ => 0,
        })
        .sum()
}

/// Where the lines of an increment are: a finding elsewhere is out of scope.
pub(super) struct Lines {
    ranges: BTreeMap<String, Vec<(u32, u32)>>,
    /// A header that could not be read: everything counts as inside.
    unreadable: bool,
}

impl Lines {
    pub(super) fn of(diff: &str) -> Self {
        let mut ranges: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
        let (mut file, mut unreadable) = (None::<String>, false);
        for line in diff.lines() {
            if let Some(path) = line.strip_prefix("+++ ") {
                match path.strip_prefix("b/") {
                    Some(path) if !path.starts_with('"') => file = Some(path.to_owned()),
                    _ if path == "/dev/null" => file = None,
                    _ => unreadable = true,
                }
            } else if let (Some(path), Some(hunk)) = (&file, line.strip_prefix("@@ ")) {
                let new = hunk.split(' ').find_map(|part| part.strip_prefix('+'));
                let mut parts = new.unwrap_or("").split(',');
                let start: Option<u32> = parts.next().and_then(|n| n.parse().ok());
                let count: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
                match start {
                    Some(start) => ranges
                        .entry(path.clone())
                        .or_default()
                        .push((start, start + count.saturating_sub(1))),
                    None => unreadable = true,
                }
            }
        }
        Self { ranges, unreadable }
    }

    /// Whether a finding at `file:line` is inside the increment's hunks (with
    /// the context lines the reviewers saw). A deleted file has no lines to
    /// cite, so a finding on it names no line and counts as inside.
    pub(super) fn holds(&self, reported: &Reported) -> bool {
        if self.unreadable {
            return true;
        }
        let file = reported.file.trim_start_matches("./");
        let Some(hunks) = self.ranges.get(file) else {
            return false;
        };
        reported
            .line
            .is_none_or(|n| hunks.iter().any(|&(from, to)| (from..=to).contains(&n)))
    }
}

/// Keeps what a review may report: the findings inside the increment, and
/// records the others as notes that cannot block.
pub(super) struct Filter {
    lines: Option<Lines>,
    pub(super) outside: Vec<Reported>,
}

impl Filter {
    pub(super) fn new(lines: Option<Lines>) -> Self {
        Self {
            lines,
            outside: Vec::new(),
        }
    }

    pub(super) fn admit(&mut self, reported: Vec<Reported>) -> Vec<Reported> {
        let Some(lines) = &self.lines else {
            return reported;
        };
        let (inside, outside): (Vec<_>, Vec<_>) =
            reported.into_iter().partition(|r| lines.holds(r));
        self.outside.extend(outside);
        inside
    }
}

#[cfg(test)]
mod tests;
