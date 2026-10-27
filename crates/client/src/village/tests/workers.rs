use crate::playground::game::village::hud::worker_resource;
use aoe_core::{EntityId, PlayerId, WorldPosition};
use aoe_protocol::{CarriedResource, GameplayUnitState, ResourceKind, UnitKind, UnitTask};
use wasm_bindgen_test::wasm_bindgen_test;

pub(super) fn villager(task: UnitTask) -> GameplayUnitState {
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
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        stance: Default::default(),
    }
}

/// AoE2's top bar counts every villager on a food, wood, gold or stone job,
/// whatever stage of the job it is in.
#[wasm_bindgen_test]
fn every_stage_of_every_food_job_counts_as_a_food_villager() {
    for task in [
        UnitTask::Gathering,
        UnitTask::Farming,
        UnitTask::Fishing,
        UnitTask::Moving,
        UnitTask::Returning,
        UnitTask::Carrying,
    ] {
        let mut u = villager(task);
        u.gathering = Some(ResourceKind::Food);
        assert_eq!(worker_resource(&u), Some(ResourceKind::Food), "{task:?}");
        u.gathering = Some(ResourceKind::Wood);
        assert_eq!(worker_resource(&u), Some(ResourceKind::Wood), "{task:?}");
    }
    // Hunters and shepherds before the carcass job have only a hunt target.
    for task in [UnitTask::Hunting, UnitTask::Shepherding, UnitTask::Moving] {
        let mut u = villager(task);
        u.hunt_target = Some(EntityId(9));
        assert_eq!(worker_resource(&u), Some(ResourceKind::Food), "{task:?}");
    }
    for task in [
        UnitTask::Idle,
        UnitTask::Building,
        UnitTask::Attacking,
        UnitTask::Moving,
        UnitTask::Carrying,
    ] {
        let mut u = villager(task);
        u.carried = Some(CarriedResource {
            kind: ResourceKind::Food,
            amount: 3,
        });
        assert_eq!(worker_resource(&u), None, "{task:?} without a job");
    }
    let mut dead = villager(UnitTask::Gathering);
    dead.gathering = Some(ResourceKind::Gold);
    dead.hp = 0;
    assert_eq!(worker_resource(&dead), None);
}
