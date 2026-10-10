use super::*;
use tempfile::TempDir;

mod context;

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
    let hooks = directory.path().join(".git/hooks").display().to_string();
    git(directory.path(), &["config", "core.hooksPath", &hooks]);
    directory
}

#[test]
fn install_writes_exact_dispatchers_and_is_idempotent() {
    if context::isolated("install_writes_exact_dispatchers_and_is_idempotent") {
        return;
    }
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
fn install_replaces_only_our_superseded_pre_push_dispatcher() {
    if context::isolated("install_replaces_only_our_superseded_pre_push_dispatcher") {
        return;
    }
    let root = repository();
    install(root.path()).expect("install");
    let pre_push = hook_path(root.path(), "pre-push").unwrap();
    fs::write(&pre_push, b"#!/bin/sh\nexec make preflight\n").unwrap();
    assert!(check(root.path()).is_err(), "the old dispatcher is stale");
    install(root.path()).expect("upgrade the superseded dispatcher");
    assert_eq!(
        fs::read(&pre_push).unwrap(),
        b"#!/bin/sh\nexec make pre-push\n"
    );
    check(root.path()).expect("upgraded hooks");
    fs::write(&pre_push, b"#!/bin/sh\nexec make preflight # mine\n").unwrap();
    assert!(
        install(root.path()).is_err(),
        "a foreign hook is never overwritten"
    );
}

#[test]
fn rejects_commented_unreachable_and_malformed_dispatchers() {
    if context::isolated("rejects_commented_unreachable_and_malformed_dispatchers") {
        return;
    }
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
            assert!(install(root.path()).is_err(), "never replace a custom hook");
            assert_eq!(
                fs::read(&path).unwrap(),
                content.as_bytes(),
                "preserve {name}"
            );
            fs::write(&path, expected).expect("restore fixture dispatcher");
        }
        fs::write(&path, [0xff]).expect("invalid UTF-8 hook");
        assert!(check(root.path()).is_err());
        assert!(install(root.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), [0xff]);
        fs::write(&path, expected).expect("restore fixture dispatcher");
        check(root.path()).expect("restored hooks");
    }
}

#[cfg(unix)]
#[test]
fn rejects_unsafe_permissions_and_repairs_regular_files() {
    if context::isolated("rejects_unsafe_permissions_and_repairs_regular_files") {
        return;
    }
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
    if context::isolated("refuses_symlink_hooks_without_overwriting_the_target") {
        return;
    }
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
fn existing_user_hook_is_preserved_without_partial_dispatcher_install() {
    if context::isolated("existing_user_hook_is_preserved_without_partial_dispatcher_install") {
        return;
    }
    let root = repository();
    let pre_commit = hook_path(root.path(), "pre-commit").unwrap();
    let pre_push = hook_path(root.path(), "pre-push").unwrap();
    let sentinel = b"user pre-push hook\n";
    fs::write(&pre_push, sentinel).unwrap();

    assert!(install(root.path()).is_err());
    assert!(!pre_commit.exists());
    assert_eq!(fs::read(pre_push).unwrap(), sentinel);
}

#[test]
fn rejects_directory_hook_and_non_repository() {
    if context::isolated("rejects_directory_hook_and_non_repository") {
        return;
    }
    let root = repository();
    fs::create_dir(hook_path(root.path(), "pre-commit").unwrap()).unwrap();
    assert!(install(root.path()).is_err());
    assert!(check(root.path()).is_err());
    let non_repository = tempfile::tempdir().unwrap();
    assert!(install(non_repository.path()).is_err());
    assert!(check(non_repository.path()).is_err());
}

#[test]
fn refuses_configured_hooks_paths_without_touching_shared_or_external_hooks() {
    if context::isolated("refuses_configured_hooks_paths_without_touching_shared_or_external_hooks")
    {
        return;
    }
    let root = repository();
    let external = tempfile::tempdir().unwrap();
    for configured in [
        "custom hooks ".to_owned(),
        external
            .path()
            .join("absolute hooks ")
            .display()
            .to_string(),
    ] {
        git(root.path(), &["config", "core.hooksPath", &configured]);
        let output = git(
            root.path(),
            &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
        );
        let directory = PathBuf::from(output.strip_suffix('\n').unwrap_or(&output));
        fs::create_dir_all(&directory).unwrap();
        let sentinel = b"user-owned shared hook; preserve me\n";
        for (name, _) in HOOKS {
            fs::write(directory.join(name), sentinel).unwrap();
        }
        assert!(hook_path(root.path(), "pre-commit").is_err());
        assert!(install(root.path()).is_err());
        assert!(check(root.path()).is_err());
        for (name, _) in HOOKS {
            assert_eq!(fs::read(directory.join(name)).unwrap(), sentinel);
        }
    }
    assert!(!root.path().join(".git/hooks/pre-commit").exists());
}

#[test]
fn linked_worktree_uses_common_hooks_not_git_pointer_directory() {
    if context::isolated("linked_worktree_uses_common_hooks_not_git_pointer_directory") {
        return;
    }
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
    if context::isolated("dispatchers_preserve_gate_arguments_and_failure_status") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let root = repository();
    install(root.path()).unwrap();
    let tools = root.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let make = tools.join("make");
    fs::write(&make, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 23\n").unwrap();
    fs::set_permissions(&make, fs::Permissions::from_mode(0o755)).unwrap();
    for (name, gate) in [("pre-commit", "pre-commit"), ("pre-push", "pre-push")] {
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
