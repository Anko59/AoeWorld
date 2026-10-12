//! Main-session driver: a tester's red tests, then an implementer's green code.
mod authored;
mod baseline;
mod config;
#[cfg(test)]
#[path = "tests/fixes.rs"]
mod fixes_tests;
pub(crate) mod focus;
#[cfg(test)]
#[path = "tests/focus.rs"]
mod focus_tests;
mod runner;
mod scope;
mod specs;
pub(crate) mod stages;
#[cfg(test)]
mod tests;
mod tree;

use crate::{process::CaptureExit, ship::git};
pub(crate) use config::{Options, read_task};
use std::{fs, path::Path};

const CONTRACT: &str = "Read AGENTS.md, docs/agent-engineering.md and the scoped AGENTS.md. Use Dockerized Make targets only. Never touch protected classes, weaken existing tests, commit, push, change branches or run make duel. The duel driver commits each step. Run only the focused check named below. Do not run make preflight or the whole suite: the driver and make ship own the wider gates. End with a short report of paths, commands and results.";

pub(crate) fn execute(root: &Path) -> git::Result<()> {
    if !crate::agents::main_session() {
        return Err("make duel belongs to the main session only".into());
    }
    let get = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    let options = Options::from_env(get)?;
    let task = get("DUEL_TASK").ok_or("DUEL_TASK is required: name the task file")?;
    let task = read_task(&root.join(task))?;
    run(root, &task, &options).map(|_| ())
}

pub(crate) fn run(root: &Path, task: &str, options: &Options) -> git::Result<String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let subject = git::subject(&root)?;
    let mut summary = format!(
        "# Duel: {}\n\nBranch: {}\nRuntime: {} / {}\n",
        task.lines().next().unwrap_or("Task"),
        subject.branch,
        options.runtime.label(),
        options.model.as_deref().unwrap_or("configured default")
    );
    summary.push_str(&format!("\nTask:\n{task}\n"));
    let result = rounds(&root, task, options, &subject.branch, &mut summary);
    match &result {
        Ok(()) => summary.push_str("\nResult: PASS. Main session: read the diff, film the change with make showcase, then make ship.\n"),
        Err(error) => summary.push_str(&format!("\nResult: FAIL: {error}\nMain session: inspect git diff and the last commit, finish the failing implementation with the same tests, rerun the check, then commit before filming and make ship.\n")),
    }
    let path = root
        .join(".cache/duel")
        .join(format!("{}.md", subject.branch.replace(['/', '\\'], "_")));
    fs::create_dir_all(path.parent().ok_or("summary has no parent")?).map_err(|e| e.to_string())?;
    fs::write(&path, &summary).map_err(|e| e.to_string())?;
    println!("{summary}\nSummary: {}", path.display());
    result.map(|()| summary)
}

