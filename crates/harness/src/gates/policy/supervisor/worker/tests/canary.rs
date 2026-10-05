//! Explicit opt-in local Docker canaries; absence is a tested request contract, not a runtime pass.
use super::super::{
    journal::{Journal, nonce},
    lifecycle::Controller,
    transport::{self, Action, Backend, Binding},
};
use super::*;
use crate::process::{self, CaptureExit, Captured};
use std::{fs, path::PathBuf, process::Command, time::Instant};
#[derive(Clone, Copy)]
enum Scenario {
    Sandbox,
    Nonzero,
    Flood,
    Deadline,
    Cancel,
    Lint,
    Unit,
}
impl Scenario {
    fn operation(self) -> Operation {
        match self {
            Self::Sandbox => Operation::FmtCheck,
            Self::Nonzero => Operation::StructureCheck,
            Self::Flood => Operation::ArchitectureCheck,
            Self::Deadline | Self::Cancel => Operation::DocsCheck,
            Self::Lint => Operation::Lint,
            Self::Unit => Operation::TestUnit,
        }
    }
}
struct FixtureDocker {
    start: Instant,
    client: PathBuf,
    cancel: Cancellation,
    cancel_wait: bool,
}
impl Backend for FixtureDocker {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    fn call(
        &mut self,
        b: &Binding,
        a: &Action,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Captured {
        if self.cancel_wait && matches!(a, Action::Wait(_)) {
            self.cancel.cancel();
        }
        let mut cmd = Command::new("/usr/bin/docker");
        cmd.env_clear()
            .current_dir("/")
            .env("HOME", "/var/empty")
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .args(["--host", "unix:///var/run/docker.sock", "--config"])
            .arg(&self.client)
            .args(transport::arguments(b, a));
        process::capture_command(cmd, timeout, cancel)
    }
}
#[test]
fn local_canary_request_is_explicit_and_never_asserts_authority() {
    let image = std::env::var("AOE_WORKER_CANARY_IMAGE").ok();
    let root = std::env::var_os("AOE_WORKER_CANARY_ROOT").map(PathBuf::from);
    match (image, root) {
        (None, None) => {
            assert_eq!(Observation::empty().status, Status::Unavailable);
            assert!(Observation::empty().transport.is_empty());
            eprintln!("LOCAL_CANARY_NOT_REQUESTED; actual Docker lifecycle remains UNOBSERVED");
        }
        (Some(image), Some(root)) => {
            assert!(
                root.is_absolute() && root.is_dir(),
                "canary root must be an existing daemon-visible absolute directory"
            );
            let canonical = fs::canonicalize(&root).unwrap();
            assert_eq!(canonical, root, "fixture root must be canonical");
            for scenario in [
                Scenario::Sandbox,
                Scenario::Nonzero,
                Scenario::Flood,
                Scenario::Deadline,
                Scenario::Cancel,
                Scenario::Lint,
                Scenario::Unit,
            ] {
                actual(&image, &root, scenario);
            }
        }
        _ => panic!("both canary request fields are required; no fallback or skipped runtime"),
    }
}
fn actual(image: &str, parent: &Path, scenario: Scenario) {
    use std::os::unix::fs::PermissionsExt;
    let owner = tempfile::Builder::new()
        .prefix("worker-canary-")
        .tempdir_in(parent)
        .unwrap();
    let source = owner.path().join("source");
    let output = owner.path().join("output");
    let client = owner.path().join("client");
    for path in [&source, &output, &client] {
        fs::create_dir(path).unwrap();
    }
    fs::set_permissions(&source, fs::Permissions::from_mode(0o777)).unwrap();
    fs::write(source.join("sentinel"), b"immutable local fixture source\n").unwrap();
    fs::set_permissions(source.join("sentinel"), fs::Permissions::from_mode(0o666)).unwrap();
    let config_path = client.join("config.json");
    fs::write(&config_path, b"{}").unwrap();
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let out = PrivateOutput::new(&output, &[source.clone(), client.clone()]).unwrap();
    let mut config = config();
    config.image = image.into();
    config.workload_s = 2;
    config.command_s = 2;
    config.cleanup_s = 15;
    // Reparse every fixture config exactly as production constraints, with no mutable tags.
    let config = Config::parse(&serde_json::to_vec(&config).unwrap()).unwrap();
    let config_digest = blake3::hash(&serde_json::to_vec(&config).unwrap())
        .to_hex()
        .to_string();
    let operation = scenario.operation();
    let id = nonce().unwrap();
    let seal = blake3::hash(&fs::read(source.join("sentinel")).unwrap())
        .to_hex()
        .to_string();
    let binding = Binding::new(&source, config, operation, id.clone()).unwrap();
    let journal = Journal::new(&out, id, &seal, &config_digest, &binding.operation).unwrap();
    let cancel = Cancellation::default();
    let backend = FixtureDocker {
        start: Instant::now(),
        client,
        cancel: cancel.clone(),
        cancel_wait: matches!(scenario, Scenario::Cancel),
    };
    let result = Controller::new(
        backend,
        journal,
        binding,
        |_| fs::read(&config_path).is_ok_and(|bytes| bytes == b"{}"),
        &cancel,
        Duration::from_secs(40),
    )
    .unwrap()
    .run();
    let observed = &result.observation;
    assert!(!observed.authoritative);
    assert_eq!(observed.independent_judge, "UNAVAILABLE");
    assert_eq!(
        observed.cleanup,
        Cleanup::VerifiedAbsent,
        "exact container absence must actually be observed: {observed:?}"
    );
    assert!(observed.journal_retained);
    assert_eq!(
        fs::read(source.join("sentinel")).unwrap(),
        b"immutable local fixture source\n"
    );
    assert!(!source.join("worker-write").exists());
    assert!(
        observed
            .transport
            .iter()
            .any(|phase| phase.phase == Phase::Start)
    );
    match scenario {
        Scenario::Sandbox | Scenario::Lint | Scenario::Unit => {
            assert_eq!(observed.status, Status::CompletedNonAuthoritative);
            assert_eq!(observed.container_exit_code, Some(0));
            let logs = result.logs.unwrap();
            assert!(matches!(logs.exit, CaptureExit::Success));
            assert!(
                logs.stdout
                    .windows(b"AOE_LOCAL_SANDBOX_CANARY_V1".len())
                    .any(|w| w == b"AOE_LOCAL_SANDBOX_CANARY_V1")
            );
        }
        Scenario::Nonzero => {
            assert_eq!(observed.status, Status::Failed);
            assert_eq!(observed.container_exit_code, Some(7));
        }
        Scenario::Flood => {
            assert_ne!(observed.status, Status::CompletedNonAuthoritative);
            let logs = result.logs.unwrap();
            assert!(logs.truncated);
            assert!(logs.stdout.len() <= 65536 && logs.stderr.len() <= 65536);
        }
        Scenario::Deadline => {
            assert_eq!(observed.status, Status::Deadline);
            assert_eq!(observed.container_exit_code, None);
        }
        Scenario::Cancel => {
            assert_eq!(observed.status, Status::Cancelled);
            assert_eq!(observed.container_exit_code, None);
        }
    }
    eprintln!(
        "LOCAL_CANARY_ACTUAL_NON_AUTHORITATIVE {} {:?} cleanup={:?} phases={} controller_ms={}",
        scenario.operation().argument(),
        observed.status,
        observed.cleanup,
        observed.transport.len(),
        observed.controller_duration_ms
    );
}
