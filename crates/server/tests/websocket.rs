use aoe_core::Region;
use aoe_map::MapRequest;
use aoe_protocol::{ClientMessage, ServerMessage, VERSION, decode_server, encode_client};
use aoe_scenario::{BEYOND_TARGET, SMOKE, Scenario};
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

async fn setup_scenario(scenario: Scenario) -> (SocketAddr, JoinHandle<()>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "test-build",
    )
    .expect("state");
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move {
        axum::serve(listener, app(state)).await.unwrap();
    });
    (address, server, ticker)
}

async fn setup() -> (SocketAddr, JoinHandle<()>, JoinHandle<()>) {
    setup_scenario(SMOKE).await
}

async fn setup_with_fallback_map() -> (
    SocketAddr,
    JoinHandle<()>,
    JoinHandle<()>,
    tempfile::TempDir,
    String,
) {
    let directory = tempfile::tempdir().expect("map directory");
    let package = aoe_map::MapPackage::new(
        aoe_map::MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
    )
    .expect("fallback package");
    let hash = package.content_hash_hex();
    std::fs::write(
        directory.path().join(format!("{hash}.json")),
        serde_json::to_vec(&package).expect("package JSON"),
    )
    .expect("package file");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let config = Config {
        bind: address,
        scenario: SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.path().to_owned()),
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = AppState::new(&config, "test-build").expect("state");
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move {
        axum::serve(listener, app(state)).await.expect("server");
    });
    (address, server, ticker, directory, hash)
}

#[tokio::test]
async fn selected_local_pack_is_served_only_when_configured() {
    let pack = tempfile::tempdir().expect("pack directory");
    std::fs::write(pack.path().join("manifest.json"), b"local-pack-test")
        .expect("manifest fixture");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let config = Config {
        bind: address,
        scenario: SMOKE,
        tick_hz: 20,
        asset_pack: Some(pack.path().to_owned()),
        map_package_directory: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app(AppState::new(&config, "test").expect("state")),
        )
        .await
        .expect("server");
    });
    let response =
        tokio::task::spawn_blocking(move || http(address, "GET", "/asset-pack/manifest.json"))
            .await
            .expect("response");
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("local-pack-test"));
    server.abort();

    let (address, server, ticker) = setup().await;
    let response =
        tokio::task::spawn_blocking(move || http(address, "GET", "/asset-pack/manifest.json"))
            .await
            .expect("response");
    assert!(response.starts_with("HTTP/1.1 404"));
    server.abort();
    ticker.abort();
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

fn http(address: SocketAddr, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("connect HTTP");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").expect("request");
    stream.flush().expect("flush");
    let mut result = String::new();
    stream.read_to_string(&mut result).expect("response");
    result
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

#[tokio::test]
async fn map_estimates_are_validated_without_creating_a_world() {
    let (address, server, ticker) = setup().await;
    let request = serde_json::to_string(&MapRequest::default()).expect("request JSON");
    let estimate_request = request.clone();
    let response = tokio::task::spawn_blocking(move || {
        http_json(address, "/maps/estimate", &estimate_request)
    })
    .await
    .expect("response");
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("\"tiles_per_side\":500"));
    let response =
        tokio::task::spawn_blocking(move || http_json(address, "/maps/activate", &request))
            .await
            .expect("response");
    assert!(response.starts_with("HTTP/1.1 403"));
    let response = tokio::task::spawn_blocking(move || {
        http_json(
            address,
            "/maps/estimate",
            "{\"schema_version\":1,\"center_latitude_e7\":0,\"center_longitude_e7\":0,\"requested_side_meters\":1,\"compression\":{\"numerator\":1,\"denominator\":1},\"year_ce\":600,\"seed\":1}",
        )
    })
    .await
    .expect("response");
    assert!(response.starts_with("HTTP/1.1 400"));
    server.abort();
    ticker.abort();
}

#[tokio::test]
async fn saved_map_preview_is_read_only_and_bounded() {
    let (address, server, ticker, _directory, hash) = setup_with_fallback_map().await;
    let response =
        tokio::task::spawn_blocking(move || http(address, "GET", &format!("/maps/{hash}/preview")))
            .await
            .expect("response");
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("\"samples_per_axis\":16"));
    assert!(response.contains("\"source_backed\":false"));
    assert!(response.contains("\"cells\":"));
    server.abort();
    ticker.abort();
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

#[tokio::test]
async fn beyond_target_hotspot_reports_explicit_protocol_overload() {
    let (address, server, ticker) = setup_scenario(BEYOND_TARGET).await;
    let mut client = open(address).await;
    send(
        &mut client,
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
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Error { code: 413, message }
            if message.contains("protocol entity limit")
    ));
    server.abort();
    ticker.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_health_replay_and_scenario_validation_use_authoritative_state() {
    let (address, server, ticker) = setup().await;
    let health = http(address, "GET", "/health");
    assert!(health.starts_with("HTTP/1.1 200"));
    assert!(health.contains("\"build\":\"test-build\""));
    assert!(health.contains("\"entities\":8000"));
    assert!(health.contains("\"tick_deadline_misses\":"));
    assert!(health.contains("\"terrain_cache\":{\"entries\":0"));

    let replay = http(address, "GET", "/replay-hash?scenario=smoke&ticks=2");
    assert!(replay.starts_with("HTTP/1.1 200"));
    assert!(replay.contains("\"ticks\":2"));
    assert!(replay.contains("\"hash\":\""));
    assert!(
        http(address, "GET", "/replay-hash?scenario=missing&ticks=1").starts_with("HTTP/1.1 400")
    );
    assert!(
        http(address, "GET", "/replay-hash?scenario=smoke&ticks=65").starts_with("HTTP/1.1 400")
    );
    assert!(http(address, "POST", "/scenario/missing").starts_with("HTTP/1.1 400"));
    assert!(http(address, "POST", "/scenario/smoke").starts_with("HTTP/1.1 200"));
    server.abort();
    ticker.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_handshakes_resync_and_scenario_change_are_visible() {
    let (address, server, ticker) = setup().await;
    let (mut malformed, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect");
    malformed
        .send(Message::Binary(vec![255].into()))
        .await
        .expect("send bad hello");
    assert!(matches!(
        receive(&mut malformed).await,
        ServerMessage::Error { code: 400, .. }
    ));

    let mut client = open(address).await;
    send(&mut client, ClientMessage::Resync).await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Error { code: 400, .. }
    ));
    send(
        &mut client,
        ClientMessage::Subscribe {
            region: Region {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            },
        },
    )
    .await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Snapshot { .. }
    ));
    send(&mut client, ClientMessage::Resync).await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Snapshot { .. }
    ));

    assert!(http(address, "POST", "/scenario/smoke").starts_with("HTTP/1.1 200"));
    let mut changed = false;
    for _ in 0..5 {
        if matches!(receive(&mut client).await, ServerMessage::Hello { .. }) {
            changed = true;
            break;
        }
    }
    assert!(changed, "scenario change must reset the client visibly");
    send(&mut client, ClientMessage::Hello { version: VERSION }).await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Error { code: 400, .. }
    ));
    server.abort();
    ticker.abort();
}
