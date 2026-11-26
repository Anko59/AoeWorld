//! `make test-e2e-repeat`: runs chosen specs many times on chosen projects and tallies passes
//! against failures per test. It is the proof that a flaky test is fixed, run by the
//! `e2e-repeat` workflow because a local run cannot be trusted on a busy machine.
//!
//! It never uses the quarantine and never retries: every attempt counts as it ends.
use super::{
    BrowserContainer, SERVER_BINARY, browser_run_deadline, command, filter, quarantine, servers,
    stderr_tail,
};
use crate::process::{self, CaptureExit};
use std::{path::Path, process::Command};

mod report;
#[cfg(test)]
mod tests;

const REPORTS: &str = "reports/e2e";
const DEFAULT_REPEAT: u32 = 30;
const MAX_REPEAT: u32 = 200;

type Failure = Box<dyn std::error::Error>;

#[derive(Debug, PartialEq)]
struct Options {
    /// Spec paths or globs under `browser/tests`, each with an optional `:line`.
    specs: Vec<String>,
    projects: Vec<String>,
    repeat: u32,
}

impl Options {
    fn parse(specs: &str, projects: &str, repeat: &str) -> Result<Self, String> {
        let specs = filter::parse_specs(specs);
        if specs.is_empty() {
            return Err("AOE_E2E_REPEAT_SPECS names no spec (tests/<path>.spec.ts[:line])".into());
        }
        let projects = filter::parse_specs(projects);
        if projects.is_empty() {
            return Err("AOE_E2E_REPEAT_PROJECTS names no Playwright project".into());
        }
        if let Some(bad) = projects.iter().find(|project| {
            !project
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }) {
            return Err(format!("AOE_E2E_REPEAT_PROJECTS: invalid project {bad:?}"));
        }
        let repeat = if repeat.is_empty() {
            DEFAULT_REPEAT
        } else {
            repeat
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| repeat.parse::<u32>().ok())
                .flatten()
                .filter(|count| (1..=MAX_REPEAT).contains(count))
                .ok_or_else(|| format!("AOE_E2E_REPEAT must be 1..={MAX_REPEAT}, got {repeat:?}"))?
        };
        Ok(Self {
            specs,
            projects,
            repeat,
        })
    }

    fn from_env() -> Result<Self, String> {
        let var = |name: &str| std::env::var(name).unwrap_or_default();
        Self::parse(
            &var("AOE_E2E_REPEAT_SPECS"),
            &var("AOE_E2E_REPEAT_PROJECTS"),
            &var("AOE_E2E_REPEAT"),
        )
    }
}

/// `tests/a.spec.ts:66` as the file part and the line.
fn split_line(spec: &str) -> (&str, Option<&str>) {
    match spec.rsplit_once(':') {
        Some((file, line)) if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => {
            (file, Some(line))
        }
        _ => (spec, None),
    }
}

/// The spec files the patterns select, each keeping its `:line`. A line needs one literal file.
fn resolve(candidates: &[String], patterns: &[String]) -> Result<Vec<String>, String> {
    let mut selected = Vec::new();
    for pattern in patterns {
        let (file, line) = split_line(pattern);
        let files = filter::resolve(candidates, &[file.to_owned()])?;
        match (line, files.as_slice()) {
            (None, _) => selected.extend(files),
            (Some(line), [file]) => selected.push(format!("{file}:{line}")),
            (Some(_), _) => {
                return Err(format!(
                    "AOE_E2E_REPEAT_SPECS {pattern:?}: a line needs one spec file, not a pattern"
                ));
            }
        }
    }
    Ok(selected)
}

/// The Playwright command line: the harness's own, with the JSON reporter alone (the
/// quarantine reporter keeps every step of every attempt, too much for 200 repeats), the
/// projects and the repeat count.
fn playwright_args(mut args: Vec<String>, options: &Options) -> Result<Vec<String>, String> {
    let reporter = args
        .iter_mut()
        .find(|arg| arg.as_str() == quarantine::REPORTER)
        .ok_or("missing JSON reporter")?;
    *reporter = "--reporter=json".to_owned();
    args.push(format!("--repeat-each={}", options.repeat));
    args.extend(options.projects.iter().map(|p| format!("--project={p}")));
    Ok(args)
}

pub(super) fn run() -> Result<(), Failure> {
    let options = Options::from_env()?;
    let root = std::env::current_dir()?.canonicalize()?;
    let specs = resolve(&filter::candidates(&root.join("browser"))?, &options.specs)?;
    std::fs::create_dir_all(REPORTS)?;
    let binary = root.join(SERVER_BINARY);
    if !binary.is_file() {
        return Err(format!("test server binary missing: {}", binary.display()).into());
    }
    let maps = match std::env::var_os("AOE_MAP_PACKAGE_DIRECTORY").filter(|v| !v.is_empty()) {
        Some(path) => std::path::PathBuf::from(path).canonicalize()?,
        None => root.join(aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY),
    };
    // One server, one worker: the attempts of a test follow each other as they do in a CI shard.
    let mut fleet = servers::Fleet::start(&root, &binary, &maps, 1)?;
    let name = command::container_name(std::process::id(), fleet.addresses[0].port(), None);
    let _cleanup = BrowserContainer(name.clone());
    let args = playwright_args(
        command::docker_args(&command::Run {
            root: &root,
            base_urls: servers::base_urls(&fleet.addresses),
            container: name,
            landscape_hash: &fleet.hash,
            specs: &specs,
            shard: None,
        })?,
        &options,
    )?;
    let outcome = attempt(&args, &options);
    // A server that died mid-run explains failures that no test caused.
    let alive = fleet.check_alive();
    fleet.shutdown()?;
    let summary = outcome?;
    alive?;
    verdict(&summary)
}

fn attempt(args: &[String], options: &Options) -> Result<report::Summary, Failure> {
    let path = Path::new(REPORTS).join("repeat-playwright.json");
    match std::fs::remove_file(&path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let mut retained = crate::ship::JsonOutput::create(&path, "Playwright")?;
    let mut docker = Command::new("docker");
    docker.args(args);
    let deadline = browser_run_deadline();
    let captured = process::capture_command_retaining_stdout(
        docker,
        deadline,
        &process::Cancellation::default(),
        &mut retained,
    );
    // Exit 1 is Playwright reporting failed attempts; the report says which.
    if !matches!(
        captured.exit,
        CaptureExit::Success | CaptureExit::Failed(Some(1))
    ) {
        return Err(format!(
            "e2e-repeat unavailable: {:?}\n{}",
            captured.exit,
            stderr_tail(&captured.stderr)
        )
        .into());
    }
    drop(retained);
    let revision = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned());
    let report = serde_json::from_slice(&std::fs::read(&path)?)?;
    let summary = report::summarize(&report, options.repeat, revision)?;
    std::fs::write(
        Path::new(REPORTS).join("repeat.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    std::fs::write(Path::new(REPORTS).join("repeat.md"), summary.markdown())?;
    Ok(summary)
}

/// Prints the tally; the run passes only when every attempt of every test passed.
fn verdict(summary: &report::Summary) -> Result<(), Failure> {
    println!("{}", summary.markdown());
    if let Some(reason) = summary.incomplete() {
        return Err(format!("e2e-repeat proves nothing: {reason}").into());
    }
    if summary.failed() > 0 {
        return Err(format!(
            "e2e-repeat: {} of {} attempts failed",
            summary.failed(),
            summary.passed() + summary.failed()
        )
        .into());
    }
    Ok(())
}
