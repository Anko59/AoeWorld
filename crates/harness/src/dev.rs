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

trait Runtime {
    fn docker(&self, args: &[&str]) -> Result<String>;
    fn recent_logs(&self, name: &str, tail: &str) -> Result<String>;
    fn healthy(&self) -> bool;
}

struct RealRuntime;

impl Runtime for RealRuntime {
    fn docker(&self, args: &[&str]) -> Result<String> {
        docker(args)
    }

    fn recent_logs(&self, name: &str, tail: &str) -> Result<String> {
        recent_logs(name, tail)
    }

    fn healthy(&self) -> bool {
        healthy()
    }
}

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
    let scenario = std::env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
    start_with(&RealRuntime, &root, &scenario)
}

fn start_with<R: Runtime>(runtime: &R, root: &Path, scenario: &str) -> Result<()> {
    let name = name(root);
    if running(runtime, &name)? {
        println!("AoeWorld Harness Lab already running: http://127.0.0.1:8080");
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
    let executable = root.join("target/release/aoe-server");
    if !executable.is_file() || !root.join("web/pkg/aoe_client_bg.wasm").is_file() {
        return Err("build-wasm and the release server build are required".into());
    }
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
        if runtime.healthy() {
            println!("AoeWorld Harness Lab: http://127.0.0.1:8080");
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
    println!("AoeWorld Harness Lab stopped: {name}");
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
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeRuntime {
        calls: RefCell<Vec<Vec<String>>>,
        exists: Cell<bool>,
        running: Cell<bool>,
        healthy: Cell<bool>,
        dies_on_start: bool,
    }

    impl Runtime for FakeRuntime {
        fn docker(&self, args: &[&str]) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|value| (*value).to_owned()).collect());
            match args {
                ["ps", ..] => Ok(if self.exists.get() { "container" } else { "" }.to_owned()),
                ["inspect", ..] => Ok(self.running.get().to_string()),
                ["rm", ..] | ["stop", ..] => {
                    self.exists.set(false);
                    self.running.set(false);
                    Ok(String::new())
                }
                ["run", ..] => {
                    self.exists.set(true);
                    self.running.set(!self.dies_on_start);
                    Ok("container".to_owned())
                }
                _ => Err(format!("unexpected Docker call: {args:?}").into()),
            }
        }

        fn recent_logs(&self, _: &str, _: &str) -> Result<String> {
            Ok("startup failed".to_owned())
        }

        fn healthy(&self) -> bool {
            self.healthy.get()
        }
    }

    fn built_checkout() -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("checkout");
        let server = temp.path().join("target/release/aoe-server");
        let wasm = temp.path().join("web/pkg/aoe_client_bg.wasm");
        std::fs::create_dir_all(server.parent().expect("server parent")).expect("server dir");
        std::fs::create_dir_all(wasm.parent().expect("WASM parent")).expect("WASM dir");
        std::fs::write(server, b"server").expect("server");
        std::fs::write(wasm, b"wasm").expect("WASM");
        temp
    }

    #[test]
    fn checkout_names_are_stable_and_isolated() {
        assert_eq!(name(Path::new("/tmp/a")), name(Path::new("/tmp/a")));
        assert_ne!(name(Path::new("/tmp/a")), name(Path::new("/tmp/b")));
    }

    #[test]
    fn development_lifecycle_uses_checkout_name_and_cleans_stale_container() {
        let checkout = built_checkout();
        let runtime = FakeRuntime::default();
        runtime.exists.set(true);
        runtime.healthy.set(true);
        start_with(&runtime, checkout.path(), "smoke").expect("start");
        let calls = runtime.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|args| args == &["rm", &name(checkout.path())])
        );
        let run = calls
            .iter()
            .find(|args| args.first().is_some_and(|arg| arg == "run"))
            .expect("Docker run");
        assert!(run.contains(&name(checkout.path())));
        assert!(run.contains(&"AOE_SCENARIO=smoke".to_owned()));
        assert!(run.contains(&"--read-only".to_owned()));
        drop(calls);
        status_with(&runtime, &name(checkout.path())).expect("running status");
        logs_with(&runtime, &name(checkout.path())).expect("logs");
        down_with(&runtime, &name(checkout.path())).expect("stop");
        assert!(!runtime.exists.get());
        assert!(logs_with(&runtime, &name(checkout.path())).is_err());
        down_with(&runtime, &name(checkout.path())).expect("idempotent stop");
        status_with(&runtime, &name(checkout.path())).expect("stopped status");
    }

    #[test]
    fn invalid_configuration_or_missing_build_never_launches_docker_container() {
        let checkout = built_checkout();
        let runtime = FakeRuntime::default();
        assert!(start_with(&runtime, checkout.path(), "unknown").is_err());
        std::fs::remove_file(checkout.path().join("web/pkg/aoe_client_bg.wasm"))
            .expect("remove WASM");
        assert!(start_with(&runtime, checkout.path(), "smoke").is_err());
        assert!(
            !runtime
                .calls
                .borrow()
                .iter()
                .any(|args| args.first().is_some_and(|arg| arg == "run"))
        );
    }

    #[test]
    fn failed_start_stops_only_its_checkout_container() {
        let checkout = built_checkout();
        let runtime = FakeRuntime {
            dies_on_start: true,
            ..Default::default()
        };
        let error = start_with(&runtime, checkout.path(), "smoke")
            .expect_err("failed start")
            .to_string();
        assert!(error.contains("startup failed"));
        let calls = runtime.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|args| args == &["stop", &name(checkout.path())])
        );
    }

    #[test]
    fn existing_running_lab_is_not_replaced() {
        let checkout = built_checkout();
        let runtime = FakeRuntime::default();
        runtime.exists.set(true);
        runtime.running.set(true);
        start_with(&runtime, checkout.path(), "smoke").expect("already running");
        assert!(
            !runtime
                .calls
                .borrow()
                .iter()
                .any(|args| matches!(args.first().map(String::as_str), Some("rm" | "run")))
        );
    }
}
