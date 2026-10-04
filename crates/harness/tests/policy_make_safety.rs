use std::{fs, path::Path, process::Command};

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn path_check(
    target: &str,
    evidence: &Path,
    input_var: &str,
    input: &Path,
) -> std::process::Output {
    Command::new("make")
        .arg("--no-print-directory")
        .arg(target)
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", evidence)
        .env(input_var, input)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .expect("run host-side Make path check")
}

fn evidence_mount_check(evidence: &Path, cargo: &Path, target: &Path) -> std::process::Output {
    Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-evidence-path-check")
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", evidence)
        .env("HARNESS_CARGO_CACHE", cargo)
        .env("HARNESS_TARGET_CACHE", target)
        .output()
        .expect("run evidence/cache Make path check")
}

#[test]
fn task_and_policy_facades_do_not_forward_host_credentials_or_docker_socket() {
    let evidence = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let task = external.path().join("task.json");
    let anchor = external.path().join("anchor.json");
    fs::write(&task, "{}").unwrap();
    fs::write(&anchor, "{}").unwrap();
    let output = Command::new("make")
        .arg("--no-print-directory")
        .args(["-n", "task-plan", "policy-prepare"])
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", evidence.path())
        .env("HARNESS_TASK_FILE", &task)
        .env("HARNESS_POLICY_ANCHOR", &anchor)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .env("GH_TOKEN", "fixture-must-not-be-forwarded")
        .env("GITHUB_TOKEN", "fixture-must-not-be-forwarded")
        .output()
        .expect("render safe task and policy Make targets");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = String::from_utf8(output.stdout).unwrap();
    assert!(commands.contains("--read-only"));
    assert!(commands.contains("--cap-drop=ALL"));
    assert!(commands.contains("--network none"));
    let root = fs::canonicalize(workspace()).unwrap();
    let root_mount = format!("-v {0}:{0}:ro", root.display());
    assert!(commands.contains(&root_mount));
    assert!(!commands.contains("/var/run/docker.sock"));
    assert!(!commands.contains("-e GH_TOKEN"));
    assert!(!commands.contains("-e GITHUB_TOKEN"));
    assert!(!commands.contains("fixture-must-not-be-forwarded"));
}

#[test]
fn make_evidence_is_disjoint_from_custom_writable_cache_mount_sources() {
    let caches = tempfile::tempdir().unwrap();
    let cargo = caches.path().join("cargo-cache");
    let target = caches.path().join("target-cache");
    fs::create_dir(&cargo).unwrap();
    fs::create_dir(&target).unwrap();
    let separate = tempfile::tempdir().unwrap();
    let valid = evidence_mount_check(separate.path(), &cargo, &target);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    let parent_overlap = evidence_mount_check(caches.path(), &cargo, &target);
    assert!(!parent_overlap.status.success());
    let cargo_child = cargo.join("evidence");
    fs::create_dir(&cargo_child).unwrap();
    let child_overlap = evidence_mount_check(&cargo_child, &cargo, &target);
    assert!(!child_overlap.status.success());
    let target_child = target.join("evidence");
    fs::create_dir(&target_child).unwrap();
    let target_overlap = evidence_mount_check(&target_child, &cargo, &target);
    assert!(!target_overlap.status.success());
}

#[test]
fn gate_run_uses_only_explicitly_validated_root_git_cache_and_evidence_mounts() {
    let evidence = tempfile::tempdir().unwrap();
    let cargo = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let docker = tempfile::tempdir().unwrap();
    let socket = docker.path().join("docker.sock");
    fs::write(&socket, "fixture socket path").unwrap();
    let output = Command::new("make")
        .arg("--no-print-directory")
        .args(["ROOT_MOUNTS=-v /tmp:/tmp", "-n", "gate-run"])
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", evidence.path())
        .env("HARNESS_CARGO_CACHE", cargo.path())
        .env("HARNESS_TARGET_CACHE", target.path())
        .env("DOCKER_HOST", format!("unix://{}", socket.display()))
        .env_remove("DOCKER_CONTEXT")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = String::from_utf8(output.stdout).unwrap();
    let evidence = fs::canonicalize(evidence.path()).unwrap();
    let cargo = fs::canonicalize(cargo.path()).unwrap();
    let target = fs::canonicalize(target.path()).unwrap();
    let socket = fs::canonicalize(socket).unwrap();
    let root = fs::canonicalize(workspace()).unwrap();
    assert!(commands.contains(&format!("-v {0}:{0}", evidence.display())));
    assert!(commands.contains(&format!("--output {}", evidence.display())));
    assert!(commands.contains(&format!("-v {}:/var/run/docker.sock", socket.display())));
    assert!(commands.contains(&format!(
        "-v {}:{}/.cache/cargo",
        cargo.display(),
        root.display()
    )));
    assert!(commands.contains(&format!(
        "-v {}:{}/target",
        target.display(),
        root.display()
    )));
    assert!(!commands.contains("-v /tmp:/tmp"));
}

