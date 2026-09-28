use super::report::SourceQualificationError;
use crate::GameplayService;
use aoe_protocol::{
    GAMEPLAY_MAX_MESSAGE, GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage,
    ResumeToken, decode_gameplay_server, encode_gameplay_client,
};
use axum::{
    Router,
    extract::{
        State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use tokio::{sync::oneshot, task::JoinHandle};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HANDSHAKE_LINES: usize = 64;

pub(super) struct NetworkServer {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl NetworkServer {
    pub(super) async fn start(service: GameplayService) -> Result<Self, SourceQualificationError> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| lifecycle_error("network listener", error))?;
        let address = listener
            .local_addr()
            .map_err(|error| lifecycle_error("network listener address", error))?;
        let router = Router::new()
            .route("/game/ws", get(gameplay_websocket))
            .with_state(service);
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = shutdown_receiver.await;
                })
                .await
        });
        Ok(Self {
            address,
            shutdown: Some(shutdown),
            task: Some(task),
        })
    }

    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(super) async fn stop(mut self) -> Result<(), SourceQualificationError> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let task = self.task.take();
        let Some(task) = task else {
            return Err(lifecycle_mismatch("network server missing task"));
        };
        match task.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(lifecycle_error("network server", error)),
            Err(error) => Err(lifecycle_error("network server join", error)),
        }
    }
}

impl Drop for NetworkServer {
    fn drop(&mut self) {
        self.shutdown.take();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn gameplay_websocket(
    State(service): State<GameplayService>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade
        .max_message_size(GAMEPLAY_MAX_MESSAGE)
        .on_upgrade(move |socket: WebSocket| {
            crate::gameplay_transport::handle_socket(service, socket)
        })
}

pub(super) struct NetworkClient {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl NetworkClient {
    pub(super) fn open(
        address: SocketAddr,
        resume_token: Option<ResumeToken>,
    ) -> Result<(Self, GameplayServerMessage), SourceQualificationError> {
        let stream = TcpStream::connect_timeout(&address, NETWORK_TIMEOUT)
            .map_err(|error| lifecycle_error("network connect", error))?;
        stream
            .set_read_timeout(Some(NETWORK_TIMEOUT))
            .map_err(|error| lifecycle_error("network read timeout", error))?;
        stream
            .set_write_timeout(Some(NETWORK_TIMEOUT))
            .map_err(|error| lifecycle_error("network write timeout", error))?;
        let writer = stream
            .try_clone()
            .map_err(|error| lifecycle_error("network stream clone", error))?;
        let mut reader = BufReader::new(stream);
        let request = concat!(
            "GET /game/ws HTTP/1.1\r\n",
            "Host: 127.0.0.1\r\n",
            "Upgrade: websocket\r\n",
            "Connection: Upgrade\r\n",
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n",
            "Sec-WebSocket-Version: 13\r\n",
            "\r\n"
        );
        let mut handshake_writer = writer
            .try_clone()
            .map_err(|error| lifecycle_error("network handshake writer", error))?;
        handshake_writer
            .write_all(request.as_bytes())
            .map_err(|error| lifecycle_error("network handshake write", error))?;
        read_handshake(&mut reader)?;
        drop(handshake_writer);
        let mut client = Self { reader, writer };
        client.send(&GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token,
        })?;
        let welcome = client
            .next()?
            .ok_or_else(|| lifecycle_mismatch("network welcome closed before registration"))?;
        Ok((client, welcome))
    }

    pub(super) fn send(
        &mut self,
        message: &GameplayClientMessage,
    ) -> Result<(), SourceQualificationError> {
        let payload = encode_gameplay_client(message)
            .map_err(|_| lifecycle_mismatch("network client message encoding"))?;
        self.write_frame(0x2, &payload)
    }

    pub(super) fn next(
        &mut self,
    ) -> Result<Option<GameplayServerMessage>, SourceQualificationError> {
        loop {
            let (opcode, payload) = self.read_frame()?;
            match opcode {
                0x1 | 0x2 => {
                    return decode_gameplay_server(&payload)
                        .map(Some)
                        .map_err(|_| lifecycle_mismatch("network server message decoding"));
                }
                0x8 => return Ok(None),
                0x9 => self.write_frame(0xA, &payload)?,
                0xA => {}
                _ => return Err(lifecycle_mismatch("network frame opcode")),
            }
        }
    }

    fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), SourceQualificationError> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        let mask_bit = 0x80;
        if payload.len() <= u8::MAX as usize {
            frame.push(mask_bit | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(mask_bit | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(mask_bit | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        let mask = [0x37, 0x4f, 0x9a, 0x21];
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % mask.len()]),
        );
        self.writer
            .write_all(&frame)
            .and_then(|()| self.writer.flush())
            .map_err(|error| lifecycle_error("network frame write", error))
    }

    fn read_frame(&mut self) -> Result<(u8, Vec<u8>), SourceQualificationError> {
        let mut header = [0_u8; 2];
        self.reader
            .read_exact(&mut header)
            .map_err(|error| lifecycle_error("network frame header", error))?;
        if header[0] & 0x70 != 0 {
            return Err(lifecycle_mismatch("network frame extensions"));
        }
        if header[0] & 0x80 == 0 {
            return Err(lifecycle_mismatch("network fragmented frame"));
        }
        let opcode = header[0] & 0x0f;
        if header[1] & 0x80 != 0 {
            return Err(lifecycle_mismatch("masked server frame"));
        }
        let short_length = usize::from(header[1] & 0x7f);
        let length = match short_length {
            126 => {
                let mut bytes = [0_u8; 2];
                self.read_exact(&mut bytes, "network frame length")?;
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0_u8; 8];
                self.read_exact(&mut bytes, "network frame length")?;
                usize::try_from(u64::from_be_bytes(bytes))
                    .map_err(|_| lifecycle_mismatch("network frame length"))?
            }
            value => value,
        };
        if length > GAMEPLAY_MAX_MESSAGE {
            return Err(lifecycle_mismatch("network frame message bound"));
        }
        let mut payload = vec![0_u8; length];
        self.read_exact(&mut payload, "network frame payload")?;
        Ok((opcode, payload))
    }

