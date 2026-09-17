use aoe_core::Region;
use aoe_protocol::{ClientMessage, ServerMessage, VERSION, decode_server, encode_client};
use aoe_scenario::SMOKE;
use aoe_server::{AppState, Config, app};
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn setup() -> (SocketAddr, JoinHandle<()>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 20,
        },
        "test-build",
    );
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move {
        axum::serve(listener, app(state)).await.unwrap();
    });
    (address, server, ticker)
}

async fn receive(socket: &mut Socket) -> ServerMessage {
    let frame = timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = frame else {
        panic!("expected binary frame");
    };
    decode_server(&bytes).unwrap()
}

async fn send(socket: &mut Socket, message: ClientMessage) {
    let bytes = encode_client(&message).unwrap();
    socket.send(Message::Binary(bytes.into())).await.unwrap();
}

async fn open(address: SocketAddr) -> Socket {
    let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    send(&mut socket, ClientMessage::Hello { version: VERSION }).await;
    assert!(matches!(
        receive(&mut socket).await,
        ServerMessage::Hello {
            version: VERSION,
            ..
        }
    ));
    socket
}

#[tokio::test]
async fn two_regions_and_reconnect_have_fresh_snapshots() {
    let (address, server, ticker) = setup().await;
    let mut first = open(address).await;
    let mut second = open(address).await;
    send(
        &mut first,
        ClientMessage::Subscribe {
            region: Region {
                x: 0,
                y: 0,
                width: 128,
                height: 128,
            },
        },
    )
    .await;
    send(
        &mut second,
        ClientMessage::Subscribe {
            region: Region {
                x: 512,
                y: 512,
                width: 128,
                height: 128,
            },
        },
    )
    .await;
    let ServerMessage::Snapshot {
        tick: first_tick,
        region: first_region,
        entities: first_entities,
        ..
    } = receive(&mut first).await
    else {
        panic!("first snapshot");
    };
    let ServerMessage::Snapshot {
        region: second_region,
        entities: second_entities,
        ..
    } = receive(&mut second).await
    else {
        panic!("second snapshot");
    };
    assert_ne!(first_region, second_region);
    assert!(
        first_entities
            .iter()
            .all(|e| first_region.contains(e.position))
    );
    assert!(
        second_entities
            .iter()
            .all(|e| second_region.contains(e.position))
    );
    assert!(matches!(
        receive(&mut first).await,
        ServerMessage::Delta { .. }
    ));
    first.close(None).await.unwrap();
    let mut reconnected = open(address).await;
    send(
        &mut reconnected,
        ClientMessage::Subscribe {
            region: first_region,
        },
    )
    .await;
    let ServerMessage::Snapshot { tick, .. } = receive(&mut reconnected).await else {
        panic!("reconnect snapshot");
    };
    assert!(tick.0 >= first_tick.0);
    server.abort();
    ticker.abort();
}

#[tokio::test]
async fn stale_version_and_invalid_region_are_rejected() {
    let (address, server, ticker) = setup().await;
    let (mut stale, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    send(
        &mut stale,
        ClientMessage::Hello {
            version: VERSION + 1,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut stale).await,
        ServerMessage::Error { code: 426, .. }
    ));
    let mut invalid = open(address).await;
    send(
        &mut invalid,
        ClientMessage::Subscribe {
            region: Region {
                x: -1,
                y: 0,
                width: 1,
                height: 1,
            },
        },
    )
    .await;
    assert!(matches!(
        receive(&mut invalid).await,
        ServerMessage::Error { code: 400, .. }
    ));
    server.abort();
    ticker.abort();
}
