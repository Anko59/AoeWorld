use super::*;
use crate::{Age, Stockpile};

#[path = "vassal/ownership.rs"]
mod ownership;
#[path = "vassal/roads.rs"]
mod roads;

fn setup() -> (GameWorld, EntityId, EntityId) {
    let (mut w, _, _) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
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
            TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
    let v = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            WorldPosition::from_tile_center(TileCoord::new(89, 92)).unwrap(),
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), v, castle).unwrap();
    advance(&mut w, 2);
    (w, v, castle)
}

#[test]
fn vassal_caps_reserve_pause_and_completed_land_stops_spending() {
    let (mut w, v, _) = setup();
    for i in 0..25 {
        let pos = WorldPosition::from_tile_center(TileCoord::new(84 + i % 5, 96 + i / 5)).unwrap();
        w.spawn_villager(PlayerId(0), pos).unwrap();
        w.spawn_kind(PlayerId(0), UnitKind::Militia, pos).unwrap();
    }
    w.issue_vassal_development(PlayerId(0), v, false).unwrap();
    advance(&mut w, 40);
    let s = w.vassal_status(v).unwrap();
    assert_eq!(s.military, 20);
    assert_eq!(s.villagers, 0, "paused builders return to the economy");
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    advance(&mut w, 40);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
    w.issue_vassal_development(PlayerId(0), v, true).unwrap();
    advance(&mut w, 20);
    assert_eq!(w.vassal_status(v).unwrap().villagers, 20);
    for _ in 0..20_000 {
        w.advance();
        if w.vassal_status(v).unwrap().developed {
            break;
        }
    }
    assert!(
        w.vassal_status(v).unwrap().developed,
        "finite fortification plan must finish: {:?}; foundations {:?}; workers {:?}",
        w.vassal_status(v),
        w.buildings()
            .filter(|b| !b.is_complete())
            .map(|b| (b.kind, b.footprint, b.work_done, b.work_total))
            .collect::<Vec<_>>(),
        w.units()
            .filter(|u| u.kind == UnitKind::Villager)
            .map(|u| (u.id, u.position.tile_floor(), u.task))
            .collect::<Vec<_>>()
    );
    assert_eq!(w.vassal_status(v).unwrap().villagers, 0);
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    advance(&mut w, 100);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, after);
    assert!(w.buildings().any(|b| b.kind.is_gate() && b.is_complete()));
    assert_eq!(
        w.issue_vassal_development(PlayerId(1), v, false),
        Err(EconomyError::NotOwner)
    );
}

#[test]
fn vassal_keeps_economy_reserve_and_releases_on_ungarrison() {
    let (mut w, v, castle) = setup();
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(87, 94)).unwrap(),
        )
        .unwrap();
    let reserve = Stockpile {
        food: 200,
        wood: 200,
        gold: 100,
        stone: 200,
    };
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = reserve;
    advance(&mut w, 80);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, reserve);
    assert_eq!(w.vassal_controller(worker), Some(v));
    w.issue_ungarrison(PlayerId(0), castle, Some(v)).unwrap();
    assert_eq!(w.vassal_controller(worker), None);
    assert_eq!(w.vassal_status(v), None);
}

#[test]
fn vassal_calls_general_at_100_tiles_but_not_beyond_and_restores_march() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let near = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::General,
            WorldPosition::new(192 * 1024, 92 * 1024),
        )
        .unwrap();
    let far = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::General,
            WorldPosition::new(192 * 1024 + 1, 92 * 1024),
        )
        .unwrap();
    let original = at(200, 130);
    w.issue_move(near, original).unwrap();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(97, 92))
        .unwrap();
    w.tick.0 = 20;
    w.advance_vassals(&mut Vec::new());
    assert!(w.vassal_status(v).unwrap().alert);
    assert_ne!(w.economy.generals.armies[&near].destination, Some(original));
    assert_eq!(w.economy.generals.armies[&far].destination, None);
    w.remove_unit(enemy);
    advance(&mut w, 40);
    assert!(!w.vassal_status(v).unwrap().alert);
    assert_eq!(w.economy.generals.armies[&near].destination, Some(original));
}

