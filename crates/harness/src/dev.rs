//! Checkout-scoped Docker lifecycle for the local synthetic lab.
use std::{
    error::Error,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn name(root: &Path) -> String {
    let hash = blake3::hash(root.to_string_lossy().as_bytes()).to_hex();
    format!("aoeworld-dev-{}", &hash[..12])
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

fn exists(name: &str) -> Result<bool> {
    Ok(!docker(&[
        "ps",
        "-a",
        "--filter",
        &format!("name=^/{name}$"),
        "--format",
        "{{.ID}}",
    ])?
    .is_empty())
}

fn running(name: &str) -> Result<bool> {
    Ok(exists(name)? && docker(&["inspect", "--format", "{{.State.Running}}", name])? == "true")
}

fn healthy() -> bool {
    let address: SocketAddr = match "127.0.0.1:8080".parse() {
        Ok(value) => value,
        Err(_) => return false,
    };
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

pub fn start() -> Result<()> {
    let root = std::env::current_dir()?.canonicalize()?;
    let name = name(&root);
    if running(&name)? {
        println!("AoeWorld Harness Lab already running: http://127.0.0.1:8080");
        return Ok(());
    }
    if exists(&name)? {
        docker(&["rm", &name])?;
    }
    let scenario = std::env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
    if aoe_scenario::named(&scenario).is_none() {
        return Err(format!("unknown AOE_SCENARIO: {scenario}").into());
    }
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{}:{}:ro", root.display(), root.display());
    let scenario_env = format!("AOE_SCENARIO={scenario}");
    let executable = root.join("target/release/aoe-server");
    if !executable.is_file() || !root.join("web/pkg/aoe_client_bg.wasm").is_file() {
        return Err("build-wasm and the release server build are required".into());
    }
    docker(&[
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
        "AOE_BIND=0.0.0.0:8080",
        "-e",
        &scenario_env,
        "-p",
        "127.0.0.1:8080:8080",
        "-v",
        &mount,
        "-w",
        root.to_str().ok_or("non-UTF-8 checkout path")?,
        "aoeworld/rust-tools:1.93.1",
        executable.to_str().ok_or("non-UTF-8 server path")?,
    ])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if healthy() {
            println!("AoeWorld Harness Lab: http://127.0.0.1:8080");
            return Ok(());
        }
        if !running(&name)? {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let logs = recent_logs(&name, "30").unwrap_or_default();
    let _ = docker(&["stop", &name]);
    Err(format!("development server did not become healthy: {logs}").into())
}

pub fn down() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    if exists(&name)? {
        docker(&["stop", &name])?;
    }
    println!("AoeWorld Harness Lab stopped: {name}");
    Ok(())
}

pub fn status() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    println!(
        "{}: {}",
        name,
        if running(&name)? {
            "running"
        } else {
            "stopped"
        }
    );
    Ok(())
}

pub fn logs() -> Result<()> {
    let name = name(&std::env::current_dir()?.canonicalize()?);
    if !exists(&name)? {
        return Err("development server is not running".into());
    }
    println!("{}", recent_logs(&name, "200")?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_names_are_stable_and_isolated() {
        assert_eq!(name(Path::new("/tmp/a")), name(Path::new("/tmp/a")));
        assert_ne!(name(Path::new("/tmp/a")), name(Path::new("/tmp/b")));
    }
}
