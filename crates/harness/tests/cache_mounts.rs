use std::{fs, process::Command};

#[test]
fn fresh_default_workspace_does_not_precreate_root_owned_cache_mounts() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(
        workspace.path().join("Makefile"),
        include_str!("../../../Makefile"),
    )
    .unwrap();
    // The Makefile includes its agent-runtime targets from make/.
    fs::create_dir(workspace.path().join("make")).unwrap();
    fs::write(
        workspace.path().join("make/agents.mk"),
        include_str!("../../../make/agents.mk"),
    )
    .unwrap();
    fs::write(
        workspace.path().join("make/ship.mk"),
        include_str!("../../../make/ship.mk"),
    )
    .unwrap();
    fs::write(
        workspace.path().join("make/qualification.mk"),
        include_str!("../../../make/qualification.mk"),
    )
    .unwrap();
    let render = |extra: &[String]| {
        let result = Command::new("make")
            .current_dir(workspace.path())
            .env_remove("HARNESS_CARGO_CACHE")
            .env_remove("HARNESS_TARGET_CACHE")
            .env_remove("MAKEFLAGS")
            .env_remove("MFLAGS")
            .env_remove("GNUMAKEFLAGS")
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
    let external = tempfile::tempdir().unwrap();
    let cargo = external.path().join("cargo");
    let target = external.path().join("target");
    fs::create_dir(&cargo).unwrap();
    fs::create_dir(&target).unwrap();
    let projected = render(&[
        format!("HARNESS_CARGO_CACHE={}", cargo.display()),
        format!("HARNESS_TARGET_CACHE={}", target.display()),
    ]);
    assert!(projected.contains(&format!("-v {}:{root}/.cache/cargo", cargo.display())));
    assert!(projected.contains(&format!("-v {}:{root}/target", target.display())));
}
