mod adoption;
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

fn trained(w: &mut GameWorld, castle: EntityId) -> EntityId {
    w.issue_train(PlayerId(0), castle, UnitKind::Vassal)
        .unwrap();
    let ticks = 60 * w.config().tick_hz;
    advance(w, ticks);
    w.units().find(|u| u.kind == UnitKind::Vassal).unwrap().id
}

#[test]
fn vassal_cost_and_one_per_castle_include_queue_and_garrison() {
    let (mut w, castle) = setup();
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    w.issue_train(PlayerId(0), castle, UnitKind::Vassal)
        .unwrap();
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (before.food - after.food, before.gold - after.gold),
        (1_000, 1_000)
    );
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_train(PlayerId(0), castle, UnitKind::Vassal),
        Err(EconomyError::VassalExists)
    );
    assert_eq!(
        w.canonical_hash(),
        hash,
        "refusal must not charge or mutate"
    );
    w.issue_cancel_training(PlayerId(0), castle).unwrap();
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
    let id = trained(&mut w, castle);
    assert_eq!(w.vassal_castle(id), None);
    w.issue_train(PlayerId(0), castle, UnitKind::Vassal)
        .unwrap();
    w.issue_cancel_training(PlayerId(0), castle).unwrap();
    w.issue_garrison(PlayerId(0), id, castle).unwrap();
    advance(&mut w, 2);
    assert!(w.unit(id).is_none());
    assert_eq!(w.building(castle).unwrap().garrison[0].unit.id, id);
    assert_eq!(
        w.issue_train(PlayerId(0), castle, UnitKind::Vassal),
        Err(EconomyError::VassalExists)
    );
}

#[test]
fn roaming_vassal_can_move_and_queue_orders_beyond_the_old_domain() {
    let (mut w, castle) = setup();
    let id = trained(&mut w, castle);
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
    assert_eq!(w.vassal_castle(id), None);
    assert_eq!(w.vassal_work().domains, 0);
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
        .economy
        .buildings
        .get_mut(&tc)
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
fn destruction_does_not_restrict_a_roaming_vassal_or_soft_separation() {
    let (mut w, castle) = setup();
    let start = WorldPosition::from_tile_center(TileCoord::new(195, 92)).unwrap();
    let id = w.spawn_kind(PlayerId(0), UnitKind::Vassal, start).unwrap();
    w.spawn_villager(PlayerId(0), start).unwrap();
    advance(&mut w, 100);
    assert_ne!(
        w.unit(id).unwrap().position,
        start,
        "normal soft separation applies"
    );
    w.destroy_building(castle);
    let far = WorldPosition::from_tile_center(TileCoord::new(210, 92)).unwrap();
    w.issue_move(id, far).unwrap();
    advance(&mut w, 1_000);
    assert_eq!(w.unit(id).unwrap().position.tile_floor(), far.tile_floor());
    assert_eq!(w.vassal_castle(id), None);
}
