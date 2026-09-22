use aoe_core::{EntityId, TileCoord, TileRect, WorldPosition};
use aoe_map::{MapRequest, Ratio};
use aoe_protocol::{
    CommandResult, GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage,
    ResumeToken, decode_gameplay_server, encode_gameplay_client,
};
use aoe_scenario::SMOKE;
use aoe_server::{AppState, Config, app};
use futures_util::{SinkExt, StreamExt};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
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
            asset_pack: None,
            map_package_directory: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "game-test",
    )
    .expect("state");
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move {
        axum::serve(listener, app(state)).await.unwrap();
    });
    (address, server, ticker)
}

async fn send(socket: &mut Socket, message: GameplayClientMessage) {
    socket
        .send(Message::Binary(
            encode_gameplay_client(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn receive(socket: &mut Socket) -> GameplayServerMessage {
    let frame = timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = frame else {
        panic!("expected binary gameplay frame")
    };
    decode_gameplay_server(&bytes).unwrap()
}

async fn open(address: SocketAddr, token: Option<ResumeToken>) -> (Socket, GameplayServerMessage) {
    let (mut socket, _) = connect_async(format!("ws://{address}/game/ws"))
        .await
        .unwrap();
    send(
        &mut socket,
        GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token: token,
        },
    )
    .await;
    let welcome = receive(&mut socket).await;
    (socket, welcome)
}

#[tokio::test]
async fn gameplay_protocol_seven_handshake_is_rejected_by_version_eight() {
    let (address, server, ticker) = setup().await;
    let (mut stale, _) = connect_async(format!("ws://{address}/game/ws"))
        .await
        .expect("connect gameplay websocket");
    send(
        &mut stale,
        GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION - 1,
            resume_token: None,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut stale).await,
        GameplayServerMessage::Error { code: 426, .. }
    ));
    server.abort();
    ticker.abort();
}

fn http_json(address: SocketAddr, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("connect HTTP");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("request");
    stream.flush().expect("flush");
    let mut result = String::new();
    stream.read_to_string(&mut result).expect("response");
    result
}

fn http_json_as_controller(
    address: SocketAddr,
    path: &str,
    body: &str,
    token: ResumeToken,
) -> String {
    let token = token
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut stream = TcpStream::connect(address).expect("connect HTTP");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-AoeWorld-Controller-Token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("request");
    stream.flush().expect("flush");
    let mut result = String::new();
    stream.read_to_string(&mut result).expect("response");
    result
}

#[tokio::test]
async fn controller_owns_orders_and_reconnects_with_a_resume_token() {
    let (address, server, ticker) = setup().await;
    let (mut controller, welcome) = open(address, None).await;
    let GameplayServerMessage::Welcome {
        role: GameplayRole::Controller,
        primary_unit_id,
        resume_token: Some(token),
        ..
    } = welcome
    else {
        panic!("first client must control")
    };
    let center = TileRect::new(TileCoord::new(8_000, 8_000), TileCoord::new(8_384, 8_384));
    send(
        &mut controller,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: center,
        },
    )
    .await;
    let GameplayServerMessage::Snapshot {
        revision: 1, units, ..
    } = receive(&mut controller).await
    else {
        panic!("controller snapshot")
    };
    assert!(units.iter().any(|unit| unit.id == primary_unit_id));

    let (mut spectator, welcome) = open(address, None).await;
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome {
            role: GameplayRole::Spectator,
            ..
        }
    ));
    send(
        &mut spectator,
        GameplayClientMessage::MoveOrder {
            sequence: 1,
            entity_id: primary_unit_id,
            destination: WorldPosition::new(8_388_608, 8_388_608),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut spectator).await,
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedNotController,
            ..
        }
    ));

    send(
        &mut controller,
        GameplayClientMessage::MoveOrder {
            sequence: 1,
            entity_id: primary_unit_id,
            destination: WorldPosition::new(8_389_632, 8_388_608),
        },
    )
    .await;
    let mut accepted = false;
    for _ in 0..4 {
        if matches!(
            receive(&mut controller).await,
            GameplayServerMessage::CommandAck {
                result: CommandResult::Accepted,
                ..
            }
        ) {
            accepted = true;
            break;
        }
    }
    assert!(accepted);
    send(
        &mut spectator,
        GameplayClientMessage::MoveOrder {
            sequence: 2,
            entity_id: primary_unit_id,
            destination: WorldPosition::new(-1, 0),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut spectator).await,
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedNotController,
            ..
        }
    ));
    send(
        &mut controller,
        GameplayClientMessage::MoveOrder {
            sequence: 2,
            entity_id: EntityId(999_999),
            destination: WorldPosition::new(8_389_632, 8_388_608),
        },
    )
    .await;
    let mut unknown_entity = false;
    for _ in 0..4 {
        if matches!(
            receive(&mut controller).await,
            GameplayServerMessage::CommandAck {
                sequence: 2,
                result: CommandResult::RejectedUnknownEntity,
                ..
            }
        ) {
            unknown_entity = true;
            break;
        }
    }
    assert!(unknown_entity);
    send(
        &mut controller,
        GameplayClientMessage::MoveOrder {
            sequence: 3,
            entity_id: primary_unit_id,
            destination: WorldPosition::new(-1, 0),
        },
    )
    .await;
    let mut invalid_destination = false;
    for _ in 0..4 {
        if matches!(
            receive(&mut controller).await,
            GameplayServerMessage::CommandAck {
                sequence: 3,
                result: CommandResult::RejectedInvalidDestination,
                ..
            }
        ) {
            invalid_destination = true;
            break;
        }
    }
    assert!(invalid_destination);
    drop(controller);
    let (_replacement, welcome) = open(address, Some(token)).await;
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome {
            role: GameplayRole::Controller,
            ..
        }
    ));
    server.abort();
    ticker.abort();
}

