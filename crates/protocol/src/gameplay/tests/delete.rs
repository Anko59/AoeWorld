// Current wire: a bounded, unique list of owned entity IDs follows stance orders.
use super::*;
use aoe_core::MAX_FORMATION_MEMBERS;

#[test]
fn delete_orders_have_a_bounded_unique_current_golden_wire() {
    let order = ClientMessage::DeleteOrder {
        sequence: 1,
        entity_ids: vec![EntityId(2), EntityId(3)],
    };
    let wire = [31, 1, 2, 2, 3];
    assert_eq!(encode_client(&order).unwrap(), wire);
    assert_eq!(decode_client(&wire).unwrap(), order);
    for ids in [
        vec![],
        vec![EntityId(1); 2],
        vec![EntityId(1); MAX_FORMATION_MEMBERS + 1],
    ] {
        let mut invalid = order.clone();
        if let ClientMessage::DeleteOrder { entity_ids, .. } = &mut invalid {
            *entity_ids = ids;
        }
        assert!(encode_client(&invalid).is_err());
        assert!(decode_client(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    }
}
