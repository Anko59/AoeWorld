use std::{fs, os::unix::fs::MetadataExt, process::Command};

fn slot_dry_run(target: &str, external: &std::path::Path) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("make")
        .current_dir(root)
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("GNUMAKEFLAGS")
        .args(["--dry-run", target])
        .arg(format!("TMPDIR={}", external.join("temp").display()))
        .arg(format!(
            "HARNESS_CARGO_CACHE={}",
            external.join("cargo").display()
        ))
        .arg(format!(
            "HARNESS_TARGET_CACHE={}",
            external.join("target").display()
        ))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find(|line| line.contains("aoe-harness slot "))
        .expect("slot wrapper command")
        .to_owned()
}

#[test]
fn slot_wrapper_mounts_external_tmpdir_at_its_host_path_for_nested_make() {
    let external = tempfile::tempdir().unwrap();
    for target in ["test-unit", "test-harness", "test-wasm"] {
        let command = slot_dry_run(target, external.path());
        let path = external.path().join("temp");
        assert!(
            command.contains(&format!("-v {}:{}", path.display(), path.display())),
            "{command}"
        );
    }
}

#[test]
fn slot_wrapper_forwards_external_caches_to_nested_make() {
    let external = tempfile::tempdir().unwrap();
    for target in ["test-unit", "test-harness", "test-wasm"] {
        let command = slot_dry_run(target, external.path());
        for (variable, directory) in [
            ("HARNESS_CARGO_CACHE", "cargo"),
            ("HARNESS_TARGET_CACHE", "target"),
        ] {
            let value = external.path().join(directory);
            assert!(
                command.contains(&format!("-e {variable}={}", value.display())),
                "nested Make loses {variable}: {command}"
            );
        }
    }
}

