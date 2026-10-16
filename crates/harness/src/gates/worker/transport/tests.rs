#![cfg(unix)]
use super::super::tests::{binding, cid, image, inspect};
use super::*;
use std::time::Duration;
fn capture(bytes: Vec<u8>, exit: CaptureExit, truncated: bool) -> Captured {
    Captured {
        exit,
        stdout: bytes,
        stderr: vec![],
        duration: Duration::from_millis(1),
        truncated,
    }
}
#[test]
fn actual_security_fields_not_counterfeit_labels_control_inspect_admission() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let base = inspect(&b, false, 0);
    assert!(inspect_matches(&base, &cid(), &b).is_some());
    let changes = [
        ("/HostConfig/Privileged", json!(true)),
        ("/HostConfig/ReadonlyRootfs", json!(false)),
        ("/HostConfig/NetworkMode", json!("host")),
        ("/HostConfig/CapDrop", json!([])),
        ("/HostConfig/GroupAdd", json!(["0"])),
        ("/HostConfig/Devices", json!([{}])),
        ("/HostConfig/PidMode", json!("host")),
        ("/HostConfig/IpcMode", json!("host")),
        ("/HostConfig/SecurityOpt", json!([])),
        ("/HostConfig/Memory", json!(0)),
        ("/HostConfig/PidsLimit", json!(0)),
        ("/HostConfig/NanoCpus", json!(0)),
        ("/Config/User", json!("0:0")),
        ("/Config/Entrypoint", json!(["sh"])),
        ("/Config/Cmd", json!(["-c", "true"])),
        ("/Config/Env", json!(["SECRET=payload"])),
        ("/Mounts/0/RW", json!(true)),
        ("/Mounts/0/Destination", json!("/var/run/docker.sock")),
        ("/State/Dead", json!(true)),
    ];
    for (pointer, value) in changes {
        let mut changed = base.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        changed
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(inspect_matches(&changed, &cid(), &b).is_none(), "{pointer}");
    }
    let mut swapped = base;
    swapped["Mounts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Type":"bind","Destination":"/judge"}));
    assert!(inspect_matches(&swapped, &cid(), &b).is_none());
}
#[test]
fn immutable_image_admission_rejects_inherited_environment_hooks_and_candidate_judge() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let base = image(&b);
    assert!(image_matches(&base, &b));
    for (name, value) in [
        ("Env", json!(["TOKEN=secret"])),
        ("Entrypoint", json!(["cargo", "run"])),
        ("User", json!("root")),
        ("Volumes", json!({"/judge":{}})),
        ("OnBuild", json!(["RUN true"])),
        ("Labels", json!({"authoritative":"true"})),
        ("Healthcheck", json!({"Test":["CMD-SHELL","true"]})),
    ] {
        let mut changed = base.clone();
        changed["Config"][name] = value;
        assert!(!image_matches(&changed, &b), "{name}");
    }
}
#[test]
fn all_environment_permutations_admit_exact_unique_values_in_image_and_worker() {
    fn permutations(entries: &mut [&str], index: usize, visit: &mut impl FnMut(&[&str])) {
        if index == entries.len() {
            visit(entries);
            return;
        }
        for other in index..entries.len() {
            entries.swap(index, other);
            permutations(entries, index + 1, visit);
            entries.swap(index, other);
        }
    }
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let mut image = image(&b);
    let mut worker = inspect(&b, false, 0);
    let mut count = 0;
    let mut entries = ENV;
    permutations(&mut entries, 0, &mut |entries| {
        let env = json!(entries);
        assert!(env_matches(&env));
        image["Config"]["Env"] = env.clone();
        worker["Config"]["Env"] = env;
        assert!(image_matches(&image, &b));
        assert!(inspect_matches(&worker, &cid(), &b).is_some());
        count += 1;
    });
    assert_eq!(count, 5040);
    // Exact ordering actually observed after Docker CLI environment overrides.
    let reordered = json!([ENV[2], ENV[3], ENV[4], ENV[5], ENV[6], ENV[0], ENV[1]]);
    assert!(env_matches(&reordered));
}
#[test]
fn environment_duplicates_missing_unknown_secret_and_oversized_values_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let base = ENV.iter().map(|entry| json!(entry)).collect::<Vec<_>>();
    let mut duplicate = base.clone();
    duplicate[1] = json!(ENV[0]);
    let mut duplicate_key = base.clone();
    duplicate_key[1] = json!("PATH=/secret");
    let mut secret = base.clone();
    secret[1] = json!("TOKEN=SECRET_PAYLOAD");
    let mut unknown = base.clone();
    unknown[1] = json!("UNKNOWN=/scratch");
    let mut oversized = base.clone();
    oversized[1] = json!(format!("HOME={}", "x".repeat(32768)));
    let mut extra = base.clone();
    extra.push(json!("TOKEN=secret"));
    let mut missing = base.clone();
    missing.pop();
    let mut numeric = base;
    numeric[1] = json!(0);
    for env in [
        Value::Array(duplicate),
        Value::Array(duplicate_key),
        Value::Array(secret),
        Value::Array(unknown),
        Value::Array(oversized),
        Value::Array(extra),
        Value::Array(missing),
        Value::Array(numeric),
        Value::Null,
        json!({}),
    ] {
        assert!(!env_matches(&env));
        let mut image = image(&b);
        image["Config"]["Env"] = env.clone();
        let mut worker = inspect(&b, false, 0);
        worker["Config"]["Env"] = env;
        assert!(!image_matches(&image, &b));
        assert!(inspect_matches(&worker, &cid(), &b).is_none());
    }
}
#[test]
fn closed_arguments_have_bounded_private_namespace_and_never_pull_run_shell_or_remove_all() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let args = arguments(&b, &Action::Create);
    for pair in [
        ["--pull", "never"],
        ["--network", "none"],
        ["--cap-drop", "ALL"],
        ["--entrypoint", "/judge/aoe-harness"],
        ["--user", "65532:65532"],
    ] {
        assert!(args.windows(2).any(|window| window == pair));
    }
    assert_eq!(args.last().unwrap(), "fmt-check");
    assert!(!args.iter().any(|a| matches!(
        a.as_str(),
        "--privileged" | "--rm" | "--group-add" | "sh" | "-c"
    )));
    let cmd = command(&b, &Action::Create);
    assert_eq!(cmd.get_program(), config::PROGRAM);
    assert_eq!(cmd.get_current_dir(), Some(Path::new("/")));
    let hostargs = cmd
        .get_args()
        .map(|a| a.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        hostargs[0..4],
        [
            "--host",
            "unix:///run/aoeworld-supervisor/docker.sock",
            "--config",
            config::CLIENT
        ]
    );
    for action in [
        Action::Stop(cid()),
        Action::Kill(cid()),
        Action::Remove(cid()),
    ] {
        let args = arguments(&b, &action);
        assert!(args.iter().any(|a| a.contains(cid().value())));
        assert!(!args.iter().any(|a| a == "--all"));
    }
}
#[test]
fn duplicate_daemon_security_keys_are_rejected_instead_of_last_value_wins() {
    for packet in [
        br#"{"Config":{"User":"0","User":"65532:65532"}}"#.as_slice(),
        br#"{"HostConfig":{"Privileged":true,"Privileged":false}}"#.as_slice(),
    ] {
        assert!(serde_json::from_slice::<Value>(packet).is_ok());
        assert!(object(&capture(packet.to_vec(), CaptureExit::Success, false)).is_none());
    }
}
#[test]
fn worker_exit_is_not_docker_cli_exit_and_malformed_incomplete_recovery_is_rejected() {
    assert_eq!(
        exit_code(&capture(b"7\n".to_vec(), CaptureExit::Success, false)),
        Some(7)
    );
    for (bytes, exit, truncated) in [
        (b"0\n".to_vec(), CaptureExit::Failed(Some(7)), false),
        (b"0\n".to_vec(), CaptureExit::Success, true),
        (b"0\n7\n".to_vec(), CaptureExit::Success, false),
        (b"256\n".to_vec(), CaptureExit::Success, false),
        (vec![], CaptureExit::Success, false),
    ] {
        assert_eq!(exit_code(&capture(bytes, exit, truncated)), None);
    }
    assert_eq!(
        recover(&capture(vec![], CaptureExit::Success, false))
            .unwrap()
            .len(),
        0
    );
    let two = format!("{}\n{}\n", cid().value(), "b".repeat(64));
    assert!(recover(&capture(two.into_bytes(), CaptureExit::Success, false)).is_none());
    assert!(Cid::parse(b"short-name").is_none());
}
