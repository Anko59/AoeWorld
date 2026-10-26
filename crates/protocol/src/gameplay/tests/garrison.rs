// Combined wire appends bridges before shelter commands, then navy and formations.
use super::*;

#[test]
fn garrison_and_selective_or_all_ungarrison_have_current_golden_wire() {
    for (order, golden) in [
        (
            ClientMessage::GarrisonOrder {
                sequence: 3,
                entity_id: EntityId(7),
                building_id: EntityId(9),
            },
            vec![25, 3, 7, 9],
        ),
        (
            ClientMessage::UngarrisonOrder {
                sequence: 4,
                building_id: EntityId(9),
                entity_id: None,
            },
            vec![26, 4, 9, 0],
        ),
        (
            ClientMessage::UngarrisonOrder {
                sequence: 5,
                building_id: EntityId(9),
                entity_id: Some(EntityId(7)),
            },
            vec![26, 5, 9, 1, 7],
        ),
    ] {
        let bytes = encode_client(&order).unwrap();
        assert_eq!(bytes, golden);
        assert_eq!(decode_client(&bytes).unwrap(), order);
    }
    let ack = ServerMessage::CommandAck {
        sequence: 3,
        result: CommandResult::RejectedGarrisonFull,
        applied_tick: Tick(4),
    };
    assert_eq!(encode_server(&ack).unwrap(), [3, 3, 34, 4]);
}
