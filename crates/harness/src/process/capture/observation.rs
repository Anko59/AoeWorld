//! Conservative metadata for actual retained process tails, never root-cause inference.
use super::{CaptureExit, Captured};
use serde::Serialize;
use std::io;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SafeObservation {
    pub(crate) schema: u16,
    pub(crate) root_cause: RootCause,
    pub(crate) outcome: Outcome,
    pub(crate) stdout: StreamTail,
    pub(crate) stderr: StreamTail,
    pub(crate) truncated: bool,
    pub(crate) duration_ms: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum RootCause {
    #[serde(rename = "ROOT_CAUSE_NOT_ASSESSED")]
    NotAssessed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Outcome {
    Success,
    Failed {
        code: Option<i32>,
        termination: Termination,
    },
    Deadline,
    Cancelled,
    Start {
        io_kind: SafeErrorKind,
    },
    Monitor {
        io_kind: SafeErrorKind,
    },
}

impl Outcome {
    pub(crate) fn exit_label(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed { .. } => "FAILED",
            Self::Deadline => "DEADLINE",
            Self::Cancelled => "CANCELLED",
            Self::Start { .. } => "START_UNAVAILABLE",
            Self::Monitor { .. } => "MONITOR_UNAVAILABLE",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Termination {
    ExitCode,
    SignalOrUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum SafeErrorKind {
    NotFound,
    PermissionDenied,
    TimedOut,
    Interrupted,
    InvalidData,
    InvalidInput,
    Unsupported,
    WouldBlock,
    AlreadyExists,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    BrokenPipe,
    UnexpectedEof,
    AddrInUse,
    AddrNotAvailable,
    WriteZero,
    OutOfMemory,
    OtherUnknown,
}

impl From<io::ErrorKind> for SafeErrorKind {
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::TimedOut => Self::TimedOut,
            io::ErrorKind::Interrupted => Self::Interrupted,
            io::ErrorKind::InvalidData => Self::InvalidData,
            io::ErrorKind::InvalidInput => Self::InvalidInput,
            io::ErrorKind::Unsupported => Self::Unsupported,
            io::ErrorKind::WouldBlock => Self::WouldBlock,
            io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            io::ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            io::ErrorKind::ConnectionAborted => Self::ConnectionAborted,
            io::ErrorKind::NotConnected => Self::NotConnected,
            io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            io::ErrorKind::AddrInUse => Self::AddrInUse,
            io::ErrorKind::AddrNotAvailable => Self::AddrNotAvailable,
            io::ErrorKind::WriteZero => Self::WriteZero,
            io::ErrorKind::OutOfMemory => Self::OutOfMemory,
            _ => Self::OtherUnknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct StreamTail {
    pub(crate) bytes: usize,
    pub(crate) raw_blake3: String,
}

impl StreamTail {
    fn observed(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.len(),
            raw_blake3: blake3::hash(bytes).to_hex().to_string(),
        }
    }
}

pub(crate) fn safe_observation(captured: &Captured) -> SafeObservation {
    let outcome = match &captured.exit {
        CaptureExit::Success => Outcome::Success,
        CaptureExit::Failed(code) => Outcome::Failed {
            code: *code,
            termination: if code.is_some() {
                Termination::ExitCode
            } else {
                Termination::SignalOrUnknown
            },
        },
        CaptureExit::Deadline => Outcome::Deadline,
        CaptureExit::Cancelled => Outcome::Cancelled,
        CaptureExit::Start(error) => Outcome::Start {
            io_kind: error.kind().into(),
        },
        CaptureExit::Monitor(error) => Outcome::Monitor {
            io_kind: error.kind().into(),
        },
    };
    SafeObservation {
        schema: 1,
        root_cause: RootCause::NotAssessed,
        outcome,
        stdout: StreamTail::observed(&captured.stdout),
        stderr: StreamTail::observed(&captured.stderr),
        truncated: captured.truncated,
        duration_ms: captured.duration.as_millis(),
    }
}

#[cfg(test)]
mod tests;
