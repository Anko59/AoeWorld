use super::*;
use std::os::unix::fs::PermissionsExt;

fn fake_cargo(body: &str) -> (tempfile::TempDir, Command) {
    let root = tempfile::tempdir().unwrap();
    let cargo = root.path().join("cargo");
    // Execute an immutable fixture: parallel spawns can inherit a just-written
    // executable's writable descriptor and transiently fail with ETXTBSY.
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/process/tests/fixtures/cargo.sh"),
        &cargo,
    )
    .unwrap();
    let mut command = Command::new(cargo);
    command
        .current_dir(root.path())
        .args(["check", "--locked", "--offline"]);
    command
        .env("CARGO_NET_OFFLINE", "true")
        .env("AOE_FAKE_CARGO_BODY", body);
    (root, command)
}

const MISS: &str = "echo 'error: failed to download `alloca v0.4.0`\nCaused by:\n  attempting to make an HTTP request, but --offline was specified' >&2\nexit 101";

#[test]
fn cargo_cache_offline_miss_fetches_online_then_retries_original_once() {
    let (root, command) = fake_cargo(&format!(
        r#"
if [ "$1" = fetch ]; then
    test "$CARGO_NET_OFFLINE" = false || exit 9
    touch fetched
elif [ ! -f fetched ]; then
    {MISS}
fi
"#
    ));
    let result = supervise(
        "cargo",
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    let calls = fs::read_to_string(root.path().join("calls")).unwrap();
    let calls: Vec<_> = calls.lines().collect();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(calls[0], "check --locked --offline");
    assert!(calls[1].starts_with("fetch --locked"), "{calls:?}");
    assert!(!calls[1].contains("--offline"));
    assert_eq!(calls[2], calls[0]);
}

#[test]
fn cargo_cache_failed_fetch_names_the_fix_and_does_not_retry() {
    let (root, command) = fake_cargo(&format!(
        "if [ \"$1\" = fetch ]; then echo 'network unavailable' >&2; exit 7; fi\n{MISS}"
    ));
    let result = supervise(
        "cargo",
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(result.is_err(), "{result:?}");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("make cargo-fetch"), "{error}");
    assert!(error.contains("network"), "{error}");
    assert_eq!(
        fs::read_to_string(root.path().join("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn cargo_cache_does_not_loop_when_retry_still_misses() {
    let (root, command) = fake_cargo(&format!("[ \"$1\" = fetch ] && exit 0\n{MISS}"));
    let result = supervise(
        "cargo",
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(result.is_err(), "{result:?}");
    assert_eq!(
        fs::read_to_string(root.path().join("calls"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn cargo_cache_does_not_retry_compiler_errors_or_online_failures() {
    for body in [
        "echo 'error[E0308]: mismatched types' >&2; exit 101",
        "echo 'failed to download crate: could not resolve host' >&2; exit 101",
    ] {
        let (root, command) = fake_cargo(body);
        let result = supervise(
            "cargo",
            command,
            Duration::from_secs(5),
            &Cancellation::default(),
        );
        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            fs::read_to_string(root.path().join("calls"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
}

#[test]
fn cargo_cache_fetch_uses_the_original_manifest_and_working_directory() {
    let (root, mut command) = fake_cargo(&format!("[ \"$1\" = fetch ] && exit 0\n{MISS}"));
    command.args(["--manifest-path", "nested/Cargo.toml"]);
    let result = supervise(
        "cargo",
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(result.is_err(), "{result:?}");
    let calls = fs::read_to_string(root.path().join("calls")).unwrap();
    let fetch = calls.lines().nth(1).unwrap();
    assert!(
        fetch.contains("--manifest-path nested/Cargo.toml"),
        "{calls}"
    );
}

#[test]
fn cargo_cache_fetch_and_retry_share_the_original_deadline() {
    let (root, command) = fake_cargo(&format!("[ \"$1\" = fetch ] && exec sleep 5\n{MISS}"));
    let started = Instant::now();
    let result = supervise(
        "cargo",
        command,
        Duration::from_millis(100),
        &Cancellation::default(),
    );
    assert!(result.unwrap_err().to_string().contains("deadline"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        fs::read_to_string(root.path().join("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

fn recovering_cargo() -> (tempfile::TempDir, Command) {
    fake_cargo(&format!(
        "if [ \"$1\" = fetch ]; then touch fetched; echo fetch-output; elif [ ! -f fetched ]; then {MISS}; else printf runner-output; fi"
    ))
}

#[test]
fn cargo_cache_nextest_metadata_exit_102_fetches_and_retries_once() {
    let (root, mut command) = fake_cargo(&format!(
        "if [ \"$1\" = fetch ]; then touch fetched; elif [ ! -f fetched ]; then {}; fi",
        MISS.replace("exit 101", "exit 102")
    ));
    command = {
        let mut nextest = Command::new(command.get_program());
        nextest
            .current_dir(root.path())
            .args(["nextest", "run", "--locked", "--offline"]);
        for (key, value) in command.get_envs() {
            if let Some(value) = value {
                nextest.env(key, value);
            }
        }
        nextest
    };
    let result = supervise(
        "cargo",
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    let calls = fs::read_to_string(root.path().join("calls")).unwrap();
    let calls: Vec<_> = calls.lines().collect();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(calls[0], "nextest run --locked --offline");
    assert_eq!(calls[1], "fetch --locked --config net.offline=false");
    assert_eq!(calls[2], calls[0]);
}

#[test]
fn cargo_cache_wasm_capture_apis_fetch_and_preserve_runner_output() {
    for api in ["capture_command", "capture_in", "retaining_stdout"] {
        let (root, command) = recovering_cargo();
        let cancel = Cancellation::default();
        let budget = Duration::from_secs(5);
        let mut retained = Vec::new();
        let captured = match api {
            "capture_command" => capture_command(command, budget, &cancel),
            "capture_in" => capture_in(
                root.path(),
                command.get_program().to_str().unwrap(),
                &["test", "--locked", "--target", "wasm32-unknown-unknown"],
                &[(
                    "AOE_FAKE_CARGO_BODY",
                    command
                        .get_envs()
                        .find(|(key, _)| *key == "AOE_FAKE_CARGO_BODY")
                        .unwrap()
                        .1
                        .unwrap()
                        .to_str()
                        .unwrap(),
                )],
                budget,
                &cancel,
            ),
            _ => capture_command_retaining_stdout(command, budget, &cancel, &mut retained),
        };
        assert!(
            matches!(captured.exit, CaptureExit::Success),
            "{api}: {captured:?}"
        );
        assert_eq!(captured.stdout, b"runner-output", "{api}");
        if api == "retaining_stdout" {
            assert_eq!(
                retained, b"runner-output",
                "fetch output must not enter runner diagnostics"
            );
        }
        let calls = fs::read_to_string(root.path().join("calls")).unwrap();
        let calls: Vec<_> = calls.lines().collect();
        assert_eq!(calls.len(), 3, "{api}: {calls:?}");
        assert!(calls[1].starts_with("fetch --locked"), "{calls:?}");
        assert_eq!(calls[2], calls[0]);
    }
}

#[test]
fn cargo_cache_supervised_docker_build_fetches_in_the_same_container_context() {
    let (root, cargo) = recovering_cargo();
    let docker = root.path().join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
printf '%s\n' "$*" >> docker-calls
while [ "$1" != aoeworld/rust-tools:test ]; do shift; done
shift
exec "./$@"
"#,
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    for network in [vec![], vec!["--network", "none"], vec!["--net=none"]] {
        for file in ["fetched", "calls", "docker-calls"] {
            let _ = fs::remove_file(root.path().join(file));
        }
        let mut command = Command::new(&docker);
        for (key, value) in cargo.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command
            .current_dir(root.path())
            .args(["run", "--rm", "--init", "--user", "123:456"])
            .args(&network)
            .args([
                "-e",
                "CARGO_HOME=/shared cargo",
                "-v",
                "/tree:/tree",
                "-w",
                "/tree",
                "aoeworld/rust-tools:test",
                "cargo",
                "build",
                "--locked",
                "--offline",
                "--target",
                "wasm32-unknown-unknown",
            ]);
        let result = supervise(
            "docker",
            command,
            Duration::from_secs(5),
            &Cancellation::default(),
        );
        assert!(result.is_ok(), "{network:?}: {result:?}");
        let calls = fs::read_to_string(root.path().join("docker-calls")).unwrap();
        let calls: Vec<_> = calls.lines().collect();
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert_eq!(calls[2], calls[0]);
        assert!(
            calls[1].contains("-e CARGO_HOME=/shared cargo -v /tree:/tree -w /tree"),
            "{calls:?}"
        );
        assert!(
            calls[1].contains(
                "cargo fetch --locked --target wasm32-unknown-unknown --config net.offline=false"
            ),
            "{calls:?}"
        );
        assert!(calls[1].contains("-e CARGO_NET_OFFLINE=false"), "{calls:?}");
        assert!(!calls[1].contains("--offline"), "{calls:?}");
        assert!(
            !calls[1].contains("none"),
            "fetch requires network: {calls:?}"
        );
    }
}

#[test]
fn cargo_cache_capture_recovery_preserves_deadline_and_cancellation() {
    for cancel_fetch in [false, true] {
        let (root, command) = fake_cargo(&format!(
            "if [ \"$1\" = fetch ]; then touch fetching; exec sleep 5; fi\n{MISS}"
        ));
        let cancel = Cancellation::default();
        let child_cancel = cancel.clone();
        let fetching = root.path().join("fetching");
        let worker = cancel_fetch.then(|| {
            thread::spawn(move || {
                let until = Instant::now() + Duration::from_secs(2);
                while !fetching.exists() && Instant::now() < until {
                    thread::sleep(Duration::from_millis(5));
                }
                child_cancel.cancel();
            })
        });
        let budget = if cancel_fetch {
            Duration::from_secs(4)
        } else {
            Duration::from_millis(150)
        };
        let started = Instant::now();
        let captured = capture_command(command, budget, &cancel);
        if let Some(worker) = worker {
            worker.join().unwrap();
        }
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(
            if cancel_fetch {
                matches!(captured.exit, CaptureExit::Cancelled)
            } else {
                matches!(captured.exit, CaptureExit::Deadline)
            },
            "{captured:?}"
        );
        assert!(captured.duration >= Duration::from_millis(5));
        let calls = fs::read_to_string(root.path().join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 2, "{calls}");
    }
}
