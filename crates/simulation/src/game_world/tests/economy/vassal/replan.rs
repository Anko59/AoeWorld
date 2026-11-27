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

#[test]
fn completed_gate_satisfies_wall_sites_even_when_damaged() {
    let (mut w, v, _, _) = pending_domain();
    let gate = w
        .insert_building(
            PlayerId(0),
            BuildingKind::StoneGate,
            TileRect::from_xywh(95, 98, 4, 1),
            true,
        )
        .unwrap();
    let sites =
        [95, 96, 97, 98].map(|x| Site::new(BuildingKind::PalisadeWall, TileCoord::new(x, 98), 0));
    let work = |w: &GameWorld| w.vassal_workload(&w.economy.vassals.domains[&v], &sites);
    assert!(work(&w).deficits.is_empty());
    w.building_mut(gate).unwrap().hp -= 20;
    let work = work(&w);
    assert!(work.deficits.is_empty());
    assert!(work.open.is_empty());
}

#[test]
fn paid_road_omitted_from_the_new_plan_remains_available_to_builders() {
    let (mut w, v, _, _) = pending_domain();
    let tile = TileCoord::new(95, 98);
    let road = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Road,
            TileRect::from_xywh(tile.x, tile.y, 1, 1),
            false,
        )
        .unwrap();
    w.economy.vassals.domains.get_mut(&v).unwrap().replan_at = Some(w.tick.0);
    w.vassal_plan(v);
    let domain = &w.economy.vassals.domains[&v];
    let work = w.vassal_workload(domain, &[]);
    assert_eq!(
        work.open.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [road]
    );
}

#[test]
fn private_road_on_the_plan_is_counted_once_by_its_owner() {
    let (mut w, v, _, _) = pending_domain();
    let player = PlayerId(0);
    let tile = TileCoord::new(95, 98);
    let road = w
        .insert_building(
            player,
            BuildingKind::Road,
            TileRect::from_xywh(tile.x, tile.y, 1, 1),
            false,
        )
        .unwrap();
    assert_eq!(w.building_at_for(player, tile), Some(road));
    assert_eq!(w.building_at_for(PlayerId(1), tile), None);
    w.economy.vassals.domains.get_mut(&v).unwrap().paid = vec![road];
    let site = Site::new(BuildingKind::Road, tile, 0);
    let work = w.vassal_workload(&w.economy.vassals.domains[&v], &[site]);
    assert!(work.deficits.is_empty());
    assert_eq!(work.open.len(), 1);
    assert_eq!(work.open[0].0, road);
}

#[test]
fn one_development_visit_fills_the_paid_pipeline_with_separate_sites() {
    let (mut w, v, _, _) = pending_domain();
    let sites: Vec<_> = (0..30)
        .map(|n| {
            Site::new(
                BuildingKind::Road,
                TileCoord::new(50 + 2 * n, 110),
                TIER_ROAD,
            )
        })
        .collect();
    let domain = w.economy.vassals.domains.get_mut(&v).unwrap();
    domain.pending = None;
    domain.plan = sites.clone();
    domain.development = true;

    w.vassal_develop(v);
    assert_eq!(
        sites
            .iter()
            .filter(|site| w.building_at_for(PlayerId(0), site.tile).is_some())
            .count(),
        24
    );
    w.vassal_develop(v);
    assert_eq!(
        sites
            .iter()
            .filter(|site| w.building_at_for(PlayerId(0), site.tile).is_some())
            .count(),
        24,
        "existing paid foundations reserve all pipeline slots"
    );
}

#[test]
fn the_paid_pipeline_extends_a_staffed_wall_run_before_opening_a_distant_one() {
    let (mut w, v, _, worker) = pending_domain();
    let player = PlayerId(0);
    let staffed = TileCoord::new(110, 110);
    let next = TileCoord::new(111, 110);
    let other = TileCoord::new(96, 94);
    let kind = BuildingKind::PalisadeWall;
    w.issue_build_wall(player, &[worker], kind, staffed, staffed, false)
        .unwrap();
    let other_paid = (0..22)
        .map(|x| {
            w.insert_building(
                player,
                BuildingKind::Road,
                TileRect::from_xywh(75 + x, 130, 1, 1),
                false,
            )
            .unwrap()
        })
        .collect();
    let domain = w.economy.vassals.domains.get_mut(&v).unwrap();
    domain.pending = None;
    domain.plan = vec![
        Site::new(kind, staffed, 1),
        Site::new(kind, next, 1),
        Site::new(kind, other, 1),
    ];
    domain.paid = other_paid;
    domain.development = true;

    w.vassal_develop(v);

    assert!(w.building_at_for(player, next).is_some());
    assert!(w.building_at_for(player, other).is_none());
}

#[test]
fn blocked_candidates_do_not_hide_valid_sites_after_the_first_thirty_two() {
    let (mut w, v, _, _) = pending_domain();
    let close = (0..38).map(|n| {
        Site::new(
            BuildingKind::Road,
            TileCoord::new(52 + 2 * n, 108),
            TIER_ROAD,
        )
    });
    let far = Site::new(BuildingKind::Road, TileCoord::new(100, 155), TIER_ROAD);
    let mut plan = Vec::new();
    for site in close {
        w.economy.blocked_tiles.insert(site.tile);
        plan.push(site);
    }
    plan.push(far);
    let d = w.economy.vassals.domains.get_mut(&v).unwrap();
    d.pending = None;
    d.plan = plan;
    d.development = true;
    w.vassal_develop(v);
    assert!(w.building_at_for(PlayerId(0), far.tile).is_some());
}

#[test]
fn tower_plan_reaches_every_work_point_beyond_the_first_eight() {
    let (mut w, v, _, _) = pending_domain();
    let center = w.economy.vassals.domains[&v].center;
    for (dx, dy) in [
        (-40, -40),
        (-20, -40),
        (0, -40),
        (20, -40),
        (40, -40),
        (-40, -20),
        (-20, -20),
        (0, -20),
        (20, -20),
        (40, -20),
    ] {
        w.insert_building(
            PlayerId(0),
            BuildingKind::MiningCamp,
            TileRect::from_xywh(center.x + dx, center.y + dy, 2, 2),
            true,
        )
        .unwrap();
    }
    let plan = sites(&w, center, PlayerId(0), Age::Castle, &BTreeMap::new(), None);
    assert_eq!(
        plan.iter().filter(|s| s.tier == TIER_WORK_TOWER).count(),
        11
    );
}
