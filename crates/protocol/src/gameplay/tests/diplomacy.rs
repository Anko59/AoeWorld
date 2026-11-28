use super::*;

#[test]
fn diplomacy_messages_pin_the_current_wire() {
    let stance = ClientMessage::DiplomacyOrder {
        sequence: 7,
        player: PlayerId(2),
        stance: DiplomaticStance::Ally,
    };
    let tribute = ClientMessage::TributeOrder {
        sequence: 8,
        player: PlayerId(2),
        market: EntityId(9),
        resource: ResourceKind::Wood,
        amount: 100,
    };
    for (message, bytes) in [
        (stance, vec![50, 7, 2, 2]),
        (tribute, vec![51, 8, 2, 9, 1, 100]),
    ] {
        assert_eq!(encode_client(&message).unwrap(), bytes);
        assert_eq!(decode_client(&bytes).unwrap(), message);
    }

    let view = ServerMessage::Diplomacy(DiplomacyView {
        revision: 7,
        viewer: PlayerId(0),
        seats: vec![DiplomacySeat {
            player: PlayerId(2),
            name: Some("Lyon".to_owned()),
            stance: DiplomaticStance::Ally,
            offer: AllianceOffer::None,
        }],
        markets: vec![AlliedMarket {
            player: PlayerId(2),
            tile: TileCoord::new(3, 4),
        }],
        tribute_market: Some(EntityId(9)),
        tribute_fee_permyriad: 0,
    });
    let bytes = [
        15, 7, 0, 1, 2, 1, 4, 76, 121, 111, 110, 2, 0, 1, 2, 6, 8, 1, 9, 0,
    ];
    assert_eq!(encode_server(&view).unwrap(), bytes);
    assert_eq!(decode_server(&bytes).unwrap(), view);
}

#[test]
fn diplomacy_rejects_non_allied_markets_and_unbounded_names() {
    let seat = DiplomacySeat {
        player: PlayerId(2),
        name: None,
        stance: DiplomaticStance::Neutral,
        offer: AllianceOffer::None,
    };
    let base = DiplomacyView {
        revision: 1,
        seats: vec![seat.clone()],
        ..DiplomacyView::default()
    };
    let misplaced = ServerMessage::Diplomacy(DiplomacyView {
        markets: vec![AlliedMarket {
            player: PlayerId(2),
            tile: TileCoord::new(3, 4),
        }],
        ..base.clone()
    });
    let long_name = ServerMessage::Diplomacy(DiplomacyView {
        seats: vec![DiplomacySeat {
            name: Some("a".repeat(MAX_DIPLOMACY_NAME + 1)),
            ..seat
        }],
        ..base
    });
    for invalid in [misplaced, long_name] {
        assert!(encode_server(&invalid).is_err());
        assert!(decode_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    }
}
