use super::*;
use aoe_core::TileRect;
use aoe_protocol::{GameplayServerMessage, encode_gameplay_server};

#[tokio::test]
async fn farming_showcase_streams_valid_initial_and_active_economy_frames() {
    let service = GameplayService::vassal_farming_demo(Seed(7)).unwrap();
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
                    .any(|u| u.vassal.is_some_and(|report| report.managed > 0))
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
        "the recording client sees its active Vassal's report"
    );
}

#[tokio::test]
async fn kingdom_showcase_streams_valid_initial_and_active_encoded_frames() {
    use aoe_core::PlayerId;
    use aoe_protocol::{ResourceKind, Stockpile, UnitKind, decode_gameplay_server};
    let service = GameplayService::vassal_kingdom_demo(Seed(7)).unwrap();
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
                    .any(|u| u.vassal.is_some_and(|report| report.managed == 20))
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
        "recording client receives the active twenty-worker Vassal report"
    );
}
