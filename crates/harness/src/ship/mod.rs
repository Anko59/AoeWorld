//! `make ship`: the only way a Claude, Codex, DeepSeek Harness or pi session
//! publishes work. It refuses a dirty tree, a protected branch or a detached
//! HEAD; runs the registry's preflight gates at that exact commit; writes the
//! evidence for that commit (whatever the verdict) where agents cannot write;
//! and, only on PASS, pushes the branch and creates or updates its pull request.
//! The agent policy denies `git push` and `gh pr create` to every role.
mod describe;
pub(crate) mod evidence;
pub(crate) mod git;
mod github;
mod issues;
mod metrics;
mod prepush;
mod review_gate;
mod review_pr;
pub(crate) mod run;
mod showcase;
pub(crate) mod test_first;
#[cfg(test)]
mod tests;

use crate::gates::registry::{Cadence, Registry};
use evidence::{Evidence, Verdict};
use std::{
    fs,
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
    /// File an out-of-scope task as a deduplicated GitHub issue.
    Issue,
    /// Print the highest-priority unblocked open issue.
    Next,
    /// Record the showcase video of SHOWCASE_STORYBOARD (docs/showcase.md).
    Showcase,
    /// Validate a showcase storyboard without starting Docker or speech requests.
    ShowcaseCheck,
    /// Open, comment on or close one issue per nightly job (NIGHTLY_RESULTS).
    NightlyTriage,
    /// Pre-push: succeed only if `make ship` evidence covers the pushed HEAD.
    PrepushEvidence,
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
    /// Showcase video for the "What" section (SHIP_VIDEO).
    #[arg(long)]
    pub(crate) video: Option<PathBuf>,
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
            video: None,
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
            options.video = options
                .video
                .or_else(|| env("SHIP_VIDEO").map(PathBuf::from));
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
        Commands::Showcase => showcase::make(&root).map(|_| ()),
        Commands::ShowcaseCheck => showcase::check(&root),
        Commands::PrepushEvidence => prepush::check(&root),
        Commands::ShipStatus => {
            let head = git::git(&root, &["rev-parse", "HEAD"])?;
            match evidence::read(&root, &head)? {
                Some(evidence) => println!("{}", serde_json::to_string_pretty(&evidence)?),
                None => println!("no evidence for {head}; run `make ship`"),
            }
            Ok(())
        }
        Commands::ReviewPr => review_pr::execute(&root),
        Commands::Issue => issues::issue(&root),
        Commands::Next => issues::next(&root),
        Commands::NightlyTriage => issues::triage::execute(&root),
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
    let body_text = if !options.no_pr {
        github::preflight_pull_request(root, options)?
    } else {
        None
    };
    let evidence = judge(root, options)?;
    for result in &evidence.gates {
        eprintln!("{}", result.line());
    }
    // Reported to the reviewers and in the description; never a refusal.
    eprint!(
        "ship: test-first report\n{}",
        test_first::summary(root, &evidence.merge_base, &evidence.head)
    );
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
        if let (Some(report), Some(body_text)) = (&review, body_text.as_deref()) {
            options.body_file = Some(github::description(
                root, &evidence, &options, report, body_text,
            )?);
        }
        let url = github::pull_request(root, &evidence, &options)?;
        if let (Some(video), Some(body)) = (&options.video, &options.body_file) {
            let repository = git::origin_repository(root)?;
            let body = body.display().to_string();
            let video = video.display().to_string();
            gh(
                root,
                &[
                    "pr",
                    "edit",
                    &url,
                    "--repo",
                    &repository,
                    "--body-file",
                    &body,
                    "--attach",
                    &video,
                ],
            )?;
            let remote_body = gh(
                root,
                &[
                    "pr",
                    "view",
                    &url,
                    "--repo",
                    &repository,
                    "--json",
                    "body",
                    "--jq",
                    ".body",
                ],
            )?;
            let submitted_body = fs::read_to_string(&body)?;
            let body_with_video = describe::place_video_in_what(&submitted_body, &remote_body);
            fs::write(&body, body_with_video)?;
            gh(
                root,
                &[
                    "pr",
                    "edit",
                    &url,
                    "--repo",
                    &repository,
                    "--body-file",
                    &body,
                ],
            )?;
        }
        if let Some(report) = &review {
            review_gate::publish(root, &url, report)?;
            // The review passed: a leftover that cannot be filed is reported, not fatal.
            for problem in issues::followups::file(root, &url, report) {
                eprintln!(
                    "ship: review follow-up not filed ({problem}); file it with `make issue`"
                );
            }
        }
    }
    Ok(evidence)
}

/// Git opens the SSH connection before the pre-push hook runs `make
/// preflight` for several minutes; without keepalives GitHub drops the idle
/// connection and the push dies silently (#209). A caller's own SSH
/// command (`GIT_SSH_COMMAND`, `GIT_SSH` or `core.sshCommand`) wins.
fn ssh_keepalive(caller_chose_ssh: bool) -> Option<&'static str> {
    (!caller_chose_ssh).then_some("ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=40")
}

fn caller_chose_ssh(root: &Path) -> bool {
    std::env::var_os("GIT_SSH_COMMAND").is_some()
        || std::env::var_os("GIT_SSH").is_some()
        || git::git(root, &["config", "--get", "core.sshCommand"]).is_ok()
}

fn push(root: &Path, evidence: &Evidence, force: bool) -> Result<()> {
    // The remote is fixed and the refspec names the evidenced branch only.
    let refspec = format!("{}:refs/heads/{}", evidence.head, evidence.branch);
    let mut command = Command::new("git");
    command.arg("-C").arg(root).arg("push");
    command.env(prepush::ENV, &evidence.head);
    if force {
        command.arg("--force-with-lease");
    }
    if let Some(ssh) = ssh_keepalive(caller_chose_ssh(root)) {
        command.env("GIT_SSH_COMMAND", ssh);
    }
    let status = command.args(["origin", &refspec]).status()?;
    if !status.success() {
        return Err(format!(
            "git push failed ({status}; the pre-push hook re-runs `make preflight`)"
        )
        .into());
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

pub(crate) use github::gh;
