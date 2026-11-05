use super::{is_stalled, native_walk_speed, shot_clock, walk_clock};
use aoe_protocol::UnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

// An archer's attack: ten 70 ms frames released at frame 5, reloading
// every 2 s, at 20 ticks a second.
const ANIMATION: (f64, f64) = (700.0, 70.0);
fn clock(shot: Option<u64>, now: f64) -> f64 {
    shot_clock(shot, now, 20.0, 2000.0, ANIMATION, 5)
}

#[wasm_bindgen_test]
fn the_release_frame_plays_on_the_tick_of_every_shot() {
    for k in 0..3 {
        assert_eq!(clock(Some(100), 100.0 + 40.0 * f64::from(k)), 350.0);
    }
}

#[wasm_bindgen_test]
fn the_windup_plays_before_the_next_shot_and_the_rest_after_it() {
    // 350 ms before the second shot the animation starts at its first frame.
    assert_eq!(clock(Some(100), 140.0 - 7.0), 0.0);
    assert_eq!(clock(Some(100), 140.0 - 3.5), 175.0);
    // 345 ms after a shot the animation is at its last frame (frame 9).
    assert!((clock(Some(100), 106.9) - 695.0).abs() < 1e-6);
}

#[wasm_bindgen_test]
fn the_first_frame_is_held_between_swings_before_a_shot_and_after_a_pause() {
    assert_eq!(clock(Some(100), 112.0), 0.0);
    assert_eq!(clock(None, 112.0), 0.0);
    assert_eq!(clock(Some(100), 100.0 + 20.0 * 7.0), 0.0);
    // A shot newer than the clock (clock skew) holds the release frame.
    assert_eq!(clock(Some(100), 99.0), 350.0);
}

#[wasm_bindgen_test]
fn imperial_promotions_use_their_own_pose_and_corpse() {
    use super::{decay_pair, military_poses};
    use aoe_protocol::UnitKind::*;
    for (kind, older) in [
        (TwoHandedSwordsman, LongSwordsman),
        (Champion, TwoHandedSwordsman),
        (Halberdier, Pikeman),
        (Arbalester, Crossbowman),
        (Cavalier, Knight),
        (Paladin, Cavalier),
        (Hussar, LightCavalry),
        (CappedRam, BatteringRam),
        (SiegeRam, CappedRam),
        (Onager, Mangonel),
    ] {
        let poses = military_poses(kind).unwrap();
        let old = military_poses(older).unwrap();
        for action in 0..4 {
            assert_ne!(poses[action], old[action], "{kind:?}, action {action}");
        }
        let (die, decay) = decay_pair(poses[3]).expect("native corpse lifecycle");
        assert_eq!(die, poses[3]);
        assert_ne!(die, decay);
    }
}

#[wasm_bindgen_test]
fn steward_uses_the_imported_native_de_king_for_every_pose() {
    use aoe_assets::catalog::units::{Pose, SPRITES};
    use aoe_protocol::UnitKind;
    let poses = super::roster::military_poses(UnitKind::Steward).unwrap();
    assert_eq!(
        poses,
        [
            Pose::KingStand,
            Pose::KingWalk,
            Pose::KingStand,
            Pose::KingDie
        ]
    );
    assert_eq!(super::roster::military_poses(UnitKind::Vassal), Some(poses));
    for pose in poses {
        let source = SPRITES[pose as usize];
        assert!(source.manifest_source().starts_with("de/"));
        assert_eq!(source.rows(), 16);
        assert!(source.selected_frames() > 16);
    }
}

