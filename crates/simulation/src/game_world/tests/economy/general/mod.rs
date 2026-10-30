use super::*;
use crate::{Age, GeneralState, Stockpile};

mod appearance;
mod movement;
mod stances;
mod targets;

fn setup() -> (GameWorld, EntityId) {
    let (mut w, _, _) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.population_cap = 1000;
    p.stockpile = Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
    (w, castle)
}
fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
fn soldier(w: &mut GameWorld, kind: UnitKind, x: i32, y: i32) -> EntityId {
    w.spawn_kind(PlayerId(0), kind, at(x, y)).unwrap()
}
fn decide(w: &mut GameWorld) {
    w.tick.0 += 10;
    w.advance_generals(&mut Vec::new());
}
#[test]
fn creation_claims_at_most_100_owned_military_and_never_steals() {
    let (mut w, _) = setup();
    let ids: Vec<_> = (0..120)
        .map(|n| soldier(&mut w, UnitKind::Militia, 100 + n % 8, 100 + n / 8))
        .collect();
    let worker = soldier(&mut w, UnitKind::Villager, 102, 102);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(101, 101))
        .unwrap();
    let a = soldier(&mut w, UnitKind::General, 102, 102);
    let b = soldier(&mut w, UnitKind::General, 103, 102);
    assert_eq!(w.general_status(a).unwrap().army_size, 100);
    assert_eq!(w.general_status(b).unwrap().army_size, 20);
    assert_eq!(w.general_controller(worker), None);
    assert_eq!(w.general_controller(enemy), None);
    assert_eq!(
        ids.iter()
            .filter(|&&id| w.general_controller(id) == Some(a))
            .count(),
        100
    );
    w.forget_general(a);
    assert!(ids.iter().all(|&id| w.general_controller(id) != Some(a)));
    assert_eq!(w.general_status(b).unwrap().army_size, 20);
}
#[test]
fn castle_trains_hero_for_less_than_vassal_and_binds_home() {
    let (mut w, castle) = setup();
    let training = GameWorld::unit_training(UnitKind::General).unwrap();
    assert_eq!(
        (training.cost.food, training.cost.gold, training.seconds),
        (800, 800, 60)
    );
    assert!(crate::unit_combat(UnitKind::General).hp >= 300);
    w.issue_train(PlayerId(0), castle, UnitKind::General)
        .unwrap();
    advance(&mut w, 1200);
    let id = w.units().find(|u| u.kind == UnitKind::General).unwrap().id;
    assert_eq!(w.general_castle(id), Some(castle));
}
#[test]
fn reinforcements_balance_available_production_and_reserve_the_cap() {
    let (mut w, _) = setup();
    let mut buildings = Vec::new();
    for (n, kind) in [
        BuildingKind::Barracks,
        BuildingKind::ArcheryRange,
        BuildingKind::Stable,
        BuildingKind::SiegeWorkshop,
    ]
    .into_iter()
    .enumerate()
    {
        buildings.push(
            w.insert_building(
                PlayerId(0),
                kind,
                aoe_core::TileRect::from_xywh(100 + n as i32 * 6, 80, 3, 3),
                true,
            )
            .unwrap(),
        );
    }
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    for _ in 0..12 {
        decide(&mut w);
    }
    let queued: Vec<_> = buildings
        .iter()
        .flat_map(|id| &w.building(*id).unwrap().training_queue)
        .map(|e| e.kind)
        .collect();
    for kind in [
        UnitKind::Militia,
        UnitKind::Archer,
        UnitKind::Knight,
        UnitKind::BatteringRam,
    ] {
        assert!(queued.contains(&kind), "missing {kind:?}: {queued:?}");
    }
    assert!(queued.len() <= 100);
    advance(&mut w, 1200);
    assert!(
        w.units()
            .any(|u| u.kind != UnitKind::General && w.general_controller(u.id) == Some(g))
    );
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile::default();
    let before: usize = w.buildings().map(|b| b.training_queue.len()).sum();
    decide(&mut w);
    assert_eq!(
        w.buildings().map(|b| b.training_queue.len()).sum::<usize>(),
        before
    );
}
#[test]
fn general_is_idle_until_ordered_and_idle_again_after_stop() {
    let (mut w, _) = setup();
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
    w.issue_move(g, at(145, 103)).unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Marching);
    w.issue_stop(PlayerId(0), g).unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
}
#[test]
fn march_uses_shared_formation_with_role_ordering() {
    let (mut w, _) = setup();
    let kinds = [
        UnitKind::Archer,
        UnitKind::Militia,
        UnitKind::BatteringRam,
        UnitKind::Scout,
        UnitKind::Knight,
    ];
    let ids: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(n, k)| soldier(&mut w, k, 100, 100 + n as i32))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    w.issue_move(g, at(145, 103)).unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Marching);
    let slots: Vec<_> = ids
        .iter()
        .map(|&id| w.formation_destination_slot(id).unwrap())
        .collect();
    assert!(slots[3].x > slots[1].x, "scouts ahead");
    assert!(slots[1].x > slots[0].x, "infantry shields archers");
    assert!(slots[0].x > slots[2].x, "siege at rear");
    assert!(
        slots[4].y.abs_diff(103 * 1024 + 512) > 1024,
        "cavalry flanks"
    );
}
#[test]
fn siege_prefers_visible_buildings_and_wounded_units_pull_back() {
    let (mut w, _) = setup();
    let ram = soldier(&mut w, UnitKind::BatteringRam, 104, 100);
    let wounded = soldier(&mut w, UnitKind::Knight, 105, 101);
    let g = soldier(&mut w, UnitKind::General, 103, 100);
    w.units[w.lookup[&wounded]].state.hp = 10;
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(109, 100))
        .unwrap();
    let building = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(110, 102, 2, 2),
            true,
        )
        .unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
    assert_eq!(w.unit(ram).unwrap().attack_target, Some(building));
    assert_ne!(w.unit(wounded).unwrap().attack_target, Some(enemy));
    assert!(w.unit(wounded).unwrap().moving || w.unit(wounded).unwrap().planning);
}
#[test]
fn outmatched_army_retreats_toward_castle_and_regroups() {
    let (mut w, _) = setup();
    let g = soldier(&mut w, UnitKind::General, 110, 100);
    for n in 0..20 {
        w.spawn_kind(PlayerId(1), UnitKind::Knight, at(115 + n % 4, 100 + n / 4))
            .unwrap();
    }
    w.issue_move(g, at(150, 100)).unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Retreating);
    let goal = w.formation_destination_slot(g).unwrap();
    assert!(goal.x < w.unit(g).unwrap().position.x);
}
#[test]
fn general_replay_and_work_budget_are_deterministic() {
    fn run() -> (Vec<[u8; 32]>, crate::GeneralWork) {
        let (mut w, _) = setup();
        for n in 0..30 {
            soldier(&mut w, UnitKind::Militia, 100 + n % 6, 100 + n / 6);
        }
        let g = soldier(&mut w, UnitKind::General, 104, 104);
        w.issue_move(g, at(145, 104)).unwrap();
        let mut hashes = Vec::new();
        for _ in 0..60 {
            w.advance();
            hashes.push(w.canonical_hash());
        }
        (hashes, w.general_work())
    }
    let a = run();
    let b = run();
    assert_eq!(a, b);
    assert!(a.1.generals <= 8 && a.1.unit_visits <= 8192 && a.1.orders <= 808);
}

