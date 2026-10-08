//! SessionStart gives Claude the checkout, the judge, the last check and the
//! rules; PreCompact saves progress that SessionStart gives back after compaction.
use super::{
    context::{BLOCKED, Context, RECORDS},
    stop,
};
use std::{fs, path::Path, process::Command};

const RULES: &str = "\
Rules this harness enforces in every Bash/Edit/Write call (see CLAUDE.md and docs/agent-runtimes.md):
- Compile and check only through Dockerized Make targets (`make help`); host cargo/npm are refused.
- Never bypass hooks (`--no-verify`, `git -c`, hook config) and never weaken a gate, baseline or test to pass.
- Ship by committing on a feature branch (pre-commit runs `make pre-commit`) and running `make ship` (gates, adversarial review, push, PR against `dev`); `git push` and `gh pr create` are refused. GitHub auto-merges once the review passes (8/10) and CI is green; no session merges, approves or pushes `dev`/`main`.
- Subagents keep to their role: tester writes tests only, implementer writes code only, reviewer only reads.
- Stopping runs check-fast (static stop gates from gates/registry.json); fix red results before handing off.
- Report the exact revision, commands, results and limits.";

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn safe(session: &str) -> String {
    session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(64)
        .collect()
}

fn compaction_file(root: &Path, session: &str) -> std::path::PathBuf {
    root.join(RECORDS)
        .join(format!("compact-{}.md", safe(session)))
}

pub(crate) fn start(
    context: &Context,
    runtime: super::Runtime,
    session: &str,
    source: Option<&str>,
) -> serde_json::Value {
    let root = &context.root;
    let judge = std::env::var("AOE_AGENT_HOOK_JUDGE").unwrap_or_else(|_| "unlabelled".into());
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let head = git(root, &["rev-parse", "--short=12", "HEAD"]).unwrap_or_default();
    let base = stop::merge_base(root).map_or_else(
        || "origin/dev is missing: run `git fetch origin dev`".to_owned(),
        |b| format!("merge base with origin/dev {}", &b[..b.len().min(12)]),
    );
    let mut lines = vec![
        format!(
            "AoeWorld agent harness for {} (`aoe-harness agent-hook`, judge {judge}).",
            runtime.label()
        ),
        format!(
            "Checkout {} · branch {branch} · HEAD {head} · {base}.",
            root.display()
        ),
        format!("Acting role: {}.", context.role.name()),
    ];
    if crate::hooks::check(root).is_err() {
        lines.push("Git hooks are MISSING or altered: ask the person to run `make hooks-install` before any commit.".into());
    }
    if let Some(run) = stop::last_run(root) {
        let first = run.report.lines().next().unwrap_or_default().to_owned();
        lines.push(format!(
            "Last check-fast: {} — {first}",
            run.verdict.label()
        ));
    }
    if let Ok(blocked) = fs::read_to_string(root.join(BLOCKED)) {
        let excerpt: Vec<&str> = blocked.lines().take(20).collect();
        lines.push(format!("{BLOCKED} is present:\n{}", excerpt.join("\n")));
    }
    if source == Some("compact")
        && let Ok(saved) = fs::read_to_string(compaction_file(root, session))
    {
        lines.push(format!("Progress saved before compaction:\n{saved}"));
    }
    lines.push(RULES.into());
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": lines.join("\n"),
        }
    })
}

pub(crate) fn pre_compact(context: &Context, session: &str) -> Result<(), String> {
    let root = &context.root;
    let status = git(root, &["status", "--short", "--branch"]).unwrap_or_default();
    let status: Vec<&str> = status.lines().take(200).collect();
    let log = git(root, &["log", "-8", "--oneline"]).unwrap_or_default();
    let mut saved = format!(
        "git status:\n{}\n\nlast commits:\n{log}\n",
        status.join("\n")
    );
    if let Some(run) = stop::last_run(root).filter(|r| r.verdict != stop::Verdict::Pass) {
        saved.push_str(&format!("\nlast check-fast report:\n{}\n", run.report));
    }
    fs::create_dir_all(root.join(RECORDS)).map_err(|e| e.to_string())?;
    fs::write(compaction_file(root, session), saved).map_err(|e| e.to_string())
}
