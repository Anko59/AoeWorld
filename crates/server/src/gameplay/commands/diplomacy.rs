use super::{GameplayService, PendingAction};
use aoe_protocol::{GameplayClientMessage, ResourceKind};

pub(super) fn native_resource(resource: ResourceKind) -> aoe_map::ResourceKind {
    use aoe_map::ResourceKind as Native;
    match resource {
        ResourceKind::Food => Native::Food,
        ResourceKind::Wood => Native::Wood,
        ResourceKind::Gold => Native::Gold,
        ResourceKind::Stone => Native::Stone,
    }
}

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
