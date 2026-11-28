use super::{GameplayService, PendingAction};
use aoe_protocol::GameplayClientMessage;

pub(super) async fn dispatch(
    service: &GameplayService,
    session: u64,
    message: GameplayClientMessage,
) {
    let (sequence, action) = match message {
        GameplayClientMessage::DiplomacyOrder {
            sequence,
            player,
            stance,
        } => (sequence, PendingAction::Diplomacy { player, stance }),
        GameplayClientMessage::DiplomacyMessageOrder {
            sequence,
            player,
            message,
        } => (
            sequence,
            PendingAction::DiplomacyMessage { player, message },
        ),
        GameplayClientMessage::TributeOrder {
            sequence,
            player,
            market,
            resource,
            amount,
        } => (
            sequence,
            PendingAction::Tribute {
                player,
                market,
                resource,
                amount,
            },
        ),
        _ => return,
    };
    service.queue_command(session, sequence, action).await;
}
