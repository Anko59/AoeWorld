//! The `dev` series: one point per `dev` commit that a pull request was
//! compared with, the lines they make (restarted where a row was
//! redefined), every move of 2% or more, and where each gated row stands.
use super::BundleLimit;
use super::calendar;
use super::comments::Observation;
use super::history::{Event, EventKind, Version};
use crate::perf::{THRESHOLD_PERCENT, over};
use crate::perf_comment::epoch_seconds;
use crate::perf_micro::{CREEP_PERCENT, FAIL_PERCENT, PARALLEL_TOTAL_PERCENT, WARN_PERCENT};
use serde::Serialize;
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

/// A move is reported from this many percent.
const MOVE_PERCENT: u128 = 2;
const DAY: i64 = 86_400;

/// A commit of `origin/dev`.
#[derive(Clone, Debug)]
pub(crate) struct Commit {
    pub(super) sha: String,
    pub(super) at: String,
    pub(super) subject: String,
}

/// `git log origin/dev --format=%H%x09%ct%x09%s` (newest first) as commits, oldest first.
pub(super) fn parse_commits(log: &str) -> Result<Vec<Commit>> {
    let mut commits = Vec::new();
    for line in log.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.splitn(3, '\t');
        let (Some(sha), Some(seconds), Some(subject)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!("git log: unexpected line {line:?}"));
        };
        let seconds: i64 = seconds
            .parse()
            .map_err(|_| format!("git log: no date in {line:?}"))?;
        commits.push(Commit {
            sha: sha.to_owned(),
            at: calendar::iso(seconds),
            subject: subject.to_owned(),
        });
    }
    commits.reverse();
    Ok(commits)
}

/// `title (#123)` split into its title and pull request number.
pub(super) fn split_subject(subject: &str) -> (String, Option<u64>) {
    if let Some(head) = subject.trim_end().strip_suffix(')')
        && let Some((title, number)) = head.rsplit_once(" (#")
        && let Ok(number) = number.parse()
    {
        return (title.to_owned(), Some(number));
    }
    (subject.to_owned(), None)
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DevPoint {
    pub(crate) sha: String,
    pub(crate) rev: String,
    pub(crate) at: String,
    /// Index of the commit in the `dev` log, oldest first.
    pub(crate) position: usize,
    pub(crate) pr: Option<u64>,
    pub(crate) title: String,
    /// `dev` commits between the previous point and this one.
    pub(crate) since_previous: Option<usize>,
    pub(crate) rows: BTreeMap<String, u64>,
}

/// The counts CI measured on each `dev` commit, oldest first. Comments name
/// a commit the same way every time: a later comment completes or replaces
/// the rows of an earlier one.
pub(super) fn dev_points(commits: &[Commit], observations: &[Observation]) -> Vec<DevPoint> {
    let position: BTreeMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| (commit.sha.as_str(), index))
        .collect();
    let mut sorted: Vec<&Observation> = observations.iter().collect();
    sorted.sort_by(|a, b| a.at.cmp(&b.at));
    let mut measured: BTreeMap<usize, BTreeMap<String, u64>> = BTreeMap::new();
    for observation in sorted {
        let Some(dev) = &observation.dev else {
            continue;
        };
        if let Some(&at) = position.get(dev.rev.as_str()) {
            measured
                .entry(at)
                .or_default()
                .extend(dev.rows.iter().map(|(row, count)| (row.clone(), *count)));
        }
    }
    let mut points: Vec<DevPoint> = Vec::new();
    for (at, rows) in measured {
        let commit = &commits[at];
        let (title, pr) = split_subject(&commit.subject);
        points.push(DevPoint {
            sha: commit.sha.clone(),
            rev: commit.sha.chars().take(9).collect(),
            at: commit.at.clone(),
            position: at,
            pr,
            title,
            since_previous: points.last().map(|previous| at - previous.position),
            rows,
        });
    }
    points
}

/// The indices of the points that have `row`, split where an event of
/// `Redefined` kind restarts the row: a line never joins two definitions.
pub(super) fn segments(points: &[DevPoint], row: &str, events: &[Event]) -> Vec<Vec<usize>> {
    let cuts: Vec<usize> = events
        .iter()
        .filter(|e| e.kind == EventKind::Redefined && e.rows.iter().any(|r| r == row))
        .map(|e| e.position)
        .collect();
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut last_stretch = None;
    for (index, point) in points.iter().enumerate() {
        if !point.rows.contains_key(row) {
            continue;
        }
        let stretch = cuts.iter().filter(|&&cut| cut <= point.position).count();
        match lines.last_mut() {
            Some(line) if last_stretch == Some(stretch) => line.push(index),
            _ => {
                lines.push(vec![index]);
                last_stretch = Some(stretch);
            }
        }
    }
    lines
}

/// A row that moved by 2% or more between two measured commits.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Move {
    pub(crate) at: String,
    pub(crate) rev: String,
    pub(crate) row: String,
    pub(crate) before: u64,
    pub(crate) after: u64,
    pub(crate) change: f64,
    pub(crate) pr: Option<u64>,
    pub(crate) title: String,
    /// `dev` commits between the two measurements.
    pub(crate) commits_since: usize,
    /// A redefinition or accepted increase that falls between the two commits.
    pub(crate) event: Option<EventKind>,
}

