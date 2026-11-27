//! `make game-score-pr`: the game score of a pull request, against its merge base. What `make
//! ship` runs when the change can affect the game (`gates/ci-rules.json`, `local_checks`), and
//! what an agent runs by hand to see the same table before it ships.
//!
//! The `service` numbers of a game are a function of the game's code, the map and the scenarios
//! (`key`), so the merge base is never played twice for the same game state: its report is kept
//! in the shared home (`home`, `cache`), and only a miss builds and plays it, in one persistent
//! scratch worktree (`base`). The branch is played every time its state is new, as the game
//! score plays it (`launch`, `real`). The two reports are compared number by number (`compare`),
//! judged, when the game differs, only for the numbers that moved beyond what they do across
//! seeds on the same code (`spread`, `declare`, `verdict`), and
//! written as the comment `make ship` keeps on the pull request and as terminal text (`render`).
//! Informational: no verdict here fails anything. See docs/scale/game-score.md.
mod base;
mod cache;
mod compare;
mod declare;
pub(crate) mod home;
mod key;
mod launch;
mod numbers;
mod real;
mod render;
pub(crate) mod spread;
mod verdict;

pub(crate) use render::{ADDED, MARKER};

use super::BoxError;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const CHECK: &str = "game-score";
const SCHEMA: u32 = 1;

#[derive(Args, Debug, Clone)]
pub struct Options {
    /// The branch the pull request goes into: its merge base with HEAD is what HEAD is compared with.
    #[arg(long, default_value = "origin/dev")]
    base: String,
    /// The pull request description: its `game-score:` lines declare the numbers that got worse
    /// on purpose.
    #[arg(long)]
    description: Option<PathBuf>,
    /// Where the result is written: the comment, the summary and the terminal text.
    #[arg(long, default_value = ".cache/game-score/pr.json")]
    out: PathBuf,
    /// The directory that holds the map package. Omitted: `packages` of the shared home.
    #[arg(long)]
    package_directory: Option<PathBuf>,
    /// The package to play on. Omitted: the only package of the directory.
    #[arg(long)]
    map_hash: Option<String>,
}

/// The result file: all `make ship` reads. `ran` false means nothing was measured and `summary`
/// says why.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Outcome {
    pub schema: u32,
    pub ran: bool,
    /// The commit the numbers are those of.
    pub head: String,
    pub base: Option<String>,
    /// One line: why nothing ran, or what the numbers say (at most 140 characters).
    pub summary: String,
    /// The pull request comment, under [`MARKER`]; empty when nothing ran.
    pub section: String,
    /// The same, for the terminal.
    pub terminal: String,
}

/// Everything the run needs, decided from the repository before any process starts.
pub(super) struct Run {
    pub root: PathBuf,
    pub home: PathBuf,
    pub head: String,
    pub base: String,
    pub map_hash: String,
    pub package_directory: PathBuf,
    pub branch_key: String,
    pub base_key: String,
    /// The working tree differs from HEAD in the game's inputs: the result is not the commit's,
    /// so it is not stored.
    pub dirty: bool,
    pub description: String,
}

pub(super) enum Plan {
    /// Nothing to measure: the reason, in one line.
    Skip(String),
    Run(Box<Run>),
}

/// What the branch's run handed back.
pub(super) struct BranchPlayed {
    /// `None` when the run crashed before it wrote its report.
    pub report: Option<Value>,
    pub crash: Option<String>,
    pub build_seconds: f64,
    pub play_seconds: f64,
}

pub(super) struct BasePlayed {
    pub report: Value,
    pub build_seconds: f64,
    pub play_seconds: f64,
}

/// The two ways to produce a report, injected so the flow is tested without a build.
pub(super) trait Runner {
    fn play_branch(&self, run: &Run) -> Result<BranchPlayed, BoxError>;
    /// Builds and plays the merge base at one worker, the scenarios of `scenarios`.
    fn play_base(&self, run: &Run, scenarios: &[String]) -> Result<BasePlayed, BoxError>;
}

