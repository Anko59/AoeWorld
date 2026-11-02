use super::*;
use std::time::{Duration, Instant};

fn phase<T>(name: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let start = Instant::now();
    let result = operation();
    eprintln!(
        "pre-commit phase {name}: {:.3}s",
        start.elapsed().as_secs_f64()
    );
    result
}

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
    let snapshot = phase("index/export", || Snapshot::prepare(&source, kind))?;
    let home = crate::process::cargo_cache::home(&source)?;
    let home = home.to_str().ok_or("non-UTF-8 Cargo cache path")?;
    let target = source.join("target");
    let target = target.to_str().ok_or("non-UTF-8 Cargo target path")?;
    // Cache/outputs are outside the exported input; ignored evidence is not copied.
    let checked = |name: &str, operation: &dyn Fn(&Path) -> Result<()>| {
        let start = Instant::now();
        let mut elapsed = Duration::ZERO;
        let result = snapshot.run_checked(|root| {
            let inner = Instant::now();
            let result = operation(root);
            elapsed = inner.elapsed();
            eprintln!("pre-commit phase {name}: {:.3}s", elapsed.as_secs_f64());
            result
        });
        eprintln!(
            "pre-commit validation {name}: {:.3}s",
            start.elapsed().saturating_sub(elapsed).as_secs_f64()
        );
        result
    };
    let cargo = |name: &str, args: &[&str], budget: u64| -> Result<()> {
        checked(name, &|root| {
            crate::process::run_in(
                root,
                "cargo",
                args,
                &[("CARGO_TARGET_DIR", target), ("CARGO_HOME", home)],
                Duration::from_secs(budget),
            )?;
            Ok(())
        })
    };
    for args in crate::format_invocations(true) {
        cargo("fmt", &args, 120)?;
    }
    checked("structure", &|root| {
        crate::policy::structure(root).map_err(|error| error.into())
    })?;
    checked("architecture", &crate::architecture::check)?;
    checked("docs", &crate::gates::docs_check)?;
    cargo("clippy native", &clippy, 600)?;
    // A tests-only index skips test targets here too, so red wasm tests wait as well.
    if clippy.contains(&"--all-targets") {
        cargo("wasm check", &crate::native::wasm_check_args(), 600)?;
    }
    println!(
        "static input: {}",
        serde_json::to_string(&snapshot.identity)?
    );
    Ok(())
}
