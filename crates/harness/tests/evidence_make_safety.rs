use std::{fs, path::Path, process::Command};

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn path_check(evidence: &Path) -> std::process::Output {
    evidence_mount_check(
        evidence,
        &workspace().join(".cache/cargo"),
        &workspace().join("target"),
    )
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
        .arg("harness-evidence-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", &endpoint)
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env_remove("DOCKER_HOST")
        .env("HARNESS_EVIDENCE_DIR", evidence.path())
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("Docker socket"));

    let remote_context_check = Command::new("make")
        .arg("--no-print-directory")
        .arg("harness-evidence-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", "ssh://builder.example")
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env_remove("DOCKER_HOST")
        .env("HARNESS_EVIDENCE_DIR", task_dir.path())
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
        .arg("harness-evidence-path-check")
        .current_dir(workspace())
        .env("PATH", &search_path)
        .env("HARNESS_TEST_DOCKER_ENDPOINT", &endpoint)
        .env("DOCKER_CONTEXT", "evidence-socket-test")
        .env("DOCKER_HOST", "tcp://builder.example:2376")
        .env("HARNESS_EVIDENCE_DIR", task_dir.path())
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
fn gate_run_mounts_canonical_evidence_for_a_valid_external_symlink() {
    use std::os::unix::fs::symlink;

    let external = tempfile::tempdir().unwrap();
    let evidence_real = external.path().join("evidence");
    fs::create_dir(&evidence_real).unwrap();
    let evidence_alias = external.path().join("evidence-link");
    symlink(&evidence_real, &evidence_alias).unwrap();
    let socket = external.path().join("docker.sock");
    fs::write(&socket, "fixture socket path").unwrap();

    let output = Command::new("make")
        .arg("--no-print-directory")
        .args(["-n", "gate-run"])
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", &evidence_alias)
        .env("DOCKER_HOST", format!("unix://{}", socket.display()))
        .env_remove("DOCKER_CONTEXT")
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
    assert!(commands.contains(&format!("-v {0}:{0}", evidence.display())));
    assert!(commands.contains(&format!("--output {}", evidence.display())));
    assert!(!commands.contains(&evidence_alias.display().to_string()));
}

#[cfg(unix)]
#[test]
fn make_path_checks_reject_evidence_overlapping_checkout_and_git_metadata() {
    use std::os::unix::fs::symlink;

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
        let result = path_check(&evidence);
        assert!(
            !result.status.success(),
            "writable evidence {} must not overlap checkout/Git mounts: {}",
            evidence.display(),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let external = tempfile::tempdir().unwrap();
    let alias = external.path().join("evidence-git-alias");
    symlink(&common, &alias).unwrap();
    let result = path_check(&alias);
    assert!(!result.status.success());
}

#[cfg(unix)]
#[test]
fn make_path_checks_do_not_bind_host_runtime_or_docker_socket_paths_as_evidence() {
    for path in ["/", "/run", "/var/run", "/proc", "/sys", "/dev"] {
        let evidence = Path::new(path);
        if evidence.is_dir() {
            let result = path_check(evidence);
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
        .arg("harness-evidence-path-check")
        .current_dir(workspace())
        .env("HARNESS_EVIDENCE_DIR", socket_root.path())
        .env("HARNESS_CARGO_CACHE", workspace().join(".cache/cargo"))
        .env("HARNESS_TARGET_CACHE", workspace().join("target"))
        .env("DOCKER_HOST", docker_host)
        .output()
        .expect("run custom Docker socket path check");
    assert!(!result.status.success());
}
