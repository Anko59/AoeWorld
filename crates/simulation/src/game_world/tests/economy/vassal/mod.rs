mod adoption;
mod manager;
mod resources;
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
fn vassal_cost_home_and_one_per_castle_include_queue_and_garrison() {
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
    assert_eq!(w.vassal_castle(id), Some(castle));
    assert_eq!(
        w.issue_train(PlayerId(0), castle, UnitKind::Vassal),
        Err(EconomyError::VassalExists)
    );
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
fn vassal_leash_refuses_without_losing_orders_and_only_admits_home() {
    let (mut w, castle) = setup();
    let id = trained(&mut w, castle);
    let inside = WorldPosition::from_tile_center(TileCoord::new(95, 96)).unwrap();
    w.issue_move(id, inside).unwrap();
    let order = w.movement_order(id);
    assert_eq!(
        w.issue_move(
            id,
            WorldPosition::from_tile_center(TileCoord::new(194, 92)).unwrap()
        ),
        Err(GameWorldError::VassalLeash)
    );
    assert_eq!(w.movement_order(id), order);
    let other = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(100, 90, 4, 4),
            true,
        )
        .unwrap();
    assert_eq!(
        w.issue_garrison(PlayerId(0), id, other),
        Err(EconomyError::VassalHome)
    );
    assert!(
        !w.vassal_position_allowed(
            id,
            WorldPosition::from_tile_center(TileCoord::new(163, 163)).unwrap()
        ),
        "circle, not square"
    );
    let index = w.lookup[&id];
    w.kill_unit(index);
    w.issue_train(PlayerId(0), castle, UnitKind::Vassal)
        .unwrap();
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
fn a_detour_and_soft_separation_cannot_cross_the_leash() {
    let (mut w, castle) = setup();
    let start = WorldPosition::from_tile_center(TileCoord::new(191, 92)).unwrap();
    let id = w.spawn_kind(PlayerId(0), UnitKind::Vassal, start).unwrap();
    w.bind_vassal(id, castle, w.building(castle).unwrap().footprint);
    w.issue_move(
        id,
        WorldPosition::from_tile_center(TileCoord::new(190, 95)).unwrap(),
    )
    .unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_queued_move(
            id,
            WorldPosition::from_tile_center(TileCoord::new(195, 92)).unwrap()
        ),
        Err(EconomyError::Movement(GameWorldError::VassalLeash))
    );
    assert_eq!(w.canonical_hash(), hash);
    // Simulate a planner's detour toward a point beyond the circle, even though
    // its final destination is inside. Admission alone cannot enforce a leash.
    let index = w.lookup[&id];
    let order = w.units[index].order.as_mut().unwrap();
    order.waypoint = WorldPosition::from_tile_center(TileCoord::new(194, 92)).unwrap();
    order.segment_length = 3 * FIXED_SUBUNITS_PER_TILE as u32;
    for _ in 0..100 {
        w.advance();
        assert!(w.vassal_position_allowed(id, w.unit(id).unwrap().position));
    }
    assert_eq!(w.movement_failure(id), Some(GameWorldError::VassalLeash));
    let at = w.unit(id).unwrap().position;
    w.spawn_villager(PlayerId(0), at).unwrap();
    for _ in 0..100 {
        w.advance();
        assert!(w.vassal_position_allowed(id, w.unit(id).unwrap().position));
    }
    w.destroy_building(castle);
    assert_eq!(
        w.issue_move(
            id,
            WorldPosition::from_tile_center(TileCoord::new(195, 92)).unwrap()
        ),
        Err(GameWorldError::VassalLeash),
        "destroying the home never frees the leash"
    );
}
