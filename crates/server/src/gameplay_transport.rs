use crate::gameplay::{Access, GameplayService};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, ResumeToken,
    decode_gameplay_client,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{fs::File, sync::OnceLock, time::Duration};
use tokio::{sync::mpsc, time::timeout};

mod degrade;
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
    Encode(String),
    #[error("socket write timed out")]
    Timeout,
    #[error("socket write failed")]
    Write,
}

/// Gives one socket operation [`SEND_TIMEOUT`] to finish.
async fn within_send_timeout<E>(
    operation: impl Future<Output = Result<(), E>>,
) -> Result<(), SendFailure> {
    timeout(SEND_TIMEOUT, operation)
        .await
        .map_err(|_| SendFailure::Timeout)?
        .map_err(|_| SendFailure::Write)
}

async fn send_socket<S>(sink: &mut S, message: GameplayServerMessage) -> Result<(), SendFailure>
where
    S: Sink<Message> + Unpin,
{
    let bytes = degrade::encode(message)?;
    within_send_timeout(sink.send(Message::Binary(bytes.into()))).await
}

/// Writes the session's queue to its socket until the queue closes or a world
/// reset has left. A tick queues several small frames for a client: the frames
/// already waiting are handed to the socket one after the other and flushed
/// once, so they leave in one write.
async fn write_messages<S>(
    sink: &mut S,
    receiver: &mut mpsc::Receiver<GameplayServerMessage>,
) -> Result<(), SendFailure>
where
    S: Sink<Message> + Unpin,
{
    while let Some(first) = receiver.recv().await {
        let mut queued = Some(first);
        let mut reset = false;
        while let Some(message) = queued {
            reset = matches!(message, GameplayServerMessage::WorldReset { .. });
            let bytes = degrade::encode(message)?;
            within_send_timeout(sink.feed(Message::Binary(bytes.into()))).await?;
            // The next frame leaves the bounded queue only now that the socket
            // took this one, so what a slow peer has not taken stays queued.
            // Nothing after a reset is taken.
            queued = if reset {
                None
            } else {
                receiver.try_recv().ok()
            };
        }
        within_send_timeout(sink.flush()).await?;
        if reset {
            break;
        }
    }
    Ok(())
}

/// Fills `bytes` from the operating system's random source. It stays open from
/// the first token on, which the server makes as it starts, so that running out
/// of file descriptors later cannot stop it.
fn fill_random(bytes: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    static SOURCE: OnceLock<File> = OnceLock::new();
    let mut source = match SOURCE.get() {
        Some(source) => source,
        None => {
            let opened = File::open("/dev/urandom")?;
            SOURCE.get_or_init(|| opened)
        }
    };
    source.read_exact(bytes)
}

/// An unpredictable 192-bit token. `world_id` and `counter` are no part of it:
/// they say which one could not be made. No guessable one is made instead, the
/// server ends.
pub(super) fn make_token(world_id: u64, counter: u64) -> ResumeToken {
    let mut bytes = [0_u8; 24];
    if let Err(error) = fill_random(&mut bytes) {
        tracing::error!(world_id, counter, %error, "no random source for tokens");
        std::process::exit(1);
    }
    ResumeToken(bytes)
}

/// Whether two secrets are equal. Every byte is compared, so the time taken
/// does not tell how much of a guess is right.
pub(super) fn same_secret(a: &[u8], b: &[u8]) -> bool {
    let difference = a.iter().zip(b).fold(0_u8, |all, (a, b)| all | (a ^ b));
    std::hint::black_box(difference) == 0 && a.len() == b.len()
}

/// `same_secret` for tokens.
pub(super) fn same_token(a: ResumeToken, b: ResumeToken) -> bool {
    same_secret(&a.0, &b.0)
}

#[path = "gameplay_transport/tests.rs"]
#[cfg(test)]
mod tests;
