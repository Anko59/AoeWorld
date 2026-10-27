//! `make ship`: the only way a Claude, Codex, DeepSeek Harness or pi session
//! publishes work. It refuses a dirty tree, a protected branch or a detached
//! HEAD; runs the registry's preflight gates at that exact commit; writes the
//! evidence for that commit (whatever the verdict) where agents cannot write;
//! and, only on PASS, pushes the branch and creates or updates its pull request.
//! The agent policy denies `git push` and `gh pr create` to every role.
mod check;
pub(crate) mod compare;
mod describe;
mod duel;
pub(crate) use duel::output::JsonOutput;
pub(crate) mod evidence;
pub(crate) mod git;
mod github;
mod issues;
mod levels;
pub(crate) mod lock;
mod metrics;
mod playtest;
mod postpush;
pub(crate) mod prepush;
mod recheck;
pub(crate) mod review_gate;
mod review_pr;
pub(crate) mod run;
mod showcase;
pub(crate) mod stack;
#[cfg(test)]
mod tests;

use evidence::{Evidence, Verdict};
#[cfg(test)]
use git::publish::{caller_chose_ssh, push, push_command, ssh_keepalive};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Run tester red then implementer green and commit each step (DUEL_*).
    Duel,
    /// Run only tester-changed Cargo packages and browser spec files in Docker.
    DuelCheck,
    /// Finish the focused check, running only selected browser specs when present.
    DuelBrowserCheck,
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
    /// Check a pull request description (SHIP_BODY, SHIP_TITLE, SHIP_VIDEO) without shipping.
    DescribeCheck,
    /// Print the lowest pull request level the change allows, and why.
    LevelFloor,
    /// Print the levels of the last merged pull requests against the target.
    LevelStats,
    /// Screenshot bookmarked cameras on the build before and after the change (COMPARE_*).
    Compare,
    /// Build, start or keep current the shared dev server (docs/dev-server.md).
    DevServer,
    /// Print the shared dev server's revision, port, uptime and prepared maps.
    DevServerStatus,
    /// Stop the shared dev server and its compare view servers.
    DevServerStop,
    /// Record the showcase video of SHOWCASE_STORYBOARD (docs/showcase.md).
    Showcase,
    /// Validate a showcase storyboard without starting Docker or speech requests.
    ShowcaseCheck,
    /// Print the `agent-session` steps of a headless agent log (SESSION=<jsonl>).
    ShowcaseSession,
    /// Record nightly failures in the configured workstream (NIGHTLY_RESULTS).
    NightlyTriage,
    /// Record nightly metric regressions in the configured workstream.
    MetricRegressions,
    /// Carry `harness/review` to the rebased head of pull request CARRY_PR when its patch and the policy are unchanged.
    CarryReview,
    /// Update the open pull requests with auto-merge that are behind dev (UPDATE_MAX_PRS).
    UpdatePrs,
    /// Write and print the metrics snapshot of HEAD or METRICS_REV.
    Metrics,
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
    /// `dev`, or a parent branch (local before its PR exists) to stack on
    /// (SHIP_BASE only; not settable from the command line).
    #[arg(skip = String::from("dev"))]
    pub(crate) base: String,
    /// The open pull request of a stacked `base`, found before the gates run.
    #[arg(skip)]
    pub(crate) parent: Option<stack::Parent>,
    /// Parent has no PR yet: store the review, without pushing or opening one.
    #[arg(skip)]
    pub(crate) review_ahead: bool,
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
    /// The level, its floor and the reason, recorded in the evidence.
    #[arg(skip)]
    pub(crate) level: Option<Box<levels::Decision>>,
    /// Showcase video for the "What" section (SHIP_VIDEO).
    #[arg(long)]
    pub(crate) video: Option<PathBuf>,
    /// The heavy-slot environment of this process, read once by `execute`.
    #[arg(skip)]
    pub(crate) slots: Box<crate::slots::SlotEnv>,
    /// The ship lock, let go during the review and taken again for the push.
    #[arg(skip)]
    pub(crate) lock: lock::Handoff,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            title: None,
            body_file: None,
            force: false,
            base: "dev".into(),
            parent: None,
            review_ahead: false,
            no_pr: false,
            no_fetch: false,
            tier: None,
            runtime: crate::agents::Runtime::Claude,
            no_review: false,
            level: None,
            video: None,
            slots: Box::default(),
            lock: lock::Handoff::default(),
        }
    }
}

