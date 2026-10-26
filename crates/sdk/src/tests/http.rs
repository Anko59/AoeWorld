use crate::{Endpoint, parse_token};

#[test]
fn endpoints_parse_http_ws_and_bare_authorities() {
    for url in [
        "http://127.0.0.1:9000",
        "ws://127.0.0.1:9000/game/ws",
        "127.0.0.1:9000/",
    ] {
        let endpoint = Endpoint::parse(url).unwrap();
        assert_eq!(endpoint.authority, "127.0.0.1:9000");
        assert_eq!(endpoint.websocket_url(), "ws://127.0.0.1:9000/game/ws");
    }
    for bad in [
        "",
        "localhost",
        "https://host:1",
        "http://u@host:1",
        "http://:",
    ] {
        assert!(Endpoint::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn slot_tokens_are_exactly_48_hex_digits() {
    let token = parse_token(&"ab".repeat(24)).unwrap();
    assert_eq!(token.0, [0xab; 24]);
    for bad in ["", "ab", &"zz".repeat(24), &"ab".repeat(25)] {
        assert!(parse_token(bad).is_err());
    }
}

#[tokio::test]
async fn seeded_match_requests_seed_both_terrain_and_village_layout() {
    use axum::{Json, Router, routing::post};
    use std::sync::{Arc, Mutex};

    for (map, terrain_field) in [
        (crate::MatchMap::flat(300, 3), "flat"),
        (crate::MatchMap::square(600, 3), "map"),
    ] {
        let captured = Arc::new(Mutex::new(serde_json::Value::Null));
        let received = captured.clone();
        let app = Router::new().route(
            "/match",
            post(move |Json(body): Json<serde_json::Value>| async move {
                *received.lock().unwrap() = body;
                Json(serde_json::json!({
                    "world_id": 1, "content_hash": "test", "tiles_per_side": 300,
                    "slots": [],
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        crate::start_match(&url, "test", &map, &[crate::SlotKind::Ai])
            .await
            .unwrap();
        server.abort();
        let body = captured.lock().unwrap();
        assert_eq!(body[terrain_field]["seed"], 3);
        // A terrain-only seed left arena comparisons on clock-chosen layouts.
        assert_eq!(body["seed"], 3, "the village layout must repeat too");
    }
}
