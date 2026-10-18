//! Tiered adversarial review (docs/review.md). Reviewers with distinct
//! personas review the change blind; later rounds cross-examine each other's
//! findings until no status changes or the tier's round cap; a grader writes a
//! grade /10 and two lines, which confirmed findings cap. The report is stored
//! next to the ship evidence, keyed by commit, where agents cannot write.
mod arbiter;
pub(crate) mod base;
mod calibrate;
pub(crate) mod closing;
mod cluster;
pub(crate) mod config;
mod entry;
pub(crate) mod increment;
mod prompt;
pub(crate) mod protocol;
mod report;
mod reuse;
pub(crate) use reuse::{Reuse, announce, take_announced};
mod runner;
mod session;
#[cfg(test)]
mod tests;
mod trusted;

use crate::{agents::Runtime, gates::registry::Registry, ship::git};
pub(crate) use closing::Plan;
use config::Config;
pub(crate) use config::Tier;
#[cfg(test)]
pub(crate) use entry::review_on_base;
pub(crate) use entry::{execute, review, review_for_branch};
pub(crate) use increment::Increment;
use protocol::{Answer, Cast, Finding, Grade, Status};
pub(crate) use report::{Report, history};
pub(crate) use session::Ask;
use session::{Live, ask_checked, readable_answer, readable_grade};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub(crate) use trusted::trusted;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Review HEAD against origin/dev or SHIP_BASE (a local parent is allowed).
    /// `make review`: REVIEW_TIER, REVIEW_RUNTIME, REVIEW_TASK.
    Review {
        /// Print ship's stored-review decision; no reviewer, gates or approval write.
        #[arg(long)]
        explain: bool,
        /// Defaults to REVIEW_TIER, then to the floor.
        #[arg(long, value_enum)]
        tier: Option<Tier>,
        /// Defaults to REVIEW_RUNTIME, then to claude.
        #[arg(long, value_enum)]
        runtime: Option<Runtime>,
        /// File holding the task: the request, issue or PR description
        /// (REVIEW_TASK).
        #[arg(long)]
        task: Option<PathBuf>,
    },
    /// Print the lowest review tier the change allows.
    ReviewFloor,
    /// Run the review on the calibration seed set and write the recall and
    /// false-positive tables (`make review-calibrate`: CALIBRATE_TIER,
    /// CALIBRATE_RUNTIME, CALIBRATE_CASES). A tool, not a gate.
    ReviewCalibrate,
}

fn changed_suites(root: &Path) -> Result<(String, String, std::collections::BTreeSet<String>)> {
    let (base, merge_base) = git::base(root, &base::current(), false)?;
    let changed = git::changed(root, &merge_base)?;
    Ok((base, merge_base, suites_of(root, &changed)?))
}

fn suites_of(root: &Path, changed: &[String]) -> Result<std::collections::BTreeSet<String>> {
    let registry = Registry::parse(trusted(root, "gates/registry.json")?.as_bytes())?;
    // A misspelt floor would silently give the low tier.
    let known: std::collections::BTreeSet<&str> =
        registry.suites.iter().map(|s| s.id.as_str()).collect();
    if let Some(unknown) = Config::load(root)?
        .floors
        .keys()
        .find(|k| !known.contains(k.as_str()))
    {
        return Err(format!("gates/review.json: floor for unknown suite `{unknown}`").into());
    }
    Ok(registry.classify(changed).suites)
}

/// The lowest tier that the change to `files` alone allows.
pub(crate) fn floor_of(root: &Path, files: &[String]) -> Result<Tier> {
    Ok(Config::load(root)?.floor(&suites_of(root, files)?))
}

