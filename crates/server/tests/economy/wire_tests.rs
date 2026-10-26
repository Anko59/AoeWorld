use super::*;
use aoe_core::{EntityId, WorldPosition};
use aoe_protocol::{GameplayRole, UnitTask};

async fn controller() -> (
    GameplayService,
    u64,
    mpsc::Receiver<GameplayServerMessage>,
    EntityId,
) {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(64);
    let (id, welcome) = service.register(None, tx).await;
    let GameplayServerMessage::Welcome {
        primary_unit_id,
        role: GameplayRole::Controller,
        ..
    } = welcome
    else {
        panic!("controller")
    };
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384)),
        )
        .await;
    drain(&mut rx);
    (service, id, rx, primary_unit_id)
}

#[tokio::test]
async fn panning_keeps_owned_villagers_authoritative_for_selection_and_worker_counters() {
    let (service, id, mut rx, _) = controller().await;
    let distant = TileRect::new(TileCoord::new(64, 64), TileCoord::new(68, 68));
    service.subscribe(id, 2, distant).await;
    let GameplayServerMessage::Snapshot { units, .. } = rx.try_recv().unwrap() else {
        panic!("snapshot")
    };
    assert_eq!(
        units.len(),
        4,
        "the player's village survives camera panning"
    );
    assert!(
        units
            .iter()
            .all(|unit| !distant.contains(unit.position.tile_floor()))
    );
    let villager = units
        .iter()
        .find(|unit| unit.kind == UnitKind::Villager)
        .unwrap();
    let start = villager.position.tile_floor();
    let destination =
        WorldPosition::from_tile_center(TileCoord::new(start.x - 1, start.y)).unwrap();
    service.queue_move(id, 1, villager.id, destination).await;
    service.tick().await;
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message, GameplayServerMessage::Tick { changed_units, removals, .. }
        if removals.is_empty() && changed_units.iter().any(|unit| unit.id == villager.id && unit.task == UnitTask::Moving)
    )));
}

#[tokio::test]
async fn move_gather_and_train_share_sequence_and_spectator_limits() {
    let (service, id, mut rx, scout) = controller().await;
    service
        .queue_move(id, 1, scout, WorldPosition::new(8_389_632, 8_388_608))
        .await;
    service.queue_gather(id, 1, scout, 2).await;
    service
        .queue_train(id, 1, EntityId(1), UnitKind::Villager)
        .await;
    let rejected = drain(&mut rx);
    assert_eq!(
        rejected
            .iter()
            .filter(|message| matches!(
                message,
                GameplayServerMessage::CommandAck {
                    result: CommandResult::RejectedSequence,
                    ..
                }
            ))
            .count(),
        2
    );
    let (tx, mut observer) = mpsc::channel(64);
    let (spectator, _) = service.register(None, tx).await;
    service.queue_gather(spectator, 2, scout, 2).await;
    service
        .queue_train(spectator, 3, EntityId(1), UnitKind::Villager)
        .await;
    assert_eq!(
        drain(&mut observer)
            .iter()
            .filter(|message| matches!(
                message,
                GameplayServerMessage::CommandAck {
                    result: CommandResult::RejectedNotController,
                    ..
                }
            ))
            .count(),
        2
    );
    service.tick().await;
    assert!(drain(&mut rx).iter().any(|message| matches!(message, GameplayServerMessage::Tick { changed_units, .. } if changed_units.iter().any(|unit| unit.id == scout && unit.task == UnitTask::Moving))));
}

#[tokio::test]
async fn economy_updates_reach_controller_and_spectator_and_resync_is_complete() {
    let (service, id, mut controller, _) = controller().await;
    let (tx, mut observer) = mpsc::channel(64);
    let (spectator, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(spectator, 1, region).await;
    let GameplayServerMessage::Snapshot { buildings, .. } = observer.try_recv().unwrap() else {
        panic!("snapshot")
    };
    service
        .queue_train(id, 1, buildings[0].id, UnitKind::Villager)
        .await;
    service.tick().await;
    let first = drain(&mut controller);
    let second = drain(&mut observer);
    for messages in [first, second] {
        assert!(messages.iter().any(|message| matches!(message, GameplayServerMessage::Tick { revision:1, changed_players, changed_buildings, .. } if changed_players[0].stockpile.food == 150 && changed_buildings[0].training_queue[0].progress_ticks == 1)));
    }
    service
        .queue_train(id, 2, buildings[0].id, UnitKind::Villager)
        .await;
    service.tick().await;
    // The current prepaid queue accepts beyond housing capacity. Resync must
    // include both paid entries, rather than the former population rejection.
    assert!(drain(&mut controller).iter().any(|message| matches!(
        message,
        GameplayServerMessage::CommandAck {
            sequence: 2,
            result: CommandResult::Accepted,
            ..
        }
    )));
    drain(&mut observer);
    service.resync(spectator, 2).await;
    assert!(drain(&mut observer).iter().any(|message| matches!(message, GameplayServerMessage::Snapshot { revision:2, units, players, buildings, .. } if units.len() == 4 && players[0].stockpile.food == 100 && buildings[0].training_queue.len() == 2 && buildings[0].training_queue[0].progress_ticks == 2 && buildings[0].training_queue[1].progress_ticks == 0)));
}

/// Drains `rx` between ticks until a delta carries the changed economy, so a
/// reader that stalled and caught up sees what it missed.
async fn catches_up_on_economy(
    service: &GameplayService,
    rx: &mut mpsc::Receiver<GameplayServerMessage>,
) -> bool {
    for _ in 0..60 {
        service.tick().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let seen = drain(rx).iter().any(|message| {
            matches!(message, GameplayServerMessage::Tick { changed_players, .. } if changed_players.iter().any(|p| p.stockpile.food == 150))
        });
        if seen {
            return true;
        }
    }
    false
}

#[tokio::test]
async fn a_full_outgoing_queue_keeps_the_session_and_catches_up_on_economy_changes() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(1);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(id, 1, region).await;
    let GameplayServerMessage::Snapshot { buildings, .. } = rx.try_recv().unwrap() else {
        panic!("snapshot")
    };
    // The one-slot queue is full from the second frame on: the session stays,
    // the remaining frames wait for room, and ticks fold into the next delta.
    assert!(!rx.is_closed());
    service
        .queue_train(id, 1, buildings[0].id, UnitKind::Villager)
        .await;
    service.tick().await;
    assert!(!rx.is_closed(), "a slow reader is not a lost client");
    assert!(catches_up_on_economy(&service, &mut rx).await);
}