#[test]
fn fresh_default_workspace_does_not_precreate_root_owned_cache_mounts() {
    let workspace = workspace();
    let render = |extra: &[String]| {
        let result = make(&workspace)
            .args(["--dry-run", "ci-select"])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    let root = workspace.path().display();
    let default = render(&[]);
    assert!(default.contains(&mount(&root.to_string(), &root.to_string())));
    assert!(!default.contains(&mount(&format!("{root}/target"), &format!("{root}/target"))));
    assert!(!default.contains(&mount(
        &format!("{root}/.cache/cargo"),
        &format!("{root}/.cache/cargo")
    )));
    assert!(!workspace.path().join("target").exists());
    assert!(!workspace.path().join(".cache/cargo").exists());
}

/// With `HARNESS_CARGO_CACHE` or `HARNESS_TARGET_CACHE` pointing outside the
/// checkout, the facade mounts them at `<root>/target` and
/// `<root>/.cache/cargo`; Docker would create missing mount targets as root
/// (#115), so the invoking user owns them before any `docker run`.
#[test]
fn overridden_caches_create_their_mount_targets_as_the_invoking_user() {
    let workspace = workspace();
    let external = tempfile::tempdir().unwrap();
    let cargo = external.path().join("cargo");
    let target = external.path().join("target");
    fs::create_dir(&cargo).unwrap();
    fs::create_dir(&target).unwrap();
    let root = workspace.path().display();
    let projected = make(&workspace)
        .args(["--dry-run", "ci-select"])
        .args([format!("HARNESS_CARGO_CACHE={}", cargo.display())])
        .args([format!("HARNESS_TARGET_CACHE={}", target.display())])
        .output()
        .unwrap();
    let projected = String::from_utf8(projected.stdout).unwrap();
    assert!(projected.contains(&mount(
        &cargo.display().to_string(),
        &format!("{root}/.cache/cargo")
    )));
    assert!(projected.contains(&mount(
        &target.display().to_string(),
        &format!("{root}/target")
    )));
    fs::create_dir(workspace.path().join("bin")).unwrap();
    let docker = workspace.path().join("bin/docker");
    fs::write(
        &docker,
        "#!/bin/sh\ntest -d target && test -d .cache/cargo\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let result = make(&workspace)
        .args(["ci-select"])
        .env(
            "PATH",
            format!(
                "{}:{}",
                workspace.path().join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .args([format!("HARNESS_CARGO_CACHE={}", cargo.display())])
        .args([format!("HARNESS_TARGET_CACHE={}", target.display())])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for mount_point in ["target", ".cache/cargo"] {
        let path = workspace.path().join(mount_point);
        assert!(path.is_dir(), "{mount_point} was not created before docker");
        let owner = path.metadata().unwrap().uid().to_string();
        assert_eq!(
            owner,
            nix::unistd::getuid().to_string(),
            "{mount_point} is not owned by the invoking user (docker would create it root-owned)"
        );
        // The failure the root-owned mount point caused: a user process could
        // not write into it.
        fs::write(path.join(".mount-point-probe"), b"").unwrap();
        fs::remove_file(path.join(".mount-point-probe")).unwrap();
    }
}

fn workspace() -> tempfile::TempDir {
    workspace_named("cache-mount-")
}

fn workspace_named(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    fs::write(
        workspace.path().join("Makefile"),
        include_str!("../../../Makefile"),
    )
    .unwrap();
    // The Makefile includes its agent-runtime targets from make/.
    fs::create_dir(workspace.path().join("make")).unwrap();
    for included in [
        ("make/agents.mk", include_str!("../../../make/agents.mk")),
        ("make/ship.mk", include_str!("../../../make/ship.mk")),
        (
            "make/qualification.mk",
            include_str!("../../../make/qualification.mk"),
        ),
        ("make/heavy.mk", include_str!("../../../make/heavy.mk")),
    ] {
        fs::write(workspace.path().join(included.0), included.1).unwrap();
    }
    workspace
}

fn make(workspace: &tempfile::TempDir) -> Command {
    let mut command = Command::new("make");
    command.current_dir(workspace.path());
    for name in ["HARNESS_CARGO_CACHE", "HARNESS_TARGET_CACHE"] {
        command.env_remove(name);
    }
    for name in ["MAKEFLAGS", "MFLAGS", "GNUMAKEFLAGS"] {
        command.env_remove(name);
    }
    command
}

#[test]
fn showcase_colon_checkout_uses_an_unambiguous_root_bind() {
    let workspace = workspace_named("showcase:root-");
    let result = make(&workspace)
        .args(["--dry-run", "ci-select"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let root = workspace.path().display();
    assert!(
        output.contains(&format!(
            "--mount 'type=bind,\"source={root}\",\"target={root}\"'"
        )),
        "{output}"
    );
    assert!(!output.contains(&format!("-v {root}:{root}")), "{output}");
}

#[test]
fn showcase_video_probe_mounts_a_colon_parent_without_volume_syntax() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = workspace_named("showcase:probe-");
    let tools = tempfile::tempdir().unwrap();
    let bin = tools.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n").unwrap();
    fs::set_permissions(docker, fs::Permissions::from_mode(0o755)).unwrap();
    let video = workspace.path().join("part:one/$literal/video.webm");
    fs::create_dir_all(video.parent().unwrap()).unwrap();
    fs::write(&video, "fixture").unwrap();
    let result = make(&workspace)
        .arg("video-probe")
        .env("SHIP_VIDEO", &video)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("<--mount>"), "{output}");
    assert!(
        output.contains(&format!(
            "<type=bind,\"source={}\",\"target=/video\",readonly>",
            video.display()
        )),
        "{output}"
    );
    assert!(!output.contains("<-v>"), "{output}");
}

fn mount(source: &str, target: &str) -> String {
    format!("--mount 'type=bind,\"source={source}\",\"target={target}\"'")
}

#[test]
fn focused_test_filter_reaches_docker_as_literal_data() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = workspace();
    let tools = tempfile::tempdir().unwrap();
    let docker = tools.path().join("docker");
    fs::write(&docker, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let filter = "can't match $literal";
    let result = make(&workspace)
        .args([
            "test-harness-focused",
            &format!("HARNESS_TEST_FILTER={filter}"),
        ])
        .env(
            "PATH",
            format!(
                "{}:{}",
                tools.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains(&format!("<{filter}>"))
    );
}
