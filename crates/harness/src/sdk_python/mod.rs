//! `aoe-harness sdk-python build|test`: the Python bindings of the AI SDK.
//!
//! The bindings (`crates/sdk-python`) sit outside the Cargo workspace because
//! they need a Python toolchain; `make sdk-python*` runs this inside the
//! pinned `aoeworld/sdk-python` image. Make only starts the container: every
//! step, path and check lives here.
use crate::process;
use std::{
    error::Error,
    path::{Path, PathBuf},
    time::Duration,
};

const MANIFEST: &str = "crates/sdk-python/Cargo.toml";
/// Test directories with the project file that configures pytest for each:
/// the bindings themselves, then the example AI that is built on them.
const TESTS: [(&str, &str); 2] = [
    (
        "crates/sdk-python/tests",
        "crates/sdk-python/pyproject.toml",
    ),
    ("ai/tests", "ai/pyproject.toml"),
];
const AI_SOURCE: &str = "ai";
const BINDINGS_TARGET: &str = "target/sdk-python";
const SITE: &str = "target/sdk-python/site";
const RELEASE_WHEELS: &str = "target/wheels";
const TEST_WHEELS: &str = "target/sdk-python/wheels";
const SERVER: &str = "target/debug/aoe-server";
const STEP_DEADLINE: Duration = Duration::from_secs(60 * 40);

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::Subcommand)]
pub(crate) enum Action {
    /// Build the release wheel into `target/wheels`.
    Build,
    /// Build a debug wheel and the server, then run pytest against a local server.
    Test,
}

pub(crate) fn run(action: Action) -> Result<(), Box<dyn Error>> {
    let root = std::env::current_dir()?;
    match action {
        Action::Build => {
            let wheel = build_wheel(&root, true, RELEASE_WHEELS)?;
            println!("built {}", wheel.display());
        }
        Action::Test => test(&root)?,
    }
    Ok(())
}

fn build_wheel(root: &Path, release: bool, out: &str) -> Result<PathBuf, Box<dyn Error>> {
    let target = root.join(BINDINGS_TARGET);
    let mut args = vec![
        "build",
        "--locked",
        "--manifest-path",
        MANIFEST,
        "--out",
        out,
    ];
    if release {
        args.push("--release");
    }
    process::run_with_env(
        "maturin",
        &args,
        &[("CARGO_TARGET_DIR", &target.to_string_lossy())],
        STEP_DEADLINE,
    )?;
    newest_wheel(&root.join(out))
}

fn test(root: &Path) -> Result<(), Box<dyn Error>> {
    process::run(
        "cargo",
        &["build", "--locked", "-p", "aoe-server"],
        STEP_DEADLINE,
    )?;
    let wheel = build_wheel(root, false, TEST_WHEELS)?;
    let site = root.join(SITE);
    // A fresh install directory: a stale native module must never be tested.
    if site.exists() {
        std::fs::remove_dir_all(&site)?;
    }
    process::run(
        "python3",
        &[
            "-m",
            "pip",
            "install",
            "--no-deps",
            "--no-cache-dir",
            "--target",
            &site.to_string_lossy(),
            &wheel.to_string_lossy(),
        ],
        STEP_DEADLINE,
    )?;
    let server = root.join(SERVER);
    // The wheel under test plus the example AI's sources, never an installed copy.
    let python_path = format!("{}:{}", site.display(), root.join(AI_SOURCE).display());
    for (tests, project) in TESTS {
        let rootdir = Path::new(project)
            .parent()
            .map_or(".".into(), |p| p.to_string_lossy());
        process::run_with_env(
            "python3",
            &[
                "-m",
                "pytest",
                "-p",
                "no:cacheprovider",
                "--rootdir",
                &rootdir,
                "-c",
                project,
                tests,
            ],
            &[
                ("PYTHONPATH", &python_path),
                ("AOE_SERVER_BIN", &server.to_string_lossy()),
                ("PYTHONDONTWRITEBYTECODE", "1"),
            ],
            STEP_DEADLINE,
        )?;
    }
    Ok(())
}

/// The most recently written `aoeworld-*.whl` in `directory`.
pub(crate) fn newest_wheel(directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut wheels = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("aoeworld-") && name.ends_with(".whl") {
            wheels.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    wheels
        .into_iter()
        .max()
        .map(|(_, path)| path)
        .ok_or_else(|| format!("no aoeworld wheel in {}", directory.display()).into())
}

#[path = "tests.rs"]
#[cfg(test)]
mod tests;
