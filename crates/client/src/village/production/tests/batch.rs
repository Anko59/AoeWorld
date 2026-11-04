use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn units_here_rejected_rally_never_releases_training() {
    let mut batch = Batch::new(UnitKind::Militia);
    batch.rallies.push((1, EntityId(7), 3));
    assert!(matches!(
        batch.acknowledge(1, CommandResult::RejectedRateLimited),
        Some(Ack::Rally(EntityId(7), 0))
    ));
    assert!(!batch.active());
    assert_eq!(batch.accepted, 0);
}

#[wasm_bindgen_test]
fn units_here_counts_only_accepted_training_and_ignores_duplicate_acks() {
    let mut batch = Batch::new(UnitKind::Militia);
    batch.rallies.push((1, EntityId(7), 3));
    assert!(matches!(
        batch.acknowledge(1, CommandResult::Accepted),
        Some(Ack::Rally(EntityId(7), 3))
    ));
    batch.trains.extend([2, 3, 4]);
    batch.acknowledge(2, CommandResult::Accepted);
    assert_eq!(batch.accepted, 1);
    assert!(batch.acknowledge(2, CommandResult::Accepted).is_none());
    batch.acknowledge(3, CommandResult::RejectedRateLimited);
    assert!(batch.active());
    batch.acknowledge(4, CommandResult::Accepted);
    assert!(!batch.active());
    assert_eq!(batch.accepted, 2);
}
