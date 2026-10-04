use std::{fs, path::Path, process::Command};

fn init(path: &Path) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["init", "--quiet", "--template="])
        .output()
        .unwrap();
    assert!(output.status.success());
    let hooks = path.join(".git/hooks").display().to_string();
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["config", "core.hooksPath", &hooks])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn hook_installer_ignores_foreign_git_directory_environment() {
    let root = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    init(root.path());
    init(foreign.path());
    let foreign_hook = foreign.path().join(".git/hooks/pre-commit");
    fs::create_dir_all(foreign_hook.parent().unwrap()).unwrap();
    let sentinel = b"shared hook; do not replace\n";
    fs::write(&foreign_hook, sentinel).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root.path())
        .arg("hooks-install")
        .env("GIT_DIR", foreign.path().join(".git"))
        .env("GIT_WORK_TREE", foreign.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&foreign_hook).unwrap(), sentinel);
    assert_eq!(
        fs::read(root.path().join(".git/hooks/pre-commit")).unwrap(),
        b"#!/bin/sh\nexec make pre-commit\n"
    );
}

#[test]
fn hook_installer_rejects_config_injection_that_masks_shared_hooks() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    init(root.path());
    let shared = external.path().join("shared-hooks");
    fs::create_dir_all(&shared).unwrap();
    let external_hook = shared.join("pre-commit");
    let sentinel = b"shared hook; do not replace\n";
    fs::write(&external_hook, sentinel).unwrap();
    let configured = shared.display().to_string();
    let output = Command::new("git")
        .arg("-C")
        .arg(root.path())
        .args(["config", "core.hooksPath", &configured])
        .output()
        .unwrap();
    assert!(output.status.success());

    let masquerade = root.path().join(".git/hooks");
    for command in ["hooks-install", "hooks-check"] {
        let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(root.path())
            .arg(command)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.hooksPath")
            .env("GIT_CONFIG_VALUE_0", &masquerade)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("GIT_CONFIG"));
        assert_eq!(fs::read(&external_hook).unwrap(), sentinel);
        assert!(!root.path().join(".git/hooks/pre-commit").exists());
    }
}
