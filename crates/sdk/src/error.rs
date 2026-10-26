//! One error type for connecting, joining and talking to a server.
use aoe_protocol::GameplayError;

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("invalid server address: {0}")]
    InvalidUrl(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("websocket error: {0}")]
    WebSocket(String),
    #[error("wire error: {0}")]
    Wire(#[from] GameplayError),
    #[error("server refused with code {code}: {message}")]
    Refused { code: u16, message: String },
    #[error("http status {status}: {body}")]
    Http { status: u16, body: String },
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the connection is closed: {0}")]
    Closed(String),
    #[error("timed out")]
    Timeout,
    #[error("invalid slot token (expected 48 hex digits)")]
    InvalidToken,
}

pub type Result<T> = std::result::Result<T, SdkError>;
