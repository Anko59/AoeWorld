//! The heavy-slot tests do not depend on the environment of the process that runs them. The
//! `AOE_HEAVY_*` variables a holder passes down reach the code only as a `SlotEnv`, and only the
//! process entry point reads them.
use super::{
    SERIAL, checkout,
    support::{hold, run_in_slot},
    take, tickets,
};
use crate::slots::{DIR_VAR, SlotEnv, TOKEN_VAR, guard::dir};
use std::{fs, path::Path, path::PathBuf, process::Command, time::Duration};

/// Marks the child process of `an_inherited_slot_moves_no_scenario_that_does_not_pass_it_on`.
const CHILD: &str = "AOE_TEST_INHERITED_CHILD";
/// The real repository's queue and its live token, which the child is named by.
const REAL_QUEUE: &str = "AOE_TEST_REAL_QUEUE";
const REAL_TOKEN: &str = "AOE_TEST_REAL_TOKEN";

/// The inherited case a ship gate hits: a child process starts with `AOE_HEAVY_SLOT` and
/// `AOE_HEAVY_DIR` pointing at a live slot of a real repository. The scenarios must still use
/// their own temporary queue; only the one that passes those values on uses the real queue.
#[test]
fn an_inherited_slot_moves_no_scenario_that_does_not_pass_it_on() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_real_temp, real) = checkout();
    let real_dir = dir(&real).unwrap();
    let real_slot = take(&real_dir, 1, "outer", Duration::ZERO).expect("the real queue's slot");
    let log = tempfile::tempdir().expect("temp");
    let (out, err) = (log.path().join("out"), log.path().join("err"));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "slots::tests::environment::inherited_slot_child",
            "--nocapture",
        ])
        .env(TOKEN_VAR, real_slot.token())
        .env(DIR_VAR, &real_dir)
        .env(REAL_TOKEN, real_slot.token())
        .env(REAL_QUEUE, &real_dir)
        .env(CHILD, "1")
        .stdout(fs::File::create(&out).unwrap())
        .stderr(fs::File::create(&err).unwrap())
        .spawn()
        .expect("the child runs");
    // A child that queues in the real queue waits there for good: bound it, then fail.
    let begun = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if begun.elapsed() > Duration::from_secs(120) {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let report = format!(
        "{}\n{}",
        fs::read_to_string(&out).unwrap_or_default(),
        fs::read_to_string(&err).unwrap_or_default()
    );
    assert!(
        status.is_some_and(|s| s.success()),
        "the child did not pass within 120 s: {report}"
    );
    assert_eq!(tickets(&real_dir), 1, "the child left the real queue alone");
}

/// The body of the test above, run only by that child (a no-op in a normal test run).
#[test]
fn inherited_slot_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let real_dir = PathBuf::from(std::env::var_os(REAL_QUEUE).expect("the parent names the queue"));
    let inherited = SlotEnv {
        token: std::env::var(REAL_TOKEN).ok(),
        dir: Some(real_dir.clone().into_os_string()),
        ..SlotEnv::default()
    };

    let (_temp, root) = checkout();
    let own_dir = dir(&root).unwrap();
    let fuzz = root.join("fuzz");
    fs::create_dir_all(&fuzz).unwrap();

    let slot = hold(&fuzz, "slow", &SlotEnv::default()).expect("its own queue's only slot");
    assert_eq!(tickets(&own_dir), 1, "its own temporary queue");
    assert_eq!(tickets(&real_dir), 1, "the real queue is untouched");
    assert_eq!(
        Path::new(&slot.env()[1].1).canonicalize().unwrap(),
        own_dir.canonicalize().unwrap(),
        "the child is pointed at its own queue"
    );
    drop(slot);
    assert_eq!(tickets(&own_dir), 0);

    let light = hold(&root, "fmt-check", &SlotEnv::default()).expect("a static gate never waits");
    assert!(light.env().is_empty(), "a light gate passes no slot");
    drop(light);

    // `env -u` clears what this process inherited, so the child sees only what the wrapper adds.
    let cleared = vec![
        "env".to_owned(),
        "-u".to_owned(),
        "AOE_HEAVY_SLOT".to_owned(),
        "-u".to_owned(),
        "AOE_HEAVY_DIR".to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        r#"test -z "${AOE_HEAVY_SLOT:-}" && exit 4; exit 5"#.to_owned(),
    ];
    let code = run_in_slot(&root, "fmt-check", &cleared, &SlotEnv::default())
        .expect("a light gate never waits");
    assert_eq!(code, 4, "a light gate passes no slot, inherited or not");
    assert_eq!(tickets(&real_dir), 1, "still only the outer holder");

    let passed = hold(&root, "slow", &inherited).expect("the passed slot is live");
    assert_eq!(
        passed.env()[0].1,
        inherited.token.unwrap(),
        "passed on purpose"
    );
    assert_eq!(passed.env()[1].1, real_dir.display().to_string());
    assert_eq!(tickets(&own_dir), 0, "no ticket in its own queue");
    assert_eq!(tickets(&real_dir), 1, "the passed slot is the holder's");
}

/// The five variables are read in one place. A new reader would bring a process's slot back into
/// the tests, so this fails until the reader takes its values as an input instead.
#[test]
fn only_the_process_entry_reads_the_slot_variables() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut readers = Vec::new();
    let mut pending = vec![src.clone()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(fs::read_dir(&path).unwrap().flatten().map(|e| e.path()));
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_this_file(&path) {
            let text = fs::read_to_string(&path).unwrap();
            for (number, line) in text.lines().enumerate() {
                if reads_slot_variable(line) {
                    readers.push(format!("{}:{}", relative(&src, &path), number + 1));
                }
            }
        }
    }
    // One read per variable, all in `SlotEnv::process` (slots/mod.rs).
    readers.sort();
    let in_process = readers
        .iter()
        .filter(|r| r.starts_with("slots/mod.rs:"))
        .count();
    assert_eq!(readers.len(), 5, "{readers:?}");
    assert_eq!(
        in_process, 5,
        "only SlotEnv::process reads them: {readers:?}"
    );
}

fn reads_slot_variable(line: &str) -> bool {
    line.contains("env::var")
        && [
            "CHECKED_VAR",
            "TOKEN_VAR",
            "DIR_VAR",
            "SLOTS_VAR",
            "WAIT_VAR",
            "AOE_HEAVY_",
        ]
        .iter()
        .any(|name| line.contains(name))
}

fn is_this_file(path: &Path) -> bool {
    path.ends_with("slots/tests/environment.rs")
}

fn relative(src: &Path, path: &Path) -> String {
    path.strip_prefix(src).unwrap_or(path).display().to_string()
}
