use super::*;
use aoe_core::TileRect;
use aoe_protocol::{GameplayServerMessage, encode_gameplay_server};

#[tokio::test]
async fn vassal_demo_delivers_attack_recall_and_return() {
    use aoe_core::PlayerId;
    use aoe_protocol::UnitKind;
    let service = GameplayService::vassal_defense_demo(Seed(7)).unwrap();
    let (general, home) = {
        let w = service.world.read().await;
        let g = w
            .units()
            .find(|u| u.kind == aoe_simulation::UnitKind::General)
            .unwrap();
        (g.id, g.position)
    };
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(70, 70, 180, 180))
        .await;
    let (mut alert, mut called, mut arrived, mut returned) = (false, false, false, false);
    let mut damaged = false;
    for _ in 0..6_000 {
        while let Ok(message) = receiver.try_recv() {
            encode_gameplay_server(&message).expect("valid Vassal match wire frame");
            let (tick, buildings, units) = match message {
                GameplayServerMessage::Snapshot {
                    tick,
                    buildings,
                    units,
                    ..
                } => (tick, buildings, units),
                GameplayServerMessage::Tick {
                    tick,
                    changed_buildings,
                    changed_units,
                    ..
                } => (tick, changed_buildings, changed_units),
                _ => continue,
            };
            damaged |= units
                .iter()
                .any(|u| u.player == PlayerId(0) && u.last_hit.is_some());
            for report in buildings
                .iter()
                .flat_map(|b| &b.garrison)
                .filter_map(|g| g.vassal)
            {
                if report.alert && !alert {
                    // The filmed call must cause the march: the approach must
                    // not have lured the General into an earlier fight.
                    assert_eq!(
                        service
                            .world
                            .read()
                            .await
                            .unit(general)
                            .unwrap()
                            .position
                            .tile_floor(),
                        home.tile_floor(),
                        "demo General must still be waiting when the Vassal calls"
                    );
                    let dx = i64::from(home.x) - 92 * 1024;
                    let dy = i64::from(home.y) - 92 * 1024;
                    assert!(dx * dx + dy * dy <= (100 * 1024_i64).pow(2));
                    eprintln!(
                        "Vassal alert at tick {}: {report:?}; recall position {home:?}",
                        tick.0
                    );
                }
                alert |= report.alert;
                called |= report.responding > 0;
            }
            for g in units
                .iter()
                .filter(|u| u.id == general && u.kind == UnitKind::General)
            {
                let tile = g.position.tile_floor();
                let dx = i64::from(g.position.x) - 92 * 1024;
                let dy = i64::from(g.position.y) - 92 * 1024;
                if called && dx * dx + dy * dy <= (24 * 1024_i64).pow(2) && !arrived {
                    eprintln!("General arrived at tick {}: {tile:?}", tick.0);
                    arrived = true;
                }
                if arrived
                    && g.position.tile_floor() == home.tile_floor()
                    && !g.moving
                    && !g.planning
                    && g.general
                        .is_some_and(|s| s.state == aoe_protocol::GeneralState::Idle)
                    && !returned
                {
                    eprintln!("General returned at tick {}: {tile:?}", tick.0);
                    returned = true;
                }
            }
        }
        if returned {
            break;
        }
        service.tick().await;
    }
    let w = service.world.read().await;
    let enemies: Vec<_> = w
        .units()
        .filter(|u| u.player != PlayerId(0))
        .map(|u| {
            (
                u.id,
                u.position.tile_floor(),
                u.task,
                w.move_destinations(u.id),
            )
        })
        .collect();
    assert!(alert, "server must publish the attack; enemies {enemies:?}");
    assert!(called, "server must publish the General recall");
    assert!(
        damaged,
        "the recorded fight must include real damage to an owned unit"
    );
    assert!(
        arrived,
        "called General must reach the attacked territory; {:?}",
        w.unit(general)
    );
    assert!(
        returned,
        "General must return to its earlier position; {:?}",
        w.unit(general)
    );
}

