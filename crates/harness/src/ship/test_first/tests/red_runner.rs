use crate::ship::{git, tests::run};
use std::process::Command;

#[test]
fn linked_worktree_red_mounts_external_common_git_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    std::fs::create_dir(&main).unwrap();
    run(&main, &["init", "-q", "-b", "feature"]);
    run(&main, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let linked = temp.path().join("linked");
    run(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_str().unwrap(),
        ],
    );
    assert!(linked.join(".git").is_file());
    let common = git::git(
        &linked,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    let mut command = Command::new("docker");
    super::mount_git(&mut command, &linked).unwrap();
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["-v", &format!("{common}:{common}")]);
    let mut ordinary = Command::new("docker");
    super::mount_git(&mut ordinary, &main).unwrap();
    assert_eq!(
        ordinary.get_args().count(),
        0,
        "the root bind already covers ordinary .git"
    );
}

#[test]
fn red_browser_step_deadline_names_and_stops_the_stalled_step() {
    use super::super::progress;
    use std::time::{Duration, Instant};
    let mut command = Command::new("sh");
    command.args(["-c", "printf 'RED_STEP playwright-discovery\\n'; sleep 30"]);
    let started = Instant::now();
    let error = progress::capture_with(command, &|step| {
        if step == "runner-start" {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(100)
        }
    })
    .unwrap_err();
    assert!(error.contains("playwright-discovery"), "{error}");
    assert!(error.contains("deadline"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn red_browser_steps_have_independent_budgets_and_forward_live_progress() {
    use super::super::progress;
    use std::time::Duration;
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'RED_STEP server-build\\n'; sleep 0.2; printf 'RED_STEP wasm-build\\n'; sleep 0.2; printf 'RED_RUN_RESULT {}\\n'",
    ]);
    let captured = progress::capture_with(command, &|step| {
        if step == "runner-start" {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(350)
        }
    })
    .unwrap();
    assert!(matches!(
        captured.exit,
        crate::process::CaptureExit::Success
    ));
    assert!(captured.duration >= Duration::from_millis(400));
    assert!(String::from_utf8_lossy(&captured.stdout).ends_with("RED_RUN_RESULT {}\n"));
}

#[test]
fn ordinary_output_cannot_extend_a_red_browser_step_deadline() {
    use super::super::progress;
    use std::time::Duration;
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'RED_STEP playwright-discovery\\n'; while :; do printf 'still running\\n'; sleep 0.01; done",
    ]);
    let error = progress::capture_with(command, &|step| {
        if step == "runner-start" {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(100)
        }
    })
    .unwrap_err();
    assert!(error.contains("playwright-discovery"), "{error}");
}

#[test]
fn repeated_step_announcements_cannot_keep_a_stalled_step_alive() {
    use super::super::progress;
    use std::time::Duration;
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "while :; do printf 'RED_STEP playwright-discovery\\n'; sleep 0.01; done",
    ]);
    let error = progress::capture_with(command, &|step| {
        if step == "runner-start" {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(100)
        }
    })
    .unwrap_err();
    assert!(error.contains("playwright-discovery"), "{error}");
}

#[test]
fn red_runner_mounts_the_shared_cargo_home_read_write() {
    let (_owner, root) = crate::ship::tests::fixture("true");
    let common = git::git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    let mut command = Command::new("docker");
    super::mount_cargo(&mut command, &root).unwrap();
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let home =
        std::env::var("HARNESS_CARGO_CACHE").unwrap_or_else(|_| format!("{common}/aoe-cargo"));
    assert_eq!(
        args,
        [
            "-v",
            &format!("{home}:{home}"),
            "-e",
            &format!("CARGO_HOME={home}")
        ]
    );
    assert!(std::path::Path::new(&home).is_dir());
}
