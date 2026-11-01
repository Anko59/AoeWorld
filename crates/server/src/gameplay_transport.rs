use crate::gameplay::{Access, GameplayService};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, ResumeToken,
    decode_gameplay_client, encode_gameplay_server,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::time::Duration;
use tokio::{sync::mpsc, time::timeout};

mod requests;

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
