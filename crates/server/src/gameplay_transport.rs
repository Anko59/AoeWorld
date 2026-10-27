use crate::gameplay::{Access, ControllerLease, GameplayService, Ownership, Session};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage, ResumeToken,
    decode_gameplay_client, encode_gameplay_server,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::mpsc, time::timeout};

mod requests;

const LEASE_RESERVATION: Duration = Duration::from_secs(30);
/// How long one socket write may take before the peer counts as gone: long
/// enough for a bot busy deciding, short enough to free a dead connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn handle_socket(service: GameplayService, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let hello = timeout(Duration::from_secs(5), stream.next()).await;
    let Some(Ok(Message::Binary(bytes))) = hello.ok().flatten() else {
        return;
    };
    let access = match decode_gameplay_client(&bytes) {
        Ok(GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token,
        }) => Access::Hello(resume_token),
        Ok(GameplayClientMessage::JoinSlot {
            version: GAMEPLAY_VERSION,
            token,
        }) => Access::Join(token),
        _ => {
            let _ = send_socket(
                &mut sink,
                GameplayServerMessage::Error {
                    code: 426,
                    message: "gameplay protocol version or hello mismatch".to_owned(),
                },
            )
            .await;
            return;
        }
    };
    let (sender, mut receiver) = mpsc::channel(64);
    let Ok((session_id, welcome)) = service.register_access(access, sender.clone()).await else {
        let _ = send_socket(
            &mut sink,
            GameplayServerMessage::Error {
                code: 403,
                message: "unknown slot token".to_owned(),
            },
        )
        .await;
        return;
    };
    if sender.send(welcome).await.is_err() {
        return;
    }
    drop(sender);
    // These futures run together for the entire connection. Waiting for a
    // command's world lock must not suspend the writer, and a slow write must
    // not suspend incoming requests, Ping/Pong or the peer's close frame.
    let failure = {
        tokio::select! {
            message = requests::read_requests(&service, session_id, &mut stream) => message,
            outcome = write_messages(&mut sink, &mut receiver) => {
                match outcome {
                    Ok(()) => None,
                    Err(error) => {
                        tracing::warn!(session_id, %error, "gameplay session delivery failed");
                        Some(GameplayServerMessage::Error {code:500, message:format!("The server could not send a game update: {error}. Reload to reconnect.")})
                    }
                }
            }
        }
    };
    if let Some(message) = failure {
        let _ = send_socket(&mut sink, message).await;
        let _ = timeout(
            SEND_TIMEOUT,
            sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1011,
                reason: "Server game update failed; reload to reconnect".into(),
            }))),
        )
        .await;
    }
    service.disconnect(session_id).await;
}

#[derive(Debug, thiserror::Error)]
enum SendFailure {
    #[error("invalid outgoing game state: {0}")]
    Encode(aoe_protocol::GameplayError),
    #[error("socket write timed out")]
    Timeout,
    #[error("socket write failed")]
    Write,
}

async fn send_socket<S>(sink: &mut S, message: GameplayServerMessage) -> Result<(), SendFailure>
where
    S: Sink<Message> + Unpin,
{
    let bytes = encode_gameplay_server(&message).map_err(SendFailure::Encode)?;
    timeout(SEND_TIMEOUT, sink.send(Message::Binary(bytes.into())))
        .await
        .map_err(|_| SendFailure::Timeout)?
        .map_err(|_| SendFailure::Write)
}

async fn write_messages<S>(
    sink: &mut S,
    receiver: &mut mpsc::Receiver<GameplayServerMessage>,
) -> Result<(), SendFailure>
where
    S: Sink<Message> + Unpin,
{
    while let Some(message) = receiver.recv().await {
        let reset = matches!(message, GameplayServerMessage::WorldReset { .. });
        send_socket(sink, message).await?;
        if reset {
            break;
        }
    }
    Ok(())
}

/// Gives every human slot whose controller stayed away past the lease to the
/// oldest eligible spectator, under a fresh token. Without a successor, the
/// same-world credential remains valid for a new tab returning to its seat.
/// AI slots are never reassigned: only their token reaches them.
pub(super) fn promote_expired(
    ownership: &mut Ownership,
    sessions: &mut BTreeMap<u64, Session>,
    world_id: u64,
) {
    for index in 0..ownership.slots.len() {
        let slot = &ownership.slots[index];
        let expired = slot.human
            && slot.lease.is_some_and(|lease| {
                lease
                    .disconnected_at
                    .is_some_and(|at| at.elapsed() >= LEASE_RESERVATION)
            });
        if !expired {
            continue;
        }
        let next = sessions
            .iter()
            .filter(|(_, session)| {
                session.role == GameplayRole::Spectator && session.may_claim_human_seat
            })
            .min_by_key(|(_, session)| session.connected_at)
            .map(|(id, _)| *id);
        let Some(session_id) = next else {
            // No successor took the seat. A returning tab may resume this
            // exact world/slot; tokens never match an unrelated world's slots.
            ownership.slots[index].lease = None;
            continue;
        };
        let token = make_token(world_id, ownership.next_token);
        ownership.next_token = ownership.next_token.wrapping_add(1);
        let slot = &mut ownership.slots[index];
        slot.token = token;
        slot.lease = Some(ControllerLease {
            session_id,
            disconnected_at: None,
        });
        if let Some(session) = sessions.get_mut(&session_id) {
            session.role = GameplayRole::Controller;
            session.token = Some(token);
            session.slot = Some(index);
            session.ai = false;
            if session.viewer != slot.player {
                session.change_viewer(slot.player);
            }
            session.primary_unit_id = slot.primary_unit_id;
            let _ = session.sender.try_send(GameplayServerMessage::RoleChange {
                role: GameplayRole::Controller,
                primary_unit_id: slot.primary_unit_id,
                resume_token: Some(token),
            });
        }
    }
}

/// An unpredictable 192-bit token. `world_id` and `counter` only diversify the
/// hasher input; the secrecy comes from the per-hasher random keys.
pub(super) fn make_token(world_id: u64, counter: u64) -> ResumeToken {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0_u8; 24];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(world_id);
        hasher.write_u64(counter);
        hasher.write_u64(index as u64);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    ResumeToken(bytes)
}

#[path = "gameplay_transport/tests.rs"]
#[cfg(test)]
mod tests;
