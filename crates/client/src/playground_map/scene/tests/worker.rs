use super::*;
use aoe_core::{EntityId, PlayerId, WorldPosition};
use aoe_protocol::{GameplayUnitState, ResourceKind, UnitKind, UnitTask};
use aoe_rendering::SceneUnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

fn villager(task: UnitTask, gathering: Option<ResourceKind>) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(1),
        player: PlayerId(0),
        position: WorldPosition::new(0, 0),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task,
        carried: None,
        gathering,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
    }
}

// The trial lacks a farmer: use foraging; construction uses dat builder 1496.
#[wasm_bindgen_test]
fn farmers_use_foraging_and_builders_use_the_dat_task_art() {
    let farmer = villager(UnitTask::Farming, Some(ResourceKind::Food));
    assert_eq!(worker_kind(&farmer), SceneUnitKind::VillagerForaging);
    let builder = villager(UnitTask::Building, None);
    assert_eq!(worker_kind(&builder), SceneUnitKind::VillagerBuilding);
}

#[wasm_bindgen_test]
fn militia_combat_and_hunting_never_use_the_generic_worker_pose() {
    use crate::unit_visuals::{Pose, lifecycle_pose, packed_frame, unit_pose};
    let mut u = villager(UnitTask::Idle, None);
    u.kind = UnitKind::Militia;
    assert_eq!(unit_pose(&u, None), Pose::MilitiaStand);
    u.moving = true;
    assert_eq!(unit_pose(&u, None), Pose::MilitiaWalk);
    u.moving = false;
    u.task = UnitTask::Attacking;
    assert_eq!(unit_pose(&u, None), Pose::MilitiaAttack);
    u.kind = UnitKind::Villager;
    assert_eq!(unit_pose(&u, None), Pose::VillagerAttack);
    u.task = UnitTask::Hunting;
    assert_eq!(unit_pose(&u, None), Pose::HunterAttack);
    u.task = UnitTask::Shepherding;
    assert_eq!(unit_pose(&u, None), Pose::ShepherdWork);
    u.hunt_target = Some(EntityId(9));
    u.task = UnitTask::Gathering;
    assert_eq!(unit_pose(&u, None), Pose::Butcher);
    u.moving = true;
    assert_eq!(unit_pose(&u, None), Pose::HunterWalk);
    u.moving = false;
    u.task = UnitTask::Returning;
    assert_eq!(unit_pose(&u, None), Pose::HunterCarry);
    u.hp = 0;
    u.death_hunt = Some(aoe_protocol::AnimalKind::Sheep);
    assert_eq!(unit_pose(&u, None), Pose::ShepherdDie);
    u.death_hunt = Some(aoe_protocol::AnimalKind::Deer);
    assert_eq!(unit_pose(&u, None), Pose::HunterDie);
    // Every facing retains its row and native mirror, including wraparound.
    for (facing, row, mirrored) in [
        (0, 0, false),
        (1, 1, true),
        (2, 2, true),
        (3, 3, true),
        (4, 4, false),
        (5, 3, false),
        (6, 2, false),
        (7, 1, false),
        (8, 0, false),
    ] {
        assert_eq!(
            aoe_rendering::actor_frame_index(facing, 12, 7),
            (row * 12 + 7, mirrored)
        );
    }
    // A death never loops, including a reconnect halfway through decay.
    let died = Some(aoe_core::Tick(100));
    assert_eq!(
        packed_frame(Pose::MilitiaDie, 2, 0.0, died, 100, 20) >> 3,
        0
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDie, 2, 0.0, died, 110, 20) >> 3,
        5
    );
    assert_eq!(
        lifecycle_pose(Pose::MilitiaDie, died, 119, 20),
        Pose::MilitiaDie
    );
    assert_eq!(
        lifecycle_pose(Pose::MilitiaDie, died, 120, 20),
        Pose::MilitiaDecay
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDecay, 2, 0.0, died, 120, 20) >> 3,
        0
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDecay, 2, 0.0, died, 600, 20) >> 3,
        4
    );
}

#[wasm_bindgen_test]
fn neutral_sheep_and_owned_sheep_select_separate_player_planes() {
    use crate::unit_visuals::{Pose, animal_pose};
    let mut sheep = aoe_protocol::AnimalState {
        id: EntityId(9),
        kind: aoe_protocol::AnimalKind::Sheep,
        owner: None,
        hp: 7,
        position: WorldPosition::new(0, 0),
        task: aoe_protocol::AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 0,
        moving: false,
        death_tick: None,
    };
    assert_eq!(animal_pose(&sheep), Pose::NeutralSheepStand);
    sheep.owner = Some(PlayerId(0));
    assert_eq!(animal_pose(&sheep), Pose::SheepStand);
}
