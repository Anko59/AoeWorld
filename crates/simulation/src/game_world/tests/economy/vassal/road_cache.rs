use super::*;
use std::cell::Cell;
const LIMIT: usize = 65_536;

#[test]
fn memo_reuses_false_answers_and_bounds_memory_without_changing_misses() {
    let memo = Memo::default();
    for key in 0..LIMIT {
        assert!(!memo.get(key, || false));
    }
    assert_eq!(memo.0.borrow().indices.len(), LIMIT);
    let reads = Cell::new(0);
    assert!(!memo.get(0, || {
        reads.set(reads.get() + 1);
        true
    }));
    assert_eq!(reads.get(), 0);
    for _ in 0..2 {
        assert!(memo.get(LIMIT, || {
            reads.set(reads.get() + 1);
            true
        }));
    }
    assert_eq!(reads.get(), 2);
    assert_eq!(memo.0.borrow().indices.len(), LIMIT);
}