#[wasm_bindgen_test]
fn walk_animation_follows_distance_moved_not_time_or_tick_rate() {
    // A villager covers 0.8 tiles in the second its animation lasts at native speed.
    assert!((walk_clock(UnitKind::Villager, 0.8) - 1000.0).abs() < 1e-9);
    // Half the distance is half the stride; none is none, whatever the time.
    assert!((walk_clock(UnitKind::Villager, 0.4) - 500.0).abs() < 1e-9);
    assert_eq!(walk_clock(UnitKind::Villager, 0.0), 0.0);
    // The same distance gives the same clock at any tick rate (pure in distance),
    // and a horseman strides further per cycle than a foot soldier.
    assert!(walk_clock(UnitKind::Knight, 1.0) < walk_clock(UnitKind::Villager, 1.0));
    // Moving backwards (a corrected position) never rewinds below zero.
    assert_eq!(walk_clock(UnitKind::Villager, -1.0), 0.0);
}

#[wasm_bindgen_test]
fn native_walk_speeds_are_the_simulations_own() {
    for (kind, tiles_per_second) in [
        (UnitKind::Villager, 0.8),
        (UnitKind::Scout, 1.2),
        (UnitKind::FishingShip, 1.26),
    ] {
        assert!(
            (native_walk_speed(kind) - tiles_per_second).abs() < 1e-9,
            "{kind:?}"
        );
    }
    // Every unit kind has a positive speed, so the animation clock is finite.
    for kind in UnitKind::ALL {
        assert!(native_walk_speed(kind) > 0.0, "{kind:?}");
    }
}

#[wasm_bindgen_test]
fn a_unit_under_a_fifth_of_its_stride_counts_as_standing_still() {
    // Over 0.6 s a villager strides 0.48 tiles at its native speed.
    assert!(is_stalled(UnitKind::Villager, 0.0, 0.6));
    assert!(is_stalled(UnitKind::Villager, 0.09, 0.6));
    assert!(!is_stalled(UnitKind::Villager, 0.1, 0.6));
    assert!(!is_stalled(UnitKind::Villager, 0.48, 0.6));
}

use aoe_protocol::UnitTask;

fn villager(task: UnitTask) -> aoe_protocol::GameplayUnitState {
    aoe_protocol::GameplayUnitState {
        id: aoe_core::EntityId(1),
        player: aoe_core::PlayerId(0),
        position: aoe_core::WorldPosition::new(0, 0),
        moving: false,
        planning: false,
        facing: 0,
        kind: aoe_protocol::UnitKind::Villager,
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
        sergeant: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
}

#[wasm_bindgen_test]
fn a_stalled_unit_draws_the_matching_rest_pose() {
    use crate::unit_visuals::{Pose, rest_pose, unit_pose};
    use aoe_protocol::{AnimalKind, AnimalState, AnimalTask};
    let mut unit = villager(UnitTask::Moving);
    unit.moving = true;
    unit.hunt_target = Some(aoe_core::EntityId(9));
    let mut animal = AnimalState {
        id: aoe_core::EntityId(9),
        kind: AnimalKind::Sheep,
        owner: None,
        hp: 7,
        position: aoe_core::WorldPosition::new(0, 0),
        task: AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 0,
        moving: false,
        death_tick: None,
    };
    assert_eq!(
        rest_pose(unit_pose(&unit, Some(&animal))),
        Pose::ShepherdStand
    );
    animal.kind = AnimalKind::Deer;
    assert_eq!(
        rest_pose(unit_pose(&unit, Some(&animal))),
        Pose::HunterStand
    );
    unit.hunt_target = None;
    unit.task = UnitTask::Attacking;
    assert_eq!(rest_pose(unit_pose(&unit, None)), Pose::VillagerStand);
    unit.kind = aoe_protocol::UnitKind::Militia;
    assert_eq!(rest_pose(unit_pose(&unit, None)), Pose::MilitiaStand);
    unit.kind = aoe_protocol::UnitKind::BatteringRam;
    assert_eq!(rest_pose(unit_pose(&unit, None)), Pose::BatteringRamStand);
    unit.kind = aoe_protocol::UnitKind::Villager;
    unit.task = UnitTask::Returning;
    unit.carried = Some(aoe_protocol::CarriedResource {
        kind: aoe_protocol::ResourceKind::Wood,
        amount: 5,
    });
    assert_eq!(rest_pose(unit_pose(&unit, None)), Pose::CarryWoodIdle);
}
