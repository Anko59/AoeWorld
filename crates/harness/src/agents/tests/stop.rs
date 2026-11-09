use super::super::{
    role::Role,
    stop::{Decision, ROUNDS, Run, Scope, State, Verdict, decide, shared_run},
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

fn run(verdict: Verdict, fingerprint: &str) -> Run {
    Run {
        fingerprint: fingerprint.into(),
        scope: Scope::Full,
        verdict,
        report: format!("check-fast: {}", verdict.label()),
    }
}

#[test]
fn an_agent_is_held_five_red_rounds_then_given_autonomous_recovery() {
    let mut state = State::default();
    let red = run(Verdict::Fail, "a");
    for round in 1..=ROUNDS {
        let Decision::Block(reason) = decide(&mut state, Role::Implementer, "s/1", &red, None, 100)
        else {
            panic!("round {round} must block");
        };
        assert!(reason.contains(&format!("Red round {round}/{ROUNDS}")));
        assert!(reason.contains("never weaken"));
    }
    let Decision::Block(reason) = decide(&mut state, Role::Implementer, "s/1", &red, None, 200)
    else {
        panic!("round six must direct autonomous recovery");
    };
    assert!(reason.contains("BLOCKED.md"));
    assert_recovery(&reason);
    // A BLOCKED.md older than the streak does not count.
    assert!(matches!(
        decide(&mut state, Role::Implementer, "s/1", &red, Some(99), 300),
        Decision::Block(_)
    ));
    let Decision::Note(message) =
        decide(&mut state, Role::Implementer, "s/1", &red, Some(100), 300)
    else {
        panic!("a fresh record permits handoff with recovery instructions");
    };
    assert_recovery(&message);
}

#[test]
fn green_clears_the_streak_and_actors_are_counted_apart() {
    let mut state = State::default();
    let red = run(Verdict::Fail, "a");
    decide(&mut state, Role::Tester, "s/1", &red, None, 1);
    decide(&mut state, Role::Tester, "s/2", &red, None, 1);
    assert_eq!(state.streaks["s/1"].rounds, 1);
    assert_eq!(state.streaks["s/2"].rounds, 1);
    assert_eq!(
        decide(
            &mut state,
            Role::Tester,
            "s/1",
            &run(Verdict::Pass, "b"),
            None,
            2
        ),
        Decision::Allow
    );
    assert!(!state.streaks.contains_key("s/1"));
    assert_eq!(state.streaks["s/2"].rounds, 1);
}

#[test]
fn incomplete_is_a_note_and_counts_no_round() {
    let mut state = State::default();
    let decision = decide(
        &mut state,
        Role::Implementer,
        "s/1",
        &run(Verdict::Incomplete, "a"),
        None,
        1,
    );
    assert!(matches!(decision, Decision::Note(message) if message.contains("INCOMPLETE")));
    assert!(state.streaks.is_empty());
}

#[test]
fn the_main_session_is_told_once_per_red_change_and_never_held() {
    let mut state = State::default();
    let red = run(Verdict::Fail, "a");
    assert!(matches!(
        decide(&mut state, Role::Main, "s/main", &red, None, 1),
        Decision::Block(_)
    ));
    assert!(matches!(
        decide(&mut state, Role::Main, "s/main", &red, None, 2),
        Decision::Note(_)
    ));
    let changed = run(Verdict::Fail, "b");
    assert!(matches!(
        decide(&mut state, Role::Main, "s/main", &changed, None, 3),
        Decision::Block(_)
    ));
    assert_eq!(
        decide(
            &mut state,
            Role::Main,
            "s/main",
            &run(Verdict::Pass, "c"),
            None,
            4
        ),
        Decision::Allow
    );
    assert!(state.streaks.is_empty());
}

fn assert_recovery(message: &str) {
    for required in [
        "Narrow the task",
        "make issue",
        "evidence",
        "last green commit",
        "make next",
        "main session",
    ] {
        assert!(message.contains(required), "missing {required}: {message}");
    }
    for obsolete in ["what you need", "then stop", "tell the person"] {
        assert!(!message.contains(obsolete), "obsolete ending: {message}");
    }
}

#[test]
fn the_main_session_also_gets_autonomous_recovery() {
    let mut state = State::default();
    let red = run(Verdict::Fail, "a");
    for round in 0..2 {
        let decision = decide(&mut state, Role::Main, "s/main", &red, None, 100 + round);
        let message = match (round, decision) {
            (0, Decision::Block(message)) | (1, Decision::Note(message)) => message,
            other => panic!("unexpected main session decision: {other:?}"),
        };
        assert_recovery(&message);
    }
}

#[test]
fn agents_that_stop_together_on_one_tree_share_a_single_gate_run() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let executions = AtomicUsize::new(0);
    let reports: Vec<String> = std::thread::scope(|scope| {
        let stops: Vec<_> = (0..6)
            .map(|_| {
                scope.spawn(|| {
                    shared_run(
                        root,
                        Scope::Full,
                        Duration::from_secs(30),
                        || Ok("same tree".into()),
                        |fingerprint| {
                            let nth = executions.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(200));
                            Run {
                                fingerprint,
                                scope: Scope::Full,
                                verdict: Verdict::Pass,
                                report: format!("gate run {nth}"),
                            }
                        },
                    )
                    .report
                })
            })
            .collect();
        stops.into_iter().map(|stop| stop.join().unwrap()).collect()
    });
    assert_eq!(executions.load(Ordering::SeqCst), 1, "{reports:?}");
    assert!(
        reports.iter().all(|report| report == "gate run 0"),
        "{reports:?}"
    );
}

