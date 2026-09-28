use crate::gameplay::{ControllerLease, GameplayService, Ownership, Session};
use aoe_core::EntityId;
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage, ResumeToken,
    decode_gameplay_client, encode_gameplay_server,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::mpsc, time::timeout};

const LEASE_RESERVATION: Duration = Duration::from_secs(30);
const SEND_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) async fn handle_socket(service: GameplayService, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let hello = timeout(Duration::from_secs(5), stream.next()).await;
    let Some(Ok(Message::Binary(bytes))) = hello.ok().flatten() else {
        return;
    };
    let Ok(GameplayClientMessage::Hello {
        version: GAMEPLAY_VERSION,
        resume_token,
    }) = decode_gameplay_client(&bytes)
    else {
        let _ = send_socket(
            &mut sink,
            GameplayServerMessage::Error {
                code: 426,
                message: "gameplay protocol version or hello mismatch".to_owned(),
            },
        )
        .await;
        return;
    };
    let (sender, mut receiver) = mpsc::channel(64);
    let (session_id, welcome) = service.register(resume_token, sender.clone()).await;
    if sender.send(welcome).await.is_err() {
        return;
    }
    drop(sender);
    loop {
        tokio::select! {
            outgoing = receiver.recv() => {
                let Some(message) = outgoing else { break };
                let reset = matches!(message, GameplayServerMessage::WorldReset { .. });
                if send_socket(&mut sink, message).await.is_err() || reset { break }
            }
            incoming = stream.next() => {
                let Some(Ok(Message::Binary(bytes))) = incoming else { break };
                match decode_gameplay_client(&bytes) {
                    Ok(GameplayClientMessage::Subscribe { revision, region }) => service.subscribe(session_id, revision, region).await,
                    Ok(GameplayClientMessage::MoveOrder { sequence, entity_id, destination }) => service.queue_move(session_id, sequence, entity_id, destination).await,
                    Ok(GameplayClientMessage::Resync { revision }) => service.resync(session_id, revision).await,
                    Ok(GameplayClientMessage::Hello { .. }) | Err(_) => {
                        let _ = send_socket(&mut sink, GameplayServerMessage::Error { code: 400, message: "invalid gameplay request".to_owned() }).await;
                        break;
                    }
                }
            }
        }
    }
    service.disconnect(session_id).await;
}

async fn send_socket<S>(sink: &mut S, message: GameplayServerMessage) -> Result<(), ()>
where
    S: Sink<Message> + Unpin,
{
    let bytes = encode_gameplay_server(&message).map_err(|_| ())?;
    timeout(SEND_TIMEOUT, sink.send(Message::Binary(bytes.into())))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

pub(super) fn promote_expired(
    ownership: &mut Ownership,
    sessions: &mut BTreeMap<u64, Session>,
    world_id: u64,
    primary_unit_id: EntityId,
) {
    let expired = ownership.controller.is_some_and(|lease| {
        lease
            .disconnected_at
            .is_some_and(|at| at.elapsed() >= LEASE_RESERVATION)
    });
    if !expired {
        return;
    }
    let next = sessions
        .iter()
        .filter(|(_, session)| session.role == GameplayRole::Spectator)
        .min_by_key(|(_, session)| session.connected_at)
        .map(|(id, _)| *id);
    if let Some(session_id) = next {
        let token = make_token(world_id, ownership.next_token);
        ownership.next_token = ownership.next_token.wrapping_add(1);
        if let Some(session) = sessions.get_mut(&session_id) {
            session.role = GameplayRole::Controller;
            session.token = Some(token);
            let _ = session.sender.try_send(GameplayServerMessage::RoleChange {
                role: GameplayRole::Controller,
                primary_unit_id,
                resume_token: Some(token),
            });
        }
        ownership.controller = Some(ControllerLease {
            session_id,
            token,
            disconnected_at: None,
        });
    } else {
        ownership.controller = None;
    }
}

pub(super) fn make_token(world_id: u64, counter: u64) -> ResumeToken {
    let mut state = world_id ^ counter.rotate_left(17) ^ 0x9e3779b97f4a7c15;
    let mut bytes = [0_u8; 24];
    for byte in &mut bytes {
        state ^= state << 7;
        state ^= state >> 9;
        state ^= state << 8;
        *byte = state as u8;
    }
    ResumeToken(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_core::Seed;
    use std::time::Instant;

    #[tokio::test]
    async fn expired_controller_promotes_the_oldest_spectator_and_can_fall_empty() {
        let service = GameplayService::new(Seed(1));
        let (old_tx, _old_rx) = mpsc::channel(8);
        let (old_id, _) = service.register(None, old_tx).await;
        let (next_tx, mut next_rx) = mpsc::channel(8);
        let (next_id, _) = service.register(None, next_tx).await;

        let mut ownership = service.ownership.lock().await;
        let mut sessions = service.sessions.lock().await;
        sessions.remove(&old_id);
        let expired_at = Instant::now()
            .checked_sub(LEASE_RESERVATION)
            .unwrap_or_else(Instant::now);
        ownership.controller = Some(ControllerLease {
            session_id: old_id,
            token: make_token(service.world_id, ownership.next_token),
            disconnected_at: Some(expired_at),
        });

        promote_expired(&mut ownership, &mut sessions, service.world_id, EntityId(7));
        assert_eq!(
            sessions.get(&next_id).map(|session| session.role),
            Some(GameplayRole::Controller)
        );
        assert_eq!(
            ownership.controller.as_ref().map(|lease| lease.session_id),
            Some(next_id)
        );
        assert!(matches!(
            next_rx.try_recv().expect("role change"),
            GameplayServerMessage::RoleChange {
                role: GameplayRole::Controller,
                primary_unit_id: EntityId(7),
                resume_token: Some(_),
            }
        ));

        ownership.controller = Some(ControllerLease {
            session_id: next_id,
            token: make_token(service.world_id, ownership.next_token),
            disconnected_at: Some(expired_at),
        });
        sessions.clear();
        promote_expired(&mut ownership, &mut sessions, service.world_id, EntityId(7));
        assert!(ownership.controller.is_none());
    }

    #[test]
    fn token_generation_is_deterministic_for_world_and_counter() {
        assert_eq!(make_token(42, 7), make_token(42, 7));
        assert_ne!(make_token(42, 7), make_token(42, 8));
        assert_ne!(make_token(42, 7), make_token(43, 7));
    }
}
