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

mod benchmark;

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
/// The AI's own tests, all of which run against fakes, except these that need a
/// live server (`make sdk-python-test` runs them). Discovering the rest means a
/// new test file runs without anyone listing it: a hand-kept list once left
/// `test_bridges.py` unrun.
const LOGIC_ROOT: &str = "ai/tests";
const SERVER_TESTS: [&str; 3] = [
    "ai/tests/test_full_game.py",
    "ai/tests/test_match.py",
    "ai/tests/test_start_kit.py",
];

/// Every `test_*.py` under `ai/tests` except the server tests, sorted. A server
/// test that no longer exists is an error, so the exclusions cannot go stale.
pub(super) fn logic_tests(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    for path in SERVER_TESTS {
        if !root.join(path).is_file() {
            return Err(
                format!("{path} is excluded from the logic tests but does not exist").into(),
            );
        }
    }
    let mut found = Vec::new();
    let mut pending = vec![std::path::PathBuf::from(LOGIC_ROOT)];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&directory))? {
            let entry = entry?;
            let relative = directory.join(entry.file_name());
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir() {
                pending.push(relative);
            } else if name.starts_with("test_") && name.ends_with(".py") {
                let path = relative.to_string_lossy().into_owned();
                if !SERVER_TESTS.contains(&path.as_str()) {
                    found.push(path);
                }
            }
        }
    }
    found.sort();
    Ok(found)
}
const AI_SOURCE: &str = "ai";
const BINDINGS_TARGET: &str = "target/sdk-python";
const SITE: &str = "target/sdk-python/site";
const RELEASE_WHEELS: &str = "target/wheels";
const TEST_WHEELS: &str = "target/sdk-python/wheels";
const SERVER: &str = "target/debug/aoe-server";
const STEP_DEADLINE: Duration = Duration::from_secs(60 * 40);

#[derive(Clone, Debug, Eq, PartialEq, clap::Subcommand)]
pub(crate) enum Action {
    /// Build the release wheel into `target/wheels`.
    Build,
    /// Build a debug wheel and the server, then run pytest against a local server.
    Test {
        /// Exercise bindings against the local server without the long AI games.
        #[arg(long)]
        bindings_only: bool,
    },
    /// Run only the example AI's fake-world decision regressions.
    LogicTest,
    /// Play seeded lockstep AI matches against an arena server and print JSON (`make ai-arena`).
    Arena(ArenaArgs),
    /// Measure 30/60/120-minute development on a source-backed map.
    Benchmark(benchmark::BenchmarkArgs),
}

#[derive(Clone, Debug, Eq, PartialEq, clap::Args)]
pub(crate) struct ArenaArgs {
    /// Map width in tiles.
    #[arg(long, default_value_t = 300)]
    size: u32,
    /// AI players per match.
    #[arg(long, default_value_t = 2)]
    bots: u32,
    /// Game minutes per seed.
    #[arg(long, default_value_t = 30)]
    minutes: u32,
    /// Comma-separated seeds, one match each.
    #[arg(long, default_value = "1,2,3,4,5")]
    seeds: String,
}

pub(crate) fn run(action: Action) -> Result<(), Box<dyn Error>> {
    let root = std::env::current_dir()?;
    match action {
        Action::Build => {
            let wheel = build_wheel(&root, true, RELEASE_WHEELS)?;
            println!("built {}", wheel.display());
        }
        Action::Test { bindings_only } => test(&root, bindings_only)?,
        Action::LogicTest => logic_test(&root)?,
        Action::Arena(args) => arena(&root, &args)?,
        Action::Benchmark(args) => benchmark::run(&root, &args)?,
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

fn logic_test(root: &Path) -> Result<(), Box<dyn Error>> {
    let (_, python_path) = prepare(root)?;
    let mut args = vec![
        "-m",
        "pytest",
        "-p",
        "no:cacheprovider",
        "--rootdir",
        "ai",
        "-c",
        "ai/pyproject.toml",
    ];
    let tests = logic_tests(root)?;
    args.extend(tests.iter().map(String::as_str));
    process::run_with_env(
        "python3",
        &args,
        &[
            ("PYTHONPATH", &python_path),
            ("PYTHONDONTWRITEBYTECODE", "1"),
        ],
        STEP_DEADLINE,
    )?;
    Ok(())
}

fn test(root: &Path, bindings_only: bool) -> Result<(), Box<dyn Error>> {
    let (server, python_path) = prepare(root)?;
    for &(tests, project) in &TESTS[..if bindings_only { 1 } else { TESTS.len() }] {
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

/// The AI arena: one fresh server and match per seed, results as JSON on stdout.
fn arena(root: &Path, args: &ArenaArgs) -> Result<(), Box<dyn Error>> {
    if !args
        .seeds
        .split(',')
        .all(|seed| seed.parse::<u32>().is_ok())
    {
        return Err("--seeds takes comma-separated numbers".into());
    }
    let (server, python_path) = prepare(root)?;
    process::run_with_env(
        "python3",
        &[
            "-m",
            "aoe_ai",
            "arena",
            "--server-bin",
            &server.to_string_lossy(),
            "--bots",
            &args.bots.to_string(),
            "--minutes",
            &args.minutes.to_string(),
            "--tiles",
            &args.size.to_string(),
            "--seeds",
            &args.seeds,
        ],
        &[
            ("PYTHONPATH", &python_path),
            ("PYTHONDONTWRITEBYTECODE", "1"),
        ],
        STEP_DEADLINE,
    )?;
    Ok(())
}

/// Builds the server and a debug wheel, installs the wheel into a fresh
/// directory and returns the server path and the `PYTHONPATH` for the AI.
fn prepare(root: &Path) -> Result<(PathBuf, String), Box<dyn Error>> {
    process::run(
        "cargo",
        &["build", "--locked", "-p", "aoe-server"],
        STEP_DEADLINE,
    )?;
    let wheel = build_wheel(root, false, TEST_WHEELS)?;
    let site = root.join(SITE);
    install(&wheel, &site)?;
    let server = root.join(SERVER);
    // The wheel under test plus the example AI's sources, never an installed copy.
    let python_path = format!("{}:{}", site.display(), root.join(AI_SOURCE).display());
    Ok((server, python_path))
}

fn install(wheel: &Path, site: &Path) -> Result<(), Box<dyn Error>> {
    // A fresh install directory: a stale native module must never be tested.
    if site.exists() {
        std::fs::remove_dir_all(site)?;
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
