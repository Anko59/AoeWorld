use super::*;

#[test]
fn relic_orders_and_view_round_trip_at_the_current_version() {
    let pickup = ClientMessage::RelicOrder {
        sequence: 6,
        action: RelicAction::PickUp {
            monk: EntityId(2),
            relic: EntityId(4),
        },
    };
    assert_eq!(encode_client(&pickup).unwrap(), [49, 6, 0, 2, 4]);
    assert_eq!(decode_client(&[49, 6, 0, 2, 4]).unwrap(), pickup);
    let view = ServerMessage::Relics(RelicView {
        revision: 7,
        ground: vec![GroundRelic {
            id: EntityId(4),
            position: WorldPosition::new(3, 5),
        }],
        carriers: vec![EntityId(6)],
        monasteries: vec![MonasteryRelics {
            id: EntityId(12),
            count: 2,
            full: false,
        }],
        victory: Some(RelicVictory {
            leader: PlayerId(1),
            remaining_seconds: 10,
        }),
    });
    let bytes = [14, 7, 1, 4, 6, 10, 1, 6, 1, 12, 2, 0, 1, 1, 10];
    assert_eq!(encode_server(&view).unwrap(), bytes);
    assert_eq!(decode_server(&bytes).unwrap(), view);
}

#[test]
fn relic_views_reject_duplicate_ids_and_excess_counts() {
    let mark = GroundRelic {
        id: EntityId(3),
        position: WorldPosition::new(10, 10),
    };
    let malformed = ServerMessage::Relics(RelicView {
        revision: 1,
        ground: vec![mark, mark],
        ..RelicView::default()
    });
    assert!(encode_server(&malformed).is_err());
    assert!(decode_server(&postcard::to_allocvec(&malformed).unwrap()).is_err());
    let excess = ServerMessage::Relics(RelicView {
        revision: 1,
        carriers: (0..=MAX_RELICS).map(|i| EntityId(i as u32)).collect(),
        ..RelicView::default()
    });
    assert!(encode_server(&excess).is_err());
    assert!(decode_server(&postcard::to_allocvec(&excess).unwrap()).is_err());
}
