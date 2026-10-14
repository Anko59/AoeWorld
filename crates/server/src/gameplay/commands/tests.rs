use super::*;
use aoe_core::Seed;
use tokio::sync::mpsc;

#[tokio::test]
async fn queue_move_rejects_unknown_and_non_controller_sessions_then_deduplicates_sequences() {
    let service = GameplayService::new(Seed(1));
    let (controller_tx, mut controller_rx) = mpsc::channel(MAX_ACK_HISTORY + 16);
    let (controller_id, _) = service.register(None, controller_tx).await;

    service
        .queue_move(
            controller_id + 1_000,
            1,
            EntityId(0),
            WorldPosition::default(),
        )
        .await;
    assert!(controller_rx.try_recv().is_err());

    let (spectator_tx, mut spectator_rx) = mpsc::channel(8);
    let (spectator_id, _) = service.register(None, spectator_tx).await;
    service
        .queue_move(spectator_id, 1, EntityId(0), WorldPosition::default())
        .await;
    assert!(matches!(
        spectator_rx.try_recv().expect("spectator rejection"),
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedNotController,
            ..
        }
    ));

    service
        .queue_move(controller_id, 1, EntityId(0), WorldPosition::default())
        .await;
    service
        .queue_move(controller_id, 1, EntityId(0), WorldPosition::default())
        .await;
    assert!(matches!(
        controller_rx
            .try_recv()
            .expect("duplicate sequence rejection"),
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::RejectedSequence,
            ..
        }
    ));

    for sequence in (0..MAX_ACK_HISTORY + 2).rev() {
        service
            .queue_move(
                controller_id,
                sequence as u64,
                EntityId(0),
                WorldPosition::default(),
            )
            .await;
    }
    let mut rejected = 0;
    while let Ok(GameplayServerMessage::CommandAck {
        result: CommandResult::RejectedSequence,
        ..
    }) = controller_rx.try_recv()
    {
        rejected += 1;
    }
    assert!(rejected > MAX_ACK_HISTORY);
}
