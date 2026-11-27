//! Which change of the viewer's own units and buildings is a new hit worth an
//! alert, and where. Pure: what the client knew and what it is told now go in,
//! a place comes out. The caller asks for changed own things of a tick only,
//! never for those of a snapshot.
use aoe_client::alerts::{answered, building_hit, building_lost, hunting, replaced, unit_hit};
use aoe_core::{EntityId, PlayerId, Tick, TileRect, WorldPosition};
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayBuildingState, GameplayUnitState, HitState, UnitKind,
    UnitTask,
};

const HZ: u32 = 20;

fn hit(tick: u64) -> Option<HitState> {
    Some(HitState {
        attacker: EntityId(90),
        tick: Tick(tick),
    })
}

/// Standing at [2.5, 4.25] tiles.
fn unit(last_hit: Option<HitState>) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(7),
        player: PlayerId(1),
        position: WorldPosition::new(2_560, 4_352),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 20,
        max_hp: 25,
        attack_target: None,
        last_hit,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        sergeant: false,
        cargo_count: 0,
        native_action: None,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
}

/// Its centre is [12, 21.5] tiles.
fn building(last_hit: Option<HitState>) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(8),
        player: PlayerId(1),
        kind: BuildingKind::Barracks,
        footprint: TileRect::from_xywh(10, 20, 4, 3),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 900,
        max_hp: 1200,
        los: 6,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit,
        age: aoe_protocol::Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    }
}

fn foreign(_: &HitState) -> bool {
    false
}

fn own(_: &HitState) -> bool {
    true
}

/// Finding the striker is work, so only a new hit may ask for it.
fn unasked(_: &HitState) -> bool {
    panic!("nothing new happened, yet its striker was looked up");
}

#[test]
fn a_new_hit_on_a_unit_is_told_once_where_it_stands() {
    let calm = unit(None);
    assert_eq!(unit_hit(Some(&calm), &calm, unasked), None);
    let struck = unit(hit(40));
    assert_eq!(
        unit_hit(Some(&calm), &struck, foreign),
        Some([2.5, 4.25]),
        "the first hit"
    );
    // Every later change of the unit carries the same hit again.
    let mut walking = struck;
    walking.position = WorldPosition::new(3_072, 5_120);
    walking.hp += 1;
    assert_eq!(unit_hit(Some(&struck), &walking, unasked), None);
    let mut again = walking;
    again.last_hit = hit(41);
    assert_eq!(
        unit_hit(Some(&walking), &again, foreign),
        Some([3.0, 5.0]),
        "the next one"
    );
}

#[test]
fn a_hit_on_something_first_seen_now_is_an_old_one() {
    assert_eq!(unit_hit(None, &unit(hit(40)), unasked), None);
    assert_eq!(building_hit(None, &building(hit(40)), unasked), None);
}

#[test]
fn a_hit_by_the_viewers_own_side_is_no_attack() {
    assert_eq!(unit_hit(Some(&unit(None)), &unit(hit(40)), own), None);
    assert_eq!(
        building_hit(Some(&building(hit(40))), &building(hit(41)), own),
        None
    );
    assert_eq!(building_lost(&building(hit(40)), 41, HZ, own), None);
}

#[test]
fn a_killing_hit_is_told_where_the_unit_fell_and_another_death_is_not() {
    let mut killed = unit(hit(40));
    killed.hp = 0;
    killed.task = UnitTask::Dying;
    killed.death_tick = Some(Tick(40));
    assert_eq!(
        unit_hit(Some(&unit(None)), &killed, foreign),
        Some([2.5, 4.25])
    );
    // Deleted by its owner, long after the last hit it took.
    let mut deleted = unit(hit(40));
    deleted.hp = 0;
    deleted.task = UnitTask::Dying;
    deleted.death_tick = Some(Tick(900));
    assert_eq!(unit_hit(Some(&unit(hit(40))), &deleted, unasked), None);
}

#[test]
fn a_new_hit_on_a_building_is_told_once_at_its_middle() {
    let (calm, struck) = (building(None), building(hit(40)));
    assert_eq!(
        building_hit(Some(&calm), &struck, foreign),
        Some([12.0, 21.5])
    );
    let mut repaired = struck.clone();
    repaired.hp += 10;
    assert_eq!(building_hit(Some(&struck), &repaired, unasked), None);
    assert_eq!(
        building_hit(Some(&repaired), &building(hit(41)), foreign),
        Some([12.0, 21.5])
    );
}

