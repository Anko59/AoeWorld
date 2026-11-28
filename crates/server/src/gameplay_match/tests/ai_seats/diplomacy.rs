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

#[tokio::test]
async fn rust_ai_answers_requests_and_hands_over_the_demo_after_conquest() {
    use aoe_core::{DiplomacyMessage, DiplomaticStance, PlayerId, ResourceKind};
    use aoe_protocol::GameplayClientMessage;
    let state = match_state(7).await;
    let game =
        crate::gameplay::GameplayService::diplomacy_conquest_demo(aoe_core::Seed(7)).unwrap();
    *state.gameplay.write().await = game.clone();
    assert_eq!(
        crate::gameplay_match::play_ai_seats(&state, AiRuntime::Rust).await,
        1
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let (session, _) = game.register(None, tx).await;
    game.set_paused(false);
    let human = PlayerId(0);
    let ai = PlayerId(1);
    for (sequence, message) in [
        (1, DiplomacyMessage::RequestStance(DiplomaticStance::Ally)),
        (
            2,
            DiplomacyMessage::RequestTribute {
                resource: ResourceKind::Food,
                amount: 100,
            },
        ),
    ] {
        game.dispatch_client_message(
            session,
            GameplayClientMessage::DiplomacyMessageOrder {
                sequence,
                player: ai,
                message,
            },
        )
        .await;
        let mut answered = false;
        for _ in 0..160 {
            game.tick().await;
            while rx.try_recv().is_ok() {}
            if game.world.read().await.diplomacy_log(human).iter().any(|entry|
                entry.from == ai && matches!(entry.message, DiplomacyMessage::Reply { request, accepted: true } if request == 2 * sequence - 1)
            ) {
                answered = true;
                break;
            }
        }
        assert!(answered, "request {message:?} received no acceptance");
    }
    game.dispatch_client_message(
        session,
        GameplayClientMessage::DiplomacyOrder {
            sequence: 3,
            player: ai,
            stance: DiplomaticStance::Enemy,
        },
    )
    .await;
    for _ in 0..480 {
        game.tick().await;
        while rx.try_recv().is_ok() {}
        if game.world.read().await.surrendered_to(ai).is_some() {
            break;
        }
    }
    let world = game.world.read().await;
    assert_eq!(world.surrendered_to(ai), Some(human));
    assert_eq!(
        world.diplomacy_log(human).last().unwrap().message,
        DiplomacyMessage::Surrender
    );
    assert!(world.units().all(|unit| unit.hp == 0 || unit.player != ai));
    assert!(world.buildings().all(|building| building.player != ai));
}
