use super::*;

fn hunter_load(amount: u16, task: UnitTask) -> GameplayServerMessage {
    let mut worker = unit();
    worker.carried = Some(CarriedResource {
        kind: ResourceKind::Food,
        amount,
    });
    worker.gathering = (task != UnitTask::Idle).then_some(ResourceKind::Food);
    worker.task = task;
    worker.moving = task != UnitTask::Idle;
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(3),
        units: vec![worker],
        players: vec![],
        buildings: vec![],
    }
}

#[test]
fn hunter_food_loads_roundtrip_up_to_current_dat_limit_even_after_stop() {
    for task in [UnitTask::Hunting, UnitTask::Returning, UnitTask::Idle] {
        for amount in [35, 45, 67] {
            let message = hunter_load(amount, task);
            let bytes = encode_gameplay_server(&message).unwrap();
            assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
        }
        let too_much = hunter_load(68, task);
        assert!(encode_gameplay_server(&too_much).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&too_much).unwrap()).is_err());
    }
}

#[test]
fn hunter_maximum_load_has_current_golden_wire() {
    let message = hunter_load(67, UnitTask::Returning);
    let bytes = postcard::to_allocvec(&message).unwrap();
    // The combined bundle keeps eco2's DAT hunter maximum and appends the
    // optional Vassal IDs and current stance. Regenerated with roundtrips.
    assert_eq!(
        bytes,
        vec![
            1, 1, 3, 1, 2, 0, 128, 16, 128, 16, 1, 0, 3, 0, 4, 1, 0, 67, 1, 0, 25, 25, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );
    assert_eq!(encode_gameplay_server(&message).unwrap(), bytes);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
}
