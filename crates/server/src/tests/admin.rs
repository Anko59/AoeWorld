//! Live administration exercises the same HTTP auth and session delivery paths.
use super::*;
use crate::{GameplayService, app};
use aoe_core::{PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{GameplayServerMessage as M, encode_gameplay_server};
use aoe_simulation::GameWorld;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const SECRET: &str = "admin-observability-test-secret";

mod garrison_tests;

struct Server {
    address: SocketAddr,
    state: AppState,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(gameplay: GameplayService, secret: bool) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: None,
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "admin-test",
    )
    .unwrap();
    state.gameplay = std::sync::Arc::new(tokio::sync::RwLock::new(gameplay));
    // Clear any inherited AOE_MATCH_TOKEN before testing disabled administration.
    state.match_admin_token = secret.then(|| SECRET.into());
    let served = state.clone();
    let task = tokio::spawn(async move { axum::serve(listener, app(served)).await.unwrap() });
    Server {
        address,
        state,
        task,
    }
}

async fn request(
    server: &Server,
    method: &str,
    path: &str,
    secret: Option<&str>,
    body: &str,
) -> (u16, Value) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(server.address).await.unwrap();
        let header = secret.map_or_else(String::new, |s| format!("X-AoeWorld-Admin-Token: {s}\r\n"));
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1;
        (status, serde_json::from_str(body).unwrap_or_else(|_| json!(body)))
    })
    .await
    .expect("admin request completed")
}

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn villages() -> GameplayService {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let first = world
        .initialize_village(PlayerId(0), TileCoord::new(32, 32))
        .unwrap();
    let second = world
        .initialize_village(PlayerId(1), TileCoord::new(180, 180))
        .unwrap();
    GameplayService::from_scale_world_slots(
        world,
        [(PlayerId(0), first, true), (PlayerId(1), second, false)],
    )
}

