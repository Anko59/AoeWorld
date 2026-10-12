//! Main-session driver: a tester's red tests, then an implementer's green code.
mod config;
mod runner;
#[cfg(test)]
mod tests;
mod tree;

use crate::{process::CaptureExit, ship::git};
pub(crate) use config::{Options, read_task};
use std::{fs, path::Path};

const CONTRACT: &str = "Read AGENTS.md, docs/agent-engineering.md and the scoped AGENTS.md. Use Dockerized Make targets only. Never touch protected classes, weaken existing tests, commit, push, change branches or run make duel. The duel driver commits each step. End with a short report of paths, commands and results.";

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
    let result = rounds(&root, task, options, &subject.branch, &mut summary);
    match &result {
        Ok(()) => summary.push_str("\nResult: PASS. Main session: read the diff, film the change with make showcase, then make ship.\n"),
        Err(error) => summary.push_str(&format!("\nResult: FAIL: {error}\nMain session: inspect git diff and the last commit, finish the failing implementation with the same tests, rerun the check, then commit before filming and make ship.\n")),
    }
    let path = root
        .join(".cache/duel")
        .join(format!("{}.md", subject.branch));
    fs::create_dir_all(path.parent().ok_or("summary has no parent")?).map_err(|e| e.to_string())?;
    fs::write(&path, &summary).map_err(|e| e.to_string())?;
    println!("{summary}");
    result.map(|()| summary)
}

fn rounds(
    root: &Path,
    task: &str,
    options: &Options,
    branch: &str,
    summary: &mut String,
) -> git::Result<()> {
    let title = task.lines().next().ok_or("empty task")?.trim();
    for round in 1..=options.rounds {
        summary.push_str(&format!("\n## Round {round}\n"));
        let head = git::git(root, &["rev-parse", "HEAD"])?;
        let history = runner::bounded(summary);
        let prompt = format!(
            "Task:\n{task}\n\n{} Write the smallest failing tests, test files only, no production code.\n{CONTRACT}\nHistory:\n{history}",
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
        let command = options
            .check
            .as_deref()
            .unwrap_or(runner::default_check(&paths));
        let red = runner::check(root, options, command);
        tree::validate(root, true, &head, branch)?;
        let (passed, tail) = match red {
            Ok(result) => result,
            Err(error) => {
                tree::restore(root, &head)?;
                return Err(error);
            }
        };
        summary.push_str(&format!(
            "Check: `{command}`; red: {}\n\n{tail}\n",
            if passed { "PASS" } else { "FAIL" }
        ));
        if passed {
            tree::restore(root, &head)?;
            if round == 1 {
                return Err("tests already pass at red (restored)".into());
            }
            summary.push_str("attack tests already pass; attack dropped.\n");
            return Ok(());
        }
        let tests = tree::commit(
            root,
            &format!("Tests: {title}"),
            "tests",
            options.runtime.label(),
            options.model.as_deref(),
        )?;
        summary.push_str(&format!("Tests commit: {tests}\n"));
        let mut failing = tail;
        let mut green = false;
        for attempt in 1..=3 {
            let prompt = format!(
                "Task:\n{task}\n\nImplementer: smallest production change, never a test. Tests must stay byte-identical to commit {tests}.\n{CONTRACT}\nTester report:\n{tester}\nFailing check `{command}` (attempt {attempt}):\n{failing}"
            );
            let (exit, report) = runner::agent(root, options, "implementer", &prompt)?;
            summary.push_str(&format!(
                "\nImplementer last report (attempt {attempt}):\n{report}\n"
            ));
            tree::validate(root, false, &tests, branch)?;
            if !matches!(exit, CaptureExit::Success) {
                return Err(format!(
                    "implementer did not finish: {exit:?}; tests commit stays, working tree kept"
                ));
            }
            let checked = runner::check(root, options, command);
            tree::validate(root, false, &tests, branch)?;
            let (passed, tail) = checked?;
            summary.push_str(&format!(
                "green: {} (attempt {attempt})\n{tail}\n",
                if passed { "PASS" } else { "FAIL" }
            ));
            if passed {
                green = true;
                break;
            }
            failing = tail;
        }
        if !green {
            return Err("green still fails after 3 attempts; tests commit stays and working tree is left as it is".into());
        }
        let implementation = tree::commit(
            root,
            title,
            "implementation",
            options.runtime.label(),
            options.model.as_deref(),
        )?;
        summary.push_str(&format!("Implementation commit: {implementation}\n"));
        git::subject(root)?;
    }
    Ok(())
}
