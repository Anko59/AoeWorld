// Combined current wire: bridge, garrison, ejection, load and unload precede formation.
use super::*;
use aoe_core::{Formation, MAX_FORMATION_MEMBERS};

#[test]
fn stance_orders_have_a_bounded_current_golden_wire() {
    for (stance, code) in [
        (CombatStance::Aggressive, 0),
        (CombatStance::Defensive, 1),
        (CombatStance::StandGround, 2),
        (CombatStance::NoAttack, 3),
    ] {
        let order = ClientMessage::StanceOrder {
            sequence: 1,
            unit_ids: vec![EntityId(2), EntityId(3)],
            stance,
        };
        let wire = [30, 1, 2, 2, 3, code];
        assert_eq!(encode_client(&order).unwrap(), wire);
        assert_eq!(decode_client(&wire).unwrap(), order);
        for ids in [
            vec![],
            vec![EntityId(1); 2],
            vec![EntityId(1); MAX_FORMATION_MEMBERS + 1],
        ] {
            let order = ClientMessage::StanceOrder {
                sequence: 1,
                unit_ids: ids,
                stance,
            };
            assert!(encode_client(&order).is_err());
            assert!(decode_client(&postcard::to_allocvec(&order).unwrap()).is_err());
        }
    }
}

#[test]
fn formation_order_has_a_bounded_current_golden_wire() {
    let message = ClientMessage::FormationOrder {
        sequence: 1,
        unit_ids: vec![EntityId(2), EntityId(3)],
        formation: Formation::Flank,
        destination: Some(WorldPosition::new(512, 1536)),
        queued: true,
        attack_move: false,
    };
    // The current formation wire uses an optional destination so a posture
    // button rearranges an army immediately without replacing its route/FIFO.
    let wire = [29, 1, 2, 2, 3, 3, 1, 128, 8, 128, 24, 1, 0];
    assert_eq!(encode_client(&message).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), message);
    for ids in [
        vec![],
        vec![EntityId(1); 2],
        vec![EntityId(1); MAX_FORMATION_MEMBERS + 1],
    ] {
        let mut invalid = message.clone();
        if let ClientMessage::FormationOrder { unit_ids, .. } = &mut invalid {
            *unit_ids = ids;
        }
        assert!(encode_client(&invalid).is_err());
        assert!(decode_client(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    }
    let mut invalid = message;
    if let ClientMessage::FormationOrder { destination, .. } = &mut invalid {
        *destination = Some(WorldPosition::new(-1, 1536));
    }
    assert!(encode_client(&invalid).is_err());
    assert!(decode_client(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    assert!(decode_client(&wire[..wire.len() - 1]).is_err());
}

#[test]
fn formation_posture_wire_has_no_destination_or_movement_flags() {
    let message = ClientMessage::FormationOrder {
        sequence: 1,
        unit_ids: vec![EntityId(2), EntityId(3)],
        formation: Formation::Box,
        destination: None,
        queued: false,
        attack_move: false,
    };
    let wire = [29, 1, 2, 2, 3, 1, 0, 0, 0];
    assert_eq!(encode_client(&message).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), message);
    for flag in [7, 8] {
        let mut invalid = wire;
        invalid[flag] = 1;
        assert!(decode_client(&invalid).is_err());
    }
}

#[test]
fn general_only_move_has_a_current_golden_wire() {
    let message = ClientMessage::GeneralOrder {
        sequence: 1,
        entity_id: EntityId(2),
        command: crate::GeneralCommand::MoveAlone {
            destination: WorldPosition::new(512, 1536),
        },
    };
    let wire = encode_client(&message).unwrap();
    assert_eq!(wire, [31, 1, 2, 0, 128, 8, 128, 24]);
    assert_eq!(decode_client(&wire).unwrap(), message);
    let negative = ClientMessage::GeneralOrder {
        sequence: 1,
        entity_id: EntityId(2),
        command: crate::GeneralCommand::MoveAlone {
            destination: WorldPosition::new(-1, 0),
        },
    };
    assert!(encode_client(&negative).is_err());
}

#[test]
fn general_training_orders_have_a_current_golden_wire() {
    let train = ClientMessage::GeneralOrder {
        sequence: 1,
        entity_id: EntityId(2),
        command: crate::GeneralCommand::SetTraining { enabled: true },
    };
    let wire = encode_client(&train).unwrap();
    assert_eq!(wire, [31, 1, 2, 1, 1]);
    assert_eq!(decode_client(&wire).unwrap(), train);
    let mix = ClientMessage::GeneralOrder {
        sequence: 1,
        entity_id: EntityId(2),
        command: crate::GeneralCommand::SetMix {
            mix: [35, 30, 20, 10, 5],
        },
    };
    let wire = encode_client(&mix).unwrap();
    assert_eq!(wire, [31, 1, 2, 2, 35, 30, 20, 10, 5]);
    assert_eq!(decode_client(&wire).unwrap(), mix);
    let too_much = ClientMessage::GeneralOrder {
        sequence: 1,
        entity_id: EntityId(2),
        command: crate::GeneralCommand::SetMix {
            mix: [101, 0, 0, 0, 0],
        },
    };
    assert!(encode_client(&too_much).is_err());
}
