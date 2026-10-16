use aoe_core::EntityId;
use aoe_protocol::{
    GameplayClientMessage, UnitKind, decode_gameplay_client, encode_gameplay_client,
};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn gather_and_train_encode_the_selected_actor_and_sequence() {
    for message in [
        GameplayClientMessage::GatherOrder {
            sequence: 17,
            entity_id: EntityId(3),
            resource_id: 987654321,
        },
        GameplayClientMessage::TrainOrder {
            sequence: 18,
            building_id: EntityId(5),
            kind: UnitKind::Villager,
        },
    ] {
        assert_eq!(
            decode_gameplay_client(&encode_gameplay_client(&message).unwrap()).unwrap(),
            message
        );
    }
}

#[wasm_bindgen_test]
fn economy_refusals_explain_food_population_ownership_and_exhaustion() {
    use crate::playground::game::map::feedback::order_message;
    use aoe_protocol::CommandResult::*;
    for (result, words) in [
        (RejectedInsufficientFood, "not enough food"),
        (RejectedPopulationCap, "population limit"),
        (RejectedNotOwner, "another player"),
        (RejectedInvalidResource, "exhausted"),
    ] {
        assert!(order_message(result, None).contains(words));
    }
}
