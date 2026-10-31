use super::*;

#[tokio::test]
async fn shared_pages_cannot_expand_the_match_command_storage_budget() {
    let service = GameplayService::new(Seed(7));
    let mut receivers = Vec::new();
    for _ in 0..53 {
        let (sender, receiver) = mpsc::channel(32);
        receivers.push(receiver);
        let (id, _) = service.register(None, sender).await;
        for sequence in 1..=20 {
            service.queue_stop(id, sequence, EntityId(0)).await;
        }
    }
    let sessions = service.sessions.lock().await;
    assert_eq!(total_pending(&sessions), MAX_PENDING_COMMANDS_GLOBAL);
    let rejected = receivers
        .iter_mut()
        .flat_map(|receiver| {
            let mut messages = Vec::new();
            while let Ok(message) = receiver.try_recv() {
                messages.push(message);
            }
            messages
        })
        .filter(|message| {
            matches!(
                message,
                GameplayServerMessage::CommandAck {
                    result: CommandResult::RejectedQueueFull,
                    ..
                }
            )
        })
        .count();
    assert_eq!(rejected, 53 * 20 - MAX_PENDING_COMMANDS_GLOBAL);
}
