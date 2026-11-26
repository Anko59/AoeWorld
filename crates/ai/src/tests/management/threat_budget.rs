//! The management, fortification and navy cases of
//! `ai/tests/scaling/test_threat_budget.py`: a peaceful settlement spends its
//! workforce and savings on growth, a threatened one on its defence.
use std::collections::BTreeSet;

use aoe_sdk::{Age, BuildingKind, EntityId, Event, PlayerId, ResourceKind, TileCoord, UnitKind};

use super::fortifications::{manager, step};
use super::{Kingdom, Sent, kingdom, seated};
use crate::brain::absorb;
use crate::play::management::{ManagementManager, funding_reserve};
use crate::play::naval::NavalManager;
use crate::tests::fake::{building, enemy, foreign_building, node, under_attack, unit_of};
use crate::view::Tile;

fn castle_with_vassal(realm: &mut Kingdom) -> u32 {
    realm.push_building(seated(
        950,
        BuildingKind::Castle,
        (65, 65),
        (951, UnitKind::Vassal),
    ));
    950
}

fn barracks_at(realm: &mut Kingdom, tile: Tile) {
    realm.world.enemy_buildings = vec![foreign_building(999, BuildingKind::Barracks, tile)];
}

fn lumber_camp(realm: &mut Kingdom, tile: Tile) {
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::LumberCamp, tile));
}

fn raider(realm: &mut Kingdom, tile: Tile) {
    realm.world.enemies = vec![enemy(999, UnitKind::Militia, tile)];
}

#[test]
fn test_peaceful_fishing_dock_does_not_buy_defensive_galleys() {
    let mut realm = kingdom(29, 1);
    realm.economy().age = Age::Feudal;
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::Dock, (65, 60)));
    realm
        .world
        .units
        .push(unit_of(901, UnitKind::FishingShip, (65, 61)));
    realm.world.nodes = vec![node(999, ResourceKind::Food, (66, 62))];
    realm.world.passable_fn = Some(Box::new(|_| Some(false)));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    NavalManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
    assert!(
        tape.orders().trains().is_empty(),
        "unthreatened water is an economy investment, not a war fleet"
    );
}

#[test]
fn test_peaceful_resource_site_and_distant_enemy_do_not_justify_towers() {
    let far = vec![foreign_building(999, BuildingKind::TownCenter, (800, 800))];
    for enemies in [vec![], far] {
        let mut realm = kingdom(120, 1);
        lumber_camp(&mut realm, (76, 52));
        realm.world.enemy_buildings = enemies;
        let tape = realm.tape();
        let mut ctx = realm.ctx();
        step(&mut ctx);
        assert!(
            tape.orders().builds().is_empty(),
            "income alone must not authorize a tower"
        );
    }
}

#[test]
fn test_peaceful_vassal_stops_development_and_releases_reserved_workers() {
    let mut realm = kingdom(60, 1);
    let castle = castle_with_vassal(&mut realm);
    let workers: BTreeSet<u32> = realm.world.units.iter().take(8).map(|u| u.id).collect();
    realm.board.vassal_workers.insert(castle, workers.clone());
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ctx.reserved_workers.extend(workers.iter().copied());
    let mut manager = ManagementManager::default();
    manager.settings(&mut ctx).expect("the order budget holds");
    manager.defenders(&mut ctx).expect("the order budget holds");
    assert_eq!(tape.orders().vassal_development(), vec![(951, false)]);
    assert!(ctx.reserved_workers.is_disjoint(&workers));
    assert!(ctx.board.vassal_workers.is_empty());
    assert!(tape.orders().moves().is_empty());
}

#[test]
fn test_peaceful_castle_recruits_general_without_saving_for_a_vassal() {
    let mut realm = kingdom(35, 1);
    realm.push_building(building(950, BuildingKind::Castle, (65, 65)));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    assert_eq!(
        funding_reserve(&ctx),
        [800, 0, 800, 0],
        "science must not fund an unneeded Vassal"
    );
    ManagementManager::default()
        .recruit(&mut ctx)
        .expect("the order budget holds");
    assert!(
        tape.orders()
            .trains()
            .iter()
            .any(|(_, kind)| *kind == UnitKind::General)
    );
    assert_eq!(
        funding_reserve(&ctx),
        [0, 0, 0, 0],
        "the ordered General has already been paid"
    );
}