#[test]
fn stop_cancels_the_army_march_and_death_releases_command() {
    let (mut w, _) = setup();
    let member = soldier(&mut w, UnitKind::Militia, 100, 100);
    let g = soldier(&mut w, UnitKind::General, 102, 102);
    w.issue_move(g, at(145, 103)).unwrap();
    decide(&mut w);
    w.issue_stop(PlayerId(0), g).unwrap();
    decide(&mut w);
    assert!(w.movement_order(member).is_none());
    assert!(w.formation(member).is_none());
    w.kill_unit(w.lookup[&g]);
    assert_eq!(w.general_controller(member), None);
    assert_eq!(w.general_status(g), None);
}

#[test]
fn cancelled_recruitment_releases_reservations_and_never_overfills() {
    let (mut w, _) = setup();
    // Real housing must support the army: the tick recomputes the cap from
    // completed structures rather than retaining the fixture's initial cap.
    for n in 0..4 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(150 + n * 6, 80, 4, 4),
            true,
        )
        .unwrap();
    }
    let barracks = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(110, 80, 3, 3),
            true,
        )
        .unwrap();
    for n in 0..99 {
        soldier(&mut w, UnitKind::Militia, 100 + n % 10, 100 + n / 10);
    }
    let g = soldier(&mut w, UnitKind::General, 105, 105);
    decide(&mut w);
    assert_eq!(w.building(barracks).unwrap().training_queue.len(), 1);
    decide(&mut w);
    assert_eq!(w.building(barracks).unwrap().training_queue.len(), 1);
    w.issue_cancel_training(PlayerId(0), barracks).unwrap();
    decide(&mut w);
    assert_eq!(w.building(barracks).unwrap().training_queue.len(), 1);
    advance(&mut w, 500);
    assert_eq!(
        w.general_status(g).unwrap().army_size,
        100,
        "queue {:?}, general {:?}, hz {}",
        w.building(barracks).unwrap().training_queue,
        w.unit(g),
        w.config.tick_hz
    );
    assert!(w.building(barracks).unwrap().training_queue.is_empty());
}