/// Every move of 2% or more of a row between two measured commits, newest
/// first. The raw bundle size follows the gzip one and is left out.
pub(super) fn moves(points: &[DevPoint], events: &[Event]) -> Vec<Move> {
    let mut last: BTreeMap<&str, (u64, usize)> = BTreeMap::new();
    let mut found = Vec::new();
    for point in points {
        for (row, &after) in &point.rows {
            if row == "wasm_raw" {
                continue;
            }
            let previous = last.insert(row, (after, point.position));
            let Some((before, from)) = previous else {
                continue;
            };
            let delta = u128::from(after.abs_diff(before)) * 100;
            if before == 0 || delta < u128::from(before) * MOVE_PERCENT {
                continue;
            }
            let between = |e: &&Event| {
                e.rows.contains(row) && e.position > from && e.position <= point.position
            };
            let event = events
                .iter()
                .filter(between)
                .map(|e| e.kind)
                .min_by_key(|kind| *kind != EventKind::Redefined);
            found.push(Move {
                at: point.at.clone(),
                rev: point.rev.clone(),
                row: row.clone(),
                before,
                after,
                change: (after as f64 - before as f64) * 100.0 / before as f64,
                pr: point.pr,
                title: point.title.clone(),
                commits_since: point.position - from,
                event,
            });
        }
    }
    found.reverse();
    found
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Passes,
    Warns,
    Fails,
    NewBaselineDue,
}

/// A gated row on the latest `dev` measurement against its committed limits.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Standing {
    pub(crate) row: String,
    pub(crate) now: u64,
    pub(crate) baseline: u64,
    pub(crate) over_percent: f64,
    /// The row fails above this many percent over its baseline.
    pub(crate) limit_percent: u8,
    pub(crate) reference: Option<u64>,
    /// The count an increase was accepted at, when one was.
    pub(crate) accepted_from: Option<u64>,
    /// Change since the last measurement at least a day earlier.
    pub(crate) day_change: Option<f64>,
    pub(crate) state: State,
}

fn percent(now: u64, base: u64) -> f64 {
    (now as f64 - base as f64) * 100.0 / base.max(1) as f64
}

/// The gate's own verdict on a `dev` count (`perf_micro::judge` for
/// instructions): fail above the limit or the fixed reference plus 10%, warn
/// above 1%, ask for a new baseline below -3%.
fn judge(now: u64, baseline: u64, reference: Option<u64>, fail: u8, gated: bool) -> State {
    if over(now, baseline, fail) || reference.is_some_and(|r| over(now, r, CREEP_PERCENT)) {
        State::Fails
    } else if gated && over(now, baseline, WARN_PERCENT) {
        State::Warns
    } else if gated && u128::from(now) * 100 < u128::from(baseline) * u128::from(100 - FAIL_PERCENT)
    {
        State::NewBaselineDue
    } else {
        State::Passes
    }
}

fn value_at<'a>(points: &'a [DevPoint], row: &str, not_after: Option<i64>) -> Option<&'a DevPoint> {
    points
        .iter()
        .rev()
        .filter(|point| point.rows.contains_key(row))
        .find(|point| {
            not_after.is_none_or(|limit| epoch_seconds(&point.at).is_some_and(|t| t <= limit))
        })
}

/// The limits a row is judged against: the baseline files as the commit of
/// the measurement had them (`dev` may have moved on since the comment).
struct Gate {
    baseline: u64,
    reference: Option<u64>,
    fail: u8,
    instructions: bool,
    accepted_from: Option<u64>,
}

fn instruction_gate(versions: &[Version], row: &str, position: usize) -> Option<Gate> {
    let in_force = versions
        .iter()
        .rev()
        .find(|version| version.position <= position)?;
    let split = in_force.instructions.contains_key(&format!("{row}_main"));
    Some(Gate {
        baseline: *in_force.instructions.get(row)?,
        reference: in_force.reference.get(row).copied(),
        fail: if split {
            PARALLEL_TOTAL_PERCENT
        } else {
            FAIL_PERCENT
        },
        instructions: true,
        accepted_from: in_force.accepted.get(row).map(|accepted| accepted.dev),
    })
}

fn bundle_gate(bundle: &[BundleLimit], position: usize) -> Option<Gate> {
    let in_force = bundle
        .iter()
        .rev()
        .find(|limit| limit.position <= position)?;
    Some(Gate {
        baseline: in_force.baseline,
        reference: None,
        fail: THRESHOLD_PERCENT,
        instructions: false,
        accepted_from: None,
    })
}

/// Where each gated row stands on the latest measured commit that has it,
/// against the baselines committed at that commit; the client bundle comes last.
pub(super) fn standing(
    points: &[DevPoint],
    versions: &[Version],
    bundle: &[BundleLimit],
) -> Vec<Standing> {
    let mut names: Vec<&str> = versions
        .iter()
        .flat_map(|version| version.instructions.keys().map(String::as_str))
        .collect();
    names.sort_unstable();
    names.dedup();
    names.push("wasm_gzip");
    let mut rows = Vec::new();
    for row in names {
        let Some(latest) = value_at(points, row, None) else {
            continue;
        };
        let gate = if row == "wasm_gzip" {
            bundle_gate(bundle, latest.position)
        } else {
            instruction_gate(versions, row, latest.position)
        };
        let Some(gate) = gate else {
            continue;
        };
        let now = latest.rows[row];
        let day_ago = epoch_seconds(&latest.at).map(|t| t - DAY);
        let earlier = day_ago.and_then(|limit| value_at(points, row, Some(limit)));
        rows.push(Standing {
            row: row.to_owned(),
            now,
            baseline: gate.baseline,
            over_percent: percent(now, gate.baseline),
            limit_percent: gate.fail,
            reference: gate.reference,
            accepted_from: gate.accepted_from,
            day_change: earlier.map(|point| percent(now, point.rows[row])),
            state: judge(
                now,
                gate.baseline,
                gate.reference,
                gate.fail,
                gate.instructions,
            ),
        });
    }
    rows
}