#[cfg(unix)]
#[test]
fn make_evidence_check_accounts_for_the_active_docker_context_socket() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().unwrap();
    let evidence = tempfile::tempdir().unwrap();
    let task_dir = tempfile::tempdir().unwrap();
    let task = task_dir.path().join("task.json");
    fs::write(&task, "{}").unwrap();
    let socket = evidence.path().join("context-docker.sock");
    fs::write(&socket, "fixture socket path").unwrap();
    let endpoint = format!("unix://{}", socket.display());

    let docker_dir = fixture.path().join("bin");
    fs::create_dir(&docker_dir).unwrap();
    let fake_docker = docker_dir.join("docker");
    fs::write(
        &fake_docker,
        "#!/bin/sh\n[ \"$1\" = context ] && [ \"$2\" = inspect ] || exit 2\n[ \"$DOCKER_CONTEXT\" = evidence-socket-test ] || exit 3\nprintf '%s\\n' \"$HARNESS_TEST_DOCKER_ENDPOINT\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake_docker, fs::Permissions::from_mode(0o755)).unwrap();
    let mut search_path = vec![docker_dir];
    search_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let search_path = std::env::join_paths(search_path).unwrap();

    let check = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-task-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", &endpoint)
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env_remove("DOCKER_HOST")
        .env("HARNESS_EVIDENCE_DIR", evidence.path())
        .env("HARNESS_TASK_FILE", &task)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("Docker socket"));

    let remote_context_check = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-task-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", "ssh://builder.example")
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env_remove("DOCKER_HOST")
        .env("HARNESS_EVIDENCE_DIR", task_dir.path())
        .env("HARNESS_TASK_FILE", &task)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .unwrap();
    assert!(!remote_context_check.status.success());
    assert!(
        String::from_utf8_lossy(&remote_context_check.stderr).contains("local Unix Docker context")
    );

    let remote_host_check = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-task-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", &endpoint)
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env("DOCKER_HOST", "tcp://builder.example:2376")
        .env("HARNESS_EVIDENCE_DIR", task_dir.path())
        .env("HARNESS_TASK_FILE", &task)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .unwrap();
    assert!(!remote_host_check.status.success());
    assert!(
        String::from_utf8_lossy(&remote_host_check.stderr).contains("local Unix Docker endpoint")
    );
}