#[test]
fn vassal_fortification_replay_agrees_with_one_and_four_workers() {
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let (mut w, v, _) = setup();
                for i in 0..8 {
                    w.spawn_villager(
                        PlayerId(0),
                        WorldPosition::from_tile_center(TileCoord::new(88 + i, 96)).unwrap(),
                    )
                    .unwrap();
                }
                advance(&mut w, 18_000);
                assert!(w.vassal_status(v).unwrap().developed);
                assert!(
                    w.buildings()
                        .any(|b| b.kind == BuildingKind::Road && b.is_complete())
                );
                w.canonical_hash()
            })
    };
    assert_eq!(
        run(1),
        run(4),
        "vassal_roads_one_and_four_worker_replay_matches"
    );
}

#[test]
fn vassal_preserves_busy_general_members_and_explicit_owner_orders() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let soldier = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(98, 92))
        .unwrap();
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(100, 92))
        .unwrap();
    assert_eq!(w.general_controller(soldier), Some(general));
    w.issue_move(soldier, at(99, 95)).unwrap();
    w.tick.0 = 19;
    w.advance();
    assert_eq!(w.general_controller(soldier), Some(general));
    assert_eq!(w.vassal_controller(soldier), None);
    let worker = w.spawn_villager(PlayerId(0), at(97, 95)).unwrap();
    w.tick.0 = 39;
    w.advance();
    assert_eq!(w.vassal_controller(worker), Some(v));
    w.issue_move(worker, at(120, 110)).unwrap();
    assert_eq!(
        w.vassal_controller(worker),
        None,
        "owner commands release automatic control"
    );
}

#[test]
fn vassal_demo_finishes_before_attack_and_general_response() {
    let (mut w, _) = GameWorld::vassal_defense_demo(Seed(7)).unwrap();
    let v = w
        .buildings()
        .flat_map(|b| &b.garrison)
        .find(|g| g.unit.kind == UnitKind::Vassal)
        .map(|g| g.unit.id)
        .or_else(|| w.units().find(|u| u.kind == UnitKind::Vassal).map(|u| u.id))
        .unwrap();
    let mut developed = false;
    let mut response = false;
    for _ in 0..6_000 {
        w.advance();
        if let Some(s) = w.vassal_status(v) {
            developed |= s.developed;
            if s.alert {
                assert!(developed, "attack must follow completed fortification");
            }
            response |= s.responding > 0;
        }
        if response {
            break;
        }
    }
    assert!(developed, "showcase fortification must complete");
    assert!(response, "showcase must exercise a real General response");
}

#[test]
fn vassal_called_general_with_an_army_returns_to_the_saved_position() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let origin = at(145, 92);
    for i in 0..12 {
        w.spawn_kind(PlayerId(0), UnitKind::Knight, at(145 + i % 4, 94 + i / 4))
            .unwrap();
    }
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, origin)
        .unwrap();
    w.set_fog_disabled(true);
    advance(&mut w, 40);
    assert!(w.general_status(general).unwrap().army_size > 0);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 92))
        .unwrap();
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::StandGround)
        .unwrap();
    advance(&mut w, 80);
    assert!(w.vassal_status(v).unwrap().alert);
    assert!(w.unit(general).unwrap().position.x < origin.x);
    w.remove_unit(enemy);
    advance(&mut w, 2_000);
    assert_eq!(
        w.unit(general).unwrap().position.tile_floor(),
        origin.tile_floor()
    );
    assert_eq!(
        w.general_status(general).unwrap().state,
        crate::GeneralState::Idle
    );
}

#[test]
fn vassal_reuses_existing_buildings_and_skips_obstructed_tower_sites() {
    let (mut w, v, _) = setup();
    w.insert_building(
        PlayerId(0),
        BuildingKind::WatchTower,
        TileRect::from_xywh(96, 84, 2, 2),
        true,
    )
    .unwrap();
    w.insert_building(
        PlayerId(0),
        BuildingKind::Barracks,
        TileRect::from_xywh(84, 94, 3, 3),
        true,
    )
    .unwrap();
    w.insert_building(
        PlayerId(0),
        BuildingKind::House,
        TileRect::from_xywh(86, 86, 2, 2),
        true,
    )
    .unwrap();
    for i in 0..8 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(89 + i, 98)).unwrap(),
        )
        .unwrap();
    }
    advance(&mut w, 20_000);
    assert!(w.vassal_status(v).unwrap().developed);
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::WatchTower)
            .count(),
        2
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Barracks)
            .count(),
        1
    );
}