#[test]
fn test_nearby_enemy_enables_only_the_frontier_castles_vassal() {
    let mut realm = kingdom(180, 1);
    let first = castle_with_vassal(&mut realm);
    let rear = seated(
        960,
        BuildingKind::Castle,
        (700, 700),
        (961, UnitKind::Vassal),
    );
    realm.push_building(rear);
    barracks_at(&mut realm, (130, 80));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let mut manager = ManagementManager::default();
    manager.settings(&mut ctx).expect("the order budget holds");
    assert_eq!(
        tape.orders().vassal_development(),
        vec![(951, true), (961, false)]
    );
    manager.defenders(&mut ctx).expect("the order budget holds");
    assert!(ctx.board.vassal_workers.contains_key(&first));
    assert!(!ctx.board.vassal_workers.contains_key(&960));
}

#[test]
fn test_vassal_in_a_quiet_rear_castle_does_not_fill_a_threatened_frontier_seat() {
    let mut realm = kingdom(35, 1);
    realm.config.stewards = 0;
    realm.config.generals = 0;
    realm.push_building(seated(
        950,
        BuildingKind::Castle,
        (700, 700),
        (951, UnitKind::Vassal),
    ));
    realm.push_building(building(960, BuildingKind::Castle, (65, 65)));
    barracks_at(&mut realm, (130, 80));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    assert!(
        funding_reserve(&ctx)[0] > 0,
        "the threatened seat still needs funding"
    );
    ManagementManager::default()
        .recruit(&mut ctx)
        .expect("the order budget holds");
    assert_eq!(tape.orders().trains(), vec![(960, UnitKind::Vassal)]);
}

#[test]
fn test_local_loss_allows_defences_then_expires_without_funding_remote_losses() {
    for (tile, mine, expected) in [
        ((75, 55), true, true),
        ((800, 800), true, false),
        ((75, 55), false, false),
    ] {
        let mut realm = kingdom(40, 1);
        lumber_camp(&mut realm, (76, 52));
        let tape = realm.tape();
        let died = [Event::UnitDied {
            id: EntityId(9),
            kind: UnitKind::Spearman,
            owner: PlayerId(if mine { 0 } else { 1 }),
            mine,
            tile: TileCoord::new(tile.0, tile.1),
        }];
        {
            let mut ctx = realm.ctx_with(&died);
            absorb(&mut ctx);
            step(&mut ctx);
        }
        assert_eq!(
            !tape.orders().builds().is_empty(),
            expected,
            "{tile:?} {mine}"
        );
        let before = tape.orders().builds().len();
        realm.board.plans.clear();
        realm.world.tick += realm.ticks(20 * 60);
        let mut ctx = realm.ctx();
        step(&mut ctx);
        assert_eq!(
            tape.orders().builds().len(),
            before,
            "old losses cannot fund permanent spending"
        );
    }
}

#[test]
fn test_visible_raider_protects_resource_work_without_a_known_enemy_building() {
    let mut realm = kingdom(40, 1);
    lumber_camp(&mut realm, (76, 52));
    raider(&mut realm, (85, 55));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(!tape.orders().towers().is_empty());
}

#[test]
fn test_lost_own_building_uses_its_last_observed_position() {
    let mut realm = kingdom(40, 1);
    lumber_camp(&mut realm, (76, 52));
    {
        let mut ctx = realm.ctx();
        absorb(&mut ctx);
    }
    realm.world.buildings.retain(|b| b.id != 900);
    castle_with_vassal(&mut realm);
    let tape = realm.tape();
    let destroyed = [Event::BuildingDestroyed {
        id: EntityId(900),
        kind: BuildingKind::LumberCamp,
        owner: PlayerId(0),
        mine: true,
    }];
    let mut ctx = realm.ctx_with(&destroyed);
    absorb(&mut ctx);
    ManagementManager::default()
        .settings(&mut ctx)
        .expect("the order budget holds");
    assert_eq!(tape.orders().vassal_development(), vec![(951, true)]);
}

#[test]
fn test_damage_without_a_loss_funds_local_defences_and_alert_memory_is_bounded() {
    let mut realm = kingdom(40, 1);
    castle_with_vassal(&mut realm);
    let tape = realm.tape();
    let mut events: Vec<Event> = (0..1000)
        .map(|i| under_attack(1, None, false, (i * 8, 60)))
        .collect();
    events.push(under_attack(1, None, false, (70, 60)));
    let mut ctx = realm.ctx_with(&events);
    absorb(&mut ctx);
    assert!(ctx.board.defence_alerts.len() <= ctx.config.fort_alert_limit);
    ManagementManager::default()
        .settings(&mut ctx)
        .expect("the order budget holds");
    assert_eq!(tape.orders().vassal_development(), vec![(951, true)]);
}