#[test]
fn crowded_general_admission_is_bounded_and_rotates_without_scanning_world_armies() {
    let (mut w, _) = setup();
    // Synthetic ownership/census pressure, not player or hardware qualification.
    let leaders: Vec<_> = (0..1000)
        .map(|n| {
            w.spawn_kind(
                PlayerId(n),
                UnitKind::General,
                at(100 + n as i32 % 20, 100 + n as i32 / 20),
            )
            .unwrap()
        })
        .collect();
    w.tick.0 = 10;
    let mut first = Vec::new();
    w.advance_generals(&mut first);
    let work = w.general_work();
    assert_eq!(work.generals, 8);
    assert!(work.unit_visits <= 8192);
    assert!(work.building_visits <= 6024);
    assert!(work.orders <= 136);
    w.tick.0 = 11;
    w.advance_generals(&mut Vec::new());
    assert_eq!(w.general_work().generals, 0);
    w.tick.0 = 20;
    let mut second = Vec::new();
    w.advance_generals(&mut second);
    assert!(
        first
            .iter()
            .filter(|id| leaders.contains(id))
            .all(|id| !second.contains(id))
    );
    assert_eq!(w.general_work().generals, 8);
}

#[test]
fn distant_recruits_do_not_inflate_local_fight_strength() {
    let (mut w, _) = setup();
    let riders: Vec<_> = (0..20)
        .map(|n| soldier(&mut w, UnitKind::Knight, 102 + n % 4, 100 + n / 4))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 104, 100);
    for id in riders {
        let mut unit = w.unit(id).unwrap();
        w.remove_unit(id);
        unit.position = at(200, 200);
        unit.previous_position = unit.position;
        w.insert_map_unit(unit, 0);
        assert_eq!(w.general_controller(id), Some(g));
    }
    for n in 0..10 {
        w.spawn_kind(PlayerId(1), UnitKind::Knight, at(110 + n % 3, 100 + n / 3))
            .unwrap();
    }
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Retreating);
}

#[test]
fn army_focuses_wounded_high_value_targets_before_nearby_low_value_units() {
    let (mut w, _) = setup();
    let archer = soldier(&mut w, UnitKind::Archer, 104, 100);
    soldier(&mut w, UnitKind::General, 103, 100);
    let militia = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(105, 100))
        .unwrap();
    w.units[w.lookup[&militia]].state.hp = 1;
    w.spawn_kind(PlayerId(1), UnitKind::Archer, at(107, 100))
        .unwrap();
    let weak = w
        .spawn_kind(PlayerId(1), UnitKind::Archer, at(107, 101))
        .unwrap();
    w.units[w.lookup[&weak]].state.hp = 10;
    decide(&mut w);
    assert_eq!(w.unit(archer).unwrap().attack_target, Some(weak));
}

#[test]
fn economic_buildings_cannot_hide_available_military_production() {
    let (mut w, _) = setup();
    for n in 0..200 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(10 + n % 20 * 3, 10 + n / 20 * 3, 2, 2),
            true,
        )
        .unwrap();
    }
    let barracks = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(110, 80, 3, 3),
            true,
        )
        .unwrap();
    soldier(&mut w, UnitKind::General, 105, 100);
    decide(&mut w);
    assert_eq!(w.building(barracks).unwrap().training_queue.len(), 1);
    assert!(w.general_work().building_visits <= 625 + 128);
}
