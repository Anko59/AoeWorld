use std::{fs, path::Path, process::Command};

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    // A fixture owns its Git context. Explicit injection cases set overrides
    // AFTER this constructor; production refusal is never normalized away.
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
}

fn init(path: &Path) {
    let output = command("git")
        .arg("-C")
        .arg(path)
        .args(["init", "--quiet", "--template="])
        .output()
        .unwrap();
    assert!(output.status.success());
    let hooks = path.join(".git/hooks").display().to_string();
    let output = command("git")
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

    let output = command(env!("CARGO_BIN_EXE_aoe-harness"))
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
fn every_config_override_still_refuses_management_without_touching_canonical_hooks() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    let hook = root.path().join(".git/hooks/pre-commit");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    let sentinel = b"owned hook must survive poisoned caller\n";
    fs::write(&hook, sentinel).unwrap();
    for (name, value) in [
        ("GIT_CONFIG", "/dev/null"),
        ("GIT_CONFIG_COUNT", "0"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_PARAMETERS", ""),
        ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
        ("GIT_CONFIG_VALUE_0", "false"),
    ] {
        for operation in ["hooks-install", "hooks-check"] {
            let output = command(env!("CARGO_BIN_EXE_aoe-harness"))
                .current_dir(root.path())
                .arg(operation)
                .env(name, value)
                .output()
                .unwrap();
            assert!(!output.status.success(), "must refuse {name}: {operation}");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains("Git config override") && error.contains(name),
                "{error}"
            );
            assert_eq!(fs::read(&hook).unwrap(), sentinel);
            assert!(!root.path().join(".git/hooks/pre-push").exists());
        }
    }
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
    let output = command("git")
        .arg("-C")
        .arg(root.path())
        .args(["config", "core.hooksPath", &configured])
        .output()
        .unwrap();
    assert!(output.status.success());

    let masquerade = root.path().join(".git/hooks");
    for operation in ["hooks-install", "hooks-check"] {
        let output = command(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(root.path())
            .arg(operation)
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
