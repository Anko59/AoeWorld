//! Fog reveals enemies without changing the session's economy/ownership lens.
use super::*;
use crate::gameplay::GameplayService;
use aoe_protocol::GameplayServerMessage as M;

async fn fixture(
    disabled: bool,
) -> (
    GameplayService,
    u64,
    tokio::sync::mpsc::Receiver<M>,
    EntityId,
    EntityId,
) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let enemy_building = world
        .place_town_center(PlayerId(0), TileCoord::new(80, 80))
        .unwrap();
    world
        .place_town_center(PlayerId(1), TileCoord::new(20, 20))
        .unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let primary = world.spawn_unit(PlayerId(1), at(16, 16)).unwrap();
    let enemy = world.spawn_unit(PlayerId(0), at(88, 88)).unwrap();
    world.set_fog_disabled(disabled);
    let service = GameplayService::from_scale_world(world, primary);
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    let (id, _) = service.register(None, sender).await;
    (service, id, receiver, enemy, enemy_building)
}

#[tokio::test]
async fn fog_off_snapshot_keeps_only_the_viewers_economy_while_revealing_enemies() {
    let (service, id, mut receiver, enemy, enemy_building) = fixture(true).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 128, 128))
        .await;
    let mut snapshot = false;
    while let Ok(message) = receiver.try_recv() {
        if let M::Snapshot {
            players,
            units,
            buildings,
            ..
        } = message
        {
            assert!(units.iter().any(|u| u.id == enemy));
            assert!(buildings.iter().any(|b| b.id == enemy_building));
            assert_eq!(
                players.iter().map(|p| p.player).collect::<Vec<_>>(),
                [PlayerId(1)],
                "the HUD/selection fallback must identify the authenticated viewer"
            );
            snapshot = true;
        }
    }
    assert!(snapshot);
}

#[tokio::test]
async fn fog_mode_change_never_inserts_foreign_economies_into_tick_baselines() {
    let (service, id, mut receiver, enemy, enemy_building) = fixture(false).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 128, 128))
        .await;
    while receiver.try_recv().is_ok() {}
    service.set_fog_disabled(true).await;
    let mut revealed_unit = false;
    let mut revealed_building = false;
    for _ in 0..3 {
        service.tick().await;
        while let Ok(message) = receiver.try_recv() {
            if let M::Tick {
                changed_players,
                changed_units,
                changed_buildings,
                ..
            } = message
            {
                revealed_unit |= changed_units.iter().any(|u| u.id == enemy);
                revealed_building |= changed_buildings.iter().any(|b| b.id == enemy_building);
                assert!(
                    changed_players.iter().all(|p| p.player == PlayerId(1)),
                    "fog changes visibility, not the economy used to identify our units"
                );
            }
        }
        let sessions = service.sessions.lock().await;
        assert_eq!(
            sessions[&id]
                .resident_players
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [PlayerId(1)]
        );
    }
    assert!(revealed_unit && revealed_building);
}
