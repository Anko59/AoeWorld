//! Disposable browser stack, with an isolated port and cleanup on every return path.
use crate::process;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
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

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.canonicalize()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let coverage = std::env::var("AOE_E2E_COVERAGE").ok();
    let relative_binary = server_binary(coverage.as_deref())?;
    let binary = root.join(relative_binary);
    if !binary.is_file() {
        return Err(format!("test server binary missing: {}", binary.display()).into());
    }
    let server = Command::new(binary)
        .current_dir(&root)
        .env("AOE_BIND", address.to_string())
        .env("AOE_SCENARIO", "smoke")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut server = Server(server);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("test server exited early: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("test server did not become healthy within 20s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let name = format!("aoeworld-e2e-{}-{}", std::process::id(), address.port());
    let _cleanup = BrowserContainer(name.clone());
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{}:{}", root.display(), root.display());
    let home = format!("HOME={}/.cache/browser-home", root.display());
    let url = format!("AOE_BASE_URL=http://{address}");
    let workdir = root.join("browser");
    let args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        "--init".to_owned(),
        "--network".to_owned(),
        "host".to_owned(),
        "--ipc".to_owned(),
        "host".to_owned(),
        "--user".to_owned(),
        user,
        "--name".to_owned(),
        name,
        "-e".to_owned(),
        home,
        "-e".to_owned(),
        url,
        "-v".to_owned(),
        mount,
        "-w".to_owned(),
        workdir.display().to_string(),
        "aoeworld/browser-tools:1.63.0".to_owned(),
        "xvfb-run".to_owned(),
        "-a".to_owned(),
        "npm".to_owned(),
        "run".to_owned(),
        "test:e2e".to_owned(),
    ];
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let cancellation = process::Cancellation::default();
    thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = mpsc::channel();
        scope.spawn(move || {
            let result = process::run_cancellable(
                "docker",
                &refs,
                Duration::from_secs(180),
                &worker_cancellation,
            );
            let _ = sender.send(result);
        });
        loop {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(result) => return result.map_err(Into::into),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("browser test worker disconnected".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(status) = server.0.try_wait()? {
                        cancellation.cancel();
                        return Err(
                            format!("test server exited during browser run: {status}").into()
                        );
                    }
                }
            }
        }
    })?;
    server.shutdown()?;
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

fn server_binary(coverage: Option<&str>) -> Result<&'static str, &'static str> {
    match coverage {
        None => Ok("target/release/aoe-server"),
        Some("1") => Ok("target/llvm-cov-target/debug/aoe-server"),
        Some(_) => Err("AOE_E2E_COVERAGE must be absent or 1"),
    }
}

#[cfg(test)]
mod tests {
    use super::server_binary;

    #[test]
    fn coverage_mode_cannot_silently_select_an_uninstrumented_server() {
        assert_eq!(server_binary(None), Ok("target/release/aoe-server"));
        assert_eq!(
            server_binary(Some("1")),
            Ok("target/llvm-cov-target/debug/aoe-server")
        );
        for invalid in ["", "0", "true", "release"] {
            assert!(server_binary(Some(invalid)).is_err());
        }
    }
}
