//! Disposable promotion and rollback of exact local image IDs.
use crate::release::Manifest;
use serde_json::Value;
use std::{
    error::Error,
    io::{Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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

struct Resources {
    network: String,
    containers: Vec<String>,
}

impl Drop for Resources {
    fn drop(&mut self) {
        for container in &self.containers {
            let _ = Command::new("docker")
                .args(["rm", "-f", container])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = Command::new("docker")
            .args(["network", "rm", &self.network])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct Stack {
    server: String,
    browser: String,
}

fn start(resources: &mut Resources, manifest: &Manifest) -> Result<Stack> {
    let server = docker(&[
        "run",
        "-d",
        "--init",
        "--network",
        &resources.network,
        "--network-alias",
        "server",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=64m",
        &manifest.server_image_id,
    ])?;
    resources.containers.push(server.clone());
    let browser = docker(&[
        "run",
        "-d",
        "--init",
        "--network",
        &resources.network,
        "--read-only",
        "--tmpfs",
        "/tmp:rw,nosuid,size=64m",
        "--publish",
        "127.0.0.1::8080",
        &manifest.browser_image_id,
    ])?;
    resources.containers.push(browser.clone());
    let stack = Stack { server, browser };
    wait_healthy(&stack.browser, &manifest.source_commit)?;
    Ok(stack)
}

fn stop(stack: Stack) -> Result<()> {
    docker(&["rm", "-f", &stack.browser, &stack.server])?;
    Ok(())
}

fn health(port: u16) -> Result<Value> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_secs(1),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let response = String::from_utf8(bytes)?;
    if !response.starts_with("HTTP/1.1 200") {
        return Err(format!(
            "release stack health: {}",
            response.lines().next().unwrap_or("empty response")
        )
        .into());
    }
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("health response lacks body")?;
    Ok(serde_json::from_str(body)?)
}

fn wait_healthy(browser: &str, revision: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let mapping = docker(&["port", browser, "8080/tcp"])?;
        if let Some(port) = mapping
            .lines()
            .next()
            .and_then(|line| line.rsplit(':').next())
            .and_then(|value| value.parse::<u16>().ok())
            && let Ok(value) = health(port)
            && value["status"] == "ok"
            && value["build"] == revision
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("release stack {revision} did not become healthy").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn rehearse(candidate: &Manifest, previous: &Manifest) -> Result<()> {
    if candidate.source_commit == previous.source_commit {
        return Err("promotion rehearsal requires two distinct source revisions".into());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let network = format!("aoeworld-release-{}-{nonce}", std::process::id());
    docker(&["network", "create", &network])?;
    let mut resources = Resources {
        network,
        containers: Vec::new(),
    };
    let previous_stack = start(&mut resources, previous)?;
    stop(previous_stack)?;
    let candidate_stack = start(&mut resources, candidate)?;
    stop(candidate_stack)?;
    let rollback_stack = start(&mut resources, previous)?;
    stop(rollback_stack)?;
    println!(
        "promoted {} and rolled back to {} using exact local image IDs",
        candidate.source_commit, previous.source_commit
    );
    Ok(())
}
