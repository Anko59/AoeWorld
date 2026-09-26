//! Bounded parser fuzz campaigns with exact target and toolchain selection.
use crate::process;
use serde::Serialize;
use std::{
    error::Error,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
mod maintenance;
mod seeds;
mod storage;
const TARGETS: [&str; 7] = [
    "drs",
    "slp",
    "palette",
    "manifest",
    "map_package",
    "environment_page",
    "map_chunk",
];

#[derive(Clone, Copy)]
pub enum Mode {
    Smoke,
    Nightly,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Nightly => "nightly",
        }
    }

    fn limit(self) -> &'static str {
        match self {
            Self::Smoke => "-runs=512",
            Self::Nightly => "-max_total_time=300",
        }
    }

    fn deadline(self) -> Duration {
        match self {
            Self::Smoke => Duration::from_secs(600),
            Self::Nightly => Duration::from_secs(900),
        }
    }
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    mode: &'static str,
    toolchain: &'static str,
    cargo_fuzz: &'static str,
    targets: [&'static str; 7],
    limit: &'static str,
    prepared_seeds: Vec<seeds::Seed>,
    verified_legacy_seeds: Vec<seeds::Seed>,
    corpus_directory: &'static str,
    artifact_directory: &'static str,
    storage_policy: storage::Policy,
    storage_before: storage::Snapshot,
    storage_after: storage::Snapshot,
    maintenance_actions: Vec<maintenance::Action>,
    target_results: Vec<TargetResult>,
    attempted_targets: usize,
    successful_targets: usize,
    result: &'static str,
    failure: Option<String>,
}

#[derive(Serialize)]
struct TargetResult {
    target: &'static str,
    duration_millis: u128,
    completed: bool,
    storage_after: Option<storage::Snapshot>,
    failure: Option<String>,
}

struct Execution {
    storage_after: storage::Snapshot,
    attempted_targets: usize,
    successful_targets: usize,
    failure: Option<String>,
    target_results: Vec<TargetResult>,
}

fn execute<F>(
    mode: Mode,
    root: &Path,
    storage_policy: storage::Policy,
    storage_before: storage::Snapshot,
    mut command: F,
) -> Execution
where
    F: FnMut(&[&str], Duration) -> Result<()>,
{
    let mut execution = Execution {
        storage_after: storage_before,
        attempted_targets: 0,
        successful_targets: 0,
        failure: None,
        target_results: Vec::new(),
    };
    for target in TARGETS {
        execution.attempted_targets += 1;
        let started = Instant::now();
        let command_result = command(
            &[
                "+nightly-2026-09-01",
                "fuzz",
                "run",
                target,
                "--",
                mode.limit(),
                "-max_len=1048576",
                "-timeout=5",
            ],
            mode.deadline(),
        );
        let mut failures = Vec::new();
        let mut snapshot_after = None;
        match storage_policy.inspect(root) {
            Ok(snapshot) => {
                execution.storage_after = snapshot;
                snapshot_after = Some(snapshot);
                if let Err(error) = storage_policy.validate(snapshot) {
                    failures.push(format!("after target {target}: {error}"));
                }
            }
            Err(error) => failures.push(format!(
                "after target {target}: cannot snapshot storage: {error}"
            )),
        }
        if let Err(error) = command_result {
            failures.push(format!("target {target}: {error}"));
        }
        if failures.is_empty() {
            execution.successful_targets += 1;
        }
        execution.target_results.push(TargetResult {
            target,
            duration_millis: started.elapsed().as_millis(),
            completed: failures.is_empty(),
            storage_after: snapshot_after,
            failure: (!failures.is_empty()).then(|| failures.join("; ")),
        });
        if !failures.is_empty() {
            execution.failure = Some(failures.join("; "));
            break;
        }
    }
    execution
}