#[tokio::test]
async fn every_admin_endpoint_requires_the_configured_secret() {
    for enabled in [false, true] {
        let server = server(villages(), enabled).await;
        for secret in [None, Some("wrong"), Some(SECRET)] {
            if enabled && secret == Some(SECRET) {
                continue;
            }
            for (method, path, body) in [
                ("GET", "/admin/match", ""),
                ("GET", "/admin/match/players/0", ""),
                ("GET", "/admin/perf", ""),
                ("POST", "/admin/fog", r#"{"disabled":true}"#),
            ] {
                let (status, body) = request(&server, method, path, secret, body).await;
                assert_eq!(status, 403, "{method} {path}: {body}");
            }
        }
    }
}

#[tokio::test]
async fn match_summary_reports_each_seat_and_live_economy_without_tokens() {
    let server = server(villages(), true).await;
    let (status, body) = request(&server, "GET", "/admin/match", Some(SECRET), "").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["tick"], 0);
    assert_eq!(body["fog_disabled"], false);
    let players = body["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);
    for (index, player) in players.iter().enumerate() {
        assert_eq!(player["id"], index);
        assert_eq!(player["kind"], if index == 0 { "human" } else { "ai" });
        assert_eq!(player["status"], "alive");
        assert_eq!(player["age"], "Dark");
        assert_eq!(player["population"], 4);
        assert_eq!(player["units"]["Villager"], 3);
        assert_eq!(player["units"]["Scout"], 1);
        assert_eq!(player["buildings"]["TownCenter"], 1);
        for resource in ["food", "wood", "gold", "stone"] {
            assert!(player["resources"][resource].as_u64().is_some(), "{player}");
        }
        assert_eq!(player["town_centers"].as_array().unwrap().len(), 1);
        assert_eq!(player["army"]["size"], 1);
        assert!(player["army"]["centroid"]["x"].as_f64().is_some());
        assert!(player["army"]["centroid"]["y"].as_f64().is_some());
        assert_eq!(player["activity"], "idle");
        assert_eq!(player["kills"], 0);
        assert_eq!(player["losses"], 0);
    }
    assert_eq!(
        body["combat_per_minute"],
        json!({"melee_hits":0,"ranged_hits":0,"deaths":0})
    );
    assert!(!body.to_string().contains("token"));
}

#[tokio::test]
async fn match_summary_keeps_living_garrison_occupants_in_population_and_kind_counts() {
    use aoe_simulation::{BuildingKind, UnitKind};
    let gameplay = villages();
    let (shelter, villager, militia) = {
        let mut world = gameplay.world.write().await;
        let shelter = world
            .buildings()
            .find(|b| b.player == PlayerId(0) && b.kind == BuildingKind::TownCenter)
            .unwrap()
            .id;
        let villager = world
            .units()
            .find(|u| u.player == PlayerId(0) && u.kind == UnitKind::Villager)
            .unwrap()
            .id;
        let militia = world.spawn_militia(PlayerId(0), at(31, 32)).unwrap();
        (shelter, villager, militia)
    };
    let server = server(gameplay.clone(), true).await;
    let (status, before) = request(&server, "GET", "/admin/match", Some(SECRET), "").await;
    assert_eq!(status, 200);
    {
        let mut world = gameplay.world.write().await;
        for unit in [villager, militia] {
            world.issue_garrison(PlayerId(0), unit, shelter).unwrap();
        }
    }
    for _ in 0..100 {
        gameplay.tick().await;
    }
    {
        let world = gameplay.world.read().await;
        let occupants = &world.building(shelter).unwrap().garrison;
        for id in [villager, militia] {
            assert!(occupants.iter().any(|g| g.unit.id == id && g.unit.hp > 0));
            assert!(
                world.unit(id).is_none(),
                "regression needs off-map shelter storage"
            );
        }
    }
    let (status, after) = request(&server, "GET", "/admin/match", Some(SECRET), "").await;
    assert_eq!(status, 200);
    for player in 0..2 {
        for field in ["population", "units", "buildings", "status"] {
            assert_eq!(
                after["players"][player][field], before["players"][player][field],
                "player {player}: {field}"
            );
        }
        assert_eq!(
            after["players"][player]["army"]["size"],
            before["players"][player]["army"]["size"]
        );
    }
}

#[tokio::test]
async fn player_details_page_entities_and_reject_unbounded_limits() {
    let server = server(villages(), true).await;
    let mut ids = std::collections::BTreeSet::new();
    let mut categories = std::collections::BTreeSet::new();
    for offset in [0, 2, 4] {
        let path = format!("/admin/match/players/1?offset={offset}&limit=2");
        let (status, body) = request(&server, "GET", &path, Some(SECRET), "").await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["id"], 1);
        assert_eq!(body["offset"], offset);
        assert_eq!(body["limit"], 2);
        assert_eq!(body["total"], 5);
        let entities = body["entities"].as_array().unwrap();
        assert_eq!(entities.len(), if offset == 4 { 1 } else { 2 });
        assert_eq!(
            body["next_offset"],
            if offset == 4 {
                Value::Null
            } else {
                json!(offset + 2)
            }
        );
        for entity in entities {
            assert!(
                ids.insert(entity["id"].as_u64().unwrap()),
                "duplicate entity {entity}"
            );
            categories.insert(entity["category"].as_str().unwrap().to_owned());
            assert!(entity["kind"].as_str().is_some());
            assert!(
                entity["position"]["x"].as_f64().unwrap() > 100.0,
                "wrong player's position: {entity}"
            );
            assert!(entity["position"]["y"].as_f64().unwrap() > 100.0);
            assert!(entity["task"].as_str().is_some());
        }
    }
    assert_eq!(
        categories.into_iter().collect::<Vec<_>>(),
        ["building", "unit"]
    );
    for query in ["limit=0", "limit=501", "limit=4294967295"] {
        let path = format!("/admin/match/players/1?{query}");
        assert_eq!(
            request(&server, "GET", &path, Some(SECRET), "").await.0,
            400
        );
    }
    assert_eq!(
        request(&server, "GET", "/admin/match/players/99", Some(SECRET), "")
            .await
            .0,
        404
    );
    let (_, empty) = request(
        &server,
        "GET",
        "/admin/match/players/1?offset=999999",
        Some(SECRET),
        "",
    )
    .await;
    assert!(empty["entities"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn perf_summarizes_the_current_rolling_minute_and_reuses_memory_health() {
    let server = server(villages(), true).await;
    let probe = crate::gameplay_match::TickProbe::default();
    for tick in 0..=1200 {
        let micros = if tick == 0 { 1_000_000 } else { tick };
        probe.phase("movement", Duration::from_micros(micros));
        server.state.tick_stats.record(
            tick,
            Duration::from_micros(micros),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            8,
        );
    }
    let (status, body) = request(&server, "GET", "/admin/perf", Some(SECRET), "").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["samples"], 1200);
    assert_eq!(body["window_seconds"], 60);
    // Nearest-rank percentiles over ticks 1..=1200; the expired slow tick
    // must affect neither quantiles nor this window's missed count.
    assert_eq!(
        body["phases"]["movement"],
        json!({"p50_us":600,"p95_us":1140,"max_us":1200})
    );
    assert_eq!(body["missed_ticks"], 0);
    assert_eq!(body["missed_total"], 1);
    assert!(
        body["tick_rate"]
            .as_f64()
            .is_some_and(|rate| rate.is_finite() && rate > 0.0)
    );
    assert!(
        body["top_phases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "movement")
    );
    for structure in ["gameplay_memory", "terrain_cache", "snapshot_store"] {
        assert!(body[structure].is_object(), "{structure}: {body}");
    }
    for structure in [
        "walk_cache",
        "portal_cache",
        "resource_cache",
        "route_cache",
    ] {
        assert!(
            body["gameplay_memory"][structure]["retained_bytes"]
                .as_u64()
                .is_some()
        );
    }
}

#[tokio::test]
async fn fog_toggle_updates_all_hundred_sessions_with_camera_bounded_entities() {
    let mut world = GameWorld::new(WorldConfig::new(16_384, 16_384, Seed(7)).unwrap()).unwrap();
    let mut seats = Vec::new();
    for player in 0..100 {
        let x = 100 + i32::from(player % 10) * 1000;
        let y = 100 + i32::from(player / 10) * 1000;
        world
            .place_town_center(PlayerId(player), TileCoord::new(x, y))
            .unwrap();
        let primary = world.spawn_unit(PlayerId(player), at(x - 8, y)).unwrap();
        seats.push((PlayerId(player), primary, player == 0));
    }
    // An initially hidden enemy inside one camera, and many outside it.
    let nearby = world.spawn_unit(PlayerId(1), at(120, 100)).unwrap();
    let nearby_building = world
        .place_town_center(PlayerId(1), TileCoord::new(120, 108))
        .unwrap();
    let gameplay = GameplayService::from_scale_world_slots(world, seats);
    let server = server(gameplay.clone(), true).await;
    let mut receivers = Vec::new();
    for info in gameplay.slot_infos().await {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let (id, _) = gameplay.join_slot(info.token, sender).await.unwrap();
        let region = TileRect::from_xywh(80, 80, 64, 64);
        gameplay.subscribe(id, 1, region).await;
        while receiver.try_recv().is_ok() {}
        receivers.push((info.player, receiver));
    }
    let denied = request(&server, "POST", "/admin/fog", None, r#"{"disabled":true}"#).await;
    assert_eq!(denied.0, 403);
    for disabled in [true, false] {
        let (status, body) = request(
            &server,
            "POST",
            "/admin/fog",
            Some(SECRET),
            &json!({"disabled":disabled}).to_string(),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["disabled"], disabled);
        gameplay.tick().await;
        for (player, receiver) in &mut receivers {
            let mut fog_changed = false;
            let mut local_enemy = false;
            let mut local_building = false;
            while let Ok(message) = receiver.try_recv() {
                encode_gameplay_server(&message)
                    .expect("fog changes stay within protocol message bounds");
                match message {
                    M::FogState(fog) => {
                        assert_eq!(fog.disabled, disabled);
                        assert!(fog.tiles.len() <= 64 * 64);
                        assert!(fog.overview.len() <= 65_536);
                        fog_changed = true;
                    }
                    M::Tick {
                        changed_units: units,
                        changed_buildings,
                        ..
                    } => {
                        local_enemy |= units.iter().any(|unit| unit.id == nearby);
                        local_building |= changed_buildings
                            .iter()
                            .any(|building| building.id == nearby_building);
                        if disabled {
                            // Each camera retains its owner's world-wide units,
                            // plus entities in the subscribed 64x64 region.
                            assert!(units.len() <= 4, "player {player:?}: {} units", units.len());
                            assert!(
                                changed_buildings.len() <= 3,
                                "player {player:?}: {} buildings",
                                changed_buildings.len()
                            );
                        }
                    }
                    _ => {}
                }
            }
            assert!(
                fog_changed,
                "player {player:?} must receive the toggle on the next tick"
            );
            if *player == PlayerId(0) && disabled {
                assert!(
                    local_enemy && local_building,
                    "fog off reveals camera entities"
                );
            }
        }
    }
    for body in [
        r#"{}"#,
        r#"{"disabled":"yes"}"#,
        r#"{"disabled":true,"extra":1}"#,
    ] {
        assert!(matches!(
            request(&server, "POST", "/admin/fog", Some(SECRET), body)
                .await
                .0,
            400 | 422
        ));
    }
}