#[tokio::test]
async fn gameplay_rejects_obsolete_revisions_and_invalid_regions() {
    let (address, server, ticker) = setup().await;
    let (mut socket, _) = open(address, None).await;
    send(
        &mut socket,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: TileRect::new(TileCoord::new(0, 0), TileCoord::new(1, 1)),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut socket).await,
        GameplayServerMessage::Snapshot { revision: 1, .. }
    ));
    send(
        &mut socket,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: TileRect::new(TileCoord::new(0, 0), TileCoord::new(2, 2)),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut socket).await,
        GameplayServerMessage::Error { code: 409, .. }
    ));
    send(
        &mut socket,
        GameplayClientMessage::Subscribe {
            revision: 2,
            region: TileRect::new(TileCoord::new(0, 0), TileCoord::new(513, 1)),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut socket).await,
        GameplayServerMessage::Error { code: 400, .. }
    ));
    server.abort();
    ticker.abort();
}

#[tokio::test]
async fn map_activation_keeps_existing_gameplay_when_no_land_start_exists() {
    let (address, server, ticker) = setup().await;
    let (mut controller, welcome) = open(address, None).await;
    let GameplayServerMessage::Welcome {
        world_id: previous_world_id,
        resume_token: Some(token),
        ..
    } = welcome
    else {
        panic!("welcome");
    };
    send(
        &mut controller,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: TileRect::new(TileCoord::new(0, 0), TileCoord::new(32, 32)),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut controller).await,
        GameplayServerMessage::Snapshot { .. }
    ));
    let request = serde_json::to_string(&MapRequest {
        center_latitude_e7: 600_000_000,
        center_longitude_e7: 250_000_000,
        requested_side_meters: 256,
        compression: Ratio::new(2, 1).expect("ratio"),
        ..MapRequest::default()
    })
    .expect("request JSON");
    let unauthorized_request = request.clone();
    let response = tokio::task::spawn_blocking(move || {
        http_json(address, "/maps/activate", &unauthorized_request)
    })
    .await
    .expect("response");
    assert!(response.starts_with("HTTP/1.1 403"));
    let response = tokio::task::spawn_blocking(move || {
        http_json_as_controller(address, "/maps/activate", &request, token)
    })
    .await
    .expect("response");
    assert!(response.starts_with("HTTP/1.1 200"));
    let body = response.split_once("\r\n\r\n").expect("HTTP body").1;
    let activation = serde_json::from_str::<serde_json::Value>(body).expect("activation JSON");
    assert_eq!(activation["start_available"], false);
    assert_eq!(activation["message"], "no suitable land start.");
    let (_observer, welcome) = open(address, None).await;
    let GameplayServerMessage::Welcome {
        world_id,
        map_content_hash: None,
        map_metadata: None,
        ..
    } = welcome
    else {
        panic!("existing gameplay must remain active");
    };
    assert_eq!(world_id, previous_world_id);
    server.abort();
    ticker.abort();
}

#[allow(dead_code)]
fn _entity_id_is_wire_stable(id: EntityId) -> u32 {
    id.0
}
