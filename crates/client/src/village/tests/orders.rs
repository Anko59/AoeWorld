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
        (RejectedVassalLeash, "100 tiles"),
        (RejectedVassalHome, "home Castle"),
        (RejectedVassalExists, "already has a Vassal"),
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

#[wasm_bindgen_test]
fn vassal_poses_use_the_king_for_standing_walking_and_death() {
    use crate::unit_visuals::{Pose, lifecycle_pose, unit_pose};
    use aoe_protocol::{GameplayUnitState, UnitTask};
    let mut u = GameplayUnitState {
        id: EntityId(9),
        player: aoe_core::PlayerId(0),
        position: aoe_core::WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Vassal,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 75,
        max_hp: 75,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
        home_castle: Some(EntityId(8)),
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };
    assert_eq!(unit_pose(&u, None), Pose::KingStand);
    u.moving = true;
    assert_eq!(unit_pose(&u, None), Pose::KingWalk);
    u.moving = false;
    u.hp = 0;
    u.task = UnitTask::Dying;
    u.death_tick = Some(aoe_core::Tick(10));
    assert_eq!(unit_pose(&u, None), Pose::KingDie);
    assert_eq!(
        lifecycle_pose(Pose::KingDie, u.death_tick, 30, 20),
        Pose::KingDecay
    );
}

#[wasm_bindgen_test]
fn general_poses_use_mounted_hero_for_standing_walking_and_death() {
    use crate::unit_visuals::{Pose, lifecycle_pose, unit_pose};
    use aoe_protocol::{GameplayUnitState, UnitTask};
    let mut u = GameplayUnitState {
        id: EntityId(9),
        player: aoe_core::PlayerId(0),
        position: aoe_core::WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::General,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 75,
        max_hp: 75,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
        home_castle: Some(EntityId(8)),
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };
    assert_eq!(unit_pose(&u, None), Pose::GeneralStand);
    u.moving = true;
    assert_eq!(unit_pose(&u, None), Pose::GeneralWalk);
    u.moving = false;
    u.hp = 0;
    u.task = UnitTask::Dying;
    u.death_tick = Some(aoe_core::Tick(10));
    assert_eq!(unit_pose(&u, None), Pose::GeneralDie);
    assert_eq!(
        lifecycle_pose(Pose::GeneralDie, u.death_tick, 39, 20),
        Pose::GeneralDie
    );
    assert_eq!(
        lifecycle_pose(Pose::GeneralDie, u.death_tick, 40, 20),
        Pose::GeneralDecay
    );
}
