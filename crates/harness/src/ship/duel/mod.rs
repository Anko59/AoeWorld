//! Main-session driver: a tester's red tests, then an implementer's green code.
pub(in crate::ship) mod authored;
pub(in crate::ship) mod baseline;
mod browser;
#[cfg(test)]
#[path = "tests/closing.rs"]
mod closing_tests;
mod config;
#[cfg(test)]
#[path = "tests/fixes.rs"]
mod fixes_tests;
pub(crate) mod focus;
#[cfg(test)]
#[path = "tests/focus.rs"]
mod focus_tests;
pub(in crate::ship) mod green;
pub(crate) mod output;
#[cfg(test)]
#[path = "tests/panics.rs"]
mod panics_tests;
mod qualify;
pub(in crate::ship) mod runner;
mod scope;
pub(in crate::ship) mod specs;
pub(crate) mod stages;
#[cfg(test)]
mod tests;
mod tree;
pub(in crate::ship) mod wasm;
#[cfg(test)]
#[path = "tests/wasm.rs"]
mod wasm_tests;

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
    let _lock = super::lock::acquire(root, "make duel").map_err(|e| e.to_string())?;
    run(root, &task, &options).map(|_| ())
}

pub(crate) fn run(root: &Path, task: &str, options: &Options) -> git::Result<String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    // Only a round this driver kept after an unavailable check, unchanged since, is
    // resumed. Any other change still refuses the start.
    let resume = tree::kept::matches(&root, &git::branch(&root)?)?;
    let subject = if resume {
        git::subject_with_changes(&root)?
    } else {
        git::subject(&root)?
    };
    browser::reset(&root)?;
    let cache = root.join(".cache/duel");
    clean(output::prune(&cache, 0));
    let mut summary = format!(
        "# Duel: {}\n\nBranch: {}\nRuntime: {} / {}\n",
        task.lines().next().unwrap_or("Task"),
        subject.branch,
        options.runtime.label(),
        options.model.as_deref().unwrap_or("configured default")
    );
    summary.push_str(&format!("\nTask:\n{task}\n"));
    let mut kept = false;
    let base = git::git(&root, &["rev-parse", "HEAD"])?;
    let result = rounds(
        &root,
        task,
        options,
        &subject.branch,
        (resume, &mut kept),
        &mut summary,
    )
    .and_then(|()| qualify::run(&root, task, options, (&subject.branch, &base), &mut summary));
    let record = if kept {
        tree::kept::write(&root, &subject.branch)
    } else {
        tree::kept::remove(&root, &subject.branch)
    };
    if let Err(error) = record {
        eprintln!("duel: cannot update the kept-round record: {error}");
    }
    // Keep only the last run's logs for the summary, and drop the comparison build.
    clean(output::prune(&cache, 1));
    clean(
        fs::remove_dir_all(cache.join("baseline-target")).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        }),
    );
    match &result {
        Ok(()) if options.qualify.is_some() => summary.push_str("\nResult: PASS, qualified. Main session: read the diff (git diff origin/dev...HEAD), film the change with make showcase, then make ship.\n"),
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

/// Drop a tester's rejected tests, unless they are a resumed round's: those stay on disk.
fn discard(root: &Path, head: &str, resumed: bool) -> git::Result<&'static str> {
    if resumed {
        return Ok("kept");
    }
    tree::restore(root, head)?;
    Ok("restored")
}

fn clean(result: std::io::Result<()>) {
    if let Err(error) = result {
        eprintln!("duel: cannot clean .cache/duel: {error}");
    }
}

pub(in crate::ship) fn implementer_prompt(
    task: &str,
    command: &str,
    tests: &str,
    tester: &str,
    attempt: usize,
    failing: &str,
) -> String {
    format!(
        "Task:\n{task}\n\nImplementer: smallest production change, never a test. Tests must stay byte-identical to commit {tests}. If the tests cannot compile for a reason other than missing production code (for example a syntax error in a test), stop and report `DUEL_TEST_BUILD_ERROR: <reason>` on its own line. The driver ends the duel with that message instead of retrying.\n{CONTRACT}\nFocused check: `{command}`. Run this exact command after writing the implementation.\nTester report:\n{tester}\nFailing check `{command}` (attempt {attempt}):\n{failing}"
    )
}

