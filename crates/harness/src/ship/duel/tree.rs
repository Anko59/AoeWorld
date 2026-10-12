//! Check staged, unstaged and untracked paths, including both sides of renames.
use crate::{
    agents::{is_test_path, protected_path},
    ship::git,
};
use std::{collections::BTreeSet, path::Path, process::Command};

fn paths(root: &Path, args: &[&str]) -> git::Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    output
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8(p.to_vec()).map_err(|_| "duel requires UTF-8 file paths".into()))
        .collect()
}

pub(crate) fn changed(root: &Path) -> git::Result<Vec<String>> {
    let mut all: BTreeSet<_> = paths(
        root,
        &["diff", "HEAD", "--no-renames", "--name-only", "-z", "--"],
    )?
    .into_iter()
    .collect();
    all.extend(paths(
        root,
        &[
            "diff",
            "--cached",
            "HEAD",
            "--no-renames",
            "--name-only",
            "-z",
            "--",
        ],
    )?);
    all.extend(paths(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    Ok(all.into_iter().collect())
}

pub(crate) fn since(root: &Path, base: &str) -> git::Result<Vec<String>> {
    let mut all: BTreeSet<_> = paths(
        root,
        &["diff", base, "--no-renames", "--name-only", "-z", "--"],
    )?
    .into_iter()
    .collect();
    all.extend(paths(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    Ok(all.into_iter().collect())
}

pub(crate) fn validate(
    root: &Path,
    tester: bool,
    head: &str,
    branch: &str,
) -> git::Result<Vec<String>> {
    if git::git(root, &["rev-parse", "HEAD"])? != head || git::branch(root)? != branch {
        return Err("agent moved HEAD or the branch; stop and inspect Git state".into());
    }
    let paths = match changed(root) {
        Ok(paths) => paths,
        Err(error) => {
            restore(root, head)?;
            return Err(format!("{error} (restored)"));
        }
    };
    let bad: Vec<_> = paths
        .iter()
        .filter(|p| {
            let lower = p.to_ascii_lowercase();
            let linked = root.join(p).canonicalize().ok().is_some_and(|target| {
                target.strip_prefix(root).map_or(true, |relative| {
                    protected_path(&relative.to_string_lossy().to_ascii_lowercase()).is_some()
                        || is_test_path(&relative.to_string_lossy().to_ascii_lowercase()) != tester
                })
            });
            protected_path(&lower).is_some() || linked || is_test_path(&lower) != tester
        })
        .cloned()
        .collect();
    if !bad.is_empty() {
        restore(root, head)?;
        return Err(format!(
            "{} changed forbidden paths (restored): {}",
            if tester { "tester" } else { "implementer" },
            bad.join(", ")
        ));
    }
    Ok(paths)
}

pub(crate) fn restore(root: &Path, head: &str) -> git::Result<()> {
    git::git(root, &["reset", "--hard", head])?;
    git::git(root, &["clean", "-fd"])?;
    Ok(())
}

pub(crate) fn commit(
    root: &Path,
    task: &str,
    phase: &str,
    runtime: &str,
    model: Option<&str>,
) -> git::Result<String> {
    let paths = changed(root)?;
    if paths.is_empty() {
        return Err(format!("{phase}: no change to commit"));
    }
    let mut args = vec!["add", "--all", "--"];
    args.extend(paths.iter().map(String::as_str));
    git::git(root, &args)?;
    let message = message(task, phase, runtime, model);
    git::git(root, &["commit", "--cleanup=verbatim", "-m", &message])?;
    git::git(root, &["rev-parse", "HEAD"])
}

pub(crate) fn message(task: &str, phase: &str, runtime: &str, model: Option<&str>) -> String {
    let title = task.lines().next().unwrap_or("Task").trim();
    let title = if phase == "tests" {
        format!("Tests: {title}")
    } else {
        title.to_owned()
    };
    let subject = if title.chars().count() <= 72 {
        title
    } else {
        let prefix: String = title.chars().take(71).collect();
        let end = prefix.rfind(char::is_whitespace).unwrap_or(0);
        let prefix = prefix[..end].trim_end();
        format!("{prefix}{}…", if prefix == "Tests:" { " " } else { "" })
    };
    format!(
        "{subject}\n\n{task}\n\nHarness-Phase: {phase}\nCo-Authored-By: {runtime} ({}) <noreply@openai.com>",
        model.unwrap_or("configured default")
    )
}