#[cfg(unix)]
#[test]
fn make_facades_mount_canonical_paths_for_valid_external_symlink_inputs_and_evidence() {
    use std::os::unix::fs::symlink;

    let external = tempfile::tempdir().unwrap();
    let evidence_real = external.path().join("evidence");
    fs::create_dir(&evidence_real).unwrap();
    let evidence_alias = external.path().join("evidence-link");
    symlink(&evidence_real, &evidence_alias).unwrap();
    let task_real = external.path().join("task.json");
    let anchor_real = external.path().join("anchor.json");
    fs::write(&task_real, "{}").unwrap();
    fs::write(&anchor_real, "{}").unwrap();
    let task_alias = external.path().join("task-link.json");
    let anchor_alias = external.path().join("anchor-link.json");
    symlink(&task_real, &task_alias).unwrap();
    symlink(&anchor_real, &anchor_alias).unwrap();

    let output = Command::new("make")
        .arg("--no-print-directory")
        .args(["-n", "task-plan", "policy-prepare"])
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", &evidence_alias)
        .env("HARNESS_TASK_FILE", &task_alias)
        .env("HARNESS_POLICY_ANCHOR", &anchor_alias)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .expect("render canonical Make bind mounts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = String::from_utf8(output.stdout).unwrap();
    let evidence = fs::canonicalize(&evidence_real).unwrap();
    let task = fs::canonicalize(&task_real).unwrap();
    let anchor = fs::canonicalize(&anchor_real).unwrap();
    assert!(commands.contains(&format!("-v {0}:{0}", evidence.display())));
    assert!(commands.contains(&format!("--output {}", evidence.display())));
    assert!(commands.contains(&format!("-v {0}:{0}:ro", task.display())));
    assert!(commands.contains(&format!("--task {}", task.display())));
    assert!(commands.contains(&format!("-v {0}:{0}:ro", anchor.display())));
    assert!(commands.contains(&format!("--anchor {}", anchor.display())));
    assert!(!commands.contains(&evidence_alias.display().to_string()));
    assert!(!commands.contains(&task_alias.display().to_string()));
    assert!(!commands.contains(&anchor_alias.display().to_string()));
}

#[cfg(unix)]
#[test]
fn make_path_checks_reject_host_symlink_aliases_to_reserved_evidence() {
    use std::os::unix::fs::symlink;

    let evidence = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    for (target, input_var, input_name, output_name) in [
        (
            "harness-task-path-check",
            "HARNESS_TASK_FILE",
            "task.json",
            "task-plan.json",
        ),
        (
            "harness-policy-path-check",
            "HARNESS_POLICY_ANCHOR",
            "anchor.json",
            "preparation.json",
        ),
    ] {
        let reserved = evidence.path().join(output_name);
        let sentinel = b"user data; preserve me\n";
        fs::write(&reserved, sentinel).unwrap();
        let alias = external.path().join(input_name);
        symlink(&reserved, &alias).unwrap();
        let result = path_check(target, evidence.path(), input_var, &alias);
        assert!(
            !result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(&reserved).unwrap(), sentinel);
        fs::remove_file(&alias).unwrap();
        fs::remove_file(&reserved).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn make_path_checks_reject_evidence_inside_or_aliasing_readonly_source_mounts() {
    use std::os::unix::fs::symlink;

    let external = tempfile::tempdir().unwrap();
    let task = external.path().join("task.json");
    fs::write(&task, "{}").unwrap();
    let common = Command::new("git")
        .arg("-C")
        .arg(workspace())
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .unwrap();
    assert!(common.status.success());
    let common = std::path::PathBuf::from(String::from_utf8(common.stdout).unwrap().trim());
    let mut evidence_roots = vec![workspace().to_path_buf(), common.clone()];
    if let Some(parent) = workspace().parent() {
        evidence_roots.push(parent.to_path_buf());
    }
    if let Some(parent) = common.parent() {
        evidence_roots.push(parent.to_path_buf());
    }
    let objects = common.join("objects");
    if objects.is_dir() {
        evidence_roots.push(objects);
    }
    for evidence in evidence_roots.into_iter().filter(|path| path.is_dir()) {
        let result = path_check(
            "harness-task-path-check",
            &evidence,
            "HARNESS_TASK_FILE",
            &task,
        );
        assert!(
            !result.status.success(),
            "writable evidence {} must not overlap read-only checkout/Git mounts: {}",
            evidence.display(),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let alias = external.path().join("evidence-git-alias");
    symlink(&common, &alias).unwrap();
    let result = path_check(
        "harness-task-path-check",
        &alias,
        "HARNESS_TASK_FILE",
        &task,
    );
    assert!(!result.status.success());
}

#[cfg(unix)]
#[test]
fn make_path_checks_do_not_bind_host_runtime_or_docker_socket_paths_as_evidence() {
    let external = tempfile::tempdir().unwrap();
    let task = external.path().join("task.json");
    fs::write(&task, "{}").unwrap();
    for path in ["/", "/run", "/var/run", "/proc", "/sys", "/dev"] {
        let evidence = Path::new(path);
        if evidence.is_dir() {
            let result = path_check(
                "harness-task-path-check",
                evidence,
                "HARNESS_TASK_FILE",
                &task,
            );
            assert!(
                !result.status.success(),
                "host runtime path {path} must not be writable evidence: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    let socket_root = tempfile::tempdir().unwrap();
    let socket = socket_root.path().join("docker.sock");
    fs::write(&socket, "fixture socket path").unwrap();
    let docker_host = format!("unix://{}", socket.display());
    let result = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-task-path-check")
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", socket_root.path())
        .env("HARNESS_TASK_FILE", &task)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .env("DOCKER_HOST", docker_host)
        .output()
        .expect("run custom Docker socket path check");
    assert!(!result.status.success());
}

#[cfg(unix)]
#[test]
fn make_path_checks_reject_dangling_host_symlinks_without_creating_targets() {
    use std::os::unix::fs::symlink;

    let evidence = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    for (target, input_var, input_name, output_name) in [
        (
            "harness-task-path-check",
            "HARNESS_TASK_FILE",
            "dangling-task.json",
            "task-plan.json",
        ),
        (
            "harness-policy-path-check",
            "HARNESS_POLICY_ANCHOR",
            "dangling-anchor.json",
            "preparation.json",
        ),
    ] {
        let reserved = evidence.path().join(output_name);
        let alias = external.path().join(input_name);
        symlink(&reserved, &alias).unwrap();
        let result = path_check(target, evidence.path(), input_var, &alias);
        assert!(!result.status.success());
        assert!(!reserved.exists(), "dangling alias target must stay absent");
    }
}

#[test]
fn supervisor_model_path_check_rejects_evidence_inside_readonly_input_directory() {
    let supervisor = tempfile::tempdir().unwrap();
    let evidence = supervisor.path().join("evidence");
    fs::create_dir(&evidence).unwrap();
    let requirements = supervisor.path().join("requirements.json");
    fs::write(&requirements, "{}").unwrap();
    let result = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-supervisor-path-check")
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", &evidence)
        .env("HARNESS_SUPERVISOR_DIR", supervisor.path())
        .env("HARNESS_SUPERVISOR_REQUIREMENTS", requirements)
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .expect("run supervisor-model path check");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("disjoint from supervisor input directory")
    );
}
