// Combined current command order includes bridges and shelter orders before navy.
use super::*;

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
    };
    let wire = [21, 1, 2, 128, 8, 128, 24];
    assert_eq!(encode_client(&order).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), order);
    let state = ServerMessage::MoveQueues {
        revision: 1,
        tick: Tick(3),
        queues: vec![MoveQueue {
            entity_id: EntityId(2),
            destinations: vec![WorldPosition::new(512, 1536)],
        }],
    };
    let wire = [11, 1, 3, 1, 2, 1, 128, 8, 128, 24];
    assert_eq!(encode_server(&state).unwrap(), wire);
    assert_eq!(decode_server(&wire).unwrap(), state);
}

#[test]
fn move_queue_decoding_and_encoding_enforce_bounds_order_and_positions() {
    let queue = |id, count| MoveQueue {
        entity_id: EntityId(id),
        destinations: vec![WorldPosition::new(512, 512); count],
    };
    for queues in [
        vec![queue(1, 17)],
        vec![queue(1, 0)],
        vec![queue(2, 1), queue(1, 1)],
        vec![MoveQueue {
            entity_id: EntityId(1),
            destinations: vec![WorldPosition::new(-1, 0)],
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
