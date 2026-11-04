use super::*;

#[test]
fn cockpit_reports_append_the_current_wire_without_renumbering_fields() {
    let mut p = player();
    p.recent_deliveries = [7, 17, 0, 300];
    p.population_used = 5;
    p.unit_counts[2] = 5;
    assert_eq!(p.unit_counts.as_slice().len(), UnitKind::ALL.len());
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
            1, 1, 0, 0, 1, 0, 200, 1, 200, 1, 100, 200, 1, 5, 5, 0, 0, 100, 100, 100, 7, 17, 0,
            172, 2, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
}
