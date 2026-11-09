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

/// Clippy arguments when compilation is required. A tests-only index may
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

/// Use the existing conservative suite classifier on the captured index paths.
/// Instructions, unknown paths, mixed changes and non-Markdown data keep clippy.
pub(super) fn docs_only(
    kind: &Kind,
    paths: &[String],
    registry: &crate::gates::registry::Registry,
) -> bool {
    matches!(kind, Kind::Index)
        && !paths.is_empty()
        && paths.iter().all(|path| path.ends_with(".md"))
        && registry
            .classify(paths)
            .suites
            .iter()
            .all(|suite| matches!(suite.as_str(), "docs" | "static"))
}

pub(super) fn skip_compilers(snapshot: &Snapshot) -> Result<bool> {
    if snapshot.identity.kind != Kind::Index {
        return Ok(false);
    }
    snapshot.run_checked(|root| {
        let registry = crate::gates::registry::Registry::load(root)?;
        if !docs_only(&snapshot.identity.kind, &snapshot.paths, &registry) {
            return Ok(false);
        }
        let raw = git_at(
            &snapshot.source,
            Some(&snapshot.source_index),
            &[
                "diff",
                "--cached",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--raw",
                "-z",
                &snapshot.identity.source_head,
                "--",
            ],
            None,
        )?;
        let fields: Vec<_> = raw.split(|byte| *byte == 0).collect();
        if fields.last() != Some(&&b""[..]) || fields.len() != snapshot.paths.len() * 2 + 1 {
            return Ok(false);
        }
        let mut paths = Vec::new();
        for pair in fields[..fields.len() - 1].as_chunks::<2>().0 {
            let Ok(header) = std::str::from_utf8(pair[0]) else {
                return Ok(false);
            };
            let parts: Vec<_> = header.split_ascii_whitespace().collect();
            if parts.len() != 5
                || !matches!(
                    (parts[0], parts[1], parts[4]),
                    (":000000", "100644", "A") | (":100644", "100644", "M")
                )
            {
                return Ok(false);
            }
            let Ok(path) = std::str::from_utf8(pair[1]) else {
                return Ok(false);
            };
            paths.push(path.to_owned());
        }
        paths.sort();
        Ok(paths == snapshot.paths)
    })
}

pub(super) fn run(source: &Path, kind: Kind) -> Result<()> {
    let source = fs::canonicalize(source)?;
    let snapshot = phase("index/export", || Snapshot::prepare(&source, kind))?;
    let clippy = clippy_args(&snapshot.identity.kind, &snapshot.paths);
    let skip = phase("scope selection", || skip_compilers(&snapshot))?;
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
    checked("architecture", &|root| {
        crate::architecture::check_with_source(root, &source)
    })?;
    checked("docs", &crate::gates::docs_check)?;
    if skip {
        println!(
            "pre-commit: documentation-only additions and edits; compiler checks remain in CI"
        );
    } else {
        cargo("clippy native", &clippy, 600)?;
        // Tests-only indexes also leave red wasm test targets to the next commit.
        if clippy.contains(&"--all-targets") {
            cargo("wasm check", &crate::native::wasm_check_args(), 600)?;
        }
    }
    println!(
        "static input: {}",
        serde_json::to_string(&snapshot.identity)?
    );
    Ok(())
}