    fn read_exact(
        &mut self,
        bytes: &mut [u8],
        stage: &'static str,
    ) -> Result<(), SourceQualificationError> {
        self.reader
            .read_exact(bytes)
            .map_err(|error| lifecycle_error(stage, error))
    }
}

fn read_handshake(reader: &mut BufReader<TcpStream>) -> Result<(), SourceQualificationError> {
    let mut status = String::new();
    reader
        .read_line(&mut status)
        .map_err(|error| lifecycle_error("network handshake status", error))?;
    if !status.starts_with("HTTP/1.1 101 ") {
        return Err(lifecycle_mismatch("network handshake status"));
    }
    let mut saw_upgrade = false;
    let mut saw_connection = false;
    for _ in 0..MAX_HANDSHAKE_LINES {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| lifecycle_error("network handshake header", error))?;
        if bytes == 0 {
            return Err(lifecycle_mismatch("network handshake ended"));
        }
        if line == "\r\n" || line == "\n" {
            if saw_upgrade && saw_connection {
                return Ok(());
            }
            return Err(lifecycle_mismatch("network handshake headers"));
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_ascii_lowercase();
            saw_upgrade |= name == "upgrade" && value == "websocket";
            saw_connection |= name == "connection" && value.contains("upgrade");
        }
    }
    Err(lifecycle_mismatch("network handshake header bound"))
}

fn lifecycle_error(stage: &'static str, error: impl std::fmt::Display) -> SourceQualificationError {
    let _ = error;
    SourceQualificationError::Lifecycle { stage }
}

fn lifecycle_mismatch(stage: &'static str) -> SourceQualificationError {
    SourceQualificationError::Lifecycle { stage }
}
