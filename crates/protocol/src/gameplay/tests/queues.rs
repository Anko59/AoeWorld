// Combined current command order includes bridges and shelter orders before navy.
use super::*;
use aoe_core::MAX_MOVE_DESTINATIONS;

#[test]
fn transport_orders_pin_current_wire_and_bound_the_load_group() {
    for (order, bytes) in [
        (
            ClientMessage::LoadOrder {
                sequence: 1,
                unit_ids: vec![EntityId(2), EntityId(3)],
                transport_id: EntityId(4),
            },
            vec![27, 1, 2, 2, 3, 4],
        ),
        (
            ClientMessage::UnloadOrder {
                sequence: 2,
                transport_id: EntityId(4),
                destination: TileCoord::new(12, 13),
            },
            vec![28, 2, 4, 24, 26],
        ),
    ] {
        assert_eq!(encode_client(&order).unwrap(), bytes);
        assert_eq!(decode_client(&bytes).unwrap(), order);
    }
    let oversized = ClientMessage::LoadOrder {
        sequence: 1,
        unit_ids: vec![EntityId(2); MAX_BUILD_GROUP + 1],
        transport_id: EntityId(4),
    };
    assert!(decode_client(&postcard::to_allocvec(&oversized).unwrap()).is_err());
}

#[test]
fn queued_move_and_authoritative_markers_have_current_golden_wire() {
    let order = ClientMessage::QueueMoveOrder {
        sequence: 1,
        entity_id: EntityId(2),
        destination: WorldPosition::new(512, 1536),
        attack_move: true,
    };
    let wire = [21, 1, 2, 128, 8, 128, 24, 1];
    assert_eq!(encode_client(&order).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), order);
    let state = ServerMessage::MoveQueues {
        revision: 1,
        tick: Tick(3),
        queues: vec![MoveQueue {
            entity_id: EntityId(2),
            destinations: vec![WorldPosition::new(512, 1536), WorldPosition::new(768, 1536)],
            order_ids: vec![0, 42],
            attack_move_mask: 0b10,
        }],
    };
    let wire = [
        10, 1, 3, 1, 2, 2, 128, 8, 128, 24, 128, 12, 128, 24, 2, 2, 0, 42,
    ];
    assert_eq!(encode_server(&state).unwrap(), wire);
    assert_eq!(decode_server(&wire).unwrap(), state);
}

#[test]
fn move_order_has_current_golden_wire() {
    let order = ClientMessage::MoveOrder {
        sequence: 1,
        entity_id: EntityId(2),
        destination: WorldPosition::new(512, 1536),
        attack_move: true,
    };
    let wire = [2, 1, 2, 128, 8, 128, 24, 1];
    assert_eq!(encode_client(&order).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), order);
}

#[test]
fn move_queue_decoding_and_encoding_enforce_bounds_order_and_positions() {
    let queue = |id, count| MoveQueue {
        entity_id: EntityId(id),
        destinations: vec![WorldPosition::new(512, 512); count],
        attack_move_mask: 0,
        order_ids: vec![0; count],
    };
    for queues in [
        vec![queue(1, 17)],
        vec![queue(1, 0)],
        vec![MoveQueue {
            order_ids: vec![],
            ..queue(1, 1)
        }],
        vec![MoveQueue {
            order_ids: vec![1; 17],
            ..queue(1, 1)
        }],
        vec![queue(2, 1), queue(1, 1)],
        vec![MoveQueue {
            entity_id: EntityId(1),
            destinations: vec![WorldPosition::new(-1, 0)],
            attack_move_mask: 1,
            order_ids: vec![0],
        }],
        vec![MoveQueue {
            order_ids: vec![0, 42],
            attack_move_mask: 0b10, // The second destination is absent.
            ..queue(1, 1)
        }],
    ] {
        let state = ServerMessage::MoveQueues {
            revision: 1,
            tick: Tick(1),
            queues,
        };
        assert!(encode_server(&state).is_err());
        assert!(decode_server(&postcard::to_allocvec(&state).unwrap()).is_err());
    }
}

#[test]
fn attack_move_mask_reaches_the_last_waypoint_at_full_queue_width() {
    let queue = MoveQueue {
        entity_id: EntityId(1),
        destinations: vec![WorldPosition::new(512, 512); MAX_MOVE_DESTINATIONS],
        attack_move_mask: 1 << (MAX_MOVE_DESTINATIONS - 1),
        order_ids: vec![0; MAX_MOVE_DESTINATIONS],
    };
    assert!(!queue.is_attack_move(0));
    assert!(queue.is_attack_move(MAX_MOVE_DESTINATIONS - 1));
    assert!(!queue.is_attack_move(MAX_MOVE_DESTINATIONS));
    let state = ServerMessage::MoveQueues {
        revision: 1,
        tick: Tick(1),
        queues: vec![queue],
    };
    assert_eq!(
        decode_server(&encode_server(&state).unwrap()).unwrap(),
        state
    );
}
