use crate::{process::load::Load, slots::load::wait_with};
use std::{cell::Cell, time::Duration};

fn exercise(readings: &[Option<f64>], bound: u64) -> (bool, Vec<u64>, Vec<String>) {
    let index = Cell::new(0);
    let now = Cell::new(Duration::ZERO);
    let mut sleeps = Vec::new();
    let mut lines = Vec::new();
    let result = wait_with(
        Duration::from_secs(bound),
        &mut || {
            let i = index.get();
            index.set(i + 1);
            readings[i.min(readings.len() - 1)].map(|one| Load {
                one,
                five: 1.0,
                fifteen: 1.0,
                cores: 12,
            })
        },
        &mut || now.get(),
        &mut |d| {
            sleeps.push(d.as_secs());
            now.set(now.get() + d);
        },
        &mut |line| lines.push(line.to_owned()),
    );
    (result.is_ok(), sleeps, lines)
}

#[test]
fn load_wait_is_bounded_reports_every_five_minutes_and_never_sleeps_in_tests() {
    let (ok, waits, lines) = exercise(&[Some(43.0), Some(25.0), Some(12.0)], 1800);
    assert!(ok);
    assert_eq!(waits, [30, 30]);
    assert!(
        lines[0].contains("43.0") && lines[0].contains("12 cores"),
        "{lines:?}"
    );
    assert!(lines.last().unwrap().contains("starting"));
    let (ok, waits, lines) = exercise(&[Some(25.0)], 601);
    assert!(!ok);
    assert_eq!(waits.iter().sum::<u64>(), 601);
    assert_eq!(
        lines.iter().filter(|s| s.contains("waiting")).count(),
        3,
        "{lines:?}"
    );
    assert!(
        exercise(&[Some(12.0)], 0).0,
        "equal load starts immediately"
    );
    assert!(!exercise(&[Some(12.1)], 0).0, "overload at bound refuses");
    assert!(exercise(&[None], 0).0, "unsupported load reader starts");
}

#[test]
fn load_wait_holds_the_slot_and_timeout_releases_it_before_work_starts() {
    use super::{SERIAL, checkout, tickets};
    use crate::slots::{
        SlotEnv,
        guard::{dir, hold_in},
    };
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_temp, root) = checkout();
    let queue = dir(&root).unwrap();
    let waited = Cell::new(false);
    let failed = hold_in(
        &root,
        "slow",
        &SlotEnv::default(),
        || Ok(queue.clone()),
        || {
            assert_eq!(tickets(&queue), 1, "the load waiter holds its queue turn");
            waited.set(true);
            Err("synthetic overload timeout".into())
        },
    );
    assert!(failed.is_err());
    assert!(waited.get());
    assert_eq!(tickets(&queue), 0, "timeout released the ticket");
    let slot = hold_in(
        &root,
        "slow",
        &SlotEnv::default(),
        || Ok(queue.clone()),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(tickets(&queue), 1);
    let inherited = SlotEnv {
        token: Some(slot.env()[0].1.clone()),
        dir: Some(queue.clone().into_os_string()),
        ..SlotEnv::default()
    };
    let nested = hold_in(
        &root,
        "slow",
        &inherited,
        || Ok(queue.clone()),
        || {
            waited.set(false);
            Ok(())
        },
    )
    .unwrap();
    assert!(
        !waited.get(),
        "nested heavy work rechecks load before starting"
    );
    drop(nested);
    drop(slot);
    assert_eq!(tickets(&queue), 0);
    // A child process inherits a live ticket, without the process-local HELD cache.
    // Rechecking there would spend its parent's running gate deadline on load waits.
    let parent = super::take(&queue, 1, "parent", Duration::ZERO).unwrap();
    let child_env = SlotEnv {
        token: Some(parent.token().to_owned()),
        dir: Some(queue.clone().into_os_string()),
        ..SlotEnv::default()
    };
    let child = hold_in(
        &root,
        "slow",
        &child_env,
        || Ok(queue.clone()),
        || panic!("the parent already checked load before starting its deadline"),
    )
    .unwrap();
    assert_eq!(tickets(&queue), 1);
    drop(child);
    drop(parent);
    assert_eq!(tickets(&queue), 0);
}