#[tokio::test]
async fn stability_showcase_waits_for_the_recorded_garrison_order() {
    let service = GameplayService::steward_stability_demo(Seed(7)).unwrap();
    for _ in 0..40 {
        service.tick().await;
    }
    let world = service.world.read().await;
    let king = world
        .units()
        .find(|u| u.kind == aoe_simulation::UnitKind::Steward)
        .unwrap();
    assert!(world.movement_order(king.id).is_none());
    assert!(world.steward_status(king.id).is_none());
    assert_eq!(
        world
            .units()
            .filter(|u| u.kind == aoe_simulation::UnitKind::Villager
                && world.gathering_kind(u.id) == Some(aoe_map::ResourceKind::Food))
            .count(),
        20
    );
}

#[tokio::test]
async fn farming_showcase_streams_valid_initial_and_active_economy_frames() {
    let service = GameplayService::steward_farming_demo(Seed(7)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(80, 80, 80, 80))
        .await;
    let mut snapshot = false;
    let mut active = false;
    for _ in 0..=40 {
        while let Ok(message) = receiver.try_recv() {
            encode_gameplay_server(&message).expect("showcase must stream a valid game state");
            let buildings = match &message {
                GameplayServerMessage::Snapshot { buildings, .. } => {
                    snapshot = true;
                    buildings
                }
                GameplayServerMessage::Tick {
                    changed_buildings, ..
                } => changed_buildings,
                _ => continue,
            };
            active |= buildings.iter().any(|b| {
                b.garrison
                    .iter()
                    .any(|u| u.steward.is_some_and(|report| report.managed > 0))
            });
        }
        service.tick().await;
    }
    assert!(
        snapshot,
        "the recording client receives an initial snapshot"
    );
    assert!(
        active,
        "the recording client sees its active Steward's report"
    );
}

#[tokio::test]
async fn kingdom_showcase_streams_valid_initial_and_active_encoded_frames() {
    use aoe_core::PlayerId;
    use aoe_protocol::{ResourceKind, Stockpile, UnitKind, decode_gameplay_server};
    let service = GameplayService::steward_kingdom_demo(Seed(7)).unwrap();
    let region = {
        let w = service.world.read().await;
        let min_x = w.buildings().map(|b| b.footprint.min.x).min().unwrap() - 8;
        let min_y = w.buildings().map(|b| b.footprint.min.y).min().unwrap() - 8;
        let max_x = w.buildings().map(|b| b.footprint.max.x).max().unwrap() + 8;
        let max_y = w.buildings().map(|b| b.footprint.max.y).max().unwrap() + 8;
        TileRect::from_xywh(min_x, min_y, max_x - min_x, max_y - min_y)
    };
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
    let (session, _) = service.register(None, sender).await;
    service.subscribe(session, 1, region).await;
    let mut initial = false;
    let mut active = false;
    for _ in 0..=60 {
        while let Ok(message) = receiver.try_recv() {
            let bytes = encode_gameplay_server(&message).expect("valid showcase server frame");
            assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
            let buildings = match &message {
                GameplayServerMessage::Error { .. } => {
                    panic!("showcase subscription failed: {message:?}")
                }
                GameplayServerMessage::Snapshot {
                    units,
                    players,
                    buildings,
                    ..
                } => {
                    assert!(!initial);
                    initial = true;
                    let workers: Vec<_> = units
                        .iter()
                        .filter(|u| u.kind == UnitKind::Villager)
                        .collect();
                    assert_eq!(workers.len(), 20);
                    assert!(
                        workers
                            .iter()
                            .all(|u| u.gathering == Some(ResourceKind::Food)
                                && u.managed_by.is_none())
                    );
                    let player = players.iter().find(|p| p.player == PlayerId(0)).unwrap();
                    assert_eq!(
                        player.stockpile,
                        Stockpile {
                            food: 30_000,
                            wood: 1_000,
                            gold: 30_000,
                            stone: 30_000
                        }
                    );
                    buildings
                }
                GameplayServerMessage::Tick {
                    changed_buildings, ..
                } => changed_buildings,
                _ => continue,
            };
            active |= buildings.iter().any(|b| {
                b.garrison
                    .iter()
                    .any(|u| u.steward.is_some_and(|report| report.managed == 20))
            });
        }
        if initial && active {
            break;
        }
        service.tick().await;
    }
    assert!(
        initial,
        "recording client receives the initial Farm jobs and stocks"
    );
    assert!(
        active,
        "recording client receives the active twenty-worker Steward report"
    );
}