#[test]
fn test_remote_camp_under_attack_gets_its_own_tower_without_a_threatened_town_center() {
    let mut realm = kingdom(60, 1);
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::LumberCamp, (250, 52)));
    raider(&mut realm, (255, 55));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    let towers = tape.orders().towers();
    assert!(
        towers.first().is_some_and(|t| (t.0 - 250).abs() < 15),
        "a threatened remote site authorises its own tower: {towers:?}"
    );
}

#[test]
fn test_newly_threatened_settlement_builds_though_a_quiet_one_holds_towers() {
    let mut realm = kingdom(120, 3);
    let centers: Vec<Tile> = realm
        .world
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::TownCenter)
        .map(|b| b.center())
        .collect();
    let quiet = centers[0];
    for i in 0..4 {
        realm.push_building(building(
            800 + i as u32,
            BuildingKind::WatchTower,
            (quiet.0 + 6 + 2 * i, quiet.1),
        ));
    }
    let far = *centers.iter().max().expect("centers");
    raider(&mut realm, (far.0 + 25, far.1 + 5));
    realm.world.enemy_buildings = vec![foreign_building(
        998,
        BuildingKind::Barracks,
        (far.0 + 60, far.1),
    )];
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(
        !tape.orders().towers().is_empty(),
        "towers at a quiet settlement must not consume the threatened one's allowance"
    );
}

#[test]
fn test_threatened_site_stops_at_its_own_cap() {
    let mut realm = kingdom(300, 1);
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::LumberCamp, (250, 52)));
    for i in 0..2 {
        realm.push_building(building(
            800 + i as u32,
            BuildingKind::WatchTower,
            (246 + 8 * i, 52),
        ));
    }
    raider(&mut realm, (255, 55));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(
        tape.orders().builds().is_empty(),
        "a site never exceeds its per-site tower cap"
    );
}

/// `overlapping_sites`: a threatened centre at the origin and a threatened
/// camp centred 24 tiles east of it; `towers` are offsets from the centre.
fn overlapping_sites(towers: &[Tile], villagers: u32) -> Vec<Tile> {
    let mut realm = kingdom(villagers, 1);
    let (cx, cy) = realm.world.town_center_center();
    // Tiles are chosen so the 2x2 footprints are CENTRED on the stated offsets.
    lumber_camp(&mut realm, (cx + 24, cy - 1));
    for (i, (dx, dy)) in towers.iter().enumerate() {
        realm.push_building(building(
            800 + i as u32,
            BuildingKind::WatchTower,
            (cx + dx - 1, cy + dy - 1),
        ));
    }
    raider(&mut realm, (cx + 12, cy + 8));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    tape.orders().towers()
}

#[test]
fn test_town_center_towers_on_its_far_side_do_not_deny_an_overlapping_camp() {
    assert!(
        !overlapping_sites(&[(-20, 0), (-22, 0)], 120).is_empty(),
        "each tower is counted once, for the site it serves"
    );
}

#[test]
fn test_camp_cannot_exceed_its_cap_when_its_reach_overlaps_a_threatened_center() {
    assert!(
        overlapping_sites(&[(42, 0), (25, 17)], 90).is_empty(),
        "two towers in the camp reach fill its cap"
    );
}

#[test]
fn test_candidate_tower_is_capped_against_the_site_that_will_own_it() {
    let mut realm = kingdom(90, 1);
    let (cx, cy) = realm.world.town_center_center();
    super::fortifications::exact_sites(&mut realm);
    // 2x2 footprints are centred on the stated offsets: camp (16,0), towers (16,+-12).
    let camp = building(900, BuildingKind::LumberCamp, (cx + 15, cy - 1));
    realm.push_building(camp.clone());
    for (i, dy) in [12, -12].into_iter().enumerate() {
        realm.push_building(building(
            800 + i as u32,
            BuildingKind::WatchTower,
            (cx + 15, cy + dy - 1),
        ));
    }
    barracks_at(&mut realm, (cx + 80, cy));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    // The fixture must reproduce the geometry: the centre's own lane tower lies nearer the camp.
    let lane = manager()
        .lane(&mut ctx, (cx, cy), &BTreeSet::new())
        .expect("a lane");
    assert_eq!(camp.center(), (cx + 16, cy));
    let offset = ctx.config.fort_lane_tower_offset;
    let spot = (lane.0 - offset, lane.1);
    let squared = |a: Tile, b: Tile| (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2);
    assert!(squared(spot, camp.center()) < squared(spot, (cx, cy)));
    ctx.board.fort_lanes.insert((cx, cy), lane);
    step(&mut ctx);
    assert!(
        tape.orders().towers().is_empty(),
        "a tower the nearest-site rule gives to the full camp must not be placed"
    );
}
