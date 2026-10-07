//! `make ship`: the only way a Claude, Codex, DeepSeek Harness or pi session
//! publishes work. It refuses a dirty tree, a protected branch or a detached
//! HEAD; runs the registry's preflight gates at that exact commit; writes the
//! evidence for that commit (whatever the verdict) where agents cannot write;
//! and, only on PASS, pushes the branch and creates or updates its pull request.
//! The agent policy denies `git push` and `gh pr create` to every role.
pub(crate) mod evidence;
pub(crate) mod git;
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
    /// Integration branch the change is judged against and targets.
    #[arg(long, default_value = "dev")]
    pub(crate) base: String,
    /// Push only; leave the pull request alone.
    #[arg(long)]
    pub(crate) no_pr: bool,
    /// Use the already-fetched `origin/<base>` (tests, offline).
    #[arg(long)]
    pub(crate) no_fetch: bool,
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
        }
    }
}

pub(crate) fn execute(command: Commands) -> Result<()> {
    let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    match command {
        Commands::Ship(options) => ship(&root, &options).map(|_| ()),
        Commands::ShipStatus => {
            let head = git::git(&root, &["rev-parse", "HEAD"])?;
            match evidence::read(&root, &head)? {
                Some(evidence) => println!("{}", serde_json::to_string_pretty(&evidence)?),
                None => println!("no evidence for {head}; run `make ship`"),
            }
            Ok(())
        }
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
    push(root, &evidence, options.force)?;
    if !options.no_pr {
        pull_request(root, &evidence, options)?;
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

fn gh(root: &Path, args: &[&str]) -> Result<String> {
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

fn pull_request(root: &Path, evidence: &Evidence, options: &Options) -> Result<()> {
    let branch = evidence.branch.as_str();
    let body = options.body_file.as_ref().map(|p| p.display().to_string());
    let existing = gh(
        root,
        &[
            "pr",
            "view",
            branch,
            "--json",
            "url,state",
            "--jq",
            "select(.state == \"OPEN\") | .url",
        ],
    )
    .ok();
    let url = match existing.filter(|url| !url.is_empty()) {
        Some(url) => {
            let mut args = vec!["pr", "edit", branch];
            if let Some(title) = options.title.as_deref() {
                args.extend(["--title", title]);
            }
            if let Some(body) = body.as_deref() {
                args.extend(["--body-file", body]);
            }
            if args.len() > 3 {
                gh(root, &args)?;
            }
            url
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
    Ok(())
}
