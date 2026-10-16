//! Private, finite Docker ABI. No caller-selected command, endpoint or image import.
use super::{
    Operation, Phase,
    config::{self, Config},
};
use crate::process::{self, Cancellation, CaptureExit, Captured};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub(super) const ENV: [&str; 7] = [
    "PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin",
    "HOME=/scratch",
    "LANG=C",
    "LC_ALL=C",
    "CARGO_HOME=/opt/cargo",
    "CARGO_NET_OFFLINE=true",
    "CARGO_TARGET_DIR=/target",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cid(String);
impl Cid {
    pub(super) fn parse(bytes: &[u8]) -> Option<Self> {
        let value = std::str::from_utf8(bytes)
            .ok()?
            .strip_suffix('\n')
            .unwrap_or(std::str::from_utf8(bytes).ok()?);
        (value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| Self(value.into()))
    }
    pub(super) fn value(&self) -> &str {
        &self.0
    }
}
pub(super) struct Binding {
    pub(super) source: PathBuf,
    pub(super) config: Config,
    pub(super) operation: Operation,
    pub(super) nonce: String,
    source_handle: std::fs::File,
}
impl Binding {
    #[cfg(unix)]
    pub(super) fn new(
        source: &Path,
        config: Config,
        operation: Operation,
        nonce: String,
    ) -> Option<Self> {
        let source = std::fs::canonicalize(source).ok()?;
        let text = source.to_str()?;
        if text.contains([',', ':', '\n', '\r'])
            || source == Path::new("/")
            || !source.is_dir()
            || nonce.len() != 32
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        use std::os::unix::fs::OpenOptionsExt;
        let source_handle = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
            .open(&source)
            .ok()?;
        let binding = Self {
            source,
            config,
            operation,
            nonce,
            source_handle,
        };
        binding.source_held().then_some(binding)
    }
    #[cfg(not(unix))]
    pub(super) fn new(_: &Path, _: Config, _: Operation, _: String) -> Option<Self> {
        None
    }
    #[cfg(not(unix))]
    pub(super) fn source_held(&self) -> bool {
        false
    }
    #[cfg(unix)]
    pub(super) fn source_held(&self) -> bool {
        use std::os::unix::fs::MetadataExt;
        match (
            std::fs::symlink_metadata(&self.source),
            self.source_handle.metadata(),
        ) {
            (Ok(endpoint), Ok(held)) => {
                endpoint.is_dir() && endpoint.dev() == held.dev() && endpoint.ino() == held.ino()
            }
            _ => false,
        }
    }
    pub(super) fn user(&self) -> String {
        format!("{}:{}", self.config.uid, self.config.uid)
    }
    fn tmpfs(&self) -> Value {
        json!({"/tmp":format!("rw,nosuid,nodev,noexec,size={}m,mode=1777",self.config.tmp_mib),
            "/target":format!("rw,nosuid,nodev,exec,size={}m,mode=1777",self.config.target_mib),
            "/scratch":format!("rw,nosuid,nodev,noexec,size={}m,mode=1777",self.config.tmp_mib)})
    }
    pub(super) fn labels(&self) -> Value {
        json!({"aoeworld.local-worker":"v1", "aoeworld.local-nonce":self.nonce})
    }
}
pub(super) enum Action {
    ImageInspect,
    Create,
    Recover,
    Inspect(Cid),
    Query(Cid),
    Start(Cid),
    Wait(Cid),
    Logs(Cid),
    Stop(Cid),
    Kill(Cid),
    Remove(Cid),
}
impl Action {
    pub(super) fn phase(&self) -> Phase {
        match self {
            Self::ImageInspect => Phase::ImageInspect,
            Self::Create => Phase::Create,
            Self::Recover => Phase::Recover,
            Self::Inspect(_) | Self::Query(_) => Phase::Inspect,
            Self::Start(_) => Phase::Start,
            Self::Wait(_) => Phase::Wait,
            Self::Logs(_) => Phase::Logs,
            Self::Stop(_) => Phase::Stop,
            Self::Kill(_) => Phase::Kill,
            Self::Remove(_) => Phase::Remove,
        }
    }
}
pub(super) trait Backend {
    fn now(&self) -> Duration;
    fn call(
        &mut self,
        binding: &Binding,
        action: &Action,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Captured;
}
pub(super) struct Docker {
    pub(super) start: Instant,
}
impl Backend for Docker {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    fn call(
        &mut self,
        binding: &Binding,
        action: &Action,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Captured {
        process::capture_command(command(binding, action), timeout, cancel)
    }
}
fn command(binding: &Binding, action: &Action) -> Command {
    let mut command = Command::new(config::PROGRAM);
    command
        .env_clear()
        .current_dir("/")
        .env("HOME", "/var/empty")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .args([
            "--host",
            "unix:///run/aoeworld-supervisor/docker.sock",
            "--config",
            config::CLIENT,
        ]);
    command.args(arguments(binding, action));
    command
}
pub(super) fn arguments(binding: &Binding, action: &Action) -> Vec<String> {
    let strings = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match action {
        Action::ImageInspect => strings(&[
            "image",
            "inspect",
            "--format",
            "{{json .}}",
            &binding.config.image,
        ]),
        Action::Recover => strings(&[
            "container",
            "ls",
            "--all",
            "--no-trunc",
            "--quiet",
            "--filter",
            "label=aoeworld.local-worker=v1",
            "--filter",
            &format!("label=aoeworld.local-nonce={}", binding.nonce),
        ]),
        Action::Inspect(cid) => strings(&[
            "container",
            "inspect",
            "--format",
            "{{json .}}",
            cid.value(),
        ]),
        Action::Query(cid) => strings(&[
            "container",
            "ls",
            "--all",
            "--no-trunc",
            "--quiet",
            "--filter",
            &format!("id={}", cid.value()),
        ]),
        Action::Start(cid) => strings(&["container", "start", cid.value()]),
        Action::Wait(cid) => strings(&["container", "wait", cid.value()]),
        Action::Logs(cid) => strings(&["container", "logs", "--tail", "1024", cid.value()]),
        Action::Stop(cid) => strings(&["container", "stop", "--time", "1", cid.value()]),
        Action::Kill(cid) => strings(&["container", "kill", cid.value()]),
        Action::Remove(cid) => strings(&["container", "rm", "--force", cid.value()]),
        Action::Create => {
            let mut args = strings(&[
                "container",
                "create",
                "--pull",
                "never",
                "--read-only",
                "--network",
                "none",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--ipc",
                "private",
                "--cgroupns",
                "private",
                "--user",
                &binding.user(),
                "--workdir",
                "/candidate",
                "--entrypoint",
                "/judge/aoe-harness",
                "--restart",
                "no",
                "--memory",
                &format!("{}m", binding.config.memory_mib),
                "--memory-swap",
                &format!("{}m", binding.config.memory_mib),
                "--pids-limit",
                &binding.config.pids.to_string(),
                "--cpus",
                &binding.config.cpus.to_string(),
                "--log-driver",
                "local",
                "--log-opt",
                "max-size=1m",
                "--log-opt",
                "max-file=1",
                "--log-opt",
                "compress=false",
                "--mount",
                &format!(
                    "type=bind,src={},dst=/candidate,readonly,bind-propagation=rprivate,bind-recursive=readonly",
                    binding.source.display()
                ),
                "--label",
                "aoeworld.local-worker=v1",
                "--label",
                &format!("aoeworld.local-nonce={}", binding.nonce),
            ]);
            for mount in [
                format!(
                    "/scratch:rw,nosuid,nodev,noexec,size={}m,mode=1777",
                    binding.config.tmp_mib
                ),
                format!(
                    "/target:rw,nosuid,nodev,exec,size={}m,mode=1777",
                    binding.config.target_mib
                ),
                format!(
                    "/tmp:rw,nosuid,nodev,noexec,size={}m,mode=1777",
                    binding.config.tmp_mib
                ),
            ] {
                args.extend(strings(&["--tmpfs", &mount]));
            }
            for env in ENV {
                args.extend(strings(&["--env", env]));
            }
            args.extend(strings(&[
                &binding.config.image,
                binding.operation.argument(),
            ]));
            args
        }
    }
}
pub(super) fn successful(capture: &Captured) -> bool {
    matches!(capture.exit, CaptureExit::Success) && !capture.truncated
}
pub(super) fn object(capture: &Captured) -> Option<Value> {
    if !successful(capture) || capture.stdout.len() > 32768 {
        return None;
    }
    let value: Packet = serde_json::from_slice(&capture.stdout).ok()?;
    value.0.is_object().then_some(value.0)
}
/// Exact seven admitted key/value pairs, independent of Docker's override ordering.
fn env_matches(value: &Value) -> bool {
    let Some(entries) = value.as_array() else {
        return false;
    };
    if entries.len() != ENV.len() {
        return false;
    }
    let mut keys = std::collections::BTreeSet::new();
    for entry in entries {
        let Some(entry) = entry.as_str() else {
            return false;
        };
        if entry.len() > 128 || !ENV.contains(&entry) {
            return false;
        }
        let Some((key, _)) = entry.split_once('=') else {
            return false;
        };
        if !keys.insert(key) {
            return false;
        }
    }
    true
}
fn empty(value: &Value) -> bool {
    value.is_null()
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(serde_json::Map::is_empty)
}
pub(super) fn image_matches(value: &Value, binding: &Binding) -> bool {
    let c = &value["Config"];
    value["Id"] == binding.config.image
        && c["User"] == binding.user()
        && c["WorkingDir"] == "/candidate"
        && c["Entrypoint"] == json!(["/judge/aoe-harness"])
        && empty(&c["Cmd"])
        && env_matches(&c["Env"])
        && [
            "Volumes",
            "OnBuild",
            "Healthcheck",
            "ExposedPorts",
            "Labels",
        ]
        .iter()
        .all(|key| empty(&c[*key]))
}
#[derive(Clone, Copy, Debug)]
pub(super) struct State {
    pub(super) running: bool,
    pub(super) exit_code: i32,
}
/// Inspect the real Docker configuration, not self-asserted labels/security hashes.
pub(super) fn inspect_matches(value: &Value, cid: &Cid, binding: &Binding) -> Option<State> {
    let h = &value["HostConfig"];
    let c = &value["Config"];
    let mount = json!({"Type":"bind","Source":binding.source,"Destination":"/candidate","Mode":"","RW":false,"Propagation":"rprivate"});
    let mounts = value["Mounts"].as_array()?;
    if value["Id"] != cid.value()
        || value["Image"] != binding.config.image
        || c["Image"] != binding.config.image
        || c["User"] != binding.user()
        || c["WorkingDir"] != "/candidate"
        || c["Entrypoint"] != json!(["/judge/aoe-harness"])
        || c["Cmd"] != json!([binding.operation.argument()])
        || !env_matches(&c["Env"])
        || c["Labels"] != binding.labels()
        || !empty(&c["Volumes"])
        || !empty(&c["Healthcheck"])
        || h["NetworkMode"] != "none"
        || h["ReadonlyRootfs"] != true
        || h["Privileged"] != false
        || h["CapDrop"] != json!(["ALL"])
        || !empty(&h["CapAdd"])
        || h["SecurityOpt"] != json!(["no-new-privileges"])
        || h["Memory"] != u64::from(binding.config.memory_mib) * 1048576
        || h["MemorySwap"] != u64::from(binding.config.memory_mib) * 1048576
        || h["PidsLimit"] != binding.config.pids
        || h["NanoCpus"] != u64::from(binding.config.cpus) * 1000000000
        || h["Tmpfs"] != binding.tmpfs()
        || h["AutoRemove"] != false
        || h["RestartPolicy"]["Name"] != "no"
        || h["RestartPolicy"]["MaximumRetryCount"] != 0
        || h["LogConfig"]
            != json!({"Type":"local","Config":{"max-size":"1m","max-file":"1","compress":"false"}})
        || h["PidMode"] != ""
        || h["IpcMode"] != "private"
        || h["UTSMode"] != ""
        || h["CgroupnsMode"] != "private"
        || [
            "GroupAdd",
            "Devices",
            "DeviceRequests",
            "DeviceCgroupRules",
            "VolumesFrom",
            "Binds",
            "Links",
            "PortBindings",
        ]
        .iter()
        .any(|key| !empty(&h[*key]))
        || mounts.len() != 1
        || mounts[0] != mount
    {
        return None;
    }
    let state = &value["State"];
    if state["Dead"] != false || state["Paused"] != false || state["Restarting"] != false {
        return None;
    }
    Some(State {
        running: state["Running"].as_bool()?,
        exit_code: i32::try_from(state["ExitCode"].as_i64()?).ok()?,
    })
}
/// Absence comes from successful exact-ID enumeration, not parsing Docker error text.
pub(super) fn recover(capture: &Captured) -> Option<Vec<Cid>> {
    if !successful(capture) || capture.stdout.len() > 4096 {
        return None;
    }
    let text = std::str::from_utf8(&capture.stdout).ok()?;
    let mut ids = Vec::new();
    for line in text.lines() {
        ids.push(Cid::parse(line.as_bytes())?);
        if ids.len() > 1 {
            return None;
        }
    }
    Some(ids)
}
pub(super) fn exit_code(capture: &Captured) -> Option<i32> {
    if !successful(capture) || capture.stdout.len() > 12 {
        return None;
    }
    let text = std::str::from_utf8(&capture.stdout).ok()?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok().filter(|n| (0..=255).contains(n))
}
mod packet;
use packet::Packet;
#[cfg(test)]
mod tests;
