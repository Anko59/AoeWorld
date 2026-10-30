use aoe_core::{EntityId, PlayerId, WorldPosition};
use aoe_protocol::{GameplayUnitState, ResourceKind, UnitKind, UnitTask};
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
        auto_scout: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
}

// Farmers till with DAT unit 259's own row; construction uses dat builder 1496.
#[wasm_bindgen_test]
fn farmers_and_builders_use_their_own_dat_task_art() {
    use crate::unit_visuals::{Pose, unit_pose};
    let mut farmer = villager(UnitTask::Farming, Some(ResourceKind::Food));
    assert_eq!(unit_pose(&farmer, None), Pose::FarmerWork);
    assert_ne!(unit_pose(&farmer, None), Pose::ForagerWork);
    farmer.moving = true;
    assert_eq!(unit_pose(&farmer, None), Pose::VillagerWalk);
    let builder = villager(UnitTask::Building, None);
    assert_eq!(unit_pose(&builder, None), Pose::BuilderWork);
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
    // Without its prey in reach a hunter or shepherd stands, never strikes.
    u.task = UnitTask::Hunting;
    assert_eq!(unit_pose(&u, None), Pose::HunterStand);
    u.task = UnitTask::Shepherding;
    assert_eq!(unit_pose(&u, None), Pose::ShepherdStand);
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
        packed_frame(Pose::MilitiaDie, 2, 0.0, died, 100.0, 20) >> 4,
        0
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDie, 2, 0.0, died, 110.0, 20) >> 4,
        14
    );
    assert_eq!(
        lifecycle_pose(Pose::MilitiaDie, died, 119, 20),
        Pose::MilitiaDie
    );
    assert_eq!(
        lifecycle_pose(Pose::MilitiaDie, died, 121, 20),
        Pose::MilitiaDecay
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDecay, 2, 0.0, died, 120.0, 20) >> 4,
        0
    );
    assert_eq!(
        packed_frame(Pose::MilitiaDecay, 2, 0.0, died, 601.0, 20) >> 4,
        24
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

#[wasm_bindgen_test]
fn moving_hunt_jobs_keep_hunter_or_shepherd_walk_even_with_the_moving_task() {
    use crate::unit_visuals::{Pose, unit_pose};
    use aoe_protocol::{AnimalKind, AnimalState, AnimalTask};
    let mut worker = villager(UnitTask::Moving, None);
    worker.moving = true;
    worker.hunt_target = Some(EntityId(9));
    for (kind, expected) in [
        (AnimalKind::Deer, Pose::HunterWalk),
        (AnimalKind::Boar, Pose::HunterWalk),
        (AnimalKind::Sheep, Pose::ShepherdWalk),
    ] {
        let animal = AnimalState {
            id: EntityId(9),
            kind,
            owner: None,
            hp: 1,
            position: worker.position,
            task: AnimalTask::Idle,
            food_remaining: 100,
            target: None,
            facing: 0,
            moving: false,
            death_tick: None,
        };
        assert_eq!(unit_pose(&worker, Some(&animal)), expected, "{kind:?}");
        // Walking to a sheep is an ordinary villager walk, not herding.
        assert_eq!(
            crate::unit_visuals::walks_as_villager(&worker, Some(&animal)),
            kind == AnimalKind::Sheep
        );
        worker.task = if kind == AnimalKind::Sheep {
            UnitTask::Shepherding
        } else {
            UnitTask::Hunting
        };
        assert_eq!(unit_pose(&worker, Some(&animal)), expected, "{kind:?}");
        worker.task = UnitTask::Moving;
    }
    assert_eq!(unit_pose(&worker, None), Pose::HunterWalk);
}

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
    u.moving = true;
    assert_eq!(
        unit_pose(&u, Some(&animal(AnimalKind::Sheep, 3 * TILE, 7))),
        Pose::ShepherdWalk
    );
}

// A soldier waiting out of reach (every melee spot taken, or a route still
// pending) stands; it swings only with its target within reach.
#[wasm_bindgen_test]
fn soldiers_swing_only_with_their_target_in_reach() {
    use crate::unit_visuals::{Pose, scene_pose};
    const TILE: i64 = aoe_core::FIXED_SUBUNITS_PER_TILE as i64;
    let mut u = villager(UnitTask::Attacking, None);
    u.kind = UnitKind::Militia;
    u.attack_target = Some(EntityId(9));
    assert_eq!(scene_pose(&u, None, Some(TILE)), Pose::MilitiaAttack);
    assert_eq!(scene_pose(&u, None, None), Pose::MilitiaAttack);
    assert_eq!(scene_pose(&u, None, Some(4 * TILE)), Pose::MilitiaStand);
    u.moving = true;
    assert_eq!(scene_pose(&u, None, Some(4 * TILE)), Pose::MilitiaWalk);
    u.moving = false;
    u.kind = UnitKind::ManAtArms;
    assert_eq!(scene_pose(&u, None, Some(4 * TILE)), Pose::ManAtArmsStand);
    // An archer shoots from four tiles; past its range it stands as well.
    u.kind = UnitKind::Archer;
    assert_eq!(scene_pose(&u, None, Some(4 * TILE)), Pose::ArcherAttack);
    assert_eq!(scene_pose(&u, None, Some(9 * TILE)), Pose::ArcherStand);
}

#[wasm_bindgen_test]
fn general_native_clock_visits_every_attack_and_death_cell_between_server_ticks() {
    use crate::unit_visuals::{Pose, packed_frame};
    // DE attack: 45 cells in 1.6 s; death: 45 cells in 1.85 s.
    for phase in 0..45 {
        let attack = (f64::from(phase) + 0.5) * 1600.0 / 45.0;
        assert_eq!(
            packed_frame(Pose::GeneralAttack, 2, attack, None, 0.0, 20) >> 4,
            phase
        );
        let tick = 100.0 + (f64::from(phase) + 0.5) * 1.85 / 45.0 * 20.0;
        assert_eq!(
            packed_frame(
                Pose::GeneralDie,
                2,
                0.0,
                Some(aoe_core::Tick(100)),
                tick,
                20
            ) >> 4,
            phase
        );
    }
}
#[wasm_bindgen_test::wasm_bindgen_test]
fn army_native_clock_visits_every_militia_attack_cell_without_sampling() {
    use crate::unit_visuals::{Pose, packed_frame};
    let millis = 50.00000074505806;
    for facing in 0..8 {
        for phase in 0..30 {
            let packed = packed_frame(
                Pose::MilitiaAttack,
                facing,
                (f64::from(phase) + 0.25) * millis,
                None,
                0.0,
                20,
            );
            assert_eq!(packed >> 4, phase);
            assert_eq!(packed & 15, (20 - u16::from(facing) * 2) % 16);
        }
    }
}

#[wasm_bindgen_test]
fn native_dat_replay_delay_holds_the_final_idle_cell_before_restarting() {
    use crate::unit_visuals::{Pose, packed_frame};
    let phase = |time| packed_frame(Pose::BoarStand, 0, time, None, 0.0, 20) >> 4;
    assert_eq!(phase(2999.0), 59);
    assert_eq!(phase(4500.0), 59);
    assert_eq!(phase(5999.0), 59);
    assert_eq!(phase(6001.0), 0);
}
