//! Real sockets: Castle rally/attack orders cannot poison everyone else's view.
use super::*;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_protocol::CommandResult;
use aoe_simulation::{BuildingKind, GameWorld};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message as Wire};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(socket: &mut Socket, message: GameplayClientMessage) {
    socket
        .send(Wire::Binary(
            aoe_protocol::encode_gameplay_client(&message)
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
}
/// How long a frame or an acknowledgement may take. It only stops a hang: under coverage
/// instrumentation, beside other tests on four cores, a tick of a thousand units took longer than the
/// three seconds this file allowed, and `make coverage` failed `dev` twice with "command never settled".
const PATIENCE: Duration = Duration::from_secs(60);

async fn receive(socket: &mut Socket) -> GameplayServerMessage {
    let frame = timeout(PATIENCE, socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Wire::Binary(bytes) = frame else {
        panic!("expected gameplay frame: {frame:?}")
    };
    aoe_protocol::decode_gameplay_server(&bytes).unwrap()
}

async fn open(address: std::net::SocketAddr) -> Socket {
    let (mut socket, _) = connect_async(format!("ws://{address}/game/ws"))
        .await
        .unwrap();
    send(
        &mut socket,
        GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token: None,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut socket).await,
        GameplayServerMessage::Welcome { .. }
    ));
    send(
        &mut socket,
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: TileRect::from_xywh(80, 100, 60, 60),
            unit_zoom: false,
        },
    )
    .await;
    loop {
        if matches!(
            receive(&mut socket).await,
            GameplayServerMessage::Projectiles { .. }
        ) {
            break;
        }
    }
    socket
}

async fn ack(service: &GameplayService, socket: &mut Socket, sequence: u64) -> CommandResult {
    // Let the socket reader admit the command, then advance the actual server.
    timeout(PATIENCE, async {
        loop {
            service.tick().await;
            if let Ok(GameplayServerMessage::CommandAck {
                sequence: actual,
                result,
                ..
            }) = timeout(Duration::from_millis(10), receive(socket)).await
                && actual == sequence
            {
                return result;
            }
        }
    })
    .await
    .expect("command never settled")
}

