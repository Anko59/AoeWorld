use aoe_core::EntityId;
use aoe_protocol::{
    GameplayClientMessage, UnitKind, decode_gameplay_client, encode_gameplay_client,
};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn right_click_prioritizes_hunt_and_enemy_attack_before_gather_or_move() {
    use crate::unit_visuals::{ClickOrder, click_order};
    use aoe_protocol::UnitKind::*;
    assert_eq!(
        click_order(Villager, false, true, false, true),
        ClickOrder::Hunt
    );
    assert_eq!(
        click_order(Militia, false, true, false, true),
        ClickOrder::Attack
    );
    assert_eq!(
        click_order(Scout, false, false, true, true),
        ClickOrder::Attack
    );
    assert_eq!(
        click_order(Villager, false, false, true, true),
        ClickOrder::Attack
    );
    assert_eq!(
        click_order(Villager, false, false, false, true),
        ClickOrder::Gather
    );
    assert_eq!(
        click_order(Militia, false, false, false, true),
        ClickOrder::Move
    );
    assert_eq!(
        click_order(Villager, true, true, true, true),
        ClickOrder::Move
    );
}

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

#[wasm_bindgen_test]
fn actor_hotspots_outside_native_pixels_remain_pickable_without_a_large_ground_hitbox() {
    use crate::unit_visuals::actor_hit;
    let bounds = [90.0, 160.0, 110.0, 190.0];
    let foot = [100.0, 200.0];
    assert!(actor_hit(bounds, foot, [100.0, 175.0], 1.0));
    assert!(actor_hit(bounds, foot, foot, 1.0));
    assert!(!actor_hit(bounds, foot, [106.0, 200.0], 1.0));
    assert!(!actor_hit(bounds, foot, [100.0, 206.0], 1.0));
}
