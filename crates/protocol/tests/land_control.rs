use aoe_core::{PlayerId, Tick};
use aoe_protocol::*;
#[test]
fn boundaries_have_an_independent_appended_wire_and_are_bounded() {
    let mut message = GameplayServerMessage::Holdings {
        revision: 1,
        tick: Tick(20),
        shapes: vec![HoldingSpan {
            player: PlayerId(3),
            y: 5,
            x0: 7,
            x1: 9,
        }],
        complete: true,
    };
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(bytes, [11, 1, 20, 1, 3, 10, 14, 18, 1]);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    if let GameplayServerMessage::Holdings { shapes, .. } = &mut message {
        shapes[0].x1 = 7;
    }
    assert!(encode_gameplay_server(&message).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
    if let GameplayServerMessage::Holdings { shapes, .. } = &mut message {
        *shapes = vec![
            HoldingSpan {
                player: PlayerId(0),
                y: 0,
                x0: 0,
                x1: 1
            };
            MAX_HOLDING_SPANS + 1
        ];
    }
    assert!(encode_gameplay_server(&message).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
}