#[tokio::test]
async fn castle_target_commands_and_a_slow_session_do_not_disconnect_other_sockets() {
    let (mut world, primary) = GameWorld::general_demo(Seed(7)).unwrap();
    world.set_fog_disabled(true);
    let castle = world
        .buildings()
        .find(|b| b.kind == BuildingKind::Castle)
        .unwrap()
        .id;
    let enemy = world
        .place_town_center(PlayerId(1), TileCoord::new(105, 124))
        .unwrap();
    let service = GameplayService::from_scale_world(world, primary);
    let handler = service.clone();
    let router = axum::Router::new().route(
        "/game/ws",
        axum::routing::get(move |ws: axum::extract::WebSocketUpgrade| {
            let handler = handler.clone();
            async move { ws.on_upgrade(move |socket| handle_socket(handler, socket)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut controller = open(address).await;
    let mut observer = open(address).await;
    let (blocked_tx, _blocked_rx) = mpsc::channel(1);
    let (blocked, _) = service.register(None, blocked_tx).await;
    service
        .subscribe(blocked, 1, TileRect::from_xywh(80, 100, 60, 60), true)
        .await;

    // This is the current browser's right-click on an enemy building with a
    // Castle selected: it is a ground rally, since only allies are rally objects.
    send(
        &mut controller,
        GameplayClientMessage::SetRallyOrder {
            sequence: 1,
            building_id: castle,
            target: aoe_protocol::RallyTarget::Ground(
                WorldPosition::from_tile_center(TileCoord::new(106, 125)).unwrap(),
            ),
        },
    )
    .await;
    assert_eq!(
        ack(&service, &mut controller, 1).await,
        CommandResult::Accepted
    );
    send(
        &mut controller,
        GameplayClientMessage::AttackOrder {
            sequence: 2,
            entity_id: castle,
            target_id: enemy,
        },
    )
    .await;
    assert_eq!(
        ack(&service, &mut controller, 2).await,
        CommandResult::Accepted
    );
    send(
        &mut controller,
        GameplayClientMessage::AttackOrder {
            sequence: 3,
            entity_id: castle,
            target_id: EntityId(u32::MAX),
        },
    )
    .await;
    assert_eq!(
        ack(&service, &mut controller, 3).await,
        CommandResult::RejectedInvalidTarget
    );
    for _ in 0..20 {
        service.tick().await;
    }
    // Observer keeps receiving valid frames after both accepted and bad orders.
    loop {
        if matches!(
            receive(&mut observer).await,
            GameplayServerMessage::Tick { .. }
        ) {
            break;
        }
    }
    let mut returning = open(address).await;
    send(
        &mut returning,
        GameplayClientMessage::Resync { revision: 2 },
    )
    .await;
    loop {
        if matches!(
            receive(&mut returning).await,
            GameplayServerMessage::Snapshot { revision: 2, .. }
        ) {
            break;
        }
    }
    assert!(service.world.read().await.building(enemy).unwrap().hp < 2400);
    server.abort();
}

#[tokio::test]
async fn thousand_units_move_attack_move_and_attack_in_one_frame_and_tick_each() {
    let mut world = GameWorld::new(aoe_core::WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let primary = world
        .initialize_village(PlayerId(0), TileCoord::new(100, 100))
        .unwrap();
    let ids: Vec<_> = (0..1000)
        .map(|n| {
            let kind = if n < 16 {
                aoe_simulation::UnitKind::Villager
            } else {
                aoe_simulation::UnitKind::Militia
            };
            world
                .spawn_kind(PlayerId(0), kind, at(70 + n % 40, 130 + n / 40))
                .unwrap()
        })
        .collect();
    let target_id = world
        .place_town_center(PlayerId(1), TileCoord::new(140, 148))
        .unwrap();
    let service = GameplayService::from_scale_world(world, primary);
    let handler = service.clone();
    let router = axum::Router::new().route(
        "/game/ws",
        axum::routing::get(move |ws: axum::extract::WebSocketUpgrade| {
            let handler = handler.clone();
            async move { ws.on_upgrade(move |socket| handle_socket(handler, socket)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut socket = open(address).await;
    let mut measurement = String::from("order,frame_bytes,units_applied,world_tick\n");
    for (i, message) in [
        GameplayClientMessage::MoveSelectionOrder {
            sequence: 1,
            unit_ids: ids.clone(),
            formation: aoe_core::Formation::Box,
            destination: at(120, 150),
            queued: false,
            attack_move: false,
        },
        GameplayClientMessage::MoveSelectionOrder {
            sequence: 2,
            unit_ids: ids.clone(),
            formation: aoe_core::Formation::Box,
            destination: at(124, 150),
            queued: false,
            attack_move: true,
        },
        GameplayClientMessage::AttackSelectionOrder {
            sequence: 3,
            entity_ids: ids.clone(),
            target_id,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = aoe_protocol::encode_gameplay_client(&message).unwrap();
        assert!(bytes.len() < 3_000);
        send(&mut socket, message).await;
        assert_eq!(
            ack(&service, &mut socket, i as u64 + 1).await,
            CommandResult::Accepted
        );
        let world = service.world.read().await;
        assert_eq!(
            world.order_demand()[aoe_simulation::OrderIssuer::Player as usize].decisions,
            i as u64 + 1
        );
        let applied = ids
            .iter()
            .filter(|&&id| {
                if i == 2 {
                    world.unit(id).unwrap().attack_target == Some(target_id)
                } else {
                    world.move_waypoints(id).first() == Some(&(at(120 + i as i32 * 4, 150), i == 1))
                }
            })
            .count();
        use std::fmt::Write as _;
        let order = ["move", "attack-move", "attack"][i];
        writeln!(
            measurement,
            "{order},{},{applied},{}",
            bytes.len(),
            world.tick().0
        )
        .unwrap();
        assert_eq!(applied, 1_000);
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/orders");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("army-1000.csv"), measurement).unwrap();
    server.abort();
}
