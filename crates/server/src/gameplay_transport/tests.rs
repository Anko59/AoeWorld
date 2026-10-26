use super::*;
use aoe_core::Seed;
use std::time::Instant;

async fn expire(service: &GameplayService, slot: usize, session_id: u64) {
    let expired_at = Instant::now()
        .checked_sub(LEASE_RESERVATION)
        .unwrap_or_else(Instant::now);
    let mut ownership = service.ownership.lock().await;
    ownership.slots[slot].lease = Some(ControllerLease {
        session_id,
        disconnected_at: Some(expired_at),
    });
}

#[tokio::test]
async fn expired_controller_promotes_the_oldest_spectator_and_can_fall_empty() {
    let service = GameplayService::new(Seed(1));
    let (old_tx, _old_rx) = mpsc::channel(8);
    let (old_id, _) = service.register(None, old_tx).await;
    let (next_tx, mut next_rx) = mpsc::channel(8);
    let (next_id, _) = service.register(None, next_tx).await;
    let old_token = service.ownership.lock().await.slots[0].token;

    service.sessions.lock().await.remove(&old_id);
    expire(&service, 0, old_id).await;
    let mut ownership = service.ownership.lock().await;
    let mut sessions = service.sessions.lock().await;
    promote_expired(&mut ownership, &mut sessions, service.world_id);
    assert_eq!(
        sessions.get(&next_id).map(|session| session.role),
        Some(GameplayRole::Controller)
    );
    assert_eq!(
        ownership.slots[0].lease.map(|lease| lease.session_id),
        Some(next_id)
    );
    assert_ne!(ownership.slots[0].token, old_token, "the old token is void");
    assert!(matches!(
        next_rx.try_recv().expect("role change"),
        GameplayServerMessage::RoleChange {
            role: GameplayRole::Controller,
            resume_token: Some(_),
            ..
        }
    ));

    let next_token = ownership.slots[0].token;
    ownership.slots[0].lease = Some(ControllerLease {
        session_id: next_id,
        disconnected_at: Some(
            Instant::now()
                .checked_sub(LEASE_RESERVATION)
                .unwrap_or_else(Instant::now),
        ),
    });
    sessions.clear();
    promote_expired(&mut ownership, &mut sessions, service.world_id);
    assert!(ownership.slots[0].lease.is_none());
    assert_eq!(
        ownership.slots[0].token, next_token,
        // The current requirement allows the same machine to resume this
        // world after the hold, provided nobody else took the human seat.
        "an unclaimed same-world seat keeps its returning owner's credential"
    );
}

#[test]
fn tokens_are_unpredictable_even_for_the_same_world_and_counter() {
    assert_ne!(make_token(42, 7), make_token(42, 7));
    assert_ne!(make_token(42, 7), make_token(42, 8));
}

#[tokio::test]
async fn a_retired_world_token_cannot_claim_or_be_promoted_in_an_unrelated_world() {
    let retired = GameplayService::new(Seed(1));
    let token = retired.slot_infos().await[0].token;
    let current = GameplayService::new(Seed(2));
    let (tx, _rx) = mpsc::channel(8);
    let (intruder, welcome) = current.register(Some(token), tx).await;
    assert!(
        matches!(
            welcome,
            GameplayServerMessage::Welcome {
                role: GameplayRole::Spectator,
                ..
            }
        ),
        "an unrelated token cannot claim a free human seat"
    );
    let (tx, _rx) = mpsc::channel(8);
    let (controller, _) = current.register(None, tx).await;
    current.disconnect(controller).await;
    expire(&current, 0, controller).await;
    current.tick().await;
    assert_eq!(
        current.sessions.lock().await[&intruder].role,
        GameplayRole::Spectator,
        "waiting must not authorize a retired controller for another world"
    );
}
