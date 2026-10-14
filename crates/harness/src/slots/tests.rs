use super::{
    Config, DIR_VAR, Holder, Slot, TOKEN_VAR, acquire_at,
    guard::{dir, hold, is_heavy, nonce},
    status::render,
    verify,
};
use crate::gates::registry::{Gate, Registry};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);
const NEVER: Duration = Duration::from_secs(3600);
/// `hold` keeps one process-wide record of the slot it holds; tests that use it take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn holder(command: &str) -> Holder {
    Holder {
        worktree: format!("/work/{command}"),
        branch: "feature".into(),
        command: command.into(),
        started: 1_791_900_000,
        nonce: nonce(),
    }
}

fn take(
    path: &Path,
    slots: usize,
    command: &str,
    wait: Duration,
) -> crate::slots::queue::Result<Slot> {
    acquire_at(
        path,
        slots,
        &holder(command),
        None,
        wait,
        POLL,
        NEVER,
        &mut |_| {},
    )
}

fn queue_dir() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("aoe-ship/heavy-slots");
    (temp, path)
}

fn tickets(path: &Path) -> usize {
    fs::read_dir(path).map_or(0, |entries| entries.flatten().count())
}

/// Queue `command` behind the full slots; returns once its first waiting line is out.
fn enqueue(
    path: &Path,
    slots: usize,
    command: &'static str,
    order: &Arc<Mutex<Vec<&'static str>>>,
) -> thread::JoinHandle<()> {
    let (path, order) = (path.to_owned(), order.clone());
    let (said, lines) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let _held = acquire_at(
            &path,
            slots,
            &holder(command),
            None,
            Duration::from_secs(60),
            POLL,
            NEVER,
            &mut |line| {
                let _ = said.send(line.to_owned());
            },
        )
        .expect("served in the end");
        order.lock().unwrap().push(command);
        thread::sleep(Duration::from_millis(900));
    });
    let line = lines.recv_timeout(Duration::from_secs(10)).expect("queued");
    assert!(line.starts_with("heavy slot: waiting for "), "{line}");
    waiter
}

#[test]
fn two_slots_run_two_and_queue_the_third_and_fourth_in_arrival_order() {
    let (_temp, path) = queue_dir();
    let first = take(&path, 2, "lint", Duration::ZERO).expect("free");
    let second = take(&path, 2, "test-unit", Duration::ZERO).expect("the second slot");
    let error = take(&path, 2, "test-wasm", Duration::ZERO).expect_err("full");
    assert!(error.to_string().contains("lint"), "{error}");
    let order = Arc::new(Mutex::new(Vec::new()));
    let waiters = [
        enqueue(&path, 2, "third", &order),
        enqueue(&path, 2, "fourth", &order),
    ];
    thread::sleep(Duration::from_millis(200));
    assert!(
        order.lock().unwrap().is_empty(),
        "nobody passes a full house"
    );
    drop(first);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        *order.lock().unwrap(),
        ["third"],
        "one freed slot, one waiter, the oldest"
    );
    drop(second);
    for waiter in waiters {
        waiter.join().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), ["third", "fourth"]);
    assert_eq!(tickets(&path), 0, "no ticket is left behind");
}

#[test]
fn waiting_lines_name_the_blocker_the_position_and_repeat() {
    let (_temp, path) = queue_dir();
    let _first = take(&path, 1, "lint", Duration::ZERO).expect("free");
    let mut lines = Vec::new();
    let result = acquire_at(
        &path,
        1,
        &holder("test-unit"),
        None,
        Duration::from_millis(400),
        POLL,
        Duration::from_millis(100),
        &mut |line| lines.push(line.to_owned()),
    );
    assert!(result.is_err());
    assert_eq!(
        lines[0],
        "heavy slot: waiting for lint in /work/lint (position 1)"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("heavy slot: still waiting for lint"))
    );
}

#[test]
fn the_wait_is_bounded_and_the_error_names_every_holder() {
    let (_temp, path) = queue_dir();
    let _a = take(&path, 2, "lint", Duration::ZERO).expect("free");
    let _b = take(&path, 2, "test-e2e", Duration::ZERO).expect("free");
    let begun = Instant::now();
    let error = take(&path, 2, "perf-smoke", Duration::from_millis(300))
        .expect_err("full")
        .to_string();
    assert!(begun.elapsed() < Duration::from_secs(10));
    for needle in [
        "`lint` on feature in /work/lint",
        "`test-e2e` on feature in /work/test-e2e",
        "AOE_HEAVY_WAIT_MINUTES",
        "make load-status",
    ] {
        assert!(error.contains(needle), "{needle} in {error}");
    }
    assert!(
        !error.contains("deadline"),
        "a wait is not a deadline: {error}"
    );
    assert_eq!(tickets(&path), 2, "a failed wait leaves no ticket");
}

