use super::*;
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
use aoe_protocol::GameplayServerMessage;
use axum::http::HeaderValue;

fn state(launch: Option<Launch>) -> AppState {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("address"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let mut state = AppState::new(&config, "skirmish-test").expect("state");
    state.skirmish = Arc::new(Skirmish::new(launch));
    state
}

/// A 500-tile procedural package the state holds; returns its hash.
async fn small_map(state: &AppState) -> String {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let hash = package.content_hash_hex();
    state
        .map_packages
        .write()
        .await
        .insert(hash.clone(), package);
    hash
}

/// Connects a browser-like controller and returns its token header.
async fn controller(state: &AppState) -> HeaderMap {
    let gameplay = state.gameplay.read().await.clone();
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    std::mem::forget(receiver);
    let (_, welcome) = gameplay.register(None, sender).await;
    let GameplayServerMessage::Welcome {
        resume_token: token,
        ..
    } = welcome
    else {
        panic!("a controller welcome");
    };
    let mut headers = HeaderMap::new();
    let hex: String = token.0.iter().map(|byte| format!("{byte:02x}")).collect();
    headers.insert(
        "x-aoeworld-controller-token",
        HeaderValue::from_str(&hex).expect("header"),
    );
    headers
}

async fn until(state: &AppState, phase: Phase) -> Status {
    for _ in 0..400 {
        let current = status(state).await;
        if current.phase == phase {
            return current;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never reached {phase:?}: {:?}", status(state).await);
}

#[test]
fn a_host_reads_its_slots_from_stdin_and_ninety_nine_bots_take_eight() {
    let launch = Launch {
        command: vec![
            "python3".into(),
            "-m".into(),
            "aoe_ai".into(),
            "host".into(),
        ],
        url: "http://127.0.0.1:8080".into(),
    };
    let arguments = launch.arguments();
    assert_eq!(arguments[..3], ["-m", "aoe_ai", "host"]);
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["--tokens-file", "-"])
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["--url", "http://127.0.0.1:8080"])
    );
    assert_eq!(MAX_OPPONENTS, 99);
    assert_eq!(MAX_OPPONENTS.div_ceil(fleet::BOTS_PER_HOST), 8);
}

