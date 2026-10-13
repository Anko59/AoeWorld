//! DUEL_QUALIFY: the wider command the main session would otherwise run by hand after a green duel.
use super::{CONTRACT, Options, runner, tree};
use crate::{process::CaptureExit, ship::git};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Run the qualification command; when it fails, let the implementer fix it without touching a
/// test, up to `qualify_attempts` times, committing each fix as its own `qualification` commit.
pub(super) fn run(
    root: &Path,
    task: &str,
    options: &Options,
    (branch, base): (&str, &str),
    summary: &mut String,
) -> git::Result<()> {
    let Some(command) = options.qualify.as_deref() else {
        return Ok(());
    };
    summary.push_str("\n## Qualification\n");
    let default_check = runner::default_check(base);
    let check = options.check.as_deref().unwrap_or(&default_check);
    let mut head = git::git(root, &["rev-parse", "HEAD"])?;
    for attempt in 0..=options.qualify_attempts {
        let started = Instant::now();
        let captured = runner::shell(root, command, options.qualify_budget);
        let took = human(started.elapsed());
        match captured.exit {
            CaptureExit::Success => {
                summary.push_str(&format!("qualify: PASS ({command}, {took})\n"));
                return Ok(());
            }
            CaptureExit::Failed(Some(_)) => {}
            ref other => {
                return Err(format!(
                    "qualification could not complete: {other:?} ({command}, {took})\n{}",
                    runner::tail(&captured)
                ));
            }
        }
        let excerpt = runner::tail(&captured);
        summary.push_str(&format!("qualify: FAIL ({command}, {took})\n{excerpt}\n"));
        if attempt == options.qualify_attempts {
            return Err(format!("qualification failed: {excerpt}"));
        }
        let number = attempt + 1;
        let prompt = format!(
            "Task:\n{task}\n\nImplementer: the tests are green but the qualification command `{command}` fails. Make it pass WITHOUT touching any test: test files and inline test code must stay byte-identical to commit {head}. If the failure is in a test you may not change, or cannot be fixed without changing one, change nothing and report `DUEL_QUALIFY_BLOCKED: <reason>` on its own line. The driver reruns the qualification command; do not run it yourself unless it is quick.\n{CONTRACT}\nFocused check: `{check}`. Run this exact command after the change.\nQualification failure (fix {number} of {}):\n{excerpt}",
            options.qualify_attempts
        );
        let (exit, report) = runner::agent(root, options, "implementer", &prompt)?;
        summary.push_str(&format!(
            "\nImplementer last report (qualification fix {number}):\n{report}\n"
        ));
        tree::validate(root, false, &head, branch)?;
        if let Some(reason) = report
            .lines()
            .find_map(|line| line.trim().strip_prefix("DUEL_QUALIFY_BLOCKED:"))
        {
            return Err(format!(
                "qualification blocked: {}; earlier commits stay, working tree kept\n{excerpt}",
                reason.trim()
            ));
        }
        if !matches!(exit, CaptureExit::Success) {
            return Err(format!(
                "implementer did not finish the qualification fix: {exit:?}; earlier commits stay, working tree kept"
            ));
        }
        let checked = runner::check(root, options, check, options.check.is_none());
        tree::validate(root, false, &head, branch)?;
        let (report, tail) = checked?;
        summary.push_str(&format!(
            "Check: `{check}`; {}; tests: {}\n",
            if report.passed { "PASS" } else { "FAIL" },
            runner::names(&report)
        ));
        if !report.passed {
            return Err(format!(
                "the qualification fix broke the focused check; earlier commits stay, working tree kept\n{tail}"
            ));
        }
        if tree::changed(root)?.is_empty() {
            summary.push_str("The implementer changed nothing; qualification rerun anyway.\n");
            continue;
        }
        head = tree::commit(
            root,
            task,
            "qualification",
            options.runtime.label(),
            options.model.as_deref(),
        )?;
        summary.push_str(&format!("Qualification commit: {head}\n"));
    }
    unreachable!("the last attempt returns above")
}

fn human(took: Duration) -> String {
    let seconds = took.as_secs();
    if seconds >= 60 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}