/// Re-run this test binary as a child that takes the only slot and sleeps.
#[test]
fn slot_holder_child() {
    let Ok(path) = std::env::var("AOE_TEST_SLOT_HOLDER") else {
        return;
    };
    let _held = take(Path::new(&path), 1, "child", Duration::ZERO).expect("child slot");
    println!("HELD");
    thread::sleep(Duration::from_secs(600));
}

#[test]
fn a_killed_holder_frees_its_slot() {
    let (_temp, path) = queue_dir();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "slots::tests::slot_holder_child", "--nocapture"])
        .env("AOE_TEST_SLOT_HOLDER", &path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("child");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while line.trim() != "HELD" {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "child never held");
    }
    take(&path, 1, "other", Duration::ZERO).expect_err("the child holds the only slot");
    child.kill().unwrap();
    child.wait().unwrap();
    take(&path, 1, "other", Duration::ZERO).expect("the kernel freed the slot");
}

#[test]
fn an_inherited_slot_is_accepted_without_a_second_ticket() {
    let (_temp, path) = queue_dir();
    let parent = take(&path, 1, "preflight", Duration::ZERO).expect("free");
    let token = parent.token().to_owned();
    let child = acquire_at(
        &path,
        1,
        &holder("test-unit"),
        Some(&token),
        Duration::ZERO,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("the parent's slot passes down");
    assert!(matches!(child, Slot::Inherited { .. }));
    assert_eq!(child.token(), token);
    assert_eq!(tickets(&path), 1, "no second ticket, so no double count");
    drop(child);
    assert_eq!(tickets(&path), 1, "the child never owned the slot");
}

#[test]
fn a_forged_slot_variable_is_rejected() {
    let (_temp, path) = queue_dir();
    let parent = take(&path, 1, "preflight", Duration::ZERO).expect("free");
    let (name, _) = parent.token().split_once(':').unwrap();
    for forged in [
        format!("{name}:{}", "0".repeat(32)),
        format!("{name}:"),
        name.to_owned(),
        "00000000000000000000-0000000001-000000:abc".to_owned(),
        "../ship.lock:abc".to_owned(),
        String::new(),
    ] {
        assert!(!verify(&path, &forged), "{forged:?} must not verify");
        let error = acquire_at(
            &path,
            1,
            &holder("test-unit"),
            Some(&forged),
            Duration::ZERO,
            POLL,
            NEVER,
            &mut |_| {},
        )
        .expect_err("a forged token still queues behind the real holder")
        .to_string();
        assert!(error.contains("preflight"), "{error}");
    }
    assert!(verify(&path, parent.token()));
    let token = parent.token().to_owned();
    drop(parent);
    assert!(!verify(&path, &token), "a released slot's token is dead");
}

#[test]
fn a_dead_holders_token_does_not_verify() {
    let (_temp, path) = queue_dir();
    let token = {
        let slot = take(&path, 1, "lint", Duration::ZERO).expect("free");
        slot.token().to_owned()
    };
    assert!(!verify(&path, &token));
    take(&path, 1, "next", Duration::ZERO).expect("free again");
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

/// A checkout with one light gate (`fmt-check`), one heavy gate (`slow`) and one slot.
fn checkout() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("work");
    fs::create_dir_all(root.join("gates")).unwrap();
    git(
        temp.path(),
        &["init", "-q", "-b", "dev", root.to_str().unwrap()],
    );
    let gate = |id: &str, heavy: bool| {
        serde_json::json!({
            "id": id, "command": format!("make {id}"), "requires": [], "select": "fixture",
            "evidence": "exit status", "suites": ["static"], "cadences": ["preflight", "ci"],
            "budget_s": 30, "static": !heavy, "heavy": heavy, "capabilities": [], "blocks": []
        })
    };
    let registry = serde_json::json!({
        "version": 2,
        "suites": [
            {"id": "everything", "paths": ["gates/**"], "implies": ["static"], "review": true},
            {"id": "static", "paths": ["**"], "implies": [], "review": false}
        ],
        "gates": [gate("fmt-check", false), gate("slow", true)],
        "jobs": {"static": ["fmt-check", "slow"]}
    });
    fs::write(root.join("gates/registry.json"), registry.to_string()).unwrap();
    fs::write(root.join("gates/heavy.json"), r#"{"slots": 1}"#).unwrap();
    (temp, root)
}

#[test]
fn static_gates_never_queue_and_a_nested_heavy_gate_takes_no_second_slot() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_temp, root) = checkout();
    let slots = dir(&root).unwrap();
    let outer = hold(&root, "slow").expect("the only slot is free");
    let env = outer.env();
    assert_eq!(env.len(), 2);
    assert_eq!(env[0].0, TOKEN_VAR);
    assert_eq!(env[1].0, DIR_VAR, "children find the same queue");
    assert_eq!(tickets(&slots), 1);
    let begun = Instant::now();
    let light = hold(&root, "fmt-check").expect("a static gate never waits");
    assert!(light.env().is_empty());
    let nested = hold(&root, "slow").expect("the same process already holds the slot");
    assert_eq!(nested.env(), env, "a nested gate passes the held slot on");
    assert!(begun.elapsed() < Duration::from_secs(2));
    assert_eq!(tickets(&slots), 1, "still one ticket");
    drop((light, nested));
    drop(outer);
    assert_eq!(tickets(&slots), 0);
}

