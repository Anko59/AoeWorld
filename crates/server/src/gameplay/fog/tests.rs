use super::*;
use crate::gameplay::GameplayService;
use aoe_core::{Seed, WorldConfig, WorldPosition};
use aoe_protocol::{GameplayServerMessage, MAX_SUBSCRIPTION_TILES};
use std::cell::Cell;

thread_local! {
    static WORK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

pub(super) fn record_state(region: TileRect) {
    WORK.with(|work| {
        let (calls, tiles) = work.get();
        work.set((
            calls + 1,
            tiles + (region.width() * region.height()) as usize,
        ));
    });
}

async fn fixture() -> (
    GameplayService,
    u64,
    tokio::sync::mpsc::Receiver<GameplayServerMessage>,
) {
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    let scout = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap(),
        )
        .unwrap();
    let service = GameplayService::from_world(world, scout, None, None);
    let (tx, _) = tokio::sync::mpsc::channel(64);
    service.register(None, tx).await; // Keep a controller so the tested session spectates.
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    (service, id, rx)
}

#[tokio::test]
async fn obsolete_subscriptions_do_no_fog_work() {
    let (service, id, mut rx) = fixture().await;
    let region = TileRect::from_xywh(0, 0, 512, 512);
    service.subscribe(id, 1, region).await;
    while rx.try_recv().is_ok() {}
    WORK.with(|work| work.set((0, 0)));
    for _ in 0..12 {
        service.subscribe(id, 1, region).await;
        assert!(matches!(
            rx.try_recv().unwrap(),
            GameplayServerMessage::Error { code: 409, .. }
        ));
    }
    assert_eq!(
        WORK.with(Cell::get),
        (0, 0),
        "obsolete requests computed fog"
    );
}

#[tokio::test]
async fn rate_limited_subscriptions_do_no_fog_work_and_accepted_cost_is_bounded() {
    let (service, id, mut rx) = fixture().await;
    let side = MAX_SUBSCRIPTION_TILES;
    let region = TileRect::from_xywh(0, 0, side, side);
    // Seed the existing rate window without relying on machine timing.
    service
        .sessions
        .lock()
        .await
        .get_mut(&id)
        .unwrap()
        .subscription_times = std::collections::VecDeque::from(vec![std::time::Instant::now(); 10]);
    WORK.with(|work| work.set((0, 0)));
    for revision in 1..=12 {
        service.subscribe(id, revision, region).await;
        assert!(matches!(
            rx.try_recv().unwrap(),
            GameplayServerMessage::Error { code: 429, .. }
        ));
    }
    assert_eq!(
        WORK.with(Cell::get),
        (0, 0),
        "rate-limited requests computed fog"
    );
    service
        .sessions
        .lock()
        .await
        .get_mut(&id)
        .unwrap()
        .subscription_times
        .clear();
    service.subscribe(id, 20, region).await;
    assert_eq!(WORK.with(Cell::get), (1, (side * side) as usize));
    let mut saw_fog = false;
    while let Ok(message) = rx.try_recv() {
        if let GameplayServerMessage::FogState(state) = message {
            assert_eq!(state.tiles.len(), (side * side) as usize);
            assert!(state.overview.len() <= 65_536);
            saw_fog = true;
        }
    }
    assert!(saw_fog);
    WORK.with(|work| work.set((0, 0)));
    service
        .subscribe(id, 21, TileRect::from_xywh(0, 0, side + 1, side))
        .await;
    assert!(matches!(
        rx.try_recv().unwrap(),
        GameplayServerMessage::Error { code: 400, .. }
    ));
    service.subscribe(u64::MAX, 1, region).await;
    assert_eq!(
        WORK.with(Cell::get),
        (0, 0),
        "rejected requests computed fog"
    );
}
