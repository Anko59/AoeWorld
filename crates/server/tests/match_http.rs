//! Starting a match over HTTP and joining its slots over WebSocket.
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage, ResumeToken,
    decode_gameplay_server, encode_gameplay_client,
};
use aoe_scenario::SMOKE;
use aoe_server::{AppState, Config, app};
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const SECRET: &str = "test-match-secret-0123456789";

async fn server() -> SocketAddr {
    let packages = tempfile::tempdir().unwrap().keep();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: Some(packages),
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "match-test",
    )
    .unwrap()
    .with_match_admin_token(SECRET);
    tokio::spawn(state.clone().run_ticks());
    tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    address
}

async fn post(address: SocketAddr, secret: Option<&str>, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let header = secret.map_or(String::new(), |s| {
        format!("X-AoeWorld-Admin-Token: {s}\r\n")
    });
    let request = format!(
        "POST /match HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).await.unwrap();
    let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        status,
        text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned(),
    )
}

fn token(hex: &str) -> ResumeToken {
    let mut bytes = [0_u8; 24];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    ResumeToken(bytes)
}

const REQUEST: &str = r#"{"map":{"schema_version":1,"center_latitude_e7":488500000,
"center_longitude_e7":23500000,"requested_side_meters":1000,
"compression":{"numerator":1,"denominator":1},"year_ce":600,"seed":7},
"slots":["human","ai","ai"]}"#;

async fn welcome_for(address: SocketAddr, message: GameplayClientMessage) -> GameplayServerMessage {
    let (mut socket, _) = connect_async(format!("ws://{address}/game/ws"))
        .await
        .unwrap();
    socket
        .send(Message::Binary(
            encode_gameplay_client(&message).unwrap().into(),
        ))
        .await
        .unwrap();
    let frame = timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = frame else {
        panic!("binary frame")
    };
    decode_gameplay_server(&bytes).unwrap()
}

