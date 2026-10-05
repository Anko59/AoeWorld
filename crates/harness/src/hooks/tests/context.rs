//! Test-fixture isolation only; production caller override refusal is unchanged.
use crate::process;
use std::{env, time::Duration};

pub(super) fn isolated(name: &str) -> bool {
    if !env::vars_os().any(|(name, _)| name.as_encoded_bytes().starts_with(b"GIT_")) {
        return false;
    }
    let executable = env::current_exe().expect("current hook test executable");
    let test = format!("hooks::tests::{name}");
    let root = env::current_dir().expect("test current directory");
    // capture_in clears every inherited GIT_* on this child only. No global
    // environment mutation, marker bypass, test filter omission or fake result.
    let captured = process::capture_in(
        &root,
        executable.to_str().expect("test executable UTF-8"),
        &["--exact", &test, "--nocapture"],
        &[],
        Duration::from_secs(30),
        &process::Cancellation::default(),
    );
    assert!(
        matches!(captured.exit, process::CaptureExit::Success),
        "isolated hook assertions failed: {test}, {:?}",
        captured.exit
    );
    assert!(!captured.truncated, "isolated hook test output incomplete");
    assert!(
        std::str::from_utf8(&captured.stdout)
            .unwrap()
            .contains("test result: ok. 1 passed; 0 failed; 0 ignored;"),
        "exact isolated test did not execute one unignored test: {test}"
    );
    println!("hook fixture assertions ran in cleared child: {test}");
    true
}

#[test]
fn guarded_parents_execute_every_hook_fixture_in_a_real_cleared_child() {
    let executable = env::current_exe().unwrap();
    let root = env::current_dir().unwrap();
    let mut cases = vec![
        "install_writes_exact_dispatchers_and_is_idempotent",
        "rejects_commented_unreachable_and_malformed_dispatchers",
        "existing_user_hook_is_preserved_without_partial_dispatcher_install",
        "rejects_directory_hook_and_non_repository",
        "refuses_configured_hooks_paths_without_touching_shared_or_external_hooks",
        "linked_worktree_uses_common_hooks_not_git_pointer_directory",
    ];
    #[cfg(unix)]
    cases.extend([
        "rejects_unsafe_permissions_and_repairs_regular_files",
        "refuses_symlink_hooks_without_overwriting_the_target",
        "dispatchers_preserve_gate_arguments_and_failure_status",
    ]);
    for name in cases {
        let test = format!("hooks::tests::{name}");
        let captured = process::capture_in(
            &root,
            executable.to_str().unwrap(),
            &["--exact", &test, "--nocapture"],
            &[
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
                ("GIT_CONFIG_VALUE_0", "false"),
                ("GIT_CONFIG_GLOBAL", "/dev/null"),
                ("GIT_CONFIG_NOSYSTEM", "1"),
                ("GIT_DIR", "/missing-hook-fixture-poison"),
                ("GIT_INDEX_FILE", "/missing-index-fixture-poison"),
            ],
            Duration::from_secs(30),
            &process::Cancellation::default(),
        );
        assert!(
            matches!(captured.exit, process::CaptureExit::Success),
            "guarded {test}: {:?}",
            captured.exit
        );
        assert!(!captured.truncated);
        let output = std::str::from_utf8(&captured.stdout).unwrap();
        assert!(output.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"));
        assert!(output.contains(&format!(
            "hook fixture assertions ran in cleared child: {test}"
        )));
    }
}
