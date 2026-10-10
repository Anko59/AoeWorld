use super::*;
use std::time::Duration;

/// Clippy for a scope. A pre-commit index that stages only test files may
/// hold red tests written before the code they name (test-first), so its
/// test targets are left to the product commit; every other scope, and any
/// index touching a non-test file, checks every target.
pub(super) fn clippy_args(kind: &Kind, staged: &[String]) -> Vec<&'static str> {
    let tests_only = matches!(kind, Kind::Index)
        && !staged.is_empty()
        && staged.iter().all(|path| crate::agents::is_test_path(path));
    let mut args = vec!["clippy", "--workspace"];
    if tests_only {
        println!("pre-commit: tests-only index; test targets are checked by the next commit");
    } else {
        args.push("--all-targets");
    }
    args.extend(["--locked", "--", "-D", "warnings"]);
    args
}

fn staged_paths(source: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["diff", "--cached", "--name-only", "-z", "--no-renames"])
        .output()?;
    if !output.status.success() {
        return Err("cannot list staged paths".into());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
}

pub(super) fn run(source: &Path, kind: Kind) -> Result<()> {
    let source = fs::canonicalize(source)?;
    let staged = match kind {
        Kind::Index => staged_paths(&source)?,
        _ => Vec::new(),
    };
    let clippy = clippy_args(&kind, &staged);
    let snapshot = Snapshot::prepare(&source, kind)?;
    let target = source.join("target");
    let target = target.to_str().ok_or("non-UTF-8 Cargo target path")?;
    // Cache/outputs are outside the exported input; ignored evidence is not copied.
    let cargo = |args: &[&str], budget: u64| -> Result<()> {
        snapshot.run_checked(|root| {
            crate::process::run_in(
                root,
                "cargo",
                args,
                &[("CARGO_TARGET_DIR", target)],
                Duration::from_secs(budget),
            )?;
            Ok(())
        })
    };
    for args in crate::format_invocations(true) {
        cargo(&args, 120)?;
    }
    snapshot.run_checked(|root| crate::policy::structure(root).map_err(|error| error.into()))?;
    snapshot.run_checked(crate::architecture::check)?;
    snapshot.run_checked(crate::gates::docs_check)?;
    cargo(&clippy, 600)?;
    println!(
        "static input: {}",
        serde_json::to_string(&snapshot.identity)?
    );
    Ok(())
}