#[test]
fn a_gate_that_waited_longer_than_its_budget_still_gets_its_whole_budget() {
    // The wait (4 s) is longer than the whole budget (3 s) and the command is
    // instant: if the clock included the wait the gate would be over budget.
    // Wide margins, so a loaded machine cannot fail this by timing alone.
    const WAIT: Duration = Duration::from_millis(4000);
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_temp, root) = checkout();
    let occupant = take(&dir(&root).unwrap(), 1, "lint", Duration::ZERO).expect("free");
    let releaser = thread::spawn(move || {
        thread::sleep(WAIT);
        drop(occupant);
    });
    let gate: Gate = serde_json::from_value(serde_json::json!({
        "id": "slow", "command": "true", "requires": [], "select": "fixture",
        "evidence": "exit status", "suites": ["static"], "cadences": ["preflight"],
        "budget_s": 3, "static": false, "heavy": true, "capabilities": [], "blocks": []
    }))
    .unwrap();
    let begun = Instant::now();
    let results = crate::ship::run::run_gates(&root, &[&gate]);
    releaser.join().unwrap();
    assert!(begun.elapsed() >= WAIT, "it did wait");
    let [result] = results.as_slice() else {
        panic!("one result")
    };
    assert_eq!(result.line().split_whitespace().nth(1), Some("slow"));
    assert!(
        matches!(result.verdict, crate::ship::run::GateVerdict::Pass),
        "a wait longer than the budget must not spend it: {}",
        result.line()
    );
    assert!(
        result.seconds < WAIT.as_secs_f32(),
        "the clock covers the run only, not the {WAIT:?} wait: {}",
        result.seconds
    );
}

#[test]
fn config_defaults_overrides_and_limits() {
    let config = |slots, wait| Config::from_values(2, slots, wait);
    assert_eq!(
        config(None, None).unwrap(),
        Config {
            slots: 2,
            wait: Duration::from_secs(90 * 60)
        }
    );
    assert_eq!(
        config(Some("5"), Some("0")).unwrap(),
        Config {
            slots: 5,
            wait: Duration::ZERO
        }
    );
    assert_eq!(config(Some(""), Some("")).unwrap().slots, 2);
    for bad in ["0", "9", "-1", "two"] {
        assert!(config(Some(bad), None).is_err(), "{bad}");
    }
    assert!(config(None, Some("soon")).is_err());
    assert!(Config::from_values(0, None, None).is_err());
    let committed = Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    assert!(committed.is_ok_and(|c| (1..=8).contains(&c.slots)));
}

#[test]
fn the_registry_decides_which_gates_are_heavy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = Registry::load(&root).unwrap();
    for heavy in [
        "lint",
        "test-unit",
        "test-wasm",
        "test-e2e",
        "coverage",
        "mutation-nightly",
        "perf-smoke",
        "preflight",
    ] {
        assert!(is_heavy(&registry, heavy), "{heavy}");
    }
    for light in [
        "fmt-check",
        "structure-check",
        "test-placement",
        "architecture-check",
        "docs-check",
        "deny",
        "browser-check",
        "nope",
    ] {
        assert!(!is_heavy(&registry, light), "{light}");
    }
}

#[test]
fn status_lists_running_holders_then_the_queue() {
    let held = |command: &str| Some(holder(command));
    let lines = render(
        2,
        &[held("lint"), held("test-unit"), held("test-e2e"), None],
    );
    assert_eq!(
        lines[0],
        "heavy slots: 2 of 2 in use (AOE_HEAVY_SLOTS to change)"
    );
    assert_eq!(
        lines[1],
        "  running 1. `lint` on feature in /work/lint (started 2026-10-13 14:00:00Z)"
    );
    assert!(lines[2].starts_with("  running 2. `test-unit`"));
    assert!(lines[3].starts_with("  queued 1. `test-e2e`") && lines[3].contains("(queued "));
    assert_eq!(lines[4], "  queued 2. (unreadable ticket)");
    assert_eq!(
        render(2, &[]),
        ["heavy slots: 0 of 2 in use (AOE_HEAVY_SLOTS to change)"]
    );
}
mod makefile;
mod roots;
