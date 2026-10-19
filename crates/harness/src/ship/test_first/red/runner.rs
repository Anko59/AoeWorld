//! Execute the current judge inside the existing Docker image; capture its own
//! result pipe. Never import check.json, scope.json or agent-provided evidence.
use super::git;
use crate::{
    process::{Cancellation, CaptureExit, capture_command},
    ship::duel::{
        focus::{self, CheckReport, Scope},
        wasm,
    },
};
use std::{fs, path::Path, process::Command, time::Duration};

const PREFIX: &str = "RED_RUN_RESULT ";

#[cfg(test)]
#[path = "../tests/red_runner.rs"]
mod tests;

struct Containers([String; 2]);

impl Drop for Containers {
    fn drop(&mut self) {
        // The outer deadline may kill the runner before E2E's normal RAII cleanup.
        for name in &self.0 {
            let mut cleanup = Command::new("docker");
            cleanup.args(["rm", "-f", name]);
            let _ = capture_command(cleanup, Duration::from_secs(15), &Cancellation::default());
        }
    }
}

#[derive(Debug, clap::Args)]
pub(crate) struct Options {
    #[arg(long)]
    source: std::path::PathBuf,
    #[arg(long)]
    snapshot: std::path::PathBuf,
    paths: Vec<String>,
}

pub(crate) fn execute(options: &Options) -> git::Result<()> {
    if !crate::agents::main_session() {
        return Err("red-run belongs to the main session only".into());
    }
    let explicit_specs = options.paths.iter().any(|p| {
        p.starts_with("browser/tests/") && (p.ends_with(".spec.ts") || p.ends_with(".test.ts"))
    });
    let paths: Vec<_> = options
        .paths
        .iter()
        .filter(|p| {
            !explicit_specs
                || !p.starts_with("browser/tests/")
                || p.ends_with(".spec.ts")
                || p.ends_with(".test.ts")
        })
        .cloned()
        .collect();
    let scope = Scope::from_paths(&options.snapshot, &paths)?;
    // Each export needs fresh Cargo fingerprints and browser dependencies.
    // Reusing a target across archived trees can run the previous tree's tests:
    // Git archive timestamps need not be newer than Cargo's cached artifacts.
    let cache = options.snapshot.join(".cache/duel");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let retention = options.source.join(".cache/agent-hook/red/output");
    // All captured runner output stays under the already hook-protected path.
    let report = focus::run_retained(
        &options.snapshot,
        &scope,
        &cache,
        &retention,
        &wasm::run_suites,
    )?;
    println!(
        "{PREFIX}{}",
        serde_json::to_string(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(super) fn run(root: &Path, snapshot: &Path, paths: &[String]) -> git::Result<CheckReport> {
    let binary = std::env::current_exe().map_err(|e| e.to_string())?;
    let binary = binary.canonicalize().map_err(|e| e.to_string())?;
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let socket = "/var/run/docker.sock";
    use std::os::unix::fs::MetadataExt;
    let group = fs::metadata(socket)
        .map_err(|e| e.to_string())?
        .gid()
        .to_string();
    let unique = tempfile::Builder::new()
        .prefix("container-")
        .tempdir_in(root.join(".cache/agent-hook/red"))
        .map_err(|e| e.to_string())?;
    let token = unique
        .path()
        .file_name()
        .ok_or("red run has no name")?
        .to_string_lossy();
    let outer = format!("aoe-red-{}-{token}", std::process::id());
    let browser = format!("{outer}-browser");
    let _cleanup = Containers([browser.clone(), outer.clone()]);
    let mut command = Command::new("docker");
    command.args([
        "run",
        "--rm",
        "--init",
        "--network",
        "host",
        "--user",
        &user,
        "--group-add",
        &group,
    ]);
    command.args(crate::slots::cap_args());
    command.args([
        "--name",
        &outer,
        "-e",
        &format!("AOE_RED_BROWSER_CONTAINER={browser}"),
    ]);
    command
        .arg("-v")
        .arg(format!("{}:{}", root.display(), root.display()));
    mount_git(&mut command, root)?;
    command.args(["-v", "/var/run/docker.sock:/var/run/docker.sock"]);
    command
        .arg("-v")
        .arg(format!("{}:/aoe-red-judge:ro", binary.display()));
    mount_cargo(&mut command, root)?;
    command.args(["-e", "CARGO_BUILD_JOBS=2"]);
    let temporary = unique.path().join("tmp");
    fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
    command
        .arg("-e")
        .arg(format!("TMPDIR={}", temporary.display()));
    for (key, value) in crate::slots::guard::child_env() {
        command.arg("-e").arg(format!("{key}={value}"));
    }
    if let Ok(role) = std::env::var("AOE_AGENT_ROLE") {
        command.arg("-e").arg(format!("AOE_AGENT_ROLE={role}"));
    }
    command
        .arg("-w")
        .arg(root)
        .arg("aoeworld/orchestrator:1.93.1")
        .args(["/aoe-red-judge", "red-run", "--source"])
        .arg(root)
        .arg("--snapshot")
        .arg(snapshot)
        .args(paths);
    // This includes builds and Playwright. A deadline/cancellation never counts red.
    let cancellation = Cancellation::default();
    let _signals =
        crate::gates::runner::signals::Signals::new(&cancellation).map_err(|e| e.to_string())?;
    let captured = capture_command(command, Duration::from_secs(1800), &cancellation);
    if !matches!(captured.exit, CaptureExit::Success) {
        return Err(format!(
            "red runner unavailable: {:?}\n{}\n{}",
            captured.exit,
            String::from_utf8_lossy(&captured.stdout),
            String::from_utf8_lossy(&captured.stderr)
        ));
    }
    let output = String::from_utf8_lossy(&captured.stdout);
    let json = output
        .lines()
        .last()
        .and_then(|line| line.strip_prefix(PREFIX))
        .ok_or("red runner returned no complete result")?;
    serde_json::from_str(json).map_err(|e| format!("red runner result: {e}"))
}

fn mount_git(command: &mut Command, root: &Path) -> git::Result<()> {
    let common = std::path::PathBuf::from(git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    if !common.starts_with(root) {
        command
            .arg("-v")
            .arg(format!("{}:{}", common.display(), common.display()));
    }
    Ok(())
}

fn mount_cargo(command: &mut Command, root: &Path) -> git::Result<()> {
    let home = crate::process::cargo_cache::home(root).map_err(|e| e.to_string())?;
    command.arg("-v").arg(format!("{0}:{0}", home.display()));
    command
        .arg("-e")
        .arg(format!("CARGO_HOME={}", home.display()));
    Ok(())
}
