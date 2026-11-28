use super::*;

#[tokio::test]
async fn diplomacy_orders_reach_a_rust_seat_and_it_accepts_peaceful_offers() {
    use aoe_core::{DiplomaticStance, PlayerId};
    use aoe_protocol::{CommandResult, GameplayClientMessage, GameplayServerMessage};

    let state = match_state(7).await;
    let game = state.gameplay.read().await.clone();
    assert_eq!(
        crate::gameplay_match::play_ai_seats(&state, AiRuntime::Rust).await,
        2
    );
    let human = PlayerId(0);
    let first = PlayerId(1);
    let second = PlayerId(2);
    game.world
        .write()
        .await
        .set_diplomatic_stance(second, human, DiplomaticStance::Enemy)
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let (session, _) = game.register(None, tx).await;
    for (sequence, player) in [(1, first), (2, second)] {
        game.dispatch_client_message(
            session,
            GameplayClientMessage::DiplomacyOrder {
                sequence,
                player,
                stance: DiplomaticStance::Ally,
            },
        )
        .await;
    }
    for _ in 0..80 {
        game.tick().await;
        if game.world.read().await.diplomatic_stance(human, first) == DiplomaticStance::Ally {
            break;
        }
    }
    let world = game.world.read().await;
    assert_eq!(
        world.diplomatic_stance(first, human),
        DiplomaticStance::Ally
    );
    assert_eq!(
        world.diplomatic_stance(second, human),
        DiplomaticStance::Enemy
    );
    assert!(world.alliance_requests(second).next().is_none());
    let mut verdicts = Vec::new();
    while let Ok(message) = rx.try_recv() {
        if let GameplayServerMessage::CommandAck {
            sequence, result, ..
        } = message
        {
            verdicts.push((sequence, result));
        }
    }
    assert_eq!(
        verdicts,
        [
            (1, CommandResult::Accepted),
            (2, CommandResult::RejectedInvalidTarget)
        ]
    );
}