#[test]
fn a_waiting_stop_judges_the_tree_as_it_is_when_its_turn_comes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // The tree changes while the second stop waits: it must run under the new
    // fingerprint instead of recording the new tree under the old one.
    let asked = AtomicUsize::new(0);
    let first = shared_run(
        root,
        Scope::Full,
        Duration::from_secs(30),
        || Ok("before".into()),
        |fingerprint| run_named(Verdict::Fail, &fingerprint),
    );
    assert_eq!(first.fingerprint, "before");
    let second = shared_run(
        root,
        Scope::Full,
        Duration::from_secs(30),
        || {
            Ok(if asked.fetch_add(1, Ordering::SeqCst) == 0 {
                "stale".into()
            } else {
                "after".into()
            })
        },
        |fingerprint| run_named(Verdict::Pass, &fingerprint),
    );
    assert_eq!(second.fingerprint, "after");
    assert_eq!(second.verdict, Verdict::Pass);
    // The same tree stopping again reuses the recorded run without executing.
    let again = shared_run(
        root,
        Scope::Full,
        Duration::from_secs(30),
        || Ok("after".into()),
        |_| panic!("a recorded run must be reused"),
    );
    assert_eq!(again.verdict, Verdict::Pass);
    // Scopes never share a run.
    let tester = shared_run(
        root,
        Scope::Static,
        Duration::from_secs(30),
        || Ok("after".into()),
        |fingerprint| Run {
            scope: Scope::Static,
            ..run_named(Verdict::Fail, &fingerprint)
        },
    );
    assert_eq!(tester.verdict, Verdict::Fail);
}

#[test]
fn a_run_lock_that_stays_held_is_not_waited_for_beyond_the_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let entered = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            shared_run(
                root,
                Scope::Full,
                Duration::from_secs(30),
                || Ok("slow".into()),
                |fingerprint| {
                    entered.wait();
                    std::thread::sleep(Duration::from_millis(600));
                    run_named(Verdict::Pass, &fingerprint)
                },
            )
        });
        entered.wait();
        let impatient = shared_run(
            root,
            Scope::Full,
            Duration::from_millis(50),
            || Ok("other".into()),
            |fingerprint| run_named(Verdict::Fail, &fingerprint),
        );
        assert_eq!(impatient.fingerprint, "other");
        assert_eq!(impatient.verdict, Verdict::Fail);
    });
}

fn run_named(verdict: Verdict, fingerprint: &str) -> Run {
    run(verdict, fingerprint)
}
