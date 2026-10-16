//! Fixed native test policy with final failure output, not authenticated evidence.
use crate::process::{self, ProcessError};
use std::{path::Path, time::Duration};

pub(crate) mod affected;

const ARGS: &[&str] = &[
    "nextest",
    "run",
    "--workspace",
    "--locked",
    "--no-fail-fast",
    "--failure-output",
    "final",
    "--success-output",
    "never",
];
const BUDGET: Duration = Duration::from_secs(600);

/// Preserve the existing test set and deadline. Explicit reporter arguments
/// override inherited Nextest reporter settings, without increasing capture size.
/// Git environment clearing belongs to process::run_in; other credentials and
/// PATH are not isolated by this helper.
pub(crate) fn run_in(root: &Path, environment: &[(&str, &str)]) -> Result<(), ProcessError> {
    process::run_in(root, "cargo", ARGS, environment, BUDGET)
}

pub(crate) fn lint() -> Result<(), ProcessError> {
    let args = ["clippy", "--workspace", "--all-targets", "--locked"];
    process::run(
        "cargo",
        &[args.as_slice(), &["--", "-D", "warnings"]].concat(),
        BUDGET,
    )?;
    process::run("cargo", &wasm_check_args(), BUDGET)
}

/// Compile the test targets of every package the wasm runner executes, for
/// wasm32, so wasm-only test code that stops compiling fails the static checks.
/// It checks compilation only: clippy is not denied for wasm32 yet.
pub(crate) fn wasm_check_args() -> Vec<&'static str> {
    let mut args = vec![
        "check",
        "--locked",
        "--target",
        "wasm32-unknown-unknown",
        "--tests",
    ];
    for package in crate::wasm_test::packages() {
        args.extend(["-p", package]);
    }
    args
}

/// The unit tests of the workspace, or, when `select` names a merge base and
/// this run may select, of the crates affected by the changes since it. The
/// selection line is printed first and last, so the gate evidence keeps it.
pub(crate) fn test_unit(select: Option<String>) -> Result<(), ProcessError> {
    let selection = select.and_then(|base| match affected::plan(Path::new("."), &base) {
        Ok(selection) => Some(selection),
        Err(error) => {
            eprintln!("test-unit: selection failed ({error}); running the whole workspace");
            None
        }
    });
    let Some(selection) = selection else {
        return run_scope(None);
    };
    println!("{}", selection.line());
    let result = match selection.packages() {
        Some(packages) if packages.is_empty() => Ok(()),
        packages => run_scope(packages.as_deref()),
    };
    println!("{}", selection.line());
    result
}

fn run_scope(packages: Option<&[String]>) -> Result<(), ProcessError> {
    let scope: Vec<&str> = match packages {
        None => vec!["--workspace"],
        Some(packages) => packages.iter().flat_map(|p| ["-p", p.as_str()]).collect(),
    };
    if packages.is_none() {
        run_in(Path::new("."), &[])?;
    } else {
        let nextest: Vec<&str> = ARGS
            .iter()
            .flat_map(|arg| match *arg {
                "--workspace" => scope.clone(),
                other => vec![other],
            })
            .collect();
        process::run("cargo", &nextest, BUDGET)?;
    }
    let doc_scope: Vec<String> = match packages {
        None => vec!["--workspace".to_owned()],
        Some(names) => match affected::library_crates(Path::new("."), names) {
            Ok(libs) if libs.is_empty() => {
                println!("test-unit: no selected crate has a library target; no doctests run");
                return Ok(());
            }
            Ok(libs) => libs
                .into_iter()
                .flat_map(|p| ["-p".to_owned(), p])
                .collect(),
            Err(error) => {
                eprintln!(
                    "test-unit: doctest selection failed ({error}); doctests of the whole workspace"
                );
                vec!["--workspace".to_owned()]
            }
        },
    };
    let mut doc = vec!["test"];
    doc.extend(doc_scope.iter().map(String::as_str));
    doc.extend(["--doc", "--locked"]);
    process::run("cargo", &doc, BUDGET)
}

#[cfg(test)]
mod tests;
