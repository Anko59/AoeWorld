//! The server processes themselves: started detached, found by their port,
//! stopped by signal. Nothing here is specific to a revision.
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;

const HTTP_TIMEOUT: Duration = Duration::from_millis(1500);
const STOP_GRACE: Duration = Duration::from_secs(8);
const MAX_BODY: u64 = 1 << 20;
/// A map's raster is read to its end but never kept; this bounds the read.
const MAX_REPLY: u64 = 256 << 20;

/// `GET path` on loopback `port`; the body of a 200, nothing otherwise.
pub(crate) fn http_get(port: u16, path: &str) -> Option<String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(HTTP_TIMEOUT)).ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut reply = Vec::new();
    stream.take(MAX_BODY).read_to_end(&mut reply).ok()?;
    let text = String::from_utf8_lossy(&reply);
    let (head, body) = text.split_once("\r\n\r\n")?;
    head.starts_with("HTTP/1.1 200").then(|| body.to_owned())
}

/// The status code of `GET path` on loopback `port`, reading the whole reply
/// (a server that sees the client leave abandons what it was computing) for at
/// most `patience`; `None` when nothing answered in time.
pub(crate) fn http_status(port: u16, path: &str, patience: Duration) -> Option<u16> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT).ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut head = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    let mut seen = 0_u64;
    loop {
        let left = patience.checked_sub(started.elapsed())?;
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if head.len() < 64 {
                    head.extend_from_slice(&chunk[..count.min(64)]);
                }
                seen += count as u64;
                if seen > MAX_REPLY {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    let line = String::from_utf8_lossy(&head);
    line.strip_prefix("HTTP/1.1 ")?.get(..3)?.parse().ok()
}

/// The build a healthy server on `port` reports (`AOE_BUILD_SHA`).
pub(crate) fn health_build(port: u16) -> Option<String> {
    let body = http_get(port, "/health")?;
    // The body may be chunked; the JSON object is what lies between the braces.
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(&body[start..=end]).ok()?;
    Some(value.get("build")?.as_str()?.to_owned())
}

/// A loopback port nothing listens on, the first of `first..first + 8`.
pub(crate) fn free_port(first: u16, taken: &[u16]) -> Option<u16> {
    (first..first.saturating_add(8))
        .filter(|port| !taken.contains(port))
        .find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
}

/// Start `binary` in `directory` in its own process group, so neither the
/// caller's exit nor a signal to its group ends it. Output goes to `log`.
pub(crate) fn spawn(
    binary: &Path,
    arguments: &[&str],
    directory: &Path,
    environment: &[(String, String)],
    log: &Path,
) -> Result<u32> {
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    let child = Command::new(binary)
        .args(arguments)
        .current_dir(directory)
        .envs(environment.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{}: {e}", binary.display()))?;
    Ok(child.id())
}

/// Whether `pid` is a live process whose command line names `binary`.
pub(crate) fn running(pid: u32, binary: &Path) -> bool {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let cmdline = String::from_utf8_lossy(&raw);
    // A zombie keeps its cmdline file readable but empty.
    cmdline
        .split('\0')
        .next()
        .is_some_and(|first| !first.is_empty() && Path::new(first) == binary)
}

/// SIGTERM, then SIGKILL after a grace period. A process already gone is fine.
pub(crate) fn stop(pid: u32, binary: &Path) {
    let Ok(raw) = i32::try_from(pid) else { return };
    let target = nix::unistd::Pid::from_raw(raw);
    if !running(pid, binary) {
        return;
    }
    let _ = nix::sys::signal::kill(target, nix::sys::signal::Signal::SIGTERM);
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline && running(pid, binary) {
        thread::sleep(Duration::from_millis(100));
    }
    if running(pid, binary) {
        let _ = nix::sys::signal::kill(target, nix::sys::signal::Signal::SIGKILL);
    }
}

/// One mount of a server container: the same path inside and out.
pub(crate) struct Mount {
    pub(crate) path: std::path::PathBuf,
    pub(crate) writable: bool,
}

fn docker(args: &[String]) -> Result<String> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("docker: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "docker {}: {}",
            args.first().map_or("", String::as_str),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Start `binary` in a detached tool-image container (the map worker links
/// libraries only that image has), published on loopback `port`. The
/// container runs as the caller, read-only, with only `mounts` visible.
pub(crate) fn docker_run(
    name: &str,
    port: u16,
    directory: &Path,
    binary: &Path,
    environment: &[(String, String)],
    mounts: &[Mount],
) -> Result<()> {
    let _ = docker(&["rm".into(), "-f".into(), name.into()]);
    let mut args: Vec<String> = [
        "run",
        "-d",
        "--rm",
        "--init",
        "--name",
        name,
        "--user",
        &format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid()),
        "--read-only",
        "--security-opt",
        "no-new-privileges",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=64m",
        "-p",
        &format!("127.0.0.1:{port}:8080"),
    ]
    .map(String::from)
    .to_vec();
    for (key, value) in environment {
        args.extend(["-e".into(), format!("{key}={value}")]);
    }
    args.extend(["-e".into(), "AOE_BIND=0.0.0.0:8080".into()]);
    for mount in mounts {
        let mode = if mount.writable { "rw" } else { "ro" };
        args.extend(["-v".into(), format!("{0}:{0}:{mode}", mount.path.display())]);
    }
    args.extend([
        "-w".into(),
        directory.display().to_string(),
        crate::dev::RUST_TOOLS_IMAGE.into(),
        binary.display().to_string(),
    ]);
    docker(&args).map(|_| ())
}

pub(crate) fn docker_running(name: &str) -> bool {
    docker(&[
        "inspect".into(),
        "--format".into(),
        "{{.State.Running}}".into(),
        name.into(),
    ])
    .is_ok_and(|out| out == "true")
}

pub(crate) fn docker_stop(name: &str) {
    let _ = docker(&["stop".into(), "--time".into(), "8".into(), name.into()]);
}
