//! Checkout-scoped Docker lifecycle for the local AoeWorld server.
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const RUST_TOOLS_IMAGE: &str = "aoeworld/rust-tools:1.93.1";

/// The server's port inside its container; DEV_PORT chooses the host's.
const CONTAINER_PORT: u16 = 8080;

trait Runtime {
    fn docker(&self, args: &[&str]) -> Result<String>;
    fn recent_logs(&self, name: &str, tail: &str) -> Result<String>;
    fn healthy(&self, port: u16) -> bool;
}

struct RealRuntime;

impl Runtime for RealRuntime {
    fn docker(&self, args: &[&str]) -> Result<String> {
        docker(args)
    }

    fn recent_logs(&self, name: &str, tail: &str) -> Result<String> {
        recent_logs(name, tail)
    }

    fn healthy(&self, port: u16) -> bool {
        healthy(port)
    }
}

fn name(root: &Path) -> String {
    let hash = blake3::hash(root.to_string_lossy().as_bytes()).to_hex();
    format!("aoeworld-dev-{}", &hash[..12])
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn build_identity(root: &Path) -> Result<String> {
    let revision = git(root, &["rev-parse", "HEAD"])?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid checkout revision".into());
    }
    if git(root, &["status", "--porcelain"])?.is_empty() {
        Ok(revision)
    } else {
        Ok(format!("{revision}-dirty"))
    }
}

fn docker(args: &[&str]) -> Result<String> {
    let output = Command::new("docker").args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "docker {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn recent_logs(name: &str, tail: &str) -> Result<String> {
    let output = Command::new("docker")
        .args(["logs", "--tail", tail, name])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "docker logs {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn exists<R: Runtime>(runtime: &R, name: &str) -> Result<bool> {
    Ok(!runtime
        .docker(&[
            "ps",
            "-a",
            "--filter",
            &format!("name=^/{name}$"),
            "--format",
            "{{.ID}}",
        ])?
        .is_empty())
}

fn running<R: Runtime>(runtime: &R, name: &str) -> Result<bool> {
    Ok(exists(runtime, name)?
        && runtime.docker(&["inspect", "--format", "{{.State.Running}}", name])? == "true")
}

/// DEV_PORT: the loopback host port of this checkout's server, so two
/// checkouts can run side by side. Unset or empty means 8080.
fn dev_port(value: Option<&str>) -> Result<u16> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(CONTAINER_PORT);
    };
    match value.parse::<u16>() {
        Ok(port) if port != 0 && value.bytes().all(|byte| byte.is_ascii_digit()) => Ok(port),
        _ => Err(format!("DEV_PORT={value}: use a port from 1 to 65535").into()),
    }
}

/// Docker's `-p` value: loopback only, host port to the container's port.
fn host_mapping(port: u16) -> String {
    format!("127.0.0.1:{port}:{CONTAINER_PORT}")
}

fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// The host port a running container publishes its server on.
fn published<R: Runtime>(runtime: &R, name: &str) -> Result<u16> {
    let mapping = runtime.docker(&["port", name, &format!("{CONTAINER_PORT}/tcp")])?;
    mapping
        .lines()
        .find_map(|line| line.trim().rsplit_once(':')?.1.parse().ok())
        .ok_or_else(|| format!("{name} publishes no port: {mapping}").into())
}

fn healthy(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(250)) else {
        return false;
    };
    if stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .is_err()
        || stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .is_err()
    {
        return false;
    }
    let mut bytes = [0u8; 128];
    stream
        .read(&mut bytes)
        .is_ok_and(|count| bytes[..count].starts_with(b"HTTP/1.1 200"))
}

