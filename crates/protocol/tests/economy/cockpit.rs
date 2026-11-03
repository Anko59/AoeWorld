use super::*;

#[test]
fn cockpit_delivery_report_appends_current_wire_without_changing_existing_fields() {
    let mut p = player();
    p.recent_deliveries = [7, 17, 0, 300];
    let message = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![p],
        buildings: vec![],
    };
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(
        bytes,
        vec![
            1, 1, 0, 0, 1, 0, 200, 1, 200, 1, 100, 200, 1, 4, 5, 0, 0, 100, 100, 100, 7, 17, 0,
            172, 2, 0
        ]
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
}
