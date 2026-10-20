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
    assert!(drain(&mut controller).iter().any(|message| matches!(
        message,
        GameplayServerMessage::CommandAck {
            sequence: 2,
            result: CommandResult::RejectedPopulationCap,
            ..
        }
    )));
    drain(&mut observer);
    service.resync(spectator, 2).await;
    assert!(drain(&mut observer).iter().any(|message| matches!(message, GameplayServerMessage::Snapshot { revision:2, units, players, buildings, .. } if units.len() == 4 && players[0].stockpile.food == 150 && buildings[0].training_queue[0].progress_ticks == 2)));
}

#[tokio::test]
async fn a_full_outgoing_queue_disconnects_instead_of_losing_economy_changes() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(1);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(id, 1, region).await;
    let GameplayServerMessage::Snapshot { buildings, .. } = rx.try_recv().unwrap() else {
        panic!("snapshot")
    };
    // The second subscription frame (wildlife) already overflows this same
    // one-slot queue. Reconnect is required before accepting any economy work.
    assert!(rx.is_closed());
    service.subscribe(id, 2, region).await;
    service
        .queue_train(id, 1, buildings[0].id, UnitKind::Villager)
        .await;
    service.tick().await;
    assert!(rx.try_recv().is_err());
    assert!(
        rx.is_closed(),
        "a dropped delta must require reconnect and an authoritative reset"
    );
}

#[tokio::test]
async fn full_subscription_frames_disconnect_when_an_economy_delta_cannot_be_sent() {
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
    assert!(
        rx.is_closed(),
        "failed economy delivery requires an authoritative reset"
    );
}