/// Why a round-1 red that only panicked (or aborted) outside this round's files cannot be judged:
/// the tests say nothing, so the duel stops and keeps them. An attack round drops the attack
/// like any other unrelated red result instead, and the earlier commits stand.
pub(in crate::ship) fn panic_only_refusal(
    round: u32,
    red: &focus::CheckReport,
    authored_red: bool,
) -> Option<String> {
    (round == 1 && !red.passed && !authored_red && !red.build_failed && red.failed.is_empty()).then(
        || {
            red.panics.first().map_or_else(
                || "the run failed without a named test or a panic".to_owned(),
                |p| {
                    format!(
                        "the run panicked at {} which this round did not change",
                        p.location
                    )
                },
            )
        },
    )
}

fn rounds(
    root: &Path,
    task: &str,
    options: &Options,
    branch: &str,
    (resume, kept): (bool, &mut bool),
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
        // A resumed round's tests were not written by this round's agent: never discard them.
        let resumed = resume && round == 1;
        let (exit, tester) = if resumed {
            (
                CaptureExit::Success,
                "Resumed from the tester's uncommitted test changes; the tester was skipped."
                    .to_owned(),
            )
        } else {
            runner::agent(root, options, "tester", &prompt)?
        };
        summary.push_str(&format!("\nTester last report:\n{tester}\n"));
        let paths = tree::validate_keeping(root, true, &head, branch, resumed)?;
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
            let outcome = discard(root, &head, resumed)?;
            return Err(format!("{error} ({outcome})"));
        }
        let red = runner::check(root, options, command, options.check.is_none());
        tree::validate_keeping(root, true, &head, branch, resumed)?;
        let (red_report, tail) = match red {
            Ok(result) => result,
            Err(error) => {
                // The check could not run, which says nothing about the tests: keep them.
                *kept = true;
                return Err(format!(
                    "{error}\nthe tester's test changes are kept, uncommitted, in the working tree; fix the check and rerun `make duel` to continue from them, or choose the check with DUEL_CHECK='<command>' (currently `{command}`)"
                ));
            }
        };
        summary.push_str(&format!(
            "Check: `{command}`; red: {}; tests: {}\n\n{tail}\n",
            if red_report.passed { "PASS" } else { "FAIL" },
            runner::names(&red_report)
        ));
        let authored_red = if options.check.is_some() || red_report.build_failed {
            true
        } else {
            let (authored, notes) = authored::evidence(root, &paths, &red_report)?;
            for note in notes {
                summary.push_str(&format!("{note}\n"));
            }
            authored
        };
        if let Some(what) = panic_only_refusal(round, &red_report, authored_red) {
            *kept = true;
            return Err(format!(
                "check could not complete: no tester-authored failing test: {what}\n{tail}\nthe tester's test changes are kept, uncommitted, in the working tree; fix the check and rerun `make duel` to continue from them, or choose the check with DUEL_CHECK='<command>' (currently `{command}`)"
            ));
        }
        if red_report.passed || !authored_red {
            let outcome = discard(root, &head, resumed)?;
            let unrelated = if red_report.passed {
                String::new()
            } else {
                format!(
                    "; no tester-authored failing test; unrelated failures: {}",
                    runner::names(&red_report)
                )
            };
            if round == 1 {
                return Err(format!("tests already pass at red{unrelated} ({outcome})"));
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
            let prompt = implementer_prompt(task, command, &tests, &tester, attempt, &failing);
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
            let (mut checked, mut tail) = checked?;
            let tests_ran = if options.check.is_none() {
                match green::validate(root, &paths, Some(&head), &red_report, &checked) {
                    Ok(()) => true,
                    Err(error) => {
                        checked.passed = false;
                        tail.push_str(&format!("\n{error}"));
                        false
                    }
                }
            } else {
                true // Explicit DUEL_CHECK uses exit status only.
            };
            summary.push_str(&format!(
                "green: {} (attempt {attempt}); tests: {}\n{tail}\n",
                if checked.passed { "PASS" } else { "FAIL" },
                runner::names(&checked)
            ));
            if checked.passed {
                green = true;
                break;
            }
            if options.check.is_none()
                && tests_ran
                && !checked.build_failed
                && checked.unavailable.is_none()
                && !checked.failed.is_empty()
                && checked.failed.iter().all(|n| checked.untouched.contains(n))
            {
                let baseline_command = format!("{command} HARNESS_DUEL_REVISION={pre_duel}");
                let (baseline, baseline_tail) =
                    runner::check(root, options, &baseline_command, true)?;
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
            return Err(format!(
                "green still fails after 3 attempts; tests commit stays and working tree is left as it is\n{failing}"
            ));
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
