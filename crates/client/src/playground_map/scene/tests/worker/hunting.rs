use super::*;

#[wasm_bindgen_test]
fn hunt_work_poses_play_only_in_reach_and_never_while_a_route_is_planned() {
    use crate::unit_visuals::{Pose, unit_pose};
    use aoe_core::FIXED_SUBUNITS_PER_TILE as TILE;
    use aoe_protocol::{AnimalKind, AnimalState, AnimalTask};
    let animal = |kind, x: i32, hp| AnimalState {
        id: EntityId(9),
        kind,
        owner: Some(PlayerId(0)),
        hp,
        position: WorldPosition::new(x, 0),
        task: AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 0,
        moving: false,
        death_tick: None,
    };
    let mut u = villager(UnitTask::Hunting, None);
    u.hunt_target = Some(EntityId(9));
    // Bow reach is four tiles: a deer six tiles away is not shot at.
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Deer, 6 * TILE, 5))),
        Pose::HunterStand
    );
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Deer, 4 * TILE, 5))),
        Pose::HunterAttack
    );
    u.planning = true;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Deer, 4 * TILE, 5))),
        Pose::HunterStand
    );
    u.planning = false;
    u.task = UnitTask::Attacking;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Wolf, 4 * TILE, 25))),
        Pose::VillagerStand
    );
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Wolf, TILE, 25))),
        Pose::VillagerAttack
    );
    // A shepherd kills only beside its sheep, then carves the carcass.
    u.task = UnitTask::Shepherding;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Sheep, 3 * TILE, 7))),
        Pose::ShepherdStand
    );
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Sheep, TILE, 7))),
        Pose::ShepherdWork
    );
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Sheep, TILE, 0))),
        Pose::Butcher
    );
    u.task = UnitTask::Hunting;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Deer, 4 * TILE, 0))),
        Pose::HunterStand
    );
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Deer, TILE, 0))),
        Pose::Butcher
    );
    u.task = UnitTask::Shepherding;
    u.moving = true;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Sheep, 3 * TILE, 7))),
        Pose::ShepherdWalk
    );
}