#[tokio::test]
async fn full_subscription_frames_do_not_lose_an_economy_delta() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(4);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(id, 1, region).await;
    let initial = drain(&mut rx);
    let GameplayServerMessage::Snapshot { buildings, .. } = &initial[0] else {
        panic!("snapshot");
    };
    assert!(matches!(
        initial[1],
        GameplayServerMessage::WildlifeState { revision: 1, .. }
    ));
    service.subscribe(id, 2, region).await;
    service
        .queue_train(id, 1, buildings[0].id, UnitKind::Villager)
        .await;
    service.tick().await;
    // The four frames of the second view filled the queue; the delta did not fit.
    let queued = drain(&mut rx);
    assert_eq!(queued.len(), 4);
    assert!(matches!(
        queued[0],
        GameplayServerMessage::Snapshot { revision: 2, .. }
    ));
    assert!(matches!(
        queued[1],
        GameplayServerMessage::WildlifeState { revision: 2, .. }
    ));
    assert!(matches!(&queued[2], GameplayServerMessage::FogState(state) if state.revision == 2));
    assert!(
        matches!(&queued[3], GameplayServerMessage::Projectiles {revision:2,projectiles,..} if projectiles.is_empty())
    );
    assert!(!rx.is_closed(), "the session survives the full queue");
    assert!(catches_up_on_economy(&service, &mut rx).await);
}

#[tokio::test]
async fn first_views_arrive_in_order_even_when_they_must_wait_for_room() {
    let service = GameplayService::new(Seed(1));
    // Two slots: the snapshot and the wildlife fit, fog and projectiles wait.
    let (tx, mut rx) = mpsc::channel(2);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(id, 1, region).await;
    let mut got = Vec::new();
    for _ in 0..200 {
        got.extend(drain(&mut rx));
        if got.len() >= 4 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(got.len(), 4, "every held-back frame is delivered");
    assert!(matches!(
        got[0],
        GameplayServerMessage::Snapshot { revision: 1, .. }
    ));
    assert!(matches!(
        got[1],
        GameplayServerMessage::WildlifeState { revision: 1, .. }
    ));
    assert!(matches!(&got[2], GameplayServerMessage::FogState(state) if state.revision == 1));
    assert!(matches!(
        got[3],
        GameplayServerMessage::Projectiles { revision: 1, .. }
    ));
}

#[tokio::test]
async fn stalled_camera_subscriptions_keep_only_the_in_flight_and_latest_views() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(1);
    let (id, _) = service.register(None, tx).await;
    for revision in 1..=10 {
        service
            .subscribe(id, revision, TileRect::from_xywh(8000, 8000, 64, 64))
            .await;
    }
    let mut revisions = Vec::new();
    for _ in 0..8 {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let revision = match message {
            GameplayServerMessage::Snapshot { revision, .. }
            | GameplayServerMessage::WildlifeState { revision, .. }
            | GameplayServerMessage::Projectiles { revision, .. } => revision,
            GameplayServerMessage::FogState(state) => state.revision,
            other => panic!("unexpected initial view: {other:?}"),
        };
        revisions.push(revision);
    }
    assert_eq!(
        revisions,
        [1, 1, 1, 1, 10, 10, 10, 10],
        "intermediate camera views must be replaced, not retained by waiting tasks"
    );
    assert!(!rx.is_closed());
}

#[tokio::test]
async fn replacing_a_session_cancels_its_waiting_subscription_worker() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut old_rx) = mpsc::channel(1);
    let (old, welcome) = service.register(None, tx).await;
    let GameplayServerMessage::Welcome { resume_token, .. } = welcome else {
        panic!("welcome");
    };
    service
        .subscribe(old, 1, TileRect::from_xywh(8000, 8000, 64, 64))
        .await;
    let (tx, _new_rx) = mpsc::channel(8);
    service.register(resume_token, tx).await;
    tokio::task::yield_now().await;
    assert!(matches!(
        old_rx.try_recv(),
        Ok(GameplayServerMessage::Snapshot { .. })
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), old_rx.recv())
            .await
            .unwrap()
            .is_none(),
        "the old subscription worker must release its queued messages and sender"
    );
}
