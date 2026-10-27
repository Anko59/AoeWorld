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
async fn receive(socket: &mut Socket) -> GameplayServerMessage {
    let frame = timeout(Duration::from_secs(2), socket.next())
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
            region: TileRect::from_xywh(100, 100, 60, 60),
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
    timeout(Duration::from_secs(3), async {
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
    let (mut world, primary) = GameWorld::vassal_demo(Seed(7)).unwrap();
    world.set_fog_disabled(true);
    let castle = world
        .buildings()
        .find(|b| b.kind == BuildingKind::Castle)
        .unwrap()
        .id;
    let enemy = world
        .place_town_center(PlayerId(1), TileCoord::new(146, 134))
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
        .subscribe(blocked, 1, TileRect::from_xywh(100, 100, 60, 60))
        .await;

    // This is the current browser's right-click on an enemy building with a
    // Castle selected: it is a ground rally, since only allies are rally objects.
    send(
        &mut controller,
        GameplayClientMessage::SetRallyOrder {
            sequence: 1,
            building_id: castle,
            target: aoe_protocol::RallyTarget::Ground(
                WorldPosition::from_tile_center(TileCoord::new(147, 135)).unwrap(),
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