#[tokio::test]
async fn fishing_showcase_streams_valid_managed_ships_and_a_completed_dock() {
    use aoe_protocol::{BuildingKind, UnitKind, decode_gameplay_server};
    let service = GameplayService::steward_fishing_demo(Seed(1)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(40, 0, 210, 180))
        .await;
    let (mut snapshot, mut fleet, mut dock) = (false, false, false);
    for _ in 0..600 {
        while let Ok(message) = receiver.try_recv() {
            let bytes = encode_gameplay_server(&message).expect("valid fleet game frame");
            assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
            let (units, buildings) = match &message {
                GameplayServerMessage::Snapshot {
                    units, buildings, ..
                } => {
                    snapshot = true;
                    (units, buildings)
                }
                GameplayServerMessage::Tick {
                    changed_units,
                    changed_buildings,
                    ..
                } => (changed_units, changed_buildings),
                GameplayServerMessage::Error { .. } => {
                    panic!("coast subscription failed: {message:?}")
                }
                _ => continue,
            };
            fleet |= units
                .iter()
                .any(|u| u.kind == UnitKind::FishingShip && u.managed_by.is_some());
            dock |= buildings
                .iter()
                .any(|b| b.kind == BuildingKind::Dock && b.progress_permille == 1_000);
        }
        if snapshot && fleet && dock {
            break;
        }
        service.tick().await;
    }
    assert!(
        snapshot && fleet && dock,
        "recording client must see actual managed ships and a finished Dock"
    );
}

#[tokio::test]
async fn river_recording_rejects_uniform_world_without_replacing_it() {
    let service = GameplayService::general_demo(Seed(7)).unwrap();
    let before = service.world.read().await.canonical_hash();
    assert!(service.prepare_general_river_demo().await.is_err());
    assert_eq!(service.world.read().await.canonical_hash(), before);
    assert!(service.map_content_hash.is_none());
}

#[tokio::test]
async fn movement_recovery_recording_contains_the_army() {
    for acquired in [false, true] {
        let service = GameplayService::movement_recovery_demo(Seed(1), acquired).unwrap();
        let world = service.world.read().await;
        assert_eq!(world.unit_count(), 102);
        assert!(
            world
                .units()
                .any(|u| u.kind == aoe_simulation::UnitKind::General)
        );
        assert_eq!(world.buildings().count(), 48);
    }
}

#[tokio::test]
async fn vassal_demo_streams_castle_age_stocks_and_the_active_defensive_report() {
    let service = GameplayService::vassal_defense_demo(Seed(7)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(70, 70, 90, 70))
        .await;
    let mut stocks = false;
    let mut active = false;
    for _ in 0..=40 {
        while let Ok(message) = receiver.try_recv() {
            encode_gameplay_server(&message)
                .expect("Vassal demo must stream valid current wire state");
            let buildings = match &message {
                GameplayServerMessage::Snapshot {
                    players, buildings, ..
                } => {
                    stocks |= players
                        .iter()
                        .any(|p| p.age == aoe_protocol::Age::Castle && p.stockpile.food == 20_000);
                    buildings
                }
                GameplayServerMessage::Tick {
                    changed_buildings, ..
                } => changed_buildings,
                _ => continue,
            };
            active |= buildings.iter().any(|b| {
                b.garrison
                    .iter()
                    .any(|g| g.vassal.is_some_and(|r| r.villagers == 20))
            });
        }
        service.tick().await;
    }
    assert!(
        stocks,
        "demo must show its real Castle Age economy instead of dropping an invalid player frame"
    );
    assert!(active, "demo must publish the authoritative Vassal report");
}

#[tokio::test]
async fn road_demo_stream_contains_valid_road_buildings() {
    let service = GameplayService::roads_demo(Seed(7)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(128);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(60, 60, 70, 70))
        .await;
    let mut roads = 0;
    while let Ok(message) = receiver.try_recv() {
        encode_gameplay_server(&message).unwrap();
        if let GameplayServerMessage::Snapshot { buildings, .. } = message {
            roads += buildings
                .iter()
                .filter(|b| b.kind == aoe_protocol::BuildingKind::Road)
                .count();
        }
    }
    assert_eq!(roads, 29);
}
