mod adoption;
mod census;
mod construction;
mod farming;
mod kingdom;
mod manager;
mod movement;
mod resources;
mod settings;
mod territory;
use super::*;
use crate::{Age, Stockpile};

fn setup() -> (GameWorld, EntityId) {
    let (mut w, _, _) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    let town_center = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
    for x in [15, 18, 21] {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(x, 150, 2, 2),
            true,
        )
        .unwrap();
    }
    (w, town_center)
}

fn trained(w: &mut GameWorld, town_center: EntityId) -> EntityId {
    w.issue_train(PlayerId(0), town_center, UnitKind::Steward)
        .unwrap();
    let ticks = 60 * w.config().tick_hz;
    advance(w, ticks);
    w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id
}

#[test]
fn steward_cost_and_one_per_town_center_include_queue_and_garrison() {
    let (mut w, town_center) = setup();
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    w.issue_train(PlayerId(0), town_center, UnitKind::Steward)
        .unwrap();
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (before.food - after.food, before.gold - after.gold),
        (1_000, 1_000)
    );
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_train(PlayerId(0), town_center, UnitKind::Steward),
        Err(EconomyError::StewardExists)
    );
    assert_eq!(
        w.canonical_hash(),
        hash,
        "refusal must not charge or mutate"
    );
    w.issue_cancel_training(PlayerId(0), town_center).unwrap();
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
    let id = trained(&mut w, town_center);
    assert_eq!(w.steward_town_center(id), None);
    w.issue_train(PlayerId(0), town_center, UnitKind::Steward)
        .unwrap();
    w.issue_cancel_training(PlayerId(0), town_center).unwrap();
    w.issue_garrison(PlayerId(0), id, town_center).unwrap();
    advance(&mut w, 2);
    assert!(w.unit(id).is_none());
    assert_eq!(w.building(town_center).unwrap().garrison[0].unit.id, id);
    assert_eq!(
        w.issue_train(PlayerId(0), town_center, UnitKind::Steward),
        Err(EconomyError::StewardExists)
    );
}

#[test]
fn roaming_steward_can_move_and_queue_orders_beyond_the_old_domain() {
    let (mut w, town_center) = setup();
    let id = trained(&mut w, town_center);
    let far = WorldPosition::from_tile_center(TileCoord::new(210, 92)).unwrap();
    assert_eq!(w.issue_move(id, far), Ok(true));
    w.issue_queued_move(
        id,
        WorldPosition::from_tile_center(TileCoord::new(220, 92)).unwrap(),
    )
    .unwrap();
    advance(&mut w, 4_000);
    assert_eq!(
        w.unit(id).unwrap().position.tile_floor(),
        TileCoord::new(220, 92)
    );
    assert_eq!(w.steward_town_center(id), None);
    assert_eq!(w.steward_work().domains, 0);
}

#[test]
fn many_completed_houses_do_not_limit_training_or_change_the_population_count() {
    let (mut w, _) = setup();
    for i in 0..205 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(3 + (i % 70) * 3, 110 + (i / 70) * 3, 2, 2),
            true,
        )
        .unwrap();
    }
    let before = w.player_economy(PlayerId(0)).unwrap().population_used;
    let tc = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    w.issue_train(PlayerId(0), tc, UnitKind::Villager).unwrap();
    // Both prepaid entries finish independently of housing.
    w.issue_train(PlayerId(0), tc, UnitKind::Villager).unwrap();
    let entry = w
        .building_mut(tc)
        .unwrap()
        .training_queue
        .front_mut()
        .unwrap();
    entry.progress_ticks = entry.total_ticks - 1;
    w.advance();
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().population_used,
        before + 1
    );
    for _ in 0..600 {
        w.advance();
    }
    let queue = &w.building(tc).unwrap().training_queue;
    assert!(queue.is_empty());
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().population_used,
        before + 2
    );
}

#[test]
fn destruction_does_not_restrict_a_roaming_steward_or_soft_separation() {
    let (mut w, town_center) = setup();
    let start = WorldPosition::from_tile_center(TileCoord::new(195, 92)).unwrap();
    let id = w.spawn_kind(PlayerId(0), UnitKind::Steward, start).unwrap();
    w.spawn_villager(PlayerId(0), start).unwrap();
    advance(&mut w, 100);
    assert_ne!(
        w.unit(id).unwrap().position,
        start,
        "normal soft separation applies"
    );
    w.destroy_building(town_center);
    let far = WorldPosition::from_tile_center(TileCoord::new(210, 92)).unwrap();
    w.issue_move(id, far).unwrap();
    advance(&mut w, 1_000);
    assert_eq!(w.unit(id).unwrap().position.tile_floor(), far.tile_floor());
    assert_eq!(w.steward_town_center(id), None);
}

#[test]
fn castle_vassal_has_no_economic_domain() {
    let (mut w, _) = setup();
    let town_center = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(170, 90, 4, 4),
            true,
        )
        .unwrap();
    w.issue_train(PlayerId(0), town_center, UnitKind::Vassal)
        .unwrap();
    advance(&mut w, 60 * 20);
    let vassal = w.units().find(|u| u.kind == UnitKind::Vassal).unwrap().id;
    assert_eq!(w.steward_settings(vassal), None);
    w.issue_garrison(PlayerId(0), vassal, town_center).unwrap();
    advance(&mut w, 10);
    assert_eq!(w.steward_status(vassal), None);
    assert_eq!(w.steward_work().domains, 0);
    assert!(
        w.building(town_center).unwrap().garrison[0]
            .steward
            .is_none()
    );
}

#[test]
fn steward_training_and_activation_require_town_centers() {
    let (mut w, center) = setup();
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(170, 90, 4, 4),
            true,
        )
        .unwrap();
    assert_eq!(
        w.issue_train(PlayerId(0), castle, UnitKind::Steward),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(
        w.issue_train(PlayerId(0), center, UnitKind::Vassal),
        Err(EconomyError::WrongUnitKind)
    );
    let steward = trained(&mut w, center);
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), steward, castle),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(w.canonical_hash(), hash, "wrong-home refusal is atomic");
    w.issue_garrison(PlayerId(0), steward, center).unwrap();
    advance(&mut w, 2);
    assert_eq!(w.steward_town_center(steward), Some(center));
    assert_eq!(w.steward_status(steward).unwrap().settings.cap, 100);
}

#[test]
fn training_showcase_preserves_its_workers_for_the_economy_recording() {
    let (mut w, primary) = GameWorld::steward_demo(Seed(7)).unwrap();
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    assert_eq!(workers.len(), 3);
    assert!(
        w.units().all(|u| u.player == PlayerId(0)),
        "economic footage must not inherit enemy combat units"
    );
    assert!(w.animals().all(|a| a.kind != crate::AnimalKind::Wolf));
    let center = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let steward = trained(&mut w, center);
    w.issue_garrison(PlayerId(0), steward, center).unwrap();
    let ticks = 600 * w.config().tick_hz;
    advance(&mut w, ticks);
    assert!(w.unit(primary).is_some_and(|u| u.hp > 0));
    for worker in workers {
        assert!(
            w.unit(worker).is_some_and(|u| u.hp > 0),
            "the recording keeps worker {worker:?} alive"
        );
        assert_eq!(w.steward_controller(worker), Some(steward));
    }
}
