//! Tiered adversarial review (docs/review.md). Reviewers with distinct
//! personas review the change blind; later rounds cross-examine each other's
//! findings until no status changes or the tier's round cap; a grader writes a
//! grade /10 and two lines, which confirmed findings cap. The report is stored
//! next to the ship evidence, keyed by commit, where agents cannot write.
pub(crate) mod closing;
pub(crate) mod config;
mod prompt;
pub(crate) mod protocol;
mod report;
mod runner;
mod session;
#[cfg(test)]
mod tests;

use crate::{agents::Runtime, gates::registry::Registry, ship::git};
pub(crate) use closing::Plan;
use config::Config;
pub(crate) use config::Tier;
use protocol::{Answer, Cast, Finding, Grade, Status};
pub(crate) use report::{Report, history};
pub(crate) use session::Ask;
use session::{Live, ask_checked, readable_answer, readable_grade};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Run a tiered adversarial review of HEAD against origin/dev
    /// (`make review`: REVIEW_TIER, REVIEW_RUNTIME, REVIEW_TASK).
    Review {
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
}

pub(crate) fn execute(command: Commands) -> Result<()> {
    let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    match command {
        Commands::Review {
            tier,
            runtime,
            task,
        } => {
            let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
            let tier = match (tier, env("REVIEW_TIER")) {
                (Some(tier), _) => tier,
                (None, Some(name)) => clap::ValueEnum::from_str(&name, true).map_err(|_| {
                    format!("REVIEW_TIER={name}: use low, medium, high, xhigh or max")
                })?,
                (None, None) => floor(&root)?,
            };
            let runtime = match (runtime, env("REVIEW_RUNTIME")) {
                (Some(runtime), _) => runtime,
                (None, Some(name)) => clap::ValueEnum::from_str(&name, true)
                    .map_err(|_| format!("REVIEW_RUNTIME={name}: use claude, codex, dsh or pi"))?,
                (None, None) => Runtime::Claude,
            };
            let task = task.or_else(|| env("REVIEW_TASK").map(PathBuf::from));
            let task = match task {
                Some(path) => fs::read_to_string(path)?,
                None => String::new(),
            };
            let report = review(&root, tier, runtime, &task, &Plan::Full)?;
            println!("{}", report.markdown());
            if report.passes() {
                Ok(())
            } else {
                Err(if report.failures.is_empty() {
                    format!(
                        "review grade {}/10 is below {}",
                        report.grade, report.merge_grade
                    )
                } else {
                    format!(
                        "review incomplete: {} session(s) gave no usable answer",
                        report.failures.len()
                    )
                }
                .into())
            }
        }
        Commands::ReviewFloor => {
            println!("{}", floor(&root)?.name());
            Ok(())
        }
    }
}

/// A review input from origin/dev, so a branch never rewrites its own
/// criteria; the working tree only while dev does not have it yet (bootstrap).
pub(crate) fn trusted(root: &Path, relative: &str) -> std::result::Result<String, String> {
    match git::git(
        root,
        &["show", &format!("refs/remotes/origin/dev:{relative}")],
    ) {
        Ok(text) => Ok(text),
        Err(_) => {
            std::fs::read_to_string(root.join(relative)).map_err(|e| format!("{relative}: {e}"))
        }
    }
}

fn changed_suites(root: &Path) -> Result<(String, String, std::collections::BTreeSet<String>)> {
    let (base, merge_base) = git::base(root, "dev", false)?;
    let changed = git::changed(root, &merge_base)?;
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
    let suites = registry.classify(&changed).suites;
    Ok((base, merge_base, suites))
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
fn facts(root: &Path, merge_base: &str) -> Result<String> {
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
        "- lines added: {test_lines} in tests, {other_lines} elsewhere\n- added lines containing `#[ignore]`: {}, `unsafe`: {}, `.unwrap(`: {}, `cfg(test)`: {}\n- changed CI, gate or baseline files: {}\n",
        count("#[ignore]"),
        count("unsafe"),
        count(".unwrap("),
        count("cfg(test)"),
        if sensitive.is_empty() {
            "none".to_owned()
        } else {
            sensitive.join(", ")
        }
    ))
}

pub(crate) fn review(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
) -> Result<Report> {
    let config = Config::load(root)?;
    let model = config.model_for(tier, runtime, matches!(plan, Plan::Closing { .. }));
    let live = Live {
        runtime,
        root,
        model,
    };
    review_with(root, tier, runtime, task, plan, &live)
}

pub(crate) fn review_with(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
    ask: &dyn Ask,
) -> Result<Report> {
    let config = Config::load(root)?;
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
    let subject = prompt::Subject {
        head: head.clone(),
        merge_base: merge_base.clone(),
        task: if task.trim().is_empty() {
            git::git(
                root,
                &["log", "--format=%B", &format!("{merge_base}..HEAD")],
            )?
        } else {
            task.to_owned()
        },
        stat: git::git(root, &["diff", "--stat", &merge_base, "HEAD"])?,
        // A closing review audits the fixes since the last reviewed commit.
        diff: match plan {
            Plan::Full => git::git(root, &["diff", &merge_base, "HEAD"])?,
            // Only the branch's own files: a rebase must not bring dev's changes in.
            Plan::Closing { since, .. } => {
                let mut args = vec!["diff".to_owned(), since.clone(), "HEAD".into(), "--".into()];
                args.extend(git::changed(root, &merge_base)?);
                git::git(root, &args.iter().map(String::as_str).collect::<Vec<_>>())?
            }
        },
        facts: facts(root, &merge_base)?,
    };
    let closing = matches!(plan, Plan::Closing { .. });
    let tier_config = config.tiers[&tier].clone();
    let model = config.model_for(tier, runtime, matches!(plan, Plan::Closing { .. }));
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
    let first = match plan {
        Plan::Closing { prior, .. } => {
            findings = prior.clone();
            1
        }
        Plan::Full => {
            blind_round(root, &personas, &subject, ask, &mut findings, &mut failures)?;
            for finding in &mut findings {
                finding.status = protocol::status(finding, reviewers);
            }
            2
        }
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
                let ids = findings.iter().filter(|f| f.reporter != *r);
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
                            f.id == verdict.id && ids.contains(&f.id) && f.reporter != reviewer
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
                    add_findings(&mut findings, reviewer, round, serious);
                }
                Err(error) => {
                    failures.push(format!("round {round}, {}: {error}", personas[reviewer]))
                }
            }
        }
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
    let report = Report {
        version: 1,
        head,
        branch: git::branch(root).unwrap_or_default(),
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
            Ok(answer) => add_findings(findings, reviewer, 1, answer.findings),
            Err(error) => failures.push(format!("round 1, {}: {error}", personas[reviewer])),
        }
    }
    Ok(())
}

fn add_findings(
    findings: &mut Vec<Finding>,
    reviewer: usize,
    round: u32,
    reported: Vec<protocol::Reported>,
) {
    for reported in reported {
        let id = format!("F{}", findings.len() + 1);
        findings.push(Finding {
            id,
            reporter: reviewer,
            round,
            reported,
            votes: Vec::new(),
            status: Status::Disputed,
        });
    }
}