#[test]
fn vassal_called_idle_general_physically_returns_and_becomes_idle_again() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let origin = at(145, 92);
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, origin)
        .unwrap();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 92))
        .unwrap();
    w.set_fog_disabled(true);
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::StandGround)
        .unwrap();
    advance(&mut w, 80);
    assert!(w.vassal_status(v).unwrap().alert);
    assert!(w.unit(general).unwrap().position.x < origin.x);
    w.remove_unit(enemy);
    advance(&mut w, 2_000);
    assert_eq!(
        w.unit(general).unwrap().position.tile_floor(),
        origin.tile_floor()
    );
    assert_eq!(
        w.general_status(general).unwrap().state,
        crate::GeneralState::Idle
    );
}

#[test]
fn vassal_skips_unreachable_tower_and_uses_an_accessible_alternative() {
    let (mut w, v, _) = setup();
    for y in 85..=87 {
        for x in 85..=87 {
            if x != 86 || y != 86 {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::PalisadeWall,
                    TileRect::from_xywh(x, y, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    for i in 0..8 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(88 + i, 96)).unwrap(),
        )
        .unwrap();
    }
    advance(&mut w, 20_000);
    assert!(
        w.vassal_status(v).unwrap().developed,
        "unreachable sites must not hold the plan forever"
    );
    assert!(w.building_at(TileCoord::new(86, 86)).is_none());
}

#[test]
fn vassal_does_not_take_busy_steward_builders() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let tc = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let steward = w
        .spawn_kind(PlayerId(0), UnitKind::Steward, at(63, 64))
        .unwrap();
    w.issue_garrison(PlayerId(0), steward, tc).unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(99, 91)).unwrap();
    w.issue_build(
        PlayerId(0),
        &[worker],
        BuildingKind::House,
        TileCoord::new(98, 88),
        false,
    )
    .unwrap();
    advance(&mut w, 40);
    assert_eq!(w.steward_controller(worker), Some(steward));
    assert_eq!(w.vassal_controller(worker), None);
    assert_eq!(w.vassal_status(v).unwrap().villagers, 0);
}

#[test]
fn vassal_moves_past_a_paid_foundation_that_becomes_unreachable() {
    let (mut w, v, _) = setup();
    for i in 0..3 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(88 + i, 86)).unwrap(),
        )
        .unwrap();
    }
    w.tick.0 = 19;
    w.advance();
    let foundation = w
        .player_buildings_of(PlayerId(0), BuildingKind::WatchTower)
        .find(|b| !b.is_complete())
        .unwrap()
        .id;
    for y in 85..=87 {
        for x in 85..=87 {
            if x != 86 || y != 86 {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::PalisadeWall,
                    TileRect::from_xywh(x, y, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    advance(&mut w, 4_000);
    assert!(!w.building(foundation).unwrap().is_complete());
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::WatchTower)
            .any(|b| b.id != foundation && b.is_complete()),
        "a blocked paid site must not monopolize development"
    );
    assert!(
        !w.vassal_status(v).unwrap().developed,
        "unfinished fortification is never reported as complete"
    );
}

#[test]
fn vassal_calls_for_help_when_an_enemy_tower_attacks_its_territory() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let soldier = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(106, 92))
        .unwrap();
    w.issue_stance(PlayerId(0), &[soldier], aoe_core::CombatStance::StandGround)
        .unwrap();
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(190, 92))
        .unwrap();
    let tower = w
        .insert_building(
            PlayerId(1),
            BuildingKind::WatchTower,
            TileRect::from_xywh(110, 92, 1, 1),
            true,
        )
        .unwrap();
    w.issue_attack(PlayerId(1), tower, soldier).unwrap();
    advance(&mut w, 40);
    assert!(w.unit(soldier).unwrap().last_hit.is_some());
    assert!(w.vassal_status(v).unwrap().alert);
    assert_eq!(w.vassal_status(v).unwrap().responding, 1);
    assert!(w.economy.generals.armies[&general].destination.is_some());
}
