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