fn run_monitored(
    program: &str,
    args: &[&str],
    deadline: Duration,
    root: &Path,
    policy: storage::Policy,
) -> Result<()> {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    let cancellation = process::Cancellation::default();
    let stop = Arc::new(AtomicBool::new(false));
    let failure = Arc::new(Mutex::new(None::<String>));
    let watcher = {
        let stop = stop.clone();
        let failure = failure.clone();
        let cancellation = cancellation.clone();
        let root = root.to_owned();
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match policy.inspect(&root) {
                    Ok(snapshot) if storage::near_limit(policy, snapshot) => {
                        if let Ok(mut reason) = failure.lock() {
                            *reason = Some(format!(
                                "working storage reached the 95% quota guard: {snapshot:?}"
                            ));
                        }
                        cancellation.cancel();
                        break;
                    }
                    Err(error) => {
                        if let Ok(mut reason) = failure.lock() {
                            *reason = Some(format!(
                                "cannot inspect working storage during campaign: {error}"
                            ));
                        }
                        cancellation.cancel();
                        break;
                    }
                    Ok(_) => {}
                }
                thread::sleep(Duration::from_millis(100));
            }
        })
    };
    let result = process::run_cancellable(program, args, deadline, &cancellation);
    stop.store(true, Ordering::SeqCst);
    watcher
        .join()
        .map_err(|_| "fuzz storage watcher panicked")?;
    if let Some(message) = failure
        .lock()
        .map_err(|_| "fuzz quota lock poisoned")?
        .take()
    {
        return Err(message.into());
    }
    result.map_err(Into::into)
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("cannot read Git identity for fuzz report".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn write_report(root: &Path, report: Report) -> Result<()> {
    let directory = root.join("reports/fuzz");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join(format!("{}.json", report.mode)),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

pub fn run(mode: Mode) -> Result<()> {
    let root = std::env::current_dir()?
        .parent()
        .ok_or("fuzz command must run in fuzz directory")?
        .canonicalize()?;
    if !Path::new("Cargo.toml").is_file() || !root.join("fuzz/fuzz_targets").is_dir() {
        return Err("fuzz command must run in fuzz directory".into());
    }
    fs::create_dir_all(root.join("fuzz/artifacts"))?;
    let storage_policy = storage::Policy::default();
    let storage_before = storage_policy.inspect(&root)?;
    let seeds = seeds::prepare(&root)?;
    let maintenance = maintenance::run(&root, storage_policy, |args, deadline| {
        process::run("cargo", args, deadline).map_err(Into::into)
    });
    let (maintenance_actions, execution) = match maintenance {
        Ok(actions) => {
            let execution = execute(
                mode,
                &root,
                storage_policy,
                storage_before,
                |args, deadline| run_monitored("cargo", args, deadline, &root, storage_policy),
            );
            (actions, execution)
        }
        Err(error) => (
            Vec::new(),
            Execution {
                storage_after: storage_policy.inspect(&root)?,
                attempted_targets: 0,
                successful_targets: 0,
                failure: Some(format!("corpus maintenance: {error}")),
                target_results: Vec::new(),
            },
        ),
    };
    let result = if execution.failure.is_some() {
        "FAIL"
    } else {
        "PASS"
    };
    write_report(
        &root,
        Report {
            version: 5,
            revision: git(&["rev-parse", "HEAD"])?,
            dirty: !git(&["status", "--porcelain"])?.is_empty(),
            mode: mode.label(),
            toolchain: "nightly-2026-09-01",
            cargo_fuzz: "0.13.2",
            targets: TARGETS,
            limit: mode.limit(),
            prepared_seeds: seeds.prepared_seeds,
            verified_legacy_seeds: seeds.verified_legacy_seeds,
            corpus_directory: "fuzz/corpus",
            artifact_directory: "fuzz/artifacts",
            storage_policy,
            storage_before,
            storage_after: execution.storage_after,
            maintenance_actions,
            target_results: execution.target_results,
            attempted_targets: execution.attempted_targets,
            successful_targets: execution.successful_targets,
            result,
            failure: execution.failure.clone(),
        },
    )?;
    match execution.failure {
        Some(failure) => Err(failure.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
