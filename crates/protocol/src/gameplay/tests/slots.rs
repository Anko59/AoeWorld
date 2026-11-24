use super::*;

#[test]
fn join_slot_is_an_appended_variant_with_a_pinned_wire_form() {
    // Variants 0..=18 are the browser's; the slot join must stay at 19.
    let hello = ClientMessage::Hello {
        version: VERSION,
        resume_token: None,
    };
    assert_eq!(encode_client(&hello).unwrap(), [0, 29, 0]);
    let mut token = [0_u8; 24];
    for (index, byte) in token.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let join = ClientMessage::JoinSlot {
        version: VERSION,
        token: ResumeToken(token),
    };
    let mut expected = vec![19, 29];
    expected.extend(0..24_u8);
    assert_eq!(encode_client(&join).unwrap(), expected);
    assert_eq!(decode_client(&expected).unwrap(), join);
}

#[test]
fn large_selection_and_training_click_have_one_bounded_wire_form() {
    let ids: Vec<_> = (2..1002).map(EntityId).collect();
    let attack = ClientMessage::AttackSelectionOrder {
        sequence: 3,
        entity_ids: ids,
        target_id: EntityId(1002),
    };
    let frame = encode_client(&attack).unwrap();
    assert_eq!(&frame[..4], &[42, 3, 232, 7]);
    assert_eq!(frame.len(), 1_880);
    assert_eq!(decode_client(&frame).unwrap(), attack);

    let batch = |count| ClientMessage::TrainBatchOrder {
        sequence: 4,
        kind: UnitKind::Villager,
        building_ids: vec![EntityId(40); count],
    };
    assert_eq!(
        encode_client(&batch(5)).unwrap(),
        [43, 4, 0, 5, 40, 40, 40, 40, 40]
    );
    assert!(encode_client(&batch(0)).is_err());
    assert!(encode_client(&batch(6)).is_err());
    assert!(decode_client(&[43, 4, 0, 6, 40, 40, 40, 40, 40, 40]).is_err());
}

#[test]
fn gather_patrol_garrison_and_general_walk_take_a_bounded_unit_list() {
    let ids = |count: u32| (2..2 + count).map(EntityId).collect::<Vec<_>>();
    let at = WorldPosition { x: 4096, y: 8192 };
    let orders = |count| {
        [
            ClientMessage::GatherSelectionOrder {
                sequence: 5,
                entity_ids: ids(count),
                resource_id: 9,
            },
            ClientMessage::PatrolSelectionOrder {
                sequence: 5,
                entity_ids: ids(count),
                destination: at,
            },
            ClientMessage::GarrisonSelectionOrder {
                sequence: 5,
                entity_ids: ids(count),
                building_id: EntityId(1),
            },
            ClientMessage::GeneralMoveAloneSelectionOrder {
                sequence: 5,
                entity_ids: ids(count),
                destination: at,
            },
        ]
    };
    let variants = orders(2)
        .iter()
        .map(|message| encode_client(message).unwrap()[0])
        .collect::<Vec<_>>();
    assert_eq!(variants, [45, 46, 47, 48]);
    for message in orders(aoe_core::MAX_FORMATION_MEMBERS as u32) {
        assert_eq!(
            decode_client(&encode_client(&message).unwrap()).unwrap(),
            message
        );
    }
    for message in orders(aoe_core::MAX_FORMATION_MEMBERS as u32 + 1) {
        assert!(
            encode_client(&message).is_err()
                || decode_client(&encode_client(&message).unwrap()).is_err()
        );
    }
}
