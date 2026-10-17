//! Execute wasm-bindgen-test in pinned Chromium through a disposable WebDriver.
use serde::Serialize;
use std::{
    error::Error,
    fs,
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod retry;
mod suites;
pub(crate) use suites::{SUITES, SuiteRun, packages, run_focused};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Driver {
    name: String,
}

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
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

fn unused_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn start_driver(port: u16) -> Result<Driver> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let name = format!("aoeworld-wasm-{}-{nonce}", std::process::id());
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let port_arg = format!("--port={port}");
    let cap = crate::slots::cap_args();
    let mut args = vec!["run"];
    args.extend(cap.iter().map(String::as_str));
    args.extend([
        "--rm",
        "-d",
        "--init",
        "--network",
        "host",
        "--user",
        &user,
        "--name",
        &name,
        "-e",
        "HOME=/tmp",
        "--tmpfs",
        "/tmp:rw,nosuid,size=256m",
        "aoeworld/browser-tools:1.63.0",
        "chromedriver",
        &port_arg,
        "--allowed-ips=127.0.0.1",
    ]);
    docker(&args)?;
    let driver = Driver { name };
    let address: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
            return Ok(driver);
        }
        thread::sleep(Duration::from_millis(100));
    }
    let logs = docker(&["logs", "--tail", "50", &driver.name]).unwrap_or_default();
    Err(format!("ChromeDriver did not become ready: {logs}").into())
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    target: &'static str,
    runner: &'static str,
    result: &'static str,
}

/// Bound for one `cargo test` invocation, build included.
const CARGO_DEADLINE: Duration = Duration::from_secs(600);
/// Whole-suite bound inside the browser. The runner defaults to 20 s, which
/// the 145-test rendering suite no longer fits on a CI runner.
const SUITE_TIMEOUT_SECONDS: &str = "300";

fn runner_environment<'a>(remote: &'a str, capabilities: &'a str) -> [(&'static str, &'a str); 5] {
    [
        ("WASM_BINDGEN_USE_BROWSER", "1"),
        ("CHROMEDRIVER_REMOTE", remote),
        (
            "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER",
            "wasm-bindgen-test-runner",
        ),
        ("WASM_BINDGEN_TEST_WEBDRIVER_JSON", capabilities),
        ("WASM_BINDGEN_TEST_TIMEOUT", SUITE_TIMEOUT_SECONDS),
    ]
}

pub fn run() -> Result<()> {
    let root = std::env::current_dir()?.canonicalize()?;
    let port = unused_port()?;
    let _driver = start_driver(port)?;
    let remote = format!("http://127.0.0.1:{port}/");
    let capabilities = root.join("browser/webdriver.json");
    let capabilities = capabilities
        .to_str()
        .ok_or("non-UTF-8 WebDriver configuration path")?;
    let environment = runner_environment(&remote, capabilities);
    if let Err(failure) = retry::run_suites(&environment) {
        let Some(names) = failure.context_failures else {
            return Err(failure.error);
        };
        eprintln!(
            "test-wasm: retrying once: runner WebGL context errors in: {} (#391)",
            names.join(", ")
        );
        retry::run_suites(&environment).map_err(|failure| failure.error)?;
    }
    let report = Report {
        version: 1,
        revision: git("rev-parse", "HEAD")?,
        dirty: !git("status", "--porcelain")?.is_empty(),
        target: "wasm32-unknown-unknown",
        runner: "wasm-bindgen-test-runner in pinned Chromium",
        result: "PASS",
    };
    let directory = Path::new("reports/wasm");
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("browser.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

fn git(first: &str, second: &str) -> Result<String> {
    let output = Command::new("git").args([first, second]).output()?;
    if !output.status.success() {
        return Err("cannot read Git revision for WASM test report".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests;
