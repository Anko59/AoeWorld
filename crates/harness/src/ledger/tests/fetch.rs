use super::{ONCE, raw};
use crate::ledger::{
    fetch::{Listed, Source, refresh},
    record::{Ledger, RawPr},
};
use std::cell::RefCell;

struct Fake {
    numbers: Vec<u64>,
    fail_at: Option<u64>,
    viewed: RefCell<Vec<u64>>,
}

impl Fake {
    fn new(numbers: &[u64]) -> Self {
        Self {
            numbers: numbers.to_vec(),
            fail_at: None,
            viewed: RefCell::default(),
        }
    }
}

impl Source for Fake {
    fn list(&self, _: usize) -> Result<Vec<Listed>, Box<dyn std::error::Error>> {
        Ok(self
            .numbers
            .iter()
            .map(|&number| Listed { number })
            .collect())
    }

    fn view(&self, number: u64) -> Result<RawPr, Box<dyn std::error::Error>> {
        if self.fail_at == Some(number) {
            return Err("rate limited".into());
        }
        self.viewed.borrow_mut().push(number);
        Ok(raw(number, "T", ONCE, &[]))
    }
}

#[test]
fn a_refresh_fetches_newest_first_within_the_call_budget() {
    let source = Fake::new(&[1, 2, 3, 4, 5]);
    let mut ledger = Ledger::default();
    let done = refresh(&source, &mut ledger, 200, 3);
    // One call lists, two fetch: the two newest.
    assert_eq!((done.calls, done.fetched, done.missing), (3, 2, 3));
    assert_eq!(*source.viewed.borrow(), [5, 4]);
    assert!(ledger.knows(5) && ledger.knows(4) && !ledger.knows(3));
}

#[test]
fn the_next_refresh_only_fetches_what_is_new() {
    let mut ledger = Ledger::default();
    refresh(&Fake::new(&[1, 2]), &mut ledger, 200, 10);
    let source = Fake::new(&[1, 2, 3]);
    let done = refresh(&source, &mut ledger, 200, 10);
    assert_eq!((done.calls, done.fetched, done.missing), (2, 1, 0));
    assert_eq!(*source.viewed.borrow(), [3]);
}

#[test]
fn a_failed_call_stops_and_keeps_what_was_fetched() {
    let mut source = Fake::new(&[1, 2, 3]);
    source.fail_at = Some(2);
    let mut ledger = Ledger::default();
    let done = refresh(&source, &mut ledger, 200, 10);
    assert_eq!((done.fetched, done.missing), (1, 2));
    assert!(done.stopped.unwrap().contains("rate limited"));
    assert!(ledger.knows(3));
}

#[test]
fn a_zero_budget_makes_no_call() {
    let source = Fake::new(&[1]);
    let done = refresh(&source, &mut Ledger::default(), 200, 0);
    assert_eq!(done.calls, 0);
    assert!(done.stopped.is_some());
}

#[test]
fn the_cache_round_trips_and_an_other_format_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/ledger.json");
    let mut ledger = Ledger::default();
    refresh(&Fake::new(&[7, 3]), &mut ledger, 200, 10);
    ledger.save(&path).unwrap();
    let loaded = Ledger::load(&path);
    assert_eq!(loaded.prs, ledger.prs);
    assert_eq!(
        loaded.prs.iter().map(|p| p.number).collect::<Vec<_>>(),
        [3, 7]
    );
    std::fs::write(&path, r#"{"version":0,"prs":[]}"#).unwrap();
    assert!(Ledger::load(&path).prs.is_empty());
    std::fs::write(&path, "garbage").unwrap();
    assert!(Ledger::load(&path).prs.is_empty());
}
