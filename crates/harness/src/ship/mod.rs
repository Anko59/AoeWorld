//! `make ship`: the only way a Claude, Codex, DeepSeek Harness or pi session
//! publishes work. It refuses a dirty tree, a protected branch or a detached
//! HEAD; runs the registry's preflight gates at that exact commit; writes the
//! evidence for that commit (whatever the verdict) where agents cannot write;
//! and, only on PASS, pushes the branch and creates or updates its pull request.
//! The agent policy denies `git push` and `gh pr create` to every role.
pub(crate) mod evidence;
pub(crate) mod git;
mod review_gate;
mod review_pr;
pub(crate) mod run;
#[cfg(test)]
mod tests;

use crate::gates::registry::{Cadence, Registry};
use evidence::{Evidence, Verdict};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Run the preflight gates at HEAD, record evidence, push and open or update the PR.
    Ship(Options),
    /// Print the evidence recorded for HEAD, if any.
    ShipStatus,
    /// Review an existing same-repository pull request without opening it.
    ReviewPr,
}

#[derive(clap::Args, Clone, Debug)]
pub(crate) struct Options {
    /// Pull request title (required when the branch has no PR yet).
    #[arg(long)]
    pub(crate) title: Option<String>,
    /// Markdown file holding the pull request description.
    #[arg(long)]
    pub(crate) body_file: Option<PathBuf>,
    /// Push with `--force-with-lease`, after a rebase.
    #[arg(long)]
    pub(crate) force: bool,
    /// Always `dev`: not settable from the command line.
    #[arg(skip = String::from("dev"))]
    pub(crate) base: String,
    /// Tests only: leave the pull request alone.
    #[arg(skip)]
    pub(crate) no_pr: bool,
    /// Tests only: use the already-fetched `origin/dev`.
    #[arg(skip)]
    pub(crate) no_fetch: bool,
    /// Review tier (SHIP_TIER); defaults to, and may not go below, the floor.
    #[arg(long, value_enum)]
    pub(crate) tier: Option<crate::review::Tier>,
    /// The runtime whose model family reviews (SHIP_RUNTIME).
    #[arg(long, value_enum, default_value = "claude")]
    pub(crate) runtime: crate::agents::Runtime,
    /// Tests only: skip the adversarial review.
    #[arg(skip)]
    pub(crate) no_review: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            title: None,
            body_file: None,
            force: false,
            base: "dev".into(),
            no_pr: false,
            no_fetch: false,
            tier: None,
            runtime: crate::agents::Runtime::Claude,
            no_review: false,
        }
    }
}

