use std::{fs, os::unix::fs::MetadataExt, process::Command};

mod make_fixture;

fn job_environment(target: &str, external: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let workspace = workspace();
    let binary = external.join("target/debug/aoe-harness");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::create_dir_all(external.join("cargo")).unwrap();
    fs::create_dir_all(workspace.path().join("bin")).unwrap();
    fs::write(&binary, "#!/bin/sh\nprintf '%s|%s|%s|%s\\n' \"$1\" \"$TMPDIR\" \"$HARNESS_CARGO_CACHE\" \"$HARNESS_TARGET_CACHE\"\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let docker = workspace.path().join("bin/docker");
    fs::write(&docker, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let output = make(&workspace)
        .arg(target)
        .arg(format!("TMPDIR={}", external.join("temp").display()))
        .arg(format!(
            "HARNESS_CARGO_CACHE={}",
            external.join("cargo").display()
        ))
        .arg(format!(
            "HARNESS_TARGET_CACHE={}",
            external.join("target").display()
        ))
        .env(
            "PATH",
            format!(
                "{}:{}",
                workspace.path().join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find(|line| line.starts_with("job|"))
        .unwrap()
        .to_owned()
}

#[test]
fn host_job_wrapper_preserves_external_tmpdir_and_caches_for_nested_make() {
    let external = tempfile::tempdir().unwrap();
    for target in ["test-unit", "test-harness", "test-wasm"] {
        let actual = job_environment(target, external.path());
        assert_eq!(
            actual,
            format!(
                "job|{}|{}|{}",
                external.path().join("temp").display(),
                external.path().join("cargo").display(),
                external.path().join("target").display()
            )
        );
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
    make_fixture::workspace_named(prefix)
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
    // Exercise the steps directly: the host wrapper's admission is audited separately.
    let result = make(&workspace)
        .args([
            "test-harness-focused-steps",
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

#[test]
fn cargo_cache_default_is_the_repository_common_directory() {
    let workspace = workspace();
    let result = make(&workspace)
        .args(["--dry-run", "ci-select"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(
        output.contains(&mount(
            &workspace
                .path()
                .join(".git/aoe-cargo")
                .display()
                .to_string(),
            &workspace.path().join(".cache/cargo").display().to_string(),
        )),
        "{output}"
    );
}

#[test]
fn linked_worktrees_mount_the_same_read_write_cargo_cache_for_every_runner() {
    let workspace = workspace();
    for args in [vec!["add", "."], vec!["commit", "--quiet", "-m", "fixture"]] {
        let result = Command::new("git")
            .current_dir(workspace.path())
            .env("GIT_AUTHOR_NAME", "Cache Fixture")
            .env("GIT_AUTHOR_EMAIL", "cache@example.invalid")
            .env("GIT_COMMITTER_NAME", "Cache Fixture")
            .env("GIT_COMMITTER_EMAIL", "cache@example.invalid")
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
    }
    let linked_parent = tempfile::tempdir().unwrap();
    let linked = linked_parent.path().join("linked");
    let result = Command::new("git")
        .current_dir(workspace.path())
        .args(["worktree", "add", "--quiet", "--detach"])
        .arg(&linked)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let shared = workspace.path().join(".git/aoe-cargo");
    for root in [workspace.path(), linked.as_path()] {
        for target in [
            "lint",
            "deny",
            "test-harness",
            "map-source-qualify",
            "dev",
            "agent-hook-build",
        ] {
            let result = make(&workspace)
                .current_dir(root)
                .args(["--dry-run", target])
                .output()
                .unwrap();
            assert!(result.status.success(), "{target}: {result:?}");
            let output = String::from_utf8(result.stdout).unwrap();
            assert!(
                output.contains(&format!("CARGO_HOME={}", shared.display())),
                "{target}: {output}"
            );
            assert!(
                !output.contains(&format!("CARGO_HOME={}/.cache/cargo", root.display())),
                "{target}: {output}"
            );
            assert!(
                output.contains(&mount(
                    &shared.display().to_string(),
                    &root.join(".cache/cargo").display().to_string(),
                )),
                "{target}: {output}"
            );
            assert!(
                !output.contains(&format!("source={}/.cache/cargo", root.display())),
                "{output}"
            );
        }
    }
}

#[test]
fn cache_mount_options_keep_spaces_in_cargo_and_target_paths() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = workspace();
    let bin = workspace.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let cargo = workspace.path().join("cargo cache");
    let target = workspace.path().join("target cache");
    fs::create_dir(&cargo).unwrap();
    fs::create_dir(&target).unwrap();
    fs::create_dir_all(target.join("debug")).unwrap();
    let binary = target.join("debug/aoe-harness");
    fs::write(&binary, "fixture binary").unwrap();
    fs::File::open(binary)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(86400))
        .unwrap();
    for recipe in ["ci-select", "job-bootstrap", "test-harness-focused-steps"] {
        let result = make(&workspace)
            .arg(recipe)
            .arg(format!("HARNESS_CARGO_CACHE={}", cargo.display()))
            .arg(format!("HARNESS_TARGET_CACHE={}", target.display()))
            .env("HARNESS_TEST_FILTER", "cache_mount")
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        assert!(result.status.success(), "{recipe}: {result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        for variable in ["HARNESS_CARGO_CACHE", "CARGO_HOME"] {
            assert!(
                output.contains(&format!("<{variable}={}>", cargo.display())),
                "{recipe}: {output}"
            );
        }
        assert!(
            output.contains(&format!(
                "<type=bind,\"source={0}\",\"target={0}\">",
                cargo.display()
            )),
            "{recipe}: {output}"
        );
    }
}

#[test]
fn bootstrap_remembers_a_successful_noop_cargo_build_for_newer_test_inputs() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, SystemTime},
    };
    let workspace = workspace();
    let root = workspace.path();
    let binary = root.join("target/debug/aoe-harness");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "already built command").unwrap();
    fs::File::open(&binary)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(86400))
        .unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let docker = root.join("bin/docker");
    fs::write(&docker, "#!/bin/sh\nprintf 'build\\n' >> \"$BOOT_LOG\"\n").unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let log = root.join("bootstrap.log");
    for _ in 0..2 {
        let result = make(&workspace)
            .arg("job-bootstrap")
            .env("BOOT_LOG", &log)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.join("bin").display(),
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
    }
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 1);
}
