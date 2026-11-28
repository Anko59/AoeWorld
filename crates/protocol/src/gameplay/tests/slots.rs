use super::*;

#[test]
fn join_slot_is_an_appended_variant_with_a_pinned_wire_form() {
    // Variants 0..=18 are the browser's; the slot join must stay at 19.
    let hello = ClientMessage::Hello {
        version: VERSION,
        resume_token: None,
    };
    assert_eq!(encode_client(&hello).unwrap(), [0, 36, 0]);
    let mut token = [0_u8; 24];
    for (index, byte) in token.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let join = ClientMessage::JoinSlot {
        version: VERSION,
        token: ResumeToken(token),
    };
    let mut expected = vec![19, 36];
    expected.extend(0..24_u8);
    assert_eq!(encode_client(&join).unwrap(), expected);
    assert_eq!(decode_client(&expected).unwrap(), join);
}

/// A seat's credential must not reach a log or a panic through `{:?}`, neither
/// alone nor inside the messages that carry it. A derived Debug fails this.
#[test]
fn no_debug_output_holds_a_token() {
    let token = ResumeToken(std::array::from_fn(|index| 201 + index as u8));
    let hello = ClientMessage::Hello {
        version: VERSION,
        resume_token: Some(token),
    };
    let join = ClientMessage::JoinSlot {
        version: VERSION,
        token,
    };
    let welcome = ServerMessage::Welcome {
        version: VERSION,
        world_id: 11,
        map_content_hash: None,
        map_metadata: None,
        width_tiles: 64,
        height_tiles: 64,
        coordinate_precision: 1_024,
        tick_hz: 20,
        primary_unit_id: EntityId(0),
        resume_token: token,
    };
    let printed = [
        format!("{token:?}"),
        format!("{token:#?}"),
        format!("{token:x?}"),
        format!("{:?}", Some(token)),
        format!("{hello:?}"),
        format!("{join:?}"),
        format!("{welcome:?}"),
        format!("{welcome:#?}"),
    ];
    let hex: String = token.0.iter().map(|byte| format!("{byte:02x}")).collect();
    for text in printed {
        assert!(text.contains("ResumeToken"), "{text}");
        assert!(!text.contains(&hex), "{text}");
        assert!(!text.contains("[c9, ca"), "{text}");
        for byte in token.0 {
            assert!(!text.contains(&byte.to_string()), "{byte} in {text}");
        }
    }
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

#[test]
fn office_zone_batch_has_a_bounded_current_wire_form() {
    let chunk = (TileCoord::new(1, 2), [1; 32]);
    let message = |chunks, start, complete| ServerMessage::OfficeZone {
        office: EntityId(7),
        vassal: true,
        start,
        complete,
        chunks,
    };
    let one = message(vec![chunk], true, true);
    let mut golden = vec![12, 7, 1, 1, 1, 1, 2, 4];
    golden.extend([1; 32]);
    assert_eq!(encode_server(&one).unwrap(), golden);
    assert_eq!(decode_server(&golden).unwrap(), one);
    let oversized = message(
        (0..=MAX_OFFICE_ZONE_CHUNKS)
            .map(|x| (TileCoord::new(x as i32, 0), [u32::MAX; 32]))
            .collect(),
        true,
        false,
    );
    for invalid in [
        oversized,
        message(vec![chunk, chunk], true, true),
        message(vec![(TileCoord::new(-1, 0), [1; 32])], true, true),
        message(vec![(TileCoord::new(0, 0), [0; 32])], true, true),
        message(vec![], false, false),
    ] {
        assert!(encode_server(&invalid).is_err());
        assert!(decode_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    }
    let shape: Vec<_> = (0..=MAX_OFFICE_ZONE_CHUNKS)
        .map(|x| (TileCoord::new(x as i32, 0), [1; 32]))
        .collect();
    let (first, next) = ServerMessage::office_zone_batch(EntityId(7), true, &shape, 0).unwrap();
    let (last, end) = ServerMessage::office_zone_batch(EntityId(7), true, &shape, next).unwrap();
    assert_eq!(first, message(shape[..next].to_vec(), true, false));
    assert_eq!(last, message(shape[next..].to_vec(), false, true));
    assert_eq!(next, MAX_OFFICE_ZONE_CHUNKS);
    assert!(ServerMessage::office_zone_batch(EntityId(7), true, &shape, end).is_none());
    let largest = message(
        (0..MAX_OFFICE_ZONE_CHUNKS)
            .map(|x| (TileCoord::new(32_700 + x as i32, 32_767), [u32::MAX; 32]))
            .collect(),
        true,
        false,
    );
    assert!(encode_server(&largest).unwrap().len() <= 11 * 1024);
}