pub(crate) fn execute(command: Commands) -> Result<()> {
    let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    match command {
        Commands::Ship(mut options) => {
            // `make ship` passes these through the environment, never through shell text.
            let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
            options.title = options.title.or_else(|| env("SHIP_TITLE"));
            options.body_file = options
                .body_file
                .or_else(|| env("SHIP_BODY").map(PathBuf::from));
            options.force |= env("SHIP_FORCE").as_deref() == Some("1");
            if let Some(tier) = env("SHIP_TIER") {
                options.tier = Some(clap::ValueEnum::from_str(&tier, true).map_err(|_| {
                    format!("SHIP_TIER={tier}: use low, medium, high, xhigh or max")
                })?);
            }
            if let Some(runtime) = env("SHIP_RUNTIME") {
                options.runtime = clap::ValueEnum::from_str(&runtime, true)
                    .map_err(|_| format!("SHIP_RUNTIME={runtime}: use claude, codex, dsh or pi"))?;
            }
            ship(&root, &options).map(|_| ())
        }
        Commands::ShipStatus => {
            let head = git::git(&root, &["rev-parse", "HEAD"])?;
            match evidence::read(&root, &head)? {
                Some(evidence) => println!("{}", serde_json::to_string_pretty(&evidence)?),
                None => println!("no evidence for {head}; run `make ship`"),
            }
            Ok(())
        }
        Commands::ReviewPr => review_pr::execute(&root),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Gates at the exact commit, then evidence; a moved HEAD or tree leaves none.
pub(crate) fn judge(root: &Path, options: &Options) -> Result<Evidence> {
    let subject = git::subject(root)?;
    let (base, merge_base) = git::base(root, &options.base, !options.no_fetch)?;
    let changed = git::changed(root, &merge_base)?;
    let registry = Registry::load(root)?;
    let suites = registry.classify(&changed).suites;
    let plan = registry.plan(Cadence::Preflight, &suites)?;
    let gates: Vec<_> = plan
        .gates
        .iter()
        .filter_map(|id| registry.gates.iter().find(|gate| &gate.id == id))
        .collect();
    let started = now();
    eprintln!(
        "ship: {} preflight gate(s) at {} on {}",
        gates.len(),
        &subject.head[..12],
        subject.branch
    );
    let results = run::run_gates(root, &gates);
    if git::subject(root).ok().as_ref() != Some(&subject) {
        return Err("HEAD, the tree or the branch changed while the gates ran; no evidence written. Commit, then ship again".into());
    }
    let evidence = Evidence {
        version: 1,
        cadence: "preflight".into(),
        verdict: evidence::verdict(&results),
        head: subject.head,
        tree: subject.tree,
        branch: subject.branch,
        base,
        merge_base,
        changed,
        gates: results,
        started,
        finished: now(),
    };
    let path = evidence::write(root, &evidence)?;
    eprintln!("ship: evidence {}", path.display());
    Ok(evidence)
}

pub(crate) fn ship(root: &Path, options: &Options) -> Result<Evidence> {
    ship_with(root, options, &crate::review::review)
}

/// `ship` with the reviewer injected: the real one, or a scripted one in tests.
pub(crate) fn ship_with(
    root: &Path,
    options: &Options,
    reviewer: &review_gate::Reviewer<'_>,
) -> Result<Evidence> {
    if !options.no_review && !options.no_pr && options.body_file.is_none() {
        return Err(
            "SHIP_BODY is required: the review is appended to the pull request description".into(),
        );
    }
    if !options.no_pr {
        preflight_pull_request(root, options)?;
    }
    let evidence = judge(root, options)?;
    for result in &evidence.gates {
        eprintln!("{}", result.line());
    }
    match evidence.verdict {
        Verdict::Pass => {}
        Verdict::Fail => {
            for result in evidence.gates.iter().filter(|r| !r.tail.is_empty()) {
                eprintln!("--- {} (last 40 lines) ---\n{}", result.gate, result.tail);
            }
            return Err("ship: FAIL; fix the gates (never weaken one) and ship again".into());
        }
        Verdict::Incomplete => {
            return Err(
                "ship: INCOMPLETE (a gate could not run, e.g. Docker is down); nothing was pushed"
                    .into(),
            );
        }
    }
    let review = if options.no_review {
        None
    } else {
        Some(review_gate::require(root, &evidence, options, reviewer)?)
    };
    push(root, &evidence, options.force)?;
    if !options.no_pr {
        let mut options = options.clone();
        if let Some(report) = &review {
            options.body_file = review_gate::description(root, &options, report)?;
        }
        let url = pull_request(root, &evidence, &options)?;
        if let Some(report) = &review {
            review_gate::publish(root, &url, report)?;
        }
    }
    Ok(evidence)
}

fn push(root: &Path, evidence: &Evidence, force: bool) -> Result<()> {
    // The remote is fixed and the refspec names the evidenced branch only.
    let refspec = format!("{}:refs/heads/{}", evidence.head, evidence.branch);
    let mut command = Command::new("git");
    command.arg("-C").arg(root).arg("push");
    if force {
        command.arg("--force-with-lease");
    }
    let status = command.args(["origin", &refspec]).status()?;
    if !status.success() {
        return Err("git push failed (the pre-push hook re-runs `make preflight`)".into());
    }
    // Best effort: a commit-id refspec cannot set the upstream itself.
    let _ = git::git(
        root,
        &[
            "branch",
            "--set-upstream-to",
            &format!("origin/{}", evidence.branch),
        ],
    );
    Ok(())
}

pub(crate) fn gh(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("gh").current_dir(root).args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "gh {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The oldest `gh` with `--attach` for PR videos; older ones are refused.
const GH_MINIMUM: (u32, u32) = (2, 100);

fn gh_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(2)?;
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Before any gate runs: a usable `gh`, and a title and description when the
/// branch has no open pull request yet, so nothing is pushed half-way.
fn preflight_pull_request(root: &Path, options: &Options) -> Result<()> {
    let version = gh(root, &["--version"])?;
    match gh_version(&version) {
        Some(found) if found >= GH_MINIMUM => {}
        _ => {
            return Err(format!(
                "gh {}.{} or newer is required (found: {})",
                GH_MINIMUM.0,
                GH_MINIMUM.1,
                version.lines().next().unwrap_or_default()
            )
            .into());
        }
    }
    let branch = git::branch(root)?;
    let repository = git::origin_repository(root)?;
    let open = open_pull_request(root, &repository, &branch)?.is_some();
    if !open && (options.title.is_none() || options.body_file.is_none()) {
        return Err("a new pull request needs SHIP_TITLE and SHIP_BODY (a description file); nothing was run".into());
    }
    if let Some(body) = &options.body_file
        && !body.is_file()
    {
        return Err(format!("SHIP_BODY {} is not a file", body.display()).into());
    }
    Ok(())
}

/// The open pull request whose head is `branch` (never a PR number that
/// happens to equal a numeric branch name).
fn open_pull_request(root: &Path, repository: &str, branch: &str) -> Result<Option<String>> {
    let url = gh(
        root,
        &[
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "url",
            "--jq",
            ".[0].url // empty",
        ],
    )?;
    Ok((!url.is_empty()).then_some(url))
}

fn pull_request(root: &Path, evidence: &Evidence, options: &Options) -> Result<String> {
    let branch = evidence.branch.as_str();
    let repository = git::origin_repository(root)?;
    let body = options.body_file.as_ref().map(|p| p.display().to_string());
    let existing = open_pull_request(root, &repository, branch)?;
    let url = match existing {
        Some(url) => {
            let mut args = vec!["pr", "edit", url.as_str(), "--repo", repository.as_str()];
            if let Some(title) = options.title.as_deref() {
                args.extend(["--title", title]);
            }
            if let Some(body) = body.as_deref() {
                args.extend(["--body-file", body]);
            }
            if args.len() > 5 {
                gh(root, &args)?;
            }
            url.clone()
        }
        None => {
            let (Some(title), Some(body)) = (options.title.as_deref(), body.as_deref()) else {
                return Err(
                    "a new pull request needs SHIP_TITLE and SHIP_BODY (a description file)".into(),
                );
            };
            gh(
                root,
                &[
                    "pr",
                    "create",
                    "--repo",
                    &repository,
                    "--base",
                    &options.base,
                    "--head",
                    branch,
                    "--title",
                    title,
                    "--body-file",
                    body,
                ],
            )?
        }
    };
    println!("{url}");
    Ok(url)
}
