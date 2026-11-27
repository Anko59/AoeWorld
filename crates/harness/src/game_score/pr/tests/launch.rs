use super::*;

fn shell(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

#[test]
fn a_command_that_succeeds_is_finished_with_the_last_lines_it_wrote() {
    let finished = run(shell("echo one; echo two >&2"), Duration::from_secs(30)).expect("ran");
    assert!(finished.success);
    assert!(finished.tail.iter().any(|line| line == "one"));
    assert!(finished.tail.iter().any(|line| line == "two"));
    assert!(finished.seconds < 30.0);
}

#[test]
fn a_command_that_fails_says_so_and_keeps_its_last_lines_for_the_crash_line() {
    let finished = run(
        shell("for i in $(seq 1 40); do echo line$i >&2; done; exit 3"),
        Duration::from_secs(30),
    )
    .expect("ran");
    assert!(!finished.success);
    let text = finished.tail_text();
    assert!(text.ends_with("line40"), "{text}");
    assert!(
        !text.contains("line1 "),
        "only the last lines are kept: {text}"
    );
    assert!(finished.tail.len() <= 12);
}

#[test]
fn the_crash_line_leaves_out_the_blank_lines() {
    let finished = run(
        shell("echo error: one; echo; echo tip: two"),
        Duration::from_secs(30),
    )
    .expect("ran");
    assert_eq!(finished.tail_text(), "error: one | tip: two");
}

#[test]
fn a_command_that_outlives_its_deadline_is_killed_and_says_so() {
    let started = Instant::now();
    let finished = run(shell("sleep 60"), Duration::from_secs(1)).expect("ran");
    assert!(!finished.success);
    assert!(finished.tail_text().contains("killed after 1 s"));
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn a_program_that_does_not_exist_is_an_error_not_a_crash_line() {
    assert!(
        run(
            Command::new("/nonexistent/aoe-harness"),
            Duration::from_secs(5)
        )
        .is_err()
    );
}

#[test]
fn a_game_score_child_is_told_its_flags_and_its_own_target_directory() {
    let flags = vec!["--scenario".to_owned(), "late".to_owned()];
    let command = game_score(
        Path::new("/checkout/target/release/aoe-harness"),
        Path::new("/checkout"),
        Path::new("/checkout/target"),
        &flags,
    );
    assert_eq!(
        command.get_program(),
        "/checkout/target/release/aoe-harness"
    );
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["game-score", "--scenario", "late"]);
    assert_eq!(command.get_current_dir(), Some(Path::new("/checkout")));
    let target = command
        .get_envs()
        .find(|(name, _)| *name == "CARGO_TARGET_DIR")
        .and_then(|(_, value)| value);
    assert_eq!(target, Some(std::ffi::OsStr::new("/checkout/target")));
}

#[test]
fn each_checkout_builds_into_its_own_target_directory() {
    assert_eq!(target(Path::new("/a/base")), Path::new("/a/base/target"));
    assert_eq!(
        binary(&target(Path::new("/a/base"))),
        Path::new("/a/base/target/release/aoe-harness")
    );
}