#[tokio::test]
async fn a_match_starts_with_the_secret_and_slots_join_by_token() {
    let address = server().await;
    assert_eq!(post(address, None, REQUEST).await.0, 403);
    assert_eq!(post(address, Some("wrong"), REQUEST).await.0, 403);
    let (status, body) = post(address, Some(SECRET), REQUEST).await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let slots = json["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0]["kind"], "human");
    assert_eq!(slots[2]["kind"], "ai");
    assert_eq!(slots[2]["player"], 2);
    for slot in &slots[1..] {
        let join = GameplayClientMessage::JoinSlot {
            version: GAMEPLAY_VERSION,
            token: token(slot["token"].as_str().unwrap()),
        };
        let GameplayServerMessage::Welcome {
            role,
            primary_unit_id,
            width_tiles,
            ..
        } = welcome_for(address, join).await
        else {
            panic!("welcome")
        };
        assert_eq!(role, GameplayRole::Controller);
        assert_eq!(
            u64::from(primary_unit_id.0),
            slot["primary_unit_id"].as_u64().unwrap()
        );
        assert_eq!(width_tiles, 500);
    }
    // A wrong token is refused outright, not demoted to a spectator.
    let wrong = GameplayClientMessage::JoinSlot {
        version: GAMEPLAY_VERSION,
        token: ResumeToken([9; 24]),
    };
    assert!(matches!(
        welcome_for(address, wrong).await,
        GameplayServerMessage::Error { code: 403, .. }
    ));
    // The browser's plain Hello takes the human seat.
    let hello = GameplayClientMessage::Hello {
        version: GAMEPLAY_VERSION,
        resume_token: None,
    };
    assert!(matches!(
        welcome_for(address, hello).await,
        GameplayServerMessage::Welcome {
            role: GameplayRole::Controller,
            ..
        }
    ));
}

#[tokio::test]
async fn match_requests_are_validated() {
    let address = server().await;
    for body in [
        r#"{"slots":["ai"]}"#,
        r#"{"map_hash":"00","slots":["ai"]}"#,
        r#"{"map_hash":"00","map":{},"slots":["ai"]}"#,
    ] {
        let (status, _) = post(address, Some(SECRET), body).await;
        assert!(
            status == 400 || status == 404 || status == 422,
            "{status} {body}"
        );
    }
}

#[tokio::test]
async fn a_flat_match_seats_many_players_apart_on_connected_ground() {
    let address = server().await;
    let slots = ["ai"; 8].join("\",\"");
    let body = format!(r#"{{"flat":{{"tiles_per_side":300,"seed":2}},"slots":["{slots}"]}}"#);
    let (status, body) = post(address, Some(SECRET), &body).await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["tiles_per_side"], 300);
    let starts: Vec<(i64, i64)> = json["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| {
            let tile = &slot["start_tile"];
            (tile[0].as_i64().unwrap(), tile[1].as_i64().unwrap())
        })
        .collect();
    assert_eq!(starts.len(), 8);
    for (i, a) in starts.iter().enumerate() {
        for b in &starts[i + 1..] {
            assert!((a.0 - b.0).abs().max((a.1 - b.1).abs()) >= 20, "{starts:?}");
        }
    }
    // Missing storage or a bad size is a client error, not a crash.
    let (status, _) = post(
        address,
        Some(SECRET),
        r#"{"flat":{"tiles_per_side":10},"slots":["ai"]}"#,
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn population_cap_and_start_units_are_match_rules() {
    let address = server().await;
    let rules = |extra: &str| {
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!(",{extra}")
        };
        format!(r#"{{"flat":{{"tiles_per_side":300,"seed":2}},"slots":["human","ai"]{extra}}}"#)
    };
    let (status, body) = post(
        address,
        Some(SECRET),
        &rules(r#""population_cap":700,"start_units":{"villagers":30,"militia":10}"#),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["population_cap"], 700);
    assert_eq!(
        json["seeded_units"], 80,
        "40 extra units for each of 2 seats"
    );
    assert_eq!(json["partial_starting_kits"], 0);
    // The shipped default now allows 1000 population, with nothing seeded.
    let (status, body) = post(address, Some(SECRET), &rules("")).await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (
            json["population_cap"].as_u64(),
            json["seeded_units"].as_u64()
        ),
        (Some(1000), Some(0))
    );
    for extra in [
        r#""population_cap":4"#,
        r#""population_cap":5001"#,
        r#""population_cap":100,"start_units":{"villagers":90,"militia":20}"#,
    ] {
        let (status, body) = post(address, Some(SECRET), &rules(extra)).await;
        assert_eq!(status, 400, "{extra}: {body}");
    }
}

#[tokio::test]
async fn the_stats_endpoint_needs_the_secret_and_records_ticks() {
    let address = server().await;
    let get = |secret: Option<&'static str>, path: &'static str| async move {
        let mut stream = TcpStream::connect(address).await.unwrap();
        let header = secret.map_or(String::new(), |s| {
            format!("X-AoeWorld-Admin-Token: {s}\r\n")
        });
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: x\r\n{header}Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).await.unwrap();
        let status: u16 = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        (
            status,
            text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned(),
        )
    };
    assert_eq!(get(None, "/match/stats").await.0, 403);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (status, body) = get(Some(SECRET), "/match/stats?since=0").await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let ticks = json["ticks"].as_array().unwrap();
    assert!(ticks.len() >= 3, "{body}");
    // The current row includes an object of named phase microseconds after
    // [tick, total_us, sim_us, late_us, orders, units, missed, planning_us].
    assert_eq!(ticks[0].as_array().unwrap().len(), 9);
    for phase in [
        "orders",
        "movement",
        "spacing",
        "terrain",
        "pocket",
        "route_cache",
    ] {
        assert!(ticks[0][8][phase].as_u64().is_some(), "{phase}: {body}");
    }
    assert!(ticks[0][7].as_u64().is_some());
    let next = json["next"].as_u64().unwrap();
    assert_eq!(next as usize, ticks.len());
    let (_, body) = get(Some(SECRET), "/match/stats?since=1000000").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["ticks"].as_array().unwrap().is_empty());
}
