//! Replan requests raised during an unfinished road search (review F1–F3).
use super::*;
use crate::{Stockpile, UnitKind};

fn pending_domain() -> (GameWorld, EntityId, Site, EntityId) {
    let config = aoe_core::WorldConfig::new(256, 256, aoe_core::Seed(7)).unwrap();
    let mut w = GameWorld::new(config).unwrap();
    w.set_fog_disabled(true);
    w.place_town_center(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
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
    let at = WorldPosition::from_tile_center(TileCoord::new(89, 96)).unwrap();
    let v = w.spawn_kind(PlayerId(0), UnitKind::Vassal, at).unwrap();
    w.activate_vassal(v, castle);
    let worker = w.spawn_villager(PlayerId(0), at).unwrap();
    w.tick.0 = 2_400;
    let tower = w.vassal_plan(v)[0];
    assert_eq!(tower.kind, BuildingKind::WatchTower);
    assert!(w.vassal_planning_pending(v));
    w.economy
        .vassals
        .domains
        .get_mut(&v)
        .unwrap()
        .workers
        .insert(worker);
    (w, v, tower, worker)
}

fn finish_roads(w: &mut GameWorld, v: EntityId) {
    for _ in 0..2_000 {
        if !w.vassal_planning_pending(v) {
            return;
        }
        w.poll_vassal_plan(v);
    }
    panic!("road job must finish");
}

#[test]
fn earliest_deadline_is_hashed_and_rebuilds_during_pending_roads() {
    let (mut a, v, _, _) = pending_domain();
    let (mut b, _, _, _) = pending_domain();
    for w in [&mut a, &mut b] {
        // Requests during the first minute must not lose time to saturation.
        w.tick.0 = 50;
        w.economy.vassals.domains.get_mut(&v).unwrap().planned = 50;
    }
    a.economy
        .vassals
        .domains
        .get_mut(&v)
        .unwrap()
        .request_replan(250);
    b.economy
        .vassals
        .domains
        .get_mut(&v)
        .unwrap()
        .request_replan(251);
    assert_ne!(a.canonical_hash(), b.canonical_hash());
    a.economy
        .vassals
        .domains
        .get_mut(&v)
        .unwrap()
        .request_replan(300);
    b.economy
        .vassals
        .domains
        .get_mut(&v)
        .unwrap()
        .request_replan(250);
    assert_eq!(
        a.canonical_hash(),
        b.canonical_hash(),
        "earliest request wins"
    );
    a.tick.0 = 249;
    a.vassal_plan(v);
    assert_eq!(a.economy.vassals.domains[&v].planned, 50);
    assert!(a.vassal_planning_pending(v));
    a.tick.0 = 250;
    a.vassal_plan(v);
    assert_eq!(a.economy.vassals.domains[&v].planned, 250);
    assert_eq!(a.economy.vassals.domains[&v].replan_at, None);
    assert!(
        a.vassal_planning_pending(v),
        "replacement roads remain budgeted"
    );
    b.tick.0 = 250;
    b.vassal_plan(v);
    assert_eq!(a.canonical_hash(), b.canonical_hash());
}

#[test]
fn f1_stalled_foundation_replan_survives_road_completion() {
    let (mut w, v, tower, _) = pending_domain();
    let building = w
        .insert_building(
            PlayerId(0),
            tower.kind,
            footprint(tower.kind, tower.tile),
            false,
        )
        .unwrap();
    let d = w.economy.vassals.domains.get_mut(&v).unwrap();
    d.plan = vec![tower];
    d.progress = Some((building, 0, 1_200));
    // Keep the staffing pass from purchasing unrelated foundations.
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile::default();
    w.vassal_develop(v);
    assert!(
        w.economy.vassals.domains[&v]
            .refused
            .contains_key(&tower.tile)
    );
    w.tick.0 += 1;
    finish_roads(&mut w, v);
    w.tick.0 = 2_600;
    let rebuilt = w.vassal_plan(v);
    assert_eq!(
        w.economy.vassals.domains[&v].planned, 2_600,
        "F1: fortifications must rebuild by the stall's 200-tick deadline"
    );
    assert!(
        rebuilt
            .iter()
            .filter(|s| s.kind == tower.kind)
            .all(|s| s.tile != tower.tile),
        "F1: the stalled foundation must not be reused while refused"
    );
}

#[test]
fn f2_raid_rebuilds_fortifications_while_roads_are_pending() {
    let (mut w, v, _, _) = pending_domain();
    let at = WorldPosition::from_tile_center(TileCoord::new(92, 110)).unwrap();
    let enemy = w.spawn_kind(PlayerId(1), UnitKind::Militia, at).unwrap();
    w.vassal_defend(v, &[w.unit(enemy).unwrap()], &mut Vec::new());
    assert!(w.vassal_planning_pending(v));
    let plan = w.vassal_plan(v);
    let d = &w.economy.vassals.domains[&v];
    let expected = sites(
        &w,
        d.center,
        d.player,
        Age::Castle,
        &d.refused,
        Some(at.tile_floor()),
    );
    let fortifications = |plan: &[Site]| {
        plan.iter()
            .filter(|s| !matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge))
            .map(|s| (s.kind, s.tile, s.tier, s.vertical))
            .collect::<Vec<_>>()
    };
    assert!(
        !fortifications(&plan).is_empty(),
        "F2: a road pass must retain fortifications"
    );
    assert_eq!(
        fortifications(&plan),
        fortifications(&expected),
        "F2: the new threatened side must be planned immediately"
    );
    finish_roads(&mut w, v);
    assert_eq!(
        fortifications(&w.economy.vassals.domains[&v].plan),
        fortifications(&expected)
    );
}

#[test]
fn f3_refused_placement_relocates_before_refusal_expires() {
    let (mut w, v, _, _) = pending_domain();
    let site = *w.economy.vassals.domains[&v]
        .plan
        .iter()
        .find(|s| s.kind == BuildingKind::Barracks)
        .unwrap();
    // Leave the origin empty but overlap the barracks' remaining footprint.
    w.insert_building(
        PlayerId(0),
        BuildingKind::House,
        TileRect::from_xywh(site.tile.x + 1, site.tile.y, 2, 2),
        true,
    )
    .unwrap();
    assert!(w.building_at(site.tile).is_none());
    w.economy.vassals.domains.get_mut(&v).unwrap().plan = vec![site];
    w.vassal_develop(v);
    assert!(
        w.economy.vassals.domains[&v]
            .refused
            .contains_key(&site.tile)
    );
    w.tick.0 += 1;
    finish_roads(&mut w, v);
    w.tick.0 = 2_600;
    let rebuilt = w.vassal_plan(v);
    assert!(
        rebuilt
            .iter()
            .filter(|s| s.kind == site.kind)
            .all(|s| s.tile != site.tile),
        "F3: the refused barracks must move within 200 ticks, before refusal expiry"
    );
    assert!(rebuilt.iter().any(|s| s.kind == site.kind));
}
