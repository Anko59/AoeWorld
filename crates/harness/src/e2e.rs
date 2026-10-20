//! Disposable browser stack, with an isolated port and cleanup on every return path.
use crate::process;
mod command;
mod country;
mod discovery;
mod filter;
mod landscape;
mod matrix;
mod servers;
mod shard;
mod source;
pub(crate) mod stale;
mod visual;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Server(Child);

impl Server {
    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(self.0.id())?);
        nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("test server did not exit gracefully: {status}").into())
                };
            }
            if Instant::now() >= deadline {
                return Err("test server did not shut down within 5s".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct BrowserContainer(String);

/// Whole-suite bounds for one run of the suite; each test keeps its own
/// 30 s timeout, so a hang still fails fast. The suite outgrew two bounds: 282 s
/// of a former 300 s (2026-10-10), then 102 passing tests took 588 s of 600 s in
/// CI (2026-10-13) and the run was cut off with no failing test; on 2026-10-16
/// the suite took 19.5 min of 20 on the 4-core runner and a PR adding tests
/// (#434) was cut off again. The run now gets the gate registry's own budget for
/// this suite, 2400 s, in both modes; sharding the suite and one server per
/// Playwright worker are the speed fix (#388).
/// Coverage runs the same suite against an instrumented debug server.
fn browser_run_deadline(_coverage: Option<&str>) -> Duration {
    Duration::from_secs(2400)
}

impl Drop for BrowserContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn ready(address: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) else {
        return false;
    };
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let mut prefix = [0u8; 12];
    stream.read_exact(&mut prefix).is_ok() && &prefix == b"HTTP/1.1 200"
}

/// `source-country-probe` arguments: one verified external country package.
#[derive(clap::Args, Debug)]
pub struct CountryProbeArgs {
    #[arg(long)]
    package_directory: std::path::PathBuf,
    #[arg(long)]
    content_hash: String,
    #[arg(long)]
    browser: bool,
}

pub fn country_probe(args: &CountryProbeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let (directory, hash) = (&args.package_directory, args.content_hash.as_str());
    if args.browser {
        country::run(directory, hash)
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&aoe_server::run_source_country_probe(directory, hash)?)?
        );
        Ok(())
    }
}

pub fn run_source() -> Result<(), Box<dyn std::error::Error>> {
    source::run()
}

pub fn run_matrix() -> Result<(), Box<dyn std::error::Error>> {
    matrix::run()
}

pub fn run_visuals() -> Result<(), Box<dyn std::error::Error>> {
    visual::run()
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let coverage = std::env::var("AOE_E2E_COVERAGE").ok();
    let specs = filter::parse_specs(&std::env::var("AOE_E2E_SPECS").unwrap_or_default());
    filter::refuse_in_ci(
        &specs,
        std::env::var("GITHUB_ACTIONS").ok().as_deref(),
        std::env::var("CI").ok().as_deref(),
    )?;
    let shard = shard::Shard::from_env(coverage.as_deref())?;
    if specs.is_empty() {
        return run_specs(&[], None, shard);
    }
    if shard.is_some() {
        return Err("AOE_E2E_SPECS selects part of the suite; unset AOE_E2E_SHARD".into());
    }
    let root = std::env::current_dir()?.canonicalize()?;
    let selected = filter::resolve(&filter::candidates(&root.join("browser"))?, &specs)?;
    // The JSON reporter creates its file exclusively, so a stale report goes first.
    let _ = std::fs::remove_file(filter::FILTERED_REPORT);
    run_specs(
        &selected,
        Some(std::path::Path::new(filter::FILTERED_REPORT)),
        None,
    )?;
    if let Some(report) = filter::expected_report(&selected) {
        filter::check_report(&root, report)?;
    }
    Ok(())
}

pub(crate) fn run_focused(
    specs: &[String],
    report: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if specs.is_empty() {
        return Err("no focused browser specs".into());
    }
    run_specs(specs, Some(report), None)
}