fn git(root: &Path, args: &[&str]) -> Result<String, BoxError> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The merge base of `base` (a branch of origin, or any ref) and HEAD.
fn merge_base(root: &Path, base: &str) -> Result<String, BoxError> {
    let reference = match base.strip_prefix("origin/") {
        Some(tail) => format!("refs/remotes/origin/{tail}"),
        None => base.to_owned(),
    };
    let commit = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )?;
    git(root, &["merge-base", &commit, "HEAD"])
}

/// Decides from the repository whether to run, and with which keys.
pub(super) fn plan(root: &Path, options: &Options) -> Result<Plan, BoxError> {
    let decision = crate::gates::local_check(&options.base, CHECK)?;
    if !decision.runs {
        return Ok(Plan::Skip(decision.reason));
    }
    let home = home::directory(root)?;
    let package_directory = options
        .package_directory
        .clone()
        .unwrap_or_else(|| home::packages(&home));
    let map_hash = match super::resolve_hash(&package_directory, options.map_hash.as_deref()) {
        Ok(hash) => hash,
        Err(error) => return Ok(Plan::Skip(format!("skipped: {error}"))),
    };
    let head = git(root, &["rev-parse", "HEAD"])?;
    let base = merge_base(root, &options.base)?;
    let objects = |rev: &str| key::Inputs::read(root, rev, &map_hash, &decision.inputs);
    let description = match &options.description {
        Some(path) => fs::read_to_string(path)
            .map_err(|error| format!("description {}: {error}", path.display()))?,
        None => String::new(),
    };
    Ok(Plan::Run(Box::new(Run {
        root: root.to_owned(),
        home,
        branch_key: objects("HEAD")?.key(),
        base_key: objects(&base)?.key(),
        dirty: key::dirty(root, &decision.inputs)?,
        head,
        base,
        map_hash,
        package_directory,
        description,
    })))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn worker_counts(report: &Value) -> Vec<usize> {
    report["scenarios"][0]["determinism"]["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|run| run["workers"].as_u64().map(|count| count as usize))
        .collect()
}

fn scenario_ids(report: &Value) -> Vec<String> {
    report["scenarios"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|scenario| scenario["id"].as_str().map(str::to_owned))
        .collect()
}

fn cached(entry: cache::Entry, lookup_seconds: f64) -> (Value, render::Source) {
    let source = render::Source::Cached {
        key: entry.key.clone(),
        age_seconds: unix_now().saturating_sub(entry.stored_at),
        revision: entry.revision.clone(),
        play_seconds: entry.build_seconds + entry.play_seconds,
        lookup_seconds,
    };
    (entry.report, source)
}

fn store(run: &Run, key: &str, revision: &str, report: &Value, seconds: (f64, f64)) {
    let entry = cache::Entry {
        key: key.to_owned(),
        revision: revision.to_owned(),
        stored_at: unix_now(),
        workers: worker_counts(report),
        build_seconds: seconds.0,
        play_seconds: seconds.1,
        report: cache::slim(report),
    };
    if let Err(error) = cache::store(&home::results(&run.home), &entry) {
        eprintln!("game score: the result was not stored ({error})");
    }
}

/// The branch's numbers: the entry of its game state, or a run.
fn branch_side(
    run: &Run,
    runner: &dyn Runner,
    notes: &mut Vec<String>,
) -> Result<(Option<Value>, Option<String>, render::Source), BoxError> {
    let results = home::results(&run.home);
    let started = Instant::now();
    if !run.dirty
        && let Some(entry) = cache::load(&results, &run.branch_key)
    {
        let (report, source) = cached(entry, started.elapsed().as_secs_f64());
        return Ok((Some(report), None, source));
    }
    let played = runner.play_branch(run)?;
    let holds = played.report.as_ref().is_some_and(|report| {
        report["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|scenario| scenario["determinism"]["holds"] == true)
    });
    if run.dirty {
        notes.push(
            "the working tree differs from HEAD in the game's inputs: this result is not stored"
                .to_owned(),
        );
    } else if let Some(report) = played.report.as_ref().filter(|_| holds) {
        store(
            run,
            &run.branch_key,
            &run.head,
            report,
            (played.build_seconds, played.play_seconds),
        );
    }
    let source = render::Source::Played {
        build_seconds: played.build_seconds,
        play_seconds: played.play_seconds,
        key: (!run.dirty && holds).then(|| run.branch_key.clone()),
    };
    Ok((played.report, played.crash, source))
}

/// The merge base's numbers: the entry of its game state, or a build and a run. A base that
/// cannot be built is a line, not an error: the branch's numbers are still worth showing.
fn base_side(
    run: &Run,
    runner: &dyn Runner,
    scenarios: &[String],
) -> (Option<Value>, render::Source) {
    let results = home::results(&run.home);
    let started = Instant::now();
    if let Some(entry) = cache::load(&results, &run.base_key) {
        let (report, source) = cached(entry, started.elapsed().as_secs_f64());
        return (Some(report), source);
    }
    match runner.play_base(run, scenarios) {
        Ok(played) => {
            store(
                run,
                &run.base_key,
                &run.base,
                &played.report,
                (played.build_seconds, played.play_seconds),
            );
            let source = render::Source::Played {
                build_seconds: played.build_seconds,
                play_seconds: played.play_seconds,
                key: Some(run.base_key.clone()),
            };
            (Some(played.report), source)
        }
        Err(error) => (
            None,
            render::Source::Unavailable(format!(
                "the merge base {} could not be played: {error}",
                &run.base[..12.min(run.base.len())]
            )),
        ),
    }
}

/// Runs the plan: the branch, the merge base, then the comparison, the verdicts and the text.
fn execute(run: &Run, runner: &dyn Runner, spread: &spread::Spread) -> Result<Outcome, BoxError> {
    let mut notes = Vec::new();
    let (branch, crash, branch_source) = branch_side(run, runner, &mut notes)?;
    let scenarios = branch.as_ref().map(scenario_ids).unwrap_or_default();
    let (base, base_source) = match &branch {
        Some(_) => base_side(run, runner, &scenarios),
        None => (
            None,
            render::Source::Unavailable("the branch crashed".to_owned()),
        ),
    };
    let diffs = match &branch {
        Some(branch) => compare::compare(base.as_ref(), branch),
        None => Vec::new(),
    };
    let (declarations, unreadable) = declare::parse(&run.description);
    let verdicts = verdict::verdicts(&verdict::Facts {
        branch: branch.as_ref(),
        crash: crash.as_deref(),
        diffs: &diffs,
        declarations: &declarations,
        unreadable: &unreadable,
        spread,
    });
    let page = render::Page {
        head: &run.head,
        base: Some(&run.base),
        map_hash: &run.map_hash,
        branch: branch.as_ref(),
        base_report: base.as_ref(),
        diffs: &diffs,
        verdicts: &verdicts,
        notes: &notes,
        branch_source: &branch_source,
        base_source: &base_source,
    };
    Ok(Outcome {
        schema: SCHEMA,
        ran: true,
        head: run.head.clone(),
        base: Some(run.base.clone()),
        summary: verdicts.summary.clone(),
        section: render::section(&page),
        terminal: render::terminal(&page),
    })
}

fn skipped(root: &Path, reason: &str) -> Result<Outcome, BoxError> {
    Ok(Outcome {
        schema: SCHEMA,
        ran: false,
        head: git(root, &["rev-parse", "HEAD"])?,
        summary: reason.chars().take(140).collect(),
        ..Outcome::default()
    })
}

pub fn run(options: Options) -> Result<(), BoxError> {
    let root = PathBuf::from(git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    let outcome = match plan(&root, &options)? {
        Plan::Skip(reason) => skipped(&root, &reason)?,
        Plan::Run(run) => execute(&run, &real::Real, spread::Spread::committed()?)?,
    };
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&options.out, serde_json::to_string(&outcome)? + "\n")?;
    if outcome.ran {
        eprint!("{}", outcome.terminal);
    }
    eprintln!("game score: {}", outcome.summary);
    Ok(())
}

#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod fixtures;
#[cfg(test)]
#[path = "tests/flow.rs"]
mod tests;
