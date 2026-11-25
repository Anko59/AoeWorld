//! Socket reads stay live independently of backpressured writes.
use super::*;
use futures_util::stream::SplitStream;

pub(super) async fn read_requests(
    service: &GameplayService,
    session_id: u64,
    stream: &mut SplitStream<WebSocket>,
) -> Option<GameplayServerMessage> {
    while let Some(Ok(frame)) = stream.next().await {
        let bytes = match frame {
            Message::Binary(bytes) => bytes,
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => break,
            _ => {
                return Some(GameplayServerMessage::Error {
                    code: 400,
                    message: "expected a binary gameplay request".to_owned(),
                });
            }
        };
        let refused = match decode_gameplay_client(&bytes) {
            Ok(message) => service.dispatch_client_message(session_id, message).await,
            Err(_) => Some(GameplayServerMessage::Error {
                code: 400,
                message: "invalid gameplay request".to_owned(),
            }),
        };
        if refused.is_some() {
            return refused;
        }
    }
    None
}