fn verified_worker(root: &Path) -> Result<PathBuf> {
    let worker = root
        .join("target/release/aoe-map-worker")
        .canonicalize()
        .map_err(|_| "the release aoe-map-worker executable is required")?;
    if !worker.starts_with(root) || !is_executable(&worker) {
        return Err("the release aoe-map-worker executable is required".into());
    }
    Ok(worker)
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn writable_directory(root: &Path, relative: &str) -> Result<(PathBuf, PathBuf)> {
    let requested = root.join(relative);
    fs::create_dir_all(&requested)?;
    let source = requested.canonicalize()?;
    Ok((source.clone(), source))
}

pub fn start() -> Result<()> {
    let root = std::env::current_dir()?.canonicalize()?;
    let scenario = std::env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
    let asset_pack = std::env::var("AOE_ASSET_PACK").ok();
    let port = dev_port(std::env::var("DEV_PORT").ok().as_deref())?;
    let build = build_identity(&root)?;
    start_with(
        &RealRuntime,
        &root,
        &scenario,
        &build,
        asset_pack.as_deref(),
        port,
    )
}

fn start_with<R: Runtime>(
    runtime: &R,
    root: &Path,
    scenario: &str,
    build: &str,
    asset_pack: Option<&str>,
    port: u16,
) -> Result<()> {
    let name = name(root);
    if running(runtime, &name)? {
        let current = published(runtime, &name)?;
        if current != port {
            return Err(format!(
                "this checkout's server already runs at {}; `make down` first to move it to port {port}",
                url(current)
            )
            .into());
        }
        println!("AoeWorld already running: {}", url(current));
        return Ok(());
    }
    if exists(runtime, &name)? {
        runtime.docker(&["rm", &name])?;
    }
    if aoe_scenario::named(scenario).is_none() {
        return Err(format!("unknown AOE_SCENARIO: {scenario}").into());
    }
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{}:{}:ro", root.display(), root.display());
    let scenario_env = format!("AOE_SCENARIO={scenario}");
    let build_env = format!("AOE_BUILD_SHA={build}");
    let asset_env = format!("AOE_ASSET_PACK={}", asset_pack.unwrap_or(""));
    let executable = root.join("target/release/aoe-server");
    if !executable.is_file() || !root.join("web/pkg/aoe_client_bg.wasm").is_file() {
        return Err("build-wasm and the release server build are required".into());
    }
    let worker = verified_worker(root)?;
    let (map_directory, map_target) =
        writable_directory(root, aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY)?;
    let (cache_directory, cache_target) =
        writable_directory(root, aoe_server::Config::DEFAULT_GEODATA_CACHE_DIRECTORY)?;
    let map_mount = format!("{}:{}:rw", map_directory.display(), map_target.display());
    let cache_mount = format!(
        "{}:{}:rw",
        cache_directory.display(),
        cache_target.display()
    );
    let worker_env = format!("AOE_MAP_WORKER={}", worker.display());
    let cache_env = format!("AOE_GEODATA_CACHE={}", cache_target.display());
    runtime.docker(&[
        "run",
        "--rm",
        "-d",
        "--init",
        "--name",
        &name,
        "--user",
        &user,
        "--read-only",
        "--security-opt",
        "no-new-privileges",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=64m",
        "-e",
        &format!("AOE_BIND=0.0.0.0:{CONTAINER_PORT}"),
        "-e",
        &scenario_env,
        "-e",
        &build_env,
        "-e",
        &asset_env,
        "-e",
        &worker_env,
        "-e",
        &cache_env,
        "-p",
        &host_mapping(port),
        "-v",
        &mount,
        "-v",
        &map_mount,
        "-v",
        &cache_mount,
        "-w",
        root.to_str().ok_or("non-UTF-8 checkout path")?,
        RUST_TOOLS_IMAGE,
        executable.to_str().ok_or("non-UTF-8 server path")?,
    ])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if runtime.healthy(port) {
            println!("AoeWorld: {}", url(port));
            return Ok(());
        }
        if !running(runtime, &name)? {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let logs = runtime.recent_logs(&name, "30").unwrap_or_default();
    let _ = runtime.docker(&["stop", &name]);
    Err(format!("development server did not become healthy: {logs}").into())
}

pub fn down() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    down_with(&RealRuntime, &name)
}

fn down_with<R: Runtime>(runtime: &R, name: &str) -> Result<()> {
    if exists(runtime, name)? {
        runtime.docker(&["stop", name])?;
    }
    println!("AoeWorld stopped: {name}");
    Ok(())
}

pub fn status() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    status_with(&RealRuntime, &name)
}

fn status_with<R: Runtime>(runtime: &R, name: &str) -> Result<()> {
    println!(
        "{}: {}",
        name,
        if running(runtime, name)? {
            "running"
        } else {
            "stopped"
        }
    );
    Ok(())
}

pub fn logs() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    logs_with(&RealRuntime, &name)
}

fn logs_with<R: Runtime>(runtime: &R, name: &str) -> Result<()> {
    if !exists(runtime, name)? {
        return Err("development server is not running".into());
    }
    println!("{}", runtime.recent_logs(name, "200")?);
    Ok(())
}

#[cfg(test)]
mod tests;
