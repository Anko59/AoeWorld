use super::region;
use aoe_core::{TileCoord, TileRect};
use aoe_protocol::{
    ClientMessage, GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, ServerMessage,
    VERSION, decode_gameplay_server, decode_server, encode_client, encode_gameplay_client,
};
use aoe_scenario::Scenario;
use aoe_server::{AppState, Config, app};
use futures_util::{SinkExt, StreamExt, future::join_all};
use std::{error::Error, net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Default)]
pub(crate) struct ClientResult {
    pub(crate) snapshots: u16,
    pub(crate) deltas: u64,
    pub(crate) replicated: u64,
    pub(crate) bytes: u64,
    pub(crate) visible: usize,
}

pub(crate) fn initial_client_result(handshake_bytes: usize) -> ClientResult {
    ClientResult {
        bytes: handshake_bytes as u64,
        ..Default::default()
    }
}

pub(crate) async fn receive(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<(ServerMessage, usize), Box<dyn Error + Send + Sync>> {
    let frame = timeout(Duration::from_secs(20), socket.next())
        .await?
        .ok_or("connection closed")??;
    let Message::Binary(bytes) = frame else {
        return Err("nonbinary protocol frame".into());
    };
    Ok((decode_server(&bytes)?, bytes.len()))
}

pub(crate) async fn send(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    message: ClientMessage,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    socket
        .send(Message::Binary(encode_client(&message)?.into()))
        .await?;
    Ok(())
}

async fn client(
    address: SocketAddr,
    scenario: Scenario,
    index: u16,
) -> Result<ClientResult, Box<dyn Error + Send + Sync>> {
    let (mut socket, _) = timeout(
        Duration::from_secs(20),
        connect_async(format!("ws://{address}/ws")),
    )
    .await??;
    send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
    let (hello, size) = receive(&mut socket).await?;
    if !matches!(
        hello,
        ServerMessage::Hello {
            version: VERSION,
            ..
        }
    ) {
        return Err("handshake mismatch".into());
    }
    let mut result = initial_client_result(size);
    send(
        &mut socket,
        ClientMessage::Subscribe {
            region: region(scenario, index),
        },
    )
    .await?;
    let (snapshot, size) = receive(&mut socket).await?;
    let ServerMessage::Snapshot {
        total_entities,
        entities,
        ..
    } = snapshot
    else {
        return Err("snapshot missing".into());
    };
    if total_entities != scenario.entities {
        return Err("server population mismatch".into());
    }
    result.snapshots = 1;
    result.replicated = entities.len() as u64;
    result.visible = entities.len();
    result.bytes += size as u64;
    for _ in 0..2 {
        let (message, size) = receive(&mut socket).await?;
        if !matches!(message, ServerMessage::Delta { .. }) {
            return Err("delta missing".into());
        }
        result.deltas += 1;
        result.bytes += size as u64;
    }
    Ok(result)
}

async fn receive_gameplay(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<(GameplayServerMessage, usize), Box<dyn Error + Send + Sync>> {
    let frame = timeout(Duration::from_secs(20), socket.next())
        .await?
        .ok_or("connection closed")??;
    let Message::Binary(bytes) = frame else {
        return Err("nonbinary gameplay protocol frame".into());
    };
    Ok((decode_gameplay_server(&bytes)?, bytes.len()))
}

async fn send_gameplay(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    message: GameplayClientMessage,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    socket
        .send(Message::Binary(encode_gameplay_client(&message)?.into()))
        .await?;
    Ok(())
}

async fn gameplay_client(
    address: SocketAddr,
    scenario: Scenario,
    index: u16,
) -> Result<ClientResult, Box<dyn Error + Send + Sync>> {
    let (mut socket, _) = timeout(
        Duration::from_secs(20),
        connect_async(format!("ws://{address}/game/ws")),
    )
    .await??;
    send_gameplay(
        &mut socket,
        GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token: None,
        },
    )
    .await?;
    let (welcome, size) = receive_gameplay(&mut socket).await?;
    if !matches!(welcome, GameplayServerMessage::Welcome { .. }) {
        return Err("gameplay welcome missing".into());
    }
    let mut result = initial_client_result(size);
    let requested = region(scenario, index);
    let tile_region = TileRect::new(
        TileCoord::new(requested.x, requested.y),
        TileCoord::new(
            requested.x + i32::from(requested.width),
            requested.y + i32::from(requested.height),
        ),
    );
    send_gameplay(
        &mut socket,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: tile_region,
        },
    )
    .await?;
    let (snapshot, size) = receive_gameplay(&mut socket).await?;
    let GameplayServerMessage::Snapshot { units, .. } = snapshot else {
        return Err("gameplay snapshot missing".into());
    };
    result.snapshots = 1;
    result.replicated = units.len() as u64;
    result.visible = units.len();
    result.bytes += size as u64;
    for _ in 0..2 {
        let (message, size) = receive_gameplay(&mut socket).await?;
        if !matches!(message, GameplayServerMessage::Tick { .. }) {
            return Err("gameplay tick missing".into());
        }
        result.deltas += 1;
        result.bytes += size as u64;
    }
    Ok(result)
}

fn combine(
    results: Vec<Result<ClientResult, Box<dyn Error + Send + Sync>>>,
) -> Result<ClientResult, Box<dyn Error>> {
    let mut combined = ClientResult::default();
    for item in results {
        let item = item.map_err(|error| error.to_string())?;
        combined.snapshots += item.snapshots;
        combined.deltas += item.deltas;
        combined.replicated += item.replicated;
        combined.bytes += item.bytes;
        combined.visible = combined.visible.max(item.visible);
    }
    Ok(combined)
}

pub(crate) async fn network(scenario: Scenario) -> Result<ClientResult, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let config = Config {
        bind: address,
        scenario,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
    };
    let state = AppState::new(&config, "perf-local")?;
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });
    let clients =
        join_all((0..scenario.players).map(|index| client(address, scenario, index))).await;
    ticker.abort();
    server.abort();
    combine(clients)
}

pub(crate) async fn gameplay_network(scenario: Scenario) -> Result<ClientResult, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let config = Config {
        bind: address,
        scenario,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
    };
    let state = AppState::with_gameplay_population(&config, "perf-gameplay")?;
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });
    let clients =
        join_all((0..scenario.players).map(|index| gameplay_client(address, scenario, index)))
            .await;
    ticker.abort();
    server.abort();
    combine(clients)
}
