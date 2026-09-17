//! Execute wasm-bindgen-test in pinned Chromium through a disposable WebDriver.
use crate::process;
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
    docker(&[
        "run",
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
    ])?;
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

pub fn run() -> Result<()> {
    let root = std::env::current_dir()?.canonicalize()?;
    let port = unused_port()?;
    let _driver = start_driver(port)?;
    let remote = format!("http://127.0.0.1:{port}/");
    let capabilities = root.join("browser/webdriver.json");
    let capabilities = capabilities
        .to_str()
        .ok_or("non-UTF-8 WebDriver configuration path")?;
    process::run_with_env(
        "cargo",
        &[
            "test",
            "--locked",
            "--release",
            "-p",
            "aoe-client",
            "--target",
            "wasm32-unknown-unknown",
            "--test",
            "browser",
        ],
        &[
            ("WASM_BINDGEN_USE_BROWSER", "1"),
            ("CHROMEDRIVER_REMOTE", &remote),
            (
                "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER",
                "wasm-bindgen-test-runner",
            ),
            ("WASM_BINDGEN_TEST_WEBDRIVER_JSON", capabilities),
        ],
        Duration::from_secs(600),
    )?;
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
mod tests {
    use super::*;

    #[test]
    fn current_checkout_has_report_identity_and_a_free_local_port() {
        assert_eq!(git("rev-parse", "HEAD").expect("revision").len(), 40);
        assert!(unused_port().expect("port") > 0);
    }
}