pub(crate) fn floor(root: &Path) -> Result<Tier> {
    let (_, _, suites) = changed_suites(root)?;
    Ok(Config::load(root)?.floor(&suites))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Facts every reviewer gets, computed without a model. A git failure stops
/// the review: reviewers must never be told "no tests changed" by mistake.
pub(crate) fn facts(root: &Path, merge_base: &str) -> Result<String> {
    let diff = git::git(root, &["diff", merge_base, "HEAD", "--", "."])?;
    let added: Vec<&str> = diff
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .collect();
    let count = |needle: &str| added.iter().filter(|l| l.contains(needle)).count();
    let numstat = git::git(root, &["diff", "--numstat", merge_base, "HEAD"])?;
    let (mut test_lines, mut other_lines) = (0usize, 0usize);
    let mut sensitive = Vec::new();
    for line in numstat.lines() {
        let mut parts = line.split('\t');
        let added: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let path = parts.nth(1).unwrap_or_default();
        if crate::agents::is_test_path(&path.to_ascii_lowercase()) {
            test_lines += added;
        } else {
            other_lines += added;
        }
        if path.starts_with(".github/")
            || path.starts_with("gates/")
            || path.starts_with("baselines/")
        {
            sensitive.push(path.to_owned());
        }
    }
    Ok(format!(
        "- lines added: {test_lines} in tests, {other_lines} elsewhere\n- added lines containing `#[ignore]`: {}, `unsafe`: {}, `.unwrap(`: {}, `cfg(test)`: {}\n- changed CI, gate or baseline files: {}\n{}",
        count("#[ignore]"),
        count("unsafe"),
        count(".unwrap("),
        count("cfg(test)"),
        if sensitive.is_empty() {
            "none".to_owned()
        } else {
            sensitive.join(", ")
        },
        crate::ship::test_first::summary(root, merge_base, "HEAD"),
    ))
}

#[cfg(test)]
pub(crate) fn review_with(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
    ask: &dyn Ask,
) -> Result<Report> {
    let branch = git::branch(root).unwrap_or_default();
    let policy = entry::Policy::pin(root, None, tier, runtime, plan)?;
    review_with_branch(root, &branch, tier, &policy, task, plan, ask)
}

/// Review under `policy`, pinned by the caller before it chose the model.
fn review_with_branch(
    root: &Path,
    branch: &str,
    tier: Tier,
    policy: &entry::Policy,
    task: &str,
    plan: &Plan,
    ask: &dyn Ask,
) -> Result<Report> {
    let policy_fingerprint = Report::policy_fingerprint_at(root, &policy.commit, tier.name())?;
    let (config, runtime) = (&policy.config, policy.runtime);
    let (base, merge_base, suites) = changed_suites(root)?;
    let minimum = config.floor(&suites);
    if tier < minimum {
        return Err(format!(
            "this change needs at least a {} review, not {}",
            minimum.name(),
            tier.name()
        )
        .into());
    }
    // Reviewers read the working tree: it must be the commit under review.
    if !git::git(root, &["status", "--porcelain"])?.is_empty() {
        return Err("commit or stash your changes first: reviewers read the working tree, and the review is of HEAD".into());
    }
    let head = git::git(root, &["rev-parse", "HEAD"])?;
    // The commits after an approved one: only they are judged.
    let (view, scope_note, facts_from) = match plan {
        Plan::Increment(increment) => {
            let (files, diff) = increment::view(root, &merge_base, &increment.scope)?;
            let from = match &increment.scope {
                increment::Scope::Since(since) => since.clone(),
                increment::Scope::Files(_) => merge_base.clone(),
            };
            let note = prompt::scope_note(increment, &files, &from);
            (Some((files, diff)), note, from)
        }
        _ => (None, String::new(), merge_base.clone()),
    };
    let mut filter =
        increment::Filter::new(view.as_ref().map(|(_, diff)| increment::Lines::of(diff)));
    let subject = prompt::Subject {
        head: head.clone(),
        merge_base: merge_base.clone(),
        task: Report::effective_task(root, &head, task)?,
        stat: match &view {
            Some((files, _)) => increment::stat(root, &facts_from, files)?,
            None => git::git(root, &["diff", "--stat", &merge_base, "HEAD"])?,
        },
        // A closing review audits the fixes since the last reviewed commit.
        diff: match (plan, view) {
            (Plan::Closing { since, .. }, _) => increment::since_diff(root, &merge_base, since)?,
            (_, Some((_, diff))) => diff,
            _ => git::git(root, &["diff", &merge_base, "HEAD"])?,
        },
        facts: facts(root, &facts_from)?,
        scope: scope_note,
    };
    let closing = plan.closing_style();
    let tier_config = config.tiers[&tier].clone();
    let model = policy.model.clone();
    let personas = tier_config.personas.clone();
    let started = now();
    let mut findings: Vec<Finding> = Vec::new();
    let mut failures = Vec::new();
    // A closing review needs at least two reviewers to cross-examine what it
    // finds: a one-persona tier borrows the medium tier's.
    let personas = if closing && personas.len() < 2 {
        config.tiers[&Tier::Medium].personas.clone()
    } else {
        personas
    };
    let reviewers = personas.len();
    let first = if closing {
        if let Plan::Closing { prior, .. } = plan {
            findings = prior.clone();
        }
        1
    } else {
        blind_round(
            root,
            &personas,
            &subject,
            ask,
            &mut findings,
            &mut failures,
            &mut filter,
        )?;
        cluster::merge(&mut findings);
        for finding in &mut findings {
            finding.status = protocol::status(finding, reviewers);
        }
        2
    };
    let mut rounds = 1;
    // A closing review needs a round after the first to examine what it finds.
    let max_rounds = if closing {
        tier_config.max_rounds.max(2)
    } else {
        tier_config.max_rounds
    };
    for round in first..=max_rounds {
        if (reviewers < 2 || findings.is_empty()) && !(closing && round == 1) {
            break;
        }
        let before: Vec<(String, Status)> =
            findings.iter().map(|f| (f.id.clone(), f.status)).collect();
        // A finding added in the last round could never be cross-examined.
        let last = round == max_rounds;
        let mut asked = Vec::new();
        let mut prompts = Vec::new();
        for (reviewer, persona) in personas.iter().enumerate() {
            if closing || prompt::has_work(&findings, reviewer) {
                asked.push(reviewer);
                prompts.push(prompt::cross_round(
                    root,
                    persona,
                    &subject,
                    &findings,
                    reviewer,
                    prompt::Round {
                        number: round,
                        last,
                        closing,
                    },
                )?);
            }
        }
        let shown: Vec<(usize, Vec<String>)> = asked
            .iter()
            .map(|r| {
                let ids = findings.iter().filter(|f| !f.reported_by(*r));
                (*r, ids.map(|f| f.id.clone()).collect())
            })
            .collect();
        for ((reviewer, ids), answer) in
            shown
                .into_iter()
                .zip(ask_checked(ask, prompts, readable_answer))
        {
            match answer
                .and_then(|text| serde_json::from_str::<Answer>(&text).map_err(|e| e.to_string()))
            {
                Ok(answer) => {
                    let missing: Vec<&String> = ids
                        .iter()
                        .filter(|id| !answer.verdicts.iter().any(|v| &v.id == *id))
                        .collect();
                    if !missing.is_empty() {
                        failures.push(format!(
                            "round {round}, {}: no vote on {}",
                            personas[reviewer],
                            missing
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    for verdict in answer.verdicts {
                        if let Some(finding) = findings.iter_mut().find(|f| {
                            f.id == verdict.id && ids.contains(&f.id) && !f.reported_by(reviewer)
                        }) {
                            finding.votes.push(Cast {
                                reviewer,
                                round,
                                vote: verdict.verdict,
                                evidence: verdict.evidence,
                            });
                        }
                    }
                    let serious = answer
                        .findings
                        .into_iter()
                        .filter(|f| !last && f.severity <= protocol::Severity::Major)
                        .collect();
                    let serious = filter.admit(serious);
                    add_findings(&mut findings, reviewer, round, serious);
                }
                Err(error) => {
                    failures.push(format!("round {round}, {}: {error}", personas[reviewer]))
                }
            }
        }
        cluster::merge(&mut findings);
        for finding in &mut findings {
            finding.status = protocol::status(finding, reviewers);
        }
        rounds = round;
        // Without new findings, a round where every finding is settled would
        // only be asked again verbatim.
        let settled =
            findings.len() == before.len() && findings.iter().all(|f| f.status != Status::Disputed);
        if protocol::converged(&before, &findings) || settled {
            break;
        }
    }
    // A closing review's votes mean "still there?", not "real?": no arbiter.
    if tier >= Tier::High && !closing {
        arbiter::rule(root, &subject, &mut findings, rounds + 1, ask)?;
    }
    let grade_prompt = prompt::grading(root, &subject, &findings)?;
    let grade = ask_checked(ask, vec![grade_prompt], readable_grade)
        .pop()
        .unwrap_or_else(|| Err("no answer".into()))
        .and_then(|text| serde_json::from_str::<Grade>(&text).map_err(|e| e.to_string()));
    let (written, summary) = match grade {
        Ok(grade) => (grade.grade.clamp(1, 10), grade.summary),
        Err(error) => {
            failures.push(format!("grader: {error}"));
            (
                1,
                "The grader gave no usable answer; the review is incomplete.".into(),
            )
        }
    };
    let cap = protocol::cap(&findings);
    let change_fingerprint = git::change_fingerprint(root, &base::current(), &head)
        .ok()
        .map(|(_, fingerprint)| fingerprint);
    let report = Report {
        version: 1,
        head,
        branch: branch.to_owned(),
        base,
        merge_base,
        tier: tier.name().into(),
        floor: minimum.name().into(),
        runtime: config::runtime_key(runtime).into(),
        model: model.model,
        effort: model.effort,
        personas,
        rounds,
        findings,
        written_grade: written,
        grade: written.min(cap),
        summary,
        failures,
        merge_grade: config.merge_grade,
        started,
        finished: now(),
        closing,
        change_fingerprint,
        policy_fingerprint: Some(policy_fingerprint),
        reused_from: None,
        // What the reviewers saw: the commit log stands in for an empty task.
        task_fingerprint: Some(Report::task_fingerprint(&subject.task)),
        note: None,
        decision: reuse::take_announced(),
        base_branch: base::current(),
        increment: match plan {
            Plan::Increment(increment) => Some(Increment {
                outside: filter.outside,
                ..increment.clone()
            }),
            _ => None,
        },
    };
    report.store(root)?;
    Ok(report)
}

/// Round 1: every persona reviews the change blind.
fn blind_round(
    root: &Path,
    personas: &[String],
    subject: &prompt::Subject,
    ask: &dyn Ask,
    findings: &mut Vec<Finding>,
    failures: &mut Vec<String>,
    filter: &mut increment::Filter,
) -> Result<()> {
    let prompts = personas
        .iter()
        .map(|p| prompt::first_round(root, p, subject))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (reviewer, answer) in ask_checked(ask, prompts, readable_answer)
        .into_iter()
        .enumerate()
    {
        match answer
            .and_then(|text| serde_json::from_str::<Answer>(&text).map_err(|e| e.to_string()))
        {
            Ok(answer) => {
                let inside = filter.admit(answer.findings);
                add_findings(findings, reviewer, 1, inside)
            }
            Err(error) => failures.push(format!("round 1, {}: {error}", personas[reviewer])),
        }
    }
    Ok(())
}

/// One past the highest `F<n>` in use, members included: `cluster::merge` removes
/// merged findings from the list, so its length no longer gives a free number.
pub(crate) fn next_number(findings: &[Finding]) -> usize {
    findings
        .iter()
        .flat_map(|f| std::iter::once(&f.id).chain(f.members.iter().map(|m| &m.id)))
        .filter_map(|id| id.strip_prefix('F')?.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn add_findings(
    findings: &mut Vec<Finding>,
    reviewer: usize,
    round: u32,
    reported: Vec<protocol::Reported>,
) {
    for reported in reported {
        let id = format!("F{}", next_number(findings));
        findings.push(Finding {
            id,
            reporter: reviewer,
            round,
            reported,
            votes: Vec::new(),
            status: Status::Disputed,
            members: Vec::new(),
        });
    }
}
