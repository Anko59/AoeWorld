use super::*;
use std::{fs, os::unix::fs::PermissionsExt};
fn script(root: &Path, body: &str) -> std::path::PathBuf {
    let path = root.join("fixture");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}
fn fixture(body: &str) -> Value {
    let root = tempfile::tempdir().unwrap();
    let program = script(root.path(), body);
    let report = capture(
        &program,
        &ARGS,
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert_capture_fields(&report);
    report
}
fn assert_capture_fields(report: &Value) {
    let observation = &report["capture_observation"];
    assert_eq!(observation["schema"], 1);
    assert_eq!(observation["root_cause"], "ROOT_CAUSE_NOT_ASSESSED");
    assert_eq!(report["duration_ms"], observation["duration_ms"]);
    assert_eq!(report["truncated"], observation["truncated"]);
    assert_eq!(report["stdout_blake3"], observation["stdout"]["raw_blake3"]);
    assert_eq!(report["stderr_blake3"], observation["stderr"]["raw_blake3"]);
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["admission_granted"], false);
}
#[test]
fn fixed_read_only_contract_has_no_mutation_or_ambient_context() {
    let report = contract();
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["mutating_actions"], json!([]));
    assert_eq!(report["argv"], json!(ARGS));
    assert_eq!(report["program"], PROGRAM);
    assert_eq!(report["environment"]["PATH"], "/usr/bin:/bin");
    let command = isolated(Path::new(PROGRAM), &ARGS);
    assert_eq!(command.get_program(), PROGRAM);
    assert_eq!(command.get_current_dir(), Some(Path::new("/")));
    let args: Vec<_> = command
        .get_args()
        .map(|value| value.to_str().unwrap())
        .collect();
    assert_eq!(args, ARGS);
    let env: Vec<_> = command
        .get_envs()
        .map(|(key, value)| (key.to_str().unwrap(), value.unwrap().to_str().unwrap()))
        .collect();
    assert_eq!(
        env,
        vec![
            ("HOME", "/var/empty"),
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("PATH", "/usr/bin:/bin")
        ]
    );
}
#[test]
fn successful_fixture_asserts_exact_endpoint_prefix_and_json_identifier() {
    let report = fixture(
        "test \"$1\" = --host && test \"$2\" = unix:///run/aoeworld-supervisor/docker.sock && test \"$3\" = --config && test \"$4\" = /etc/aoeworld/supervisor/docker-client && test \"$5\" = info && test \"$6\" = --format && test \"$7\" = '{{json .ID}}' || exit 9\nprintf '\"ABC:012-def.id\"\\n'",
    );
    assert_eq!(report["status"], "PROBED_NON_AUTHORITATIVE");
    assert_eq!(report["observed_daemon_id"], "ABC:012-def.id");
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["admission_granted"], false);
}
#[test]
fn malformed_binary_empty_control_non_string_and_failure_responses_never_probe() {
    for body in [
        "printf ''",
        "printf 'null'",
        "printf '{}'",
        "printf '3'",
        "printf '\"\"'",
        "printf '\"space id\"'",
        "printf '\"id\\\\u0000\"'",
        "printf '\"id\\\\n\"'",
        "printf '\\377'",
        "printf '\"id\"'; printf '\\377' >&2",
        "printf '\"id\"'; printf '\\000' >&2",
        "printf '\"id\"'; exit 3",
    ] {
        let report = fixture(body);
        assert_eq!(report["status"], "UNAVAILABLE", "{body}: {report}");
        assert_eq!(report["authoritative"], false);
        assert!(report.get("observed_daemon_id").is_none());
    }
}
#[test]
fn limits_both_streams_and_truncation_reject_even_zero_exit() {
    for body in [
        "printf '\"id\"'; /usr/bin/head -c 4097 /dev/zero >&2",
        "printf '\"'; /usr/bin/head -c 4097 /dev/zero; printf '\"'",
        "printf '\"id\"'; /usr/bin/head -c 70000 /dev/zero >&2",
        "printf '\"'; /usr/bin/head -c 70000 /dev/zero; printf '\"'",
    ] {
        let report = fixture(body);
        assert_eq!(report["status"], "UNAVAILABLE");
        assert!(report.get("observed_daemon_id").is_none());
    }
    let too_long = fixture(&format!("printf '\"{}\"'", "a".repeat(257)));
    assert_eq!(too_long["status"], "UNAVAILABLE");
}
#[test]
fn actual_deadline_cancelled_and_zero_budget_never_fake_success_or_spawn() {
    // A single child and a wider timeout keep this check stable under parallel test load.
    let report = capture(
        Path::new("/bin/sleep"),
        &["2"],
        Duration::from_millis(250),
        &Cancellation::default(),
    );
    assert_eq!(report["exit"], "DEADLINE");
    assert_capture_fields(&report);
    assert_eq!(report["capture_observation"]["outcome"]["kind"], "DEADLINE");
    assert_eq!(report["status"], "UNAVAILABLE");
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("spawned");
    let program = script(
        root.path(),
        &format!("touch '{}'; printf '\"id\"'", marker.display()),
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        capture(&program, &ARGS, Duration::from_secs(1), &cancel)["exit"],
        "CANCELLED"
    );
    assert!(!marker.exists());
    assert_eq!(
        capture(&program, &ARGS, Duration::ZERO, &Cancellation::default())["exit"],
        "DEADLINE"
    );
    assert!(!marker.exists());
    assert_eq!(probe(&cancel)["status"], "UNAVAILABLE");
    assert_eq!(
        capture(
            &root.path().join("absent"),
            &ARGS,
            Duration::from_secs(1),
            &Cancellation::default()
        )["exit"],
        "START_UNAVAILABLE"
    );
}
#[test]
fn observation_discards_monitor_errors_and_never_leaks_raw_stderr() {
    let captured = Captured {
        exit: CaptureExit::Monitor(std::io::Error::other("sensitive detail")),
        stdout: b"\"id\"".to_vec(),
        stderr: b"SECRET_DO_NOT_RENDER".to_vec(),
        truncated: false,
        duration: Duration::ZERO,
    };
    let expected = serde_json::to_value(safe_observation(&captured)).unwrap();
    let report = observed(captured);
    assert_capture_fields(&report);
    assert_eq!(report["capture_observation"], expected);
    assert_eq!(
        report["capture_observation"]["outcome"]["io_kind"],
        "OTHER_UNKNOWN"
    );
    assert_eq!(report["exit"], "MONITOR_UNAVAILABLE");
    assert!(!report.to_string().contains("SECRET_DO_NOT_RENDER"));
    assert!(!report.to_string().contains("sensitive detail"));
    let report = fixture("printf '\"id\"'; printf 'ordinary warning\\n' >&2");
    assert_eq!(report["status"], "PROBED_NON_AUTHORITATIVE");
}
#[test]
fn actual_transport_uses_the_same_safe_capture_observation_without_failure_text_classification() {
    for (body, kind, status) in [
        (
            "printf '\"model-id\"'; printf 'SECRET-TRANSPORT-TAIL-v1 panic stacktrace' >&2",
            "SUCCESS",
            "PROBED_NON_AUTHORITATIVE",
        ),
        (
            "printf 'SECRET-TRANSPORT-TAIL-v1'; printf 'panic credentials SECRET-TRANSPORT-TAIL-v1' >&2; exit 7",
            "FAILED",
            "UNAVAILABLE",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let program = script(root.path(), body);
        let actual = capture_command(
            isolated(&program, &ARGS),
            Duration::from_secs(2),
            &Cancellation::default(),
        );
        let expected = serde_json::to_value(safe_observation(&actual)).unwrap();
        let report = observed(actual);
        assert_capture_fields(&report);
        assert_eq!(report["capture_observation"], expected);
        assert_eq!(report["capture_observation"]["outcome"]["kind"], kind);
        assert_eq!(report["status"], status);
        if kind == "FAILED" {
            assert_eq!(report["capture_observation"]["outcome"]["code"], 7);
            assert_eq!(
                report["capture_observation"]["outcome"]["termination"],
                "EXIT_CODE"
            );
        }
        for raw in [
            "SECRET-TRANSPORT-TAIL-v1",
            "stacktrace",
            "credentials",
            "CODE_BUG",
        ] {
            assert!(!report.to_string().contains(raw));
        }
    }
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("SECRET-TRANSPORT-MISSING-v1");
    let actual = capture_command(
        isolated(&missing, &ARGS),
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    let expected = serde_json::to_value(safe_observation(&actual)).unwrap();
    let report = observed(actual);
    assert_capture_fields(&report);
    assert_eq!(report["capture_observation"], expected);
    assert_eq!(
        report["capture_observation"]["outcome"]["io_kind"],
        "NOT_FOUND"
    );
    assert_eq!(report["exit"], "START_UNAVAILABLE");
    assert!(!report.to_string().contains("SECRET-TRANSPORT-MISSING-v1"));
    let cancel = Cancellation::default();
    cancel.cancel();
    let actual = capture_command(
        isolated(Path::new(PROGRAM), &ARGS),
        Duration::from_secs(2),
        &cancel,
    );
    let expected = serde_json::to_value(safe_observation(&actual)).unwrap();
    let report = observed(actual);
    assert_capture_fields(&report);
    assert_eq!(report["capture_observation"], expected);
    assert_eq!(
        report["capture_observation"]["outcome"]["kind"],
        "CANCELLED"
    );
    assert_eq!(report["exit"], "CANCELLED");
}
// A self-test subprocess injects ambient secrets WITHOUT unsafe global env changes.
#[test]
fn environment_subprocess_fixture() {
    if std::env::var_os("AOE_TRANSPORT_ENV_CHILD").is_none() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let env = root.path().join("observed-env");
    let program = script(
        root.path(),
        &format!("/usr/bin/env > '{}'; printf '\"model-id\"'", env.display()),
    );
    assert_eq!(
        capture(
            &program,
            &ARGS,
            Duration::from_secs(2),
            &Cancellation::default()
        )["status"],
        "PROBED_NON_AUTHORITATIVE"
    );
    let raw = fs::read_to_string(env).unwrap();
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "CARGO_REGISTRY_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "GIT_CONFIG_COUNT",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_TLS_VERIFY",
        "LD_PRELOAD",
        "AOE_TRANSPORT_ENV_CHILD",
    ] {
        assert!(
            !raw.lines()
                .any(|line| line.starts_with(&format!("{name}="))),
            "{name}"
        );
    }
    assert!(raw.lines().any(|line| line == "HOME=/var/empty"));
    assert!(raw.lines().any(|line| line == "PATH=/usr/bin:/bin"));
}
#[test]
fn inherited_credentials_loader_and_daemon_overrides_are_actually_cleared() {
    let exe = std::env::current_exe().unwrap();
    let module = module_path!().split_once("::").unwrap().1;
    let mut child = Command::new(exe);
    child.args([
        "--exact",
        &format!("{module}::environment_subprocess_fixture"),
        "--nocapture",
    ]);
    child.env("AOE_TRANSPORT_ENV_CHILD", "1");
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "CARGO_REGISTRY_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "GIT_CONFIG_COUNT",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_TLS_VERIFY",
    ] {
        child.env(name, "ambient-sensitive-test-value");
    }
    child
        .env("PATH", "/invalid/path")
        .env("LD_PRELOAD", "/nonexistent/ambient-loader.so");
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
#[cfg(unix)]
#[test]
fn filesystem_primitive_rejects_socket_config_and_executable_aliases() {
    use std::os::unix::net::UnixListener;
    let root = tempfile::tempdir().unwrap();
    let uid = nix::unistd::geteuid().as_raw();
    let program = script(root.path(), "printf '\"model-id\"'");
    let socket = root.path().join("socket");
    let _listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let config = root.path().join("config");
    fs::create_dir(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(paths::inspect(&program, &socket, &config, uid, root.path()).is_ok());
    let cfg = config.join("config.json");
    fs::write(&cfg, b"{}").unwrap();
    fs::set_permissions(&cfg, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(paths::inspect(&program, &socket, &config, uid, root.path()).is_ok());
    for bad in [
        b"{\"credsStore\":\"evil\"}".to_vec(),
        b"{\"currentContext\":\"evil\"}".to_vec(),
        b"not-json".to_vec(),
        vec![b' '; 4097],
    ] {
        fs::write(&cfg, bad).unwrap();
        assert!(paths::inspect(&program, &socket, &config, uid, root.path()).is_err());
    }
    fs::write(&cfg, b"{}").unwrap();
    fs::set_permissions(&cfg, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(paths::inspect(&program, &socket, &config, uid, root.path()).is_err());
    fs::set_permissions(&cfg, fs::Permissions::from_mode(0o600)).unwrap();
    let linked = root.path().join("linked-program");
    std::os::unix::fs::symlink(&program, &linked).unwrap();
    assert!(paths::inspect(&linked, &socket, &config, uid, root.path()).is_err());
    let linked = root.path().join("linked-socket");
    std::os::unix::fs::symlink(&socket, &linked).unwrap();
    assert!(paths::inspect(&program, &linked, &config, uid, root.path()).is_err());
    let linked = root.path().join("linked-config");
    std::os::unix::fs::symlink(&config, &linked).unwrap();
    assert!(paths::inspect(&program, &socket, &linked, uid, root.path()).is_err());
    let linked = config.join("other");
    fs::write(&linked, b"credential-material").unwrap();
    assert!(paths::inspect(&program, &socket, &config, uid, root.path()).is_err());
}