pub(crate) fn execute(command: Commands, slots: &crate::slots::SlotEnv) -> Result<()> {
    let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    match command {
        Commands::Duel => duel::execute(&root).map_err(Into::into),
        Commands::DuelCheck => duel::stages::native(&root).map_err(Into::into),
        Commands::DuelBrowserCheck => duel::stages::browser(&root).map_err(Into::into),
        Commands::Ship(mut options) => {
            options.slots = Box::new(slots.clone());
            // `make ship` passes these through the environment, never through shell text.
            let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
            options.title = options.title.or_else(|| env("SHIP_TITLE"));
            options.body_file = options
                .body_file
                .or_else(|| env("SHIP_BODY").map(PathBuf::from));
            options.force |= env("SHIP_FORCE").as_deref() == Some("1");
            if let Some(base) = env("SHIP_BASE") {
                options.base = base;
            }
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
            check::publish(&root, &options)
        }
        Commands::Compare => compare::execute(&root),
        Commands::DevServer => crate::devserver::start(&root),
        Commands::DevServerStatus => crate::devserver::status(&root),
        Commands::DevServerStop => crate::devserver::stop(&root),
        Commands::Showcase => showcase::make(&root).map(|_| ()),
        Commands::ShowcaseCheck => showcase::check(&root),
        Commands::ShowcaseSession => {
            let log = std::env::var("SESSION")
                .ok()
                .filter(|value| !value.is_empty())
                .ok_or("SESSION is unset: run `make showcase-session SESSION=<jsonl>`")?;
            showcase::session::run(Path::new(&log))
        }
        Commands::PrepushEvidence => prepush::check(&root),
        Commands::ShipStatus => {
            let head = git::git(&root, &["rev-parse", "HEAD"])?;
            match evidence::read(&root, &head)? {
                Some(evidence) => println!("{}", serde_json::to_string_pretty(&evidence)?),
                None => println!("no evidence for {head}; run `make ship`"),
            }
            lock::print_status(&root)?;
            crate::slots::print_status(&root, slots)
        }
        Commands::ReviewPr => review_pr::execute(&root),
        Commands::Issue => issues::issue(&root),
        Commands::Next => issues::next(&root),
        Commands::DescribeCheck => check::command(&root),
        Commands::LevelFloor => levels::print_floor(&root),
        Commands::LevelStats => levels::print_stats(&root),
        Commands::NightlyTriage => issues::triage::execute(&root),
        Commands::MetricRegressions => issues::regressions::execute(&root),
        Commands::CarryReview => postpush::carry::execute(&root),
        Commands::UpdatePrs => postpush::update::execute(&root),
        Commands::Metrics => Ok(metrics::snapshot::command(&root)?),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) use evidence::judge;

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
    let branch = git::branch(root)?;
    let push_tip = git::lease::tracking(root, &branch);
    let started_head = git::git(root, &["rev-parse", "HEAD"])?;
    stack::refuse_base(&options.base, &branch)?;
    // Suites, the review diff and metrics all measure the change
    // against the selected base (origin, or an explicitly local parent).
    let _base = crate::review::base::scope(&options.base);
    let preflight = if !options.no_pr {
        github::preflight_pull_request(root, options)?
    } else {
        github::Preflight::default()
    };
    let body_text = preflight.body;
    // A restack rewrote the branch: the open PR targets another base, or
    // (GitHub already retargeted it) the last review was of another base.
    let retarget = stack::retarget(preflight.current_base.as_deref(), &options.base).is_some()
        || stack::restacked(
            &crate::review::history(root, &branch)?,
            &options.base,
            stack::remote_is_ancestor(root, &branch),
        );
    let options = &Options {
        parent: preflight.parent,
        review_ahead: preflight.review_ahead || options.review_ahead,
        level: preflight.level.map(Box::new),
        force: options.force || retarget,
        ..options.clone()
    };
    let _selected = if options.review_ahead {
        crate::review::base::scope_local(&options.base)
    } else {
        crate::review::base::scope(&options.base)
    };
    let base = crate::timeline::stage(root, "rebase", serde_json::json!({}), || {
        recheck::start::prepare(root, options)
    })?;
    // The start-of-ship rebase is done. The gates take heavy slots, which serialize the heavy
    // work, and their evidence is keyed by the exact commit, so the next ship may take the lock
    // and start meanwhile. The lock is taken again for the re-check and the push.
    if options.lock.release() {
        eprintln!("ship: lock released for the gates and the review");
    }
    let _anchor = crate::review::base::scope_at(&options.base, &base);
    let options = &Options {
        no_fetch: true,
        ..options.clone()
    };
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
    let early_draft =
        !options.no_pr && !options.no_review && !options.review_ahead && options.base == "dev";
    if early_draft {
        postpush::draft::start(
            root,
            &evidence,
            options,
            body_text.as_deref().ok_or("no validated draft body")?,
            &started_head,
            push_tip.as_deref(),
        )?;
    }
    // The next lease must expect the head the draft push wrote, including any
    // rebase performed before the gates, rather than the original checkout.
    let started_head = if early_draft {
        evidence.head.clone()
    } else {
        started_head
    };
    let push_tip = if early_draft {
        Some(evidence.head.clone())
    } else {
        push_tip
    };
    let started = playtest::start(root, &base, &evidence.head);
    let reviewed = if options.no_review {
        Ok(None)
    } else {
        match (early_draft, body_text.as_deref()) {
            (true, Some(task)) => {
                review_gate::require_task(root, &evidence, options, task, reviewer)
            }
            _ => review_gate::require(root, &evidence, options, reviewer),
        }
        .map(Some)
    };
    let playtest = match (&reviewed, started) {
        (Err(_), Some(started)) => {
            started.stop();
            None
        }
        (_, started) => started.and_then(playtest::Started::finish),
    };
    let applicable = git::changed_between(root, &base, &evidence.head)
        .is_ok_and(|paths| paths.iter().any(|p| playtest::gameplay(p)));
    let review = match reviewed {
        Ok(report) => report,
        Err(error) if early_draft => {
            playtest::draft_advisory(
                root,
                &evidence,
                body_text.as_deref(),
                playtest.as_ref(),
                applicable,
            );
            playtest::announce(playtest.as_ref(), &evidence.head);
            let grade = postpush::ci::recorded_grade(root, &evidence.branch, &evidence.head);
            return postpush::ci::outcome(root, &evidence.head, grade, Err(error));
        }
        Err(error) => return Err(error),
    };
    playtest::enforce(playtest.as_ref(), applicable)?;
    if options.review_ahead {
        playtest::announce(playtest.as_ref(), &evidence.head);
        let report = review
            .as_ref()
            .ok_or("review ahead requires a passing review")?;
        eprintln!("ship: {}", stack::ahead_message(&options.base, report));
        return Ok(evidence);
    }
    let grade = review.as_ref().map(|report| report.grade);
    let mut pushed_head = evidence.head.clone();
    let published = (|| {
        options.lock.reacquire(root, "make ship")?;
        let rebased = crate::timeline::stage(root, "recheck", serde_json::json!({}), || {
            recheck::before_push(root, &evidence, review.as_ref(), options, &started_head)
        })?;
        let (evidence, review, lease, _recheck_anchor) = match rebased {
            Some(r) => (r.evidence, r.review, r.lease, Some(r._anchor)),
            None => (evidence, review, None, None),
        };
        crate::timeline::stage(root, "push", serde_json::json!({}), || {
            postpush::draft::push(
                root,
                &evidence,
                options,
                &started_head,
                push_tip.as_deref(),
                lease,
            )
        })?;
        pushed_head = evidence.head.clone();
        if !options.no_pr {
            let mut options = options.clone();
            if let (Some(report), Some(body_text)) = (&review, body_text.as_deref()) {
                let path = github::description(root, &evidence, &options, report, body_text)?;
                playtest::append(&path, playtest.as_ref(), applicable, &evidence.head)?;
                options.body_file = Some(path);
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
                let publish = || {
                    if early_draft {
                        review_gate::publish_ready(root, &url, report, &evidence.base_branch)
                    } else {
                        review_gate::publish(root, &url, report, &evidence.base_branch)
                    }
                };
                postpush::cleanup::after_publication(root, &evidence.base_branch, publish)?;
                if let Err(error) = issues::followups::file(root, &url, report) {
                    eprintln!("warning: could not file follow-ups ({error}); file them by hand");
                }
                eprintln!("{}", stack::next_steps(options.parent.as_ref(), &url));
            }
        }
        Ok(evidence)
    })();
    playtest::announce(playtest.as_ref(), &pushed_head);
    if early_draft {
        postpush::ci::outcome(root, &pushed_head, grade, published)
    } else {
        published
    }
}

pub(crate) use github::gh;
