use super::*;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run fixture Git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("Git output")
}

fn repository() -> TempDir {
    let directory = tempfile::tempdir().expect("fixture directory");
    git(directory.path(), &["init", "--quiet"]);
    directory
}

#[test]
fn install_writes_exact_dispatchers_and_is_idempotent() {
    let root = repository();
    assert!(check(root.path()).is_err(), "missing hooks must fail");
    install(root.path()).expect("install");
    check(root.path()).expect("check correct hooks");
    for (name, expected) in HOOKS {
        let path = hook_path(root.path(), name).expect("hook path");
        assert_eq!(fs::read(&path).expect("hook bytes"), expected);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o7777,
                0o755
            );
        }
    }
    install(root.path()).expect("idempotent reinstall");
    check(root.path()).expect("reinstalled hooks");
}

#[test]
fn rejects_commented_unreachable_and_malformed_dispatchers() {
    let root = repository();
    install(root.path()).expect("install");
    for (name, expected) in HOOKS {
        let path = hook_path(root.path(), name).expect("hook path");
        let canonical = String::from_utf8(expected.to_vec()).unwrap();
        let dispatch = canonical.lines().nth(1).unwrap();
        let malformed = [
            format!("#!/bin/sh\n# {dispatch}\n"),
            format!("#!/bin/sh\nexit 0\n{dispatch}\n"),
            format!("#!/bin/sh\nif false; then\n{dispatch}\nfi\n"),
            format!("#!/bin/sh\n: '{dispatch}'\n"),
            format!("#!/bin/sh\n{dispatch} || true\n"),
            format!("#!/bin/sh\n{dispatch}\nexit 0\n"),
            format!("{dispatch}\n"),
            canonical.replace('\n', "\r\n"),
            canonical.trim_end().to_owned(),
            format!("{canonical}\0"),
            "#!/bin/sh\nexec make wrong-gate\n".into(),
            String::new(),
        ];
        for content in malformed {
            fs::write(&path, &content).expect("alter hook");
            let error = check(root.path()).expect_err("reject altered dispatcher");
            assert!(error.to_string().contains(name), "{error}: {content:?}");
        }
        fs::write(&path, [0xff]).expect("invalid UTF-8 hook");
        assert!(check(root.path()).is_err());
        install(root.path()).expect("repair malformed hook");
        check(root.path()).expect("repaired hooks");
    }
}

#[cfg(unix)]
#[test]
fn rejects_unsafe_permissions_and_repairs_regular_files() {
    use std::os::unix::fs::PermissionsExt;
    let root = repository();
    install(root.path()).expect("install");
    for (name, _) in HOOKS {
        let path = hook_path(root.path(), name).expect("hook path");
        for mode in [0o644, 0o744, 0o777, 0o4755, 0o2755] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert!(
                check(root.path()).is_err(),
                "must reject {name} mode {mode:o}"
            );
        }
        install(root.path()).expect("repair permissions");
        check(root.path()).expect("repaired permissions");
    }
}

#[cfg(unix)]
#[test]
fn refuses_symlink_hooks_without_overwriting_the_target() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = repository();
    let target = root.path().join("do-not-overwrite");
    fs::write(&target, HOOKS[0].1).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    let path = hook_path(root.path(), "pre-commit").unwrap();
    symlink(&target, &path).unwrap();
    assert_eq!(hook_path(root.path(), "pre-commit").unwrap(), path);
    assert!(check(root.path()).is_err());
    assert!(install(root.path()).is_err());
    assert_eq!(fs::read(target).unwrap(), HOOKS[0].1);
}

#[test]
fn rejects_directory_hook_and_non_repository() {
    let root = repository();
    fs::create_dir(hook_path(root.path(), "pre-commit").unwrap()).unwrap();
    assert!(install(root.path()).is_err());
    assert!(check(root.path()).is_err());
    let non_repository = tempfile::tempdir().unwrap();
    assert!(install(non_repository.path()).is_err());
    assert!(check(non_repository.path()).is_err());
}

#[test]
fn respects_configured_hook_paths_and_path_whitespace() {
    let root = repository();
    for configured in [
        "custom hooks ".to_owned(),
        root.path().join("absolute hooks ").display().to_string(),
    ] {
        git(root.path(), &["config", "core.hooksPath", &configured]);
        install(root.path()).expect("configured hooks install");
        check(root.path()).expect("configured hooks check");
        let base = root.path().join(&configured);
        for (name, expected) in HOOKS {
            assert_eq!(hook_path(root.path(), name).unwrap(), base.join(name));
            assert_eq!(fs::read(base.join(name)).unwrap(), expected);
        }
    }
    assert!(!root.path().join(".git/hooks/pre-commit").exists());
}

#[test]
fn linked_worktree_uses_common_hooks_not_git_pointer_directory() {
    let common = repository();
    let fixture = tempfile::tempdir().unwrap();
    let linked = fixture.path().join("linked checkout");
    fs::create_dir(&linked).unwrap();
    let admin = common.path().join(".git/worktrees/linked");
    fs::create_dir_all(&admin).unwrap();
    // Build Git's documented linked-worktree metadata without making a commit
    // or branch. This also works on Git versions that cannot add unborn trees.
    fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    fs::write(admin.join("commondir"), "../..\n").unwrap();
    fs::write(
        admin.join("gitdir"),
        linked.join(".git").display().to_string(),
    )
    .unwrap();
    fs::write(
        admin.join("HEAD"),
        fs::read(common.path().join(".git/HEAD")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        PathBuf::from(
            git(
                &linked,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"]
            )
            .trim()
        ),
        common.path().join(".git")
    );
    install(&linked).expect("install through linked checkout");
    check(&linked).expect("check linked checkout");
    check(common.path()).expect("check common checkout");
    for (name, expected) in HOOKS {
        let path = hook_path(&linked, name).unwrap();
        assert_eq!(path, common.path().join(".git/hooks").join(name));
        assert_eq!(fs::read(path).unwrap(), expected);
        assert!(!admin.join("hooks").join(name).exists());
    }
}

#[cfg(unix)]
#[test]
fn dispatchers_preserve_gate_arguments_and_failure_status() {
    use std::os::unix::fs::PermissionsExt;
    let root = repository();
    install(root.path()).unwrap();
    let tools = root.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let make = tools.join("make");
    fs::write(&make, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 23\n").unwrap();
    fs::set_permissions(&make, fs::Permissions::from_mode(0o755)).unwrap();
    for (name, gate) in [("pre-commit", "pre-commit"), ("pre-push", "preflight")] {
        let output = Command::new(hook_path(root.path(), name).unwrap())
            .current_dir(root.path())
            .env("PATH", &tools)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(23),
            "must propagate gate failure"
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{gate}\n")
        );
    }
}