/// The wire names a lost building only by its id: the state before it is all
/// the client has, without the blow that destroyed it.
#[test]
fn a_building_lost_under_attack_is_told_and_one_lost_long_after_is_not() {
    let second = u64::from(HZ);
    let besieged = building(hit(1_000));
    for after in [1, 2 * second] {
        assert_eq!(
            building_lost(&besieged, 1_000 + after, HZ, foreign),
            Some([12.0, 21.5]),
            "{after} ticks after its last hit"
        );
    }
    assert_eq!(
        building_lost(&besieged, 1_000 + 60 * second, HZ, foreign),
        None,
        "its last fight was over"
    );
}

/// One blow can take a wall. Anything else lost unhurt was cancelled, converted
/// or spent. The caller leaves out what the player deleted himself.
#[test]
fn only_a_wall_or_a_gate_lost_without_an_earlier_hit_is_told() {
    for kind in BuildingKind::ALL {
        let mut unhurt = building(None);
        unhurt.kind = kind;
        let barrier = matches!(
            kind,
            BuildingKind::PalisadeWall
                | BuildingKind::PalisadeGate
                | BuildingKind::StoneWall
                | BuildingKind::StoneGate
        );
        assert_eq!(
            building_lost(&unhurt, 1_000, HZ, unasked),
            barrier.then_some([12.0, 21.5]),
            "{kind:?}"
        );
    }
}

/// A lone trebuchet takes ten seconds between two shots, so twelve is the edge.
#[test]
fn a_siege_is_over_twelve_seconds_of_game_time_after_its_last_hit() {
    for hz in [10, 20, 60] {
        let edge = 1_000 + 12 * u64::from(hz);
        let besieged = building(hit(1_000));
        assert_eq!(
            building_lost(&besieged, edge, hz, foreign),
            Some([12.0, 21.5]),
            "{hz} Hz"
        );
        assert_eq!(
            building_lost(&besieged, edge + 1, hz, foreign),
            None,
            "{hz} Hz"
        );
    }
}

#[test]
fn a_removal_answers_an_order_for_five_seconds_of_game_time() {
    for hz in [10, 20, 60] {
        let edge = 1_000 + 5 * u64::from(hz);
        assert!(answered(1_000, 1_000, hz), "{hz} Hz");
        assert!(answered(1_000, edge, hz), "{hz} Hz");
        assert!(!answered(1_000, edge + 1, hz), "{hz} Hz");
    }
}

/// Stone over a palisade, and a gate over some tiles of a wall.
#[test]
fn only_his_own_building_on_some_tile_of_a_lost_one_replaces_it() {
    let lost = building(None);
    let told = |player, x, y, width, height| GameplayBuildingState {
        id: EntityId(9),
        player: PlayerId(player),
        footprint: TileRect::from_xywh(x, y, width, height),
        ..building(None)
    };
    for (place, over) in [
        ((10, 20, 4, 3), true),
        ((12, 21, 1, 1), true),
        // One common tile at each corner, then none beside each border.
        ((7, 18, 4, 3), true),
        ((13, 18, 4, 3), true),
        ((7, 22, 4, 3), true),
        ((13, 22, 4, 3), true),
        ((6, 20, 4, 3), false),
        ((14, 20, 4, 3), false),
        ((10, 17, 4, 3), false),
        ((10, 23, 4, 3), false),
    ] {
        let (x, y, width, height) = place;
        let own = told(1, x, y, width, height);
        let rival = told(2, x, y, width, height);
        assert_eq!(
            replaced(&lost, std::slice::from_ref(&own)),
            over,
            "{place:?}"
        );
        assert!(
            !replaced(&lost, std::slice::from_ref(&rival)),
            "{place:?} of a rival"
        );
        assert_eq!(replaced(&lost, &[rival, own]), over, "{place:?} of both");
    }
    assert!(!replaced(&lost, &[]));
}

/// The server tells a hunter of boar or deer as Hunting, and a villager that
/// fights a wolf as Attacking: a wolf is never game.
#[test]
fn only_a_villager_that_hunts_has_game() {
    let at = |task| GameplayUnitState {
        task,
        hunt_target: Some(EntityId(40)),
        ..unit(None)
    };
    assert!(hunting(&at(UnitTask::Hunting)));
    for task in [
        UnitTask::Attacking,
        UnitTask::Idle,
        UnitTask::Moving,
        UnitTask::Shepherding,
        UnitTask::Dying,
    ] {
        assert!(!hunting(&at(task)), "{task:?}");
    }
}
