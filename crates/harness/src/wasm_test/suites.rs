//! The wasm-bindgen-test suites: one table for `make test-wasm` and the duel's focused check.
use super::{CARGO_DEADLINE, Result, runner_environment, start_driver, unused_port};
use crate::process::{Cancellation, CaptureExit, capture_in};
use std::path::Path;

/// One `cargo test` invocation under the wasm runner.
pub(crate) struct Suite {
    pub(crate) package: &'static str,
    /// `--lib` or `--test <name>`.
    pub(crate) target: &'static [&'static str],
}

pub(crate) const SUITES: [Suite; 3] = [
    Suite {
        package: "aoe-client",
        target: &["--test", "browser"],
    },
    Suite {
        package: "aoe-client",
        target: &["--lib"],
    },
    Suite {
        package: "aoe-rendering",
        target: &["--lib"],
    },
];

impl Suite {
    pub(crate) fn cargo_arguments(&self) -> Vec<String> {
        ["test", "--locked", "--release", "-p", self.package]
            .into_iter()
            .chain(["--target", "wasm32-unknown-unknown"])
            .chain(self.target.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// The test binary: `lib`, or the integration target's name.
    pub(crate) fn binary(&self) -> &'static str {
        match self.target {
            ["--test", name] => name,
            _ => "lib",
        }
    }
}

/// Packages whose tests run under the wasm runner.
pub(crate) fn packages() -> Vec<&'static str> {
    let mut packages: Vec<_> = SUITES.iter().map(|suite| suite.package).collect();
    packages.dedup();
    packages
}

pub(super) fn focused_arguments(package: &str, filter: &str) -> Result<Vec<String>> {
    if filter.trim().is_empty() {
        return Err("AOE_WASM_TEST_FILTER must name a test or module".into());
    }
    let suite = SUITES
        .iter()
        .find(|suite| suite.package == package && suite.target == ["--lib"])
        .ok_or("AOE_WASM_TEST_PACKAGE must be aoe-client or aoe-rendering")?;
    let mut arguments = suite.cargo_arguments();
    arguments.extend(["--".into(), filter.into()]);
    Ok(arguments)
}

/// Developer-selected tests use the same driver, capabilities and deadline.
/// They do not write the full-suite PASS report.
pub(crate) fn run_selected() -> Result<()> {
    let package = std::env::var("AOE_WASM_TEST_PACKAGE")?;
    let filter = std::env::var("AOE_WASM_TEST_FILTER")?;
    let arguments = focused_arguments(&package, &filter)?;
    let root = std::env::current_dir()?.canonicalize()?;
    let port = unused_port()?;
    let _driver = start_driver(port)?;
    let remote = format!("http://127.0.0.1:{port}/");
    let capabilities = root.join("browser/webdriver.json");
    let capabilities = capabilities
        .to_str()
        .ok_or("non-UTF-8 WebDriver configuration path")?;
    let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    crate::process::run_with_env(
        "cargo",
        &arguments,
        &runner_environment(&remote, capabilities),
        CARGO_DEADLINE,
    )?;
    Ok(())
}

/// What one suite printed and how it ended.
pub(crate) struct SuiteRun {
    pub(crate) package: &'static str,
    pub(crate) binary: &'static str,
    pub(crate) exit: CaptureExit,
    pub(crate) output: String,
}

/// Run the suites of `packages` in `root`, capturing their output for the caller.
pub(crate) fn run_focused(
    root: &Path,
    packages: &[String],
    target_dir: Option<&Path>,
) -> Result<Vec<SuiteRun>> {
    let port = unused_port()?;
    let _driver = start_driver(port)?;
    let remote = format!("http://127.0.0.1:{port}/");
    let capabilities = root.join("browser/webdriver.json");
    let capabilities = capabilities
        .to_str()
        .ok_or("non-UTF-8 WebDriver configuration path")?;
    let mut environment = runner_environment(&remote, capabilities).to_vec();
    let target_dir = target_dir.map(|p| p.to_string_lossy().into_owned());
    if let Some(dir) = &target_dir {
        environment.push(("CARGO_TARGET_DIR", dir));
    }
    let mut runs = Vec::new();
    for suite in SUITES
        .iter()
        .filter(|s| packages.iter().any(|p| p == s.package))
    {
        let arguments = suite.cargo_arguments();
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let captured = capture_in(
            root,
            "cargo",
            &arguments,
            &environment,
            CARGO_DEADLINE,
            &Cancellation::default(),
        );
        runs.push(SuiteRun {
            package: suite.package,
            binary: suite.binary(),
            output: format!(
                "{}\n{}",
                String::from_utf8_lossy(&captured.stdout),
                String::from_utf8_lossy(&captured.stderr)
            ),
            exit: captured.exit,
        });
    }
    Ok(runs)
}