#[tokio::test]
async fn refusals_name_the_reason() {
    let state = state(None);
    let hash = small_map(&state).await;
    let request = |opponents| {
        Json(StartRequest {
            map_hash: hash.clone(),
            opponents,
        })
    };
    let anonymous = start(State(state.clone()), HeaderMap::new(), request(1)).await;
    assert_eq!(anonymous.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
    let headers = controller(&state).await;
    let unavailable = start(State(state.clone()), headers.clone(), request(2)).await;
    assert_eq!(
        unavailable.err().map(|e| e.0),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    let too_many = start(State(state.clone()), headers, request(100)).await;
    assert_eq!(too_many.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let idle = status(&state).await;
    assert!(!idle.ai_available);
    assert_eq!((idle.phase, idle.max_opponents), (Phase::Idle, 99));
}

#[tokio::test]
async fn a_skirmish_places_every_village_starts_its_hosts_and_stop_ends_them() {
    // A host that never connects: the match waits in ConnectingAi.
    let launch = Launch {
        command: vec![
            "sh".into(),
            "-c".into(),
            "cat >/dev/null; exec sleep 60".into(),
        ],
        url: "http://127.0.0.1:1".into(),
    };
    let state = state(Some(launch));
    let hash = small_map(&state).await;
    let headers = controller(&state).await;
    let old_world = state.gameplay.read().await.world_id();
    let request = Json(StartRequest {
        map_hash: hash.clone(),
        opponents: 2,
    });
    let (code, _) = start(State(state.clone()), headers.clone(), request)
        .await
        .expect("started");
    assert_eq!(code, StatusCode::ACCEPTED);
    let waiting = until(&state, Phase::ConnectingAi).await;
    assert_eq!(
        (waiting.completed, waiting.total, waiting.unit),
        (Some(0), Some(2), Some("AI players"))
    );
    assert_eq!(waiting.ai_hosts, 1);
    let gameplay = state.gameplay.read().await.clone();
    assert_ne!(
        gameplay.world_id(),
        old_world,
        "the match replaced the world"
    );
    assert_eq!(gameplay.presence().await, (0, 2, false));
    // The starter is not connected to the new world, yet may still stop it.
    let Json(stopped) = stop(State(state.clone()), headers).await.expect("stop");
    assert_eq!((stopped.phase, stopped.ai_hosts), (Phase::Cancelled, 0));
}

#[tokio::test]
async fn no_opponents_is_ready_once_the_match_is_in() {
    let state = state(None);
    let hash = small_map(&state).await;
    let headers = controller(&state).await;
    let request = Json(StartRequest {
        map_hash: hash,
        opponents: 0,
    });
    let _ = start(State(state.clone()), headers, request)
        .await
        .expect("started");
    let ready = until(&state, Phase::Ready).await;
    assert_eq!(ready.ai_hosts, 0);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(status(&state).await.phase, Phase::Ready);
}

#[tokio::test]
async fn a_reload_reclaims_the_human_seat_and_keeps_the_ai_running() {
    let launch = Launch {
        command: vec![
            "sh".into(),
            "-c".into(),
            "cat >/dev/null; exec sleep 60".into(),
        ],
        url: "http://127.0.0.1:1".into(),
    };
    let state = state(Some(launch));
    let hash = small_map(&state).await;
    let headers = controller(&state).await;
    let request = Json(StartRequest {
        map_hash: hash,
        opponents: 2,
    });
    let _ = start(State(state.clone()), headers, request)
        .await
        .expect("started");
    until(&state, Phase::ConnectingAi).await;
    // The AI players take their seats the way a host does: by slot token.
    let gameplay = state.gameplay.read().await.clone();
    for slot in gameplay.slot_infos().await.into_iter().filter(|s| !s.human) {
        let (sender, receiver) = tokio::sync::mpsc::channel(64);
        std::mem::forget(receiver);
        gameplay
            .join_slot(slot.token, sender)
            .await
            .expect("ai seat");
    }
    until(&state, Phase::Ready).await;
    // The browser joins the new world, then reloads: the socket closes and
    // the same tab's token reclaims the seat a few seconds later.
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    std::mem::forget(receiver);
    let (session, welcome) = gameplay.register(None, sender).await;
    let GameplayServerMessage::Welcome {
        resume_token: token,
        ..
    } = welcome
    else {
        panic!("the person takes the human seat");
    };
    gameplay.disconnect(session).await;
    assert_eq!(gameplay.presence().await, (2, 2, false));
    tokio::time::sleep(Duration::from_millis(2_600)).await;
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    std::mem::forget(receiver);
    let (_, welcome) = gameplay.register(Some(token), sender).await;
    let GameplayServerMessage::Welcome { resume_token, .. } = welcome else {
        panic!("a welcome");
    };
    assert_eq!(resume_token, token);
    assert!(gameplay.is_human_token(token));
    tokio::time::sleep(Duration::from_millis(2_600)).await;
    let resumed = status(&state).await;
    assert_eq!((resumed.phase, resumed.ai_hosts), (Phase::Ready, 1));
    assert_eq!(gameplay.presence().await, (2, 2, true));
    // Quit to Main Menu, with the reclaimed token, still ends the match.
    let mut headers = HeaderMap::new();
    let hex: String = token.0.iter().map(|byte| format!("{byte:02x}")).collect();
    headers.insert(
        "x-aoeworld-controller-token",
        HeaderValue::from_str(&hex).expect("header"),
    );
    let Json(stopped) = stop(State(state.clone()), headers).await.expect("stop");
    assert_eq!((stopped.phase, stopped.ai_hosts), (Phase::Stopped, 0));
}

#[tokio::test]
async fn starter_authority_ends_when_an_unrelated_world_replaces_the_skirmish() {
    let state = state(None);
    let hash = small_map(&state).await;
    let starter = controller(&state).await;
    let _ = start(
        State(state.clone()),
        starter.clone(),
        Json(StartRequest {
            map_hash: hash.clone(),
            opponents: 0,
        }),
    )
    .await
    .unwrap();
    until(&state, Phase::Ready).await;
    // The original starter may cancel its own replacement, without a fleet.
    assert!(authorize(&state, &starter).await.is_ok());
    let replacement = self::state(None).gameplay.read().await.clone();
    *state.gameplay.write().await = replacement;
    // No watcher sleep: authority must end synchronously with replacement.
    let refused = start(
        State(state.clone()),
        starter.clone(),
        Json(StartRequest {
            map_hash: hash,
            opponents: 0,
        }),
    )
    .await;
    assert_eq!(refused.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
    assert_eq!(
        stop(State(state.clone()), starter).await.err().map(|e| e.0),
        Some(StatusCode::FORBIDDEN)
    );
    let current = controller(&state).await;
    assert!(authorize(&state, &current).await.is_ok());
}