fn run_specs(
    specs: &[String],
    report: Option<&std::path::Path>,
    shard: Option<shard::Shard>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.canonicalize()?;
    // The browser specs write their evidence here; the harness makes the directory first.
    std::fs::create_dir_all("reports/e2e")?;
    let coverage = std::env::var("AOE_E2E_COVERAGE").ok();
    let relative_binary = server_binary(coverage.as_deref())?;
    let run_deadline = browser_run_deadline(coverage.as_deref());
    let binary = root.join(relative_binary);
    if !binary.is_file() {
        return Err(format!("test server binary missing: {}", binary.display()).into());
    }
    let existing_maps =
        match std::env::var_os("AOE_MAP_PACKAGE_DIRECTORY").filter(|value| !value.is_empty()) {
            Some(path) => std::path::PathBuf::from(path).canonicalize()?,
            None => root.join(aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY),
        };
    let workers = servers::worker_count(coverage.as_deref(), !specs.is_empty())?;

    let mut server = servers::Fleet::start(&root, &binary, &existing_maps, workers)?;
    let fixture_hash = server.hash.clone();
    let address = server.addresses[0];
    let name = command::container_name(std::process::id(), address.port(), shard);
    let _cleanup = BrowserContainer(name.clone());
    let args = command::docker_args(&command::Run {
        root: &root,
        base_urls: servers::base_urls(&server.addresses),
        container: name,
        landscape_hash: &fixture_hash,
        specs,
        shard,
    })?;
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let cancellation = process::Cancellation::default();
    let browser_result = thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = mpsc::channel();
        scope.spawn(move || {
            let result = if specs.is_empty() {
                process::run_cancellable("docker", &refs, run_deadline, &worker_cancellation)
                    .map_err(|e| e.to_string())
            } else {
                // Stream reporter stdout into the same bounded retention used for nextest.
                // No unbounded Playwright report is written by the browser container.
                (|| {
                    let report = report.ok_or("focused Playwright report path missing")?;

                    let mut listing = Command::new("docker");
                    listing.args(&refs).arg("--list");
                    discovery::run(
                        listing,
                        &report.with_extension("discovery.json"),
                        Duration::from_secs(60),
                        &worker_cancellation,
                    )
                    .map_err(|e| e.to_string())?;

                    let mut retained = crate::ship::JsonOutput::create(report, "Playwright")
                        .map_err(|e| e.to_string())?;
                    let mut command = Command::new("docker");
                    command.args(&refs);
                    let captured = process::capture_command_retaining_stdout(
                        command,
                        run_deadline,
                        &worker_cancellation,
                        &mut retained,
                    );
                    match captured.exit {
                        process::CaptureExit::Success => Ok(()),
                        other => Err(format!(
                            "focused Playwright check unavailable: {other:?}\n{}",
                            stderr_tail(&captured.stderr)
                        )),
                    }
                })()
            };
            let _ = sender.send(result);
        });
        loop {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(result) => return result.map_err(Into::into),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("browser test worker disconnected".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Err(error) = server.check_alive() {
                        cancellation.cancel();
                        return Err(error);
                    }
                }
            }
        }
    });
    // Always stop the server; focused failure reports remain available to the driver.

    server.shutdown()?;
    browser_result.map_err(|error| stale::annotate(error, &existing_maps))?;
    if !specs.is_empty() {
        return Ok(());
    }
    if let Some(shard) = shard {
        // One shard runs a part of the suite: the release evidence below needs
        // the whole run, so the shard records its own passing result instead.
        return shard.write_pass(&fixture_hash);
    }
    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !revision.status.success() {
        return Err("cannot identify E2E source revision".into());
    }
    let asset_pack = std::env::var("AOE_ASSET_PACK")
        .ok()
        .filter(|value| !value.is_empty());
    let renderer_evidence = renderer_evidence(asset_pack.is_some())?;
    let report = serde_json::json!({
        "version": 1,
        "revision": String::from_utf8(revision.stdout)?.trim(),
        "server_binary": relative_binary,
        "asset_source": if asset_pack.is_some() { "local AoE II pack" } else { "generated CI fixtures" },
        "asset_pack": asset_pack.unwrap_or_default(),
        "renderer_evidence": renderer_evidence,
        "landscape_fixture": {
            "kind": "synthetic-prepared-flat-temperate",
            "content_hash": fixture_hash,
            "schema_version": aoe_map::MAP_SCHEMA_VERSION,
            "generation_recipe_version": aoe_map::GENERATION_RECIPE_VERSION,
            "real_source_qualification": false
        },
        "result": "PASS"
    });
    std::fs::create_dir_all("reports/e2e")?;
    std::fs::write("reports/e2e/pass.json", serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn renderer_evidence(
    private_assets: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let source = if private_assets {
        "local AoE II pack"
    } else {
        "generated CI fixtures"
    };
    let prefix = if private_assets {
        "local-assets/evidence/aoeworld-map-private-"
    } else {
        "reports/e2e/aoeworld-map-"
    };
    let mut evidence = Vec::new();
    for project in ["webgpu", "canvas", "browser-defaults"] {
        let path = format!("{prefix}{project}.json");
        let bytes = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| format!("{path}: {error}"))?;
        if value.get("project").and_then(serde_json::Value::as_str) != Some(project)
            || value
                .get("asset_source")
                .and_then(serde_json::Value::as_str)
                != Some(source)
            || value
                .get("renderer")
                .and_then(serde_json::Value::as_str)
                .is_none()
        {
            return Err(format!("invalid renderer evidence: {path}").into());
        }
        evidence.push(value);
    }
    Ok(serde_json::Value::Array(evidence))
}

fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<_> = text.lines().rev().take(40).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn server_binary(coverage: Option<&str>) -> Result<&'static str, &'static str> {
    match coverage {
        None => Ok("target/release/aoe-server"),
        Some("1") => Ok("target/llvm-cov-target/debug/aoe-server"),
        Some(_) => Err("AOE_E2E_COVERAGE must be absent or 1"),
    }
}

#[cfg(test)]
mod tests;
