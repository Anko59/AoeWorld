use std::{fs, os::unix::fs::MetadataExt, process::Command};

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
    assert!(default.contains(&format!("-v {root}:{root}")));
    assert!(!default.contains(&format!("-v {root}/target:{root}/target")));
    assert!(!default.contains(&format!("-v {root}/.cache/cargo:{root}/.cache/cargo")));
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
    assert!(projected.contains(&format!("-v {}:{root}/.cache/cargo", cargo.display())));
    assert!(projected.contains(&format!("-v {}:{root}/target", target.display())));
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
    let workspace = tempfile::tempdir().unwrap();
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