fn rounds(
    root: &Path,
    task: &str,
    options: &Options,
    branch: &str,
    summary: &mut String,
) -> git::Result<()> {
    let pre_duel = git::git(root, &["rev-parse", "HEAD"])?;
    for round in 1..=options.rounds {
        summary.push_str(&format!("\n## Round {round}\n"));
        let head = git::git(root, &["rev-parse", "HEAD"])?;
        let default_command = runner::default_check(&head);
        let command = options.check.as_deref().unwrap_or(&default_command);
        let history = runner::bounded(summary);
        let prompt = format!(
            "Task:\n{task}\n\n{} Write the smallest failing tests, test files only, no production code.\n{CONTRACT}\nFocused check: `{command}`. Run this exact command after writing the tests.\nHistory:\n{history}",
            if round == 1 {
                "Tester:"
            } else {
                "Attack the completed change with new failing tests."
            }
        );
        let (exit, tester) = runner::agent(root, options, "tester", &prompt)?;
        summary.push_str(&format!("\nTester last report:\n{tester}\n"));
        let paths = tree::validate(root, true, &head, branch)?;
        if !matches!(exit, CaptureExit::Success) {
            tree::restore(root, &head)?;
            return Err(format!("tester did not finish: {exit:?} (restored)"));
        }
        if paths.is_empty() {
            if round == 1 {
                return Err("the tester wrote no test".into());
            }
            summary.push_str("Attack wrote no new test; duel complete.\n");
            return Ok(());
        }
        if options.check.is_none()
            && let Err(error) = focus::Scope::from_paths(root, &paths)
        {
            tree::restore(root, &head)?;
            return Err(error);
        }
        let red = runner::check(root, options, command, options.check.is_none());
        tree::validate(root, true, &head, branch)?;
        let (red_report, tail) = match red {
            Ok(result) => result,
            Err(error) => {
                tree::restore(root, &head)?;
                return Err(error);
            }
        };
        summary.push_str(&format!(
            "Check: `{command}`; red: {}; tests: {}\n\n{tail}\n",
            if red_report.passed { "PASS" } else { "FAIL" },
            runner::names(&red_report)
        ));
        let authored_red =
            red_report.build_failed || authored::has_failure(root, &paths, &red_report)?;
        if red_report.passed || !authored_red {
            tree::restore(root, &head)?;
            let unrelated = if red_report.passed {
                String::new()
            } else {
                format!(
                    "; no tester-authored failing test; unrelated failures: {}",
                    runner::names(&red_report)
                )
            };
            if round == 1 {
                return Err(format!("tests already pass at red{unrelated} (restored)"));
            }
            summary.push_str(&format!(
                "attack tests already pass{unrelated}; attack dropped.\n"
            ));
            return Ok(());
        }
        let tests = tree::commit(
            root,
            task,
            "tests",
            options.runtime.label(),
            options.model.as_deref(),
        )?;
        summary.push_str(&format!("Tests commit: {tests}\n"));
        let mut failing = tail;
        let mut green = false;
        for attempt in 1..=3 {
            let prompt = format!(
                "Task:\n{task}\n\nImplementer: smallest production change, never a test. Tests must stay byte-identical to commit {tests}. If the tests cannot compile for a reason other than missing production code (for example a syntax error in a test), stop and report `DUEL_TEST_BUILD_ERROR: <reason>` on its own line. The driver ends the duel with that message instead of retrying.\n{CONTRACT}\nFocused check: `{command}`. Run this exact command after writing the implementation.\nTester report:\n{tester}\nFailing check `{command}` (attempt {attempt}):\n{failing}"
            );
            let (exit, report) = runner::agent(root, options, "implementer", &prompt)?;
            summary.push_str(&format!(
                "\nImplementer last report (attempt {attempt}):\n{report}\n"
            ));
            tree::validate(root, false, &tests, branch)?;
            if let Some(reason) = report
                .lines()
                .find_map(|line| line.trim().strip_prefix("DUEL_TEST_BUILD_ERROR:"))
            {
                return Err(format!(
                    "test build error: {}; tests commit stays, working tree kept",
                    reason.trim()
                ));
            }
            if !matches!(exit, CaptureExit::Success) {
                return Err(format!(
                    "implementer did not finish: {exit:?}; tests commit stays, working tree kept"
                ));
            }
            let checked = runner::check(root, options, command, options.check.is_none());
            tree::validate(root, false, &tests, branch)?;
            let (mut checked, tail) = checked?;
            if options.check.is_some()
                && !checked.build_failed
                && checked.untouched.is_empty()
                && !checked.failed.is_empty()
            {
                checked.untouched = baseline::override_untouched(root, &head, &checked.failed)?;
            }
            summary.push_str(&format!(
                "green: {} (attempt {attempt}); tests: {}\n{tail}\n",
                if checked.passed { "PASS" } else { "FAIL" },
                runner::names(&checked)
            ));
            if checked.passed {
                green = true;
                break;
            }
            if !checked.build_failed
                && checked.unavailable.is_none()
                && !checked.failed.is_empty()
                && checked.failed.iter().all(|n| checked.untouched.contains(n))
            {
                let baseline_command = format!("{command} HARNESS_DUEL_REVISION={pre_duel}");
                let (baseline, baseline_tail) = if options.check.is_none() {
                    runner::check(root, options, &baseline_command, true)?
                } else {
                    let temporary = baseline::export(root, &pre_duel)?;
                    runner::check(temporary.path(), options, command, false)?
                };
                summary.push_str(&format!(
                    "Pre-duel comparison: {}\n{baseline_tail}\n",
                    runner::names(&baseline)
                ));
                if focus::pre_existing(&checked, &baseline) {
                    summary.push_str(&format!(
                        "pre-existing failure: {}\n",
                        checked.failed.join(", ")
                    ));
                    green = true;
                    break;
                }
            }
            failing = tail;
        }
        if !green {
            return Err("green still fails after 3 attempts; tests commit stays and working tree is left as it is".into());
        }
        let implementation = tree::commit(
            root,
            task,
            "implementation",
            options.runtime.label(),
            options.model.as_deref(),
        )?;
        summary.push_str(&format!("Implementation commit: {implementation}\n"));
        git::subject(root)?;
    }
    Ok(())
}
