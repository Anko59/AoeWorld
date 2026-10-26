//! Several controllers on one world: each owns exactly its player's units and
//! sees only what its own fog allows.
use aoe_core::{PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
use aoe_protocol::{
    CommandResult, GameplayRole, GameplayServerMessage, GameplayUnitState, ResumeToken, UnitKind,
};
use aoe_server::GameplayService;
use tokio::sync::mpsc;

type Rx = mpsc::Receiver<GameplayServerMessage>;

fn match_service(humans: &[bool]) -> GameplayService {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).unwrap(),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).unwrap();
    GameplayService::from_match(package, None, humans, &|| false).unwrap()
}

fn drain(rx: &mut Rx) -> Vec<GameplayServerMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

struct Seat {
    id: u64,
    rx: Rx,
    units: Vec<GameplayUnitState>,
    messages: Vec<GameplayServerMessage>,
}

async fn seat(service: &GameplayService, token: ResumeToken) -> Seat {
    let (tx, mut rx) = mpsc::channel(256);
    let (id, welcome) = service.join_slot(token, tx).await.unwrap();
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome {
            role: GameplayRole::Controller,
            ..
        }
    ));
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(500, 500)),
        )
        .await;
    let messages = drain(&mut rx);
    let units = messages
        .iter()
        .find_map(|message| match message {
            GameplayServerMessage::Snapshot { units, .. } => Some(units.clone()),
            _ => None,
        })
        .expect("snapshot");
    Seat {
        id,
        rx,
        units,
        messages,
    }
}

#[tokio::test]
async fn every_slot_gets_its_own_village_far_from_the_others() {
    let service = match_service(&[true, false, false]);
    let infos = service.slot_infos().await;
    assert_eq!(infos.len(), 3);
    let players: Vec<_> = infos.iter().map(|info| info.player).collect();
    assert_eq!(players, [PlayerId(0), PlayerId(1), PlayerId(2)]);
    assert!(infos[0].human && !infos[1].human);
    assert!(infos[0].token != infos[1].token && infos[1].token != infos[2].token);
    let mut seats = Vec::new();
    for info in &infos[1..] {
        seats.push(seat(&service, info.token).await);
    }
    for (seat, info) in seats.iter().zip(&infos[1..]) {
        assert_eq!(seat.units.len(), 4, "three villagers and a scout");
        assert!(seat.units.iter().all(|unit| unit.player == info.player));
        assert!(seat.units.iter().any(|u| u.id == info.primary_unit_id));
    }
    let a = seats[0].units[0].position.tile_floor();
    let b = seats[1].units[0].position.tile_floor();
    assert!((a.x - b.x).abs().max((a.y - b.y).abs()) > 60, "{a:?} {b:?}");
}

#[tokio::test]
async fn controllers_cannot_command_each_others_units() {
    let service = match_service(&[false, false]);
    let infos = service.slot_infos().await;
    let mut a = seat(&service, infos[0].token).await;
    let mut b = seat(&service, infos[1].token).await;
    let mine = a
        .units
        .iter()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap();
    let theirs = b
        .units
        .iter()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap();
    let near = mine.position.tile_floor();
    let destination = WorldPosition::from_tile_center(TileCoord::new(near.x + 1, near.y)).unwrap();
    service.queue_move(a.id, 1, theirs.id, destination).await;
    service.queue_stop(a.id, 2, mine.id).await;
    service.queue_stop(b.id, 1, mine.id).await;
    service.queue_stop(b.id, 2, theirs.id).await;
    service.tick().await;
    let results = |rx: &mut Rx| {
        drain(rx)
            .into_iter()
            .filter_map(|message| match message {
                GameplayServerMessage::CommandAck {
                    sequence, result, ..
                } => Some((sequence, result)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let ack_a = results(&mut a.rx);
    let ack_b = results(&mut b.rx);
    assert!(
        ack_a.contains(&(1, CommandResult::RejectedNotOwner))
            || ack_a.contains(&(1, CommandResult::RejectedUnknownEntity)),
        "{ack_a:?}"
    );
    assert!(ack_a.contains(&(2, CommandResult::Accepted)), "{ack_a:?}");
    assert!(
        ack_b.contains(&(1, CommandResult::RejectedNotOwner))
            || ack_b.contains(&(1, CommandResult::RejectedUnknownEntity)),
        "{ack_b:?}"
    );
    assert!(ack_b.contains(&(2, CommandResult::Accepted)), "{ack_b:?}");
}

#[tokio::test]
async fn fog_filtering_is_per_slot() {
    let service = match_service(&[false, false]);
    let infos = service.slot_infos().await;
    let a = seat(&service, infos[0].token).await;
    let b = seat(&service, infos[1].token).await;
    let mut view = Vec::new();
    for (seat, info) in [(&a, &infos[0]), (&b, &infos[1])] {
        let (players, buildings) = seat
            .messages
            .iter()
            .find_map(|message| match message {
                GameplayServerMessage::Snapshot {
                    players, buildings, ..
                } => Some((players.clone(), buildings.clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(players.len(), 1, "an opponent's economy stays private");
        assert_eq!(players[0].player, info.player);
        assert!(
            buildings.iter().all(|b| b.player == info.player),
            "no unseen enemy building"
        );
        let fog = seat
            .messages
            .iter()
            .find_map(|message| match message {
                GameplayServerMessage::FogState(fog) => Some(fog.clone()),
                _ => None,
            })
            .unwrap();
        assert!(!fog.disabled);
        view.push(fog);
    }
    let width = 500;
    let tile_at =
        |fog: &aoe_protocol::FogState, x: i32, y: i32| fog.tiles[(y * width + x) as usize];
    let a_at = a.units[0].position.tile_floor();
    let b_at = b.units[0].position.tile_floor();
    assert_eq!(tile_at(&view[0], a_at.x, a_at.y), 2, "A sees its own start");
    assert_eq!(tile_at(&view[0], b_at.x, b_at.y), 0, "A has not explored B");
    assert_eq!(tile_at(&view[1], b_at.x, b_at.y), 2);
    assert_eq!(tile_at(&view[1], a_at.x, a_at.y), 0);
}

#[tokio::test]
async fn join_requires_the_exact_token_and_never_lobby_joins() {
    let service = match_service(&[true, false]);
    let infos = service.slot_infos().await;
    let (tx, _rx) = mpsc::channel(8);
    let mut wrong = infos[1].token;
    wrong.0[0] ^= 1;
    assert!(service.join_slot(wrong, tx).await.is_err());
    // The human seat is still free for a plain Hello, the AI seat is not.
    let (tx, _rx) = mpsc::channel(8);
    let (_, welcome) = service.register(None, tx).await;
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome { role: GameplayRole::Controller, primary_unit_id, .. }
            if primary_unit_id == infos[0].primary_unit_id
    ));
    let (tx, _rx) = mpsc::channel(8);
    let (_, welcome) = service.register(None, tx).await;
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome {
            role: GameplayRole::Spectator,
            resume_token: None,
            ..
        }
    ));
}

#[tokio::test]
async fn rejoining_with_the_token_replaces_the_old_connection() {
    let service = match_service(&[false, false]);
    let infos = service.slot_infos().await;
    let mut first = seat(&service, infos[0].token).await;
    let second = seat(&service, infos[0].token).await;
    assert_eq!(second.units.len(), 4);
    drop(second);
    // The first connection's channel was closed by the takeover.
    assert!(matches!(
        first.rx.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected | mpsc::error::TryRecvError::Empty)
    ));
    service.disconnect(first.id).await;
}

#[tokio::test]
async fn ai_slots_do_not_authorize_server_administration() {
    let service = match_service(&[true, false]);
    let infos = service.slot_infos().await;
    let (tx, _rx) = mpsc::channel(8);
    service.register(Some(infos[0].token), tx).await;
    let (tx, _rx) = mpsc::channel(8);
    service.join_slot(infos[1].token, tx).await.unwrap();
    assert!(service.is_controller(infos[0].token).await);
    assert!(!service.is_controller(infos[1].token).await);
}

#[tokio::test]
async fn sessions_survive_a_stalled_reader_and_catch_up_in_one_delta() {
    let service = match_service(&[false, false, false]);
    let region = TileRect::new(TileCoord::new(0, 0), TileCoord::new(500, 500));
    let mut seats = Vec::new();
    for info in service.slot_infos().await {
        // A tiny queue: the first views fill it, then nobody reads.
        let (tx, mut rx) = mpsc::channel(3);
        let (id, _) = service.join_slot(info.token, tx).await.unwrap();
        service.subscribe(id, 1, region).await;
        let buildings = drain(&mut rx)
            .into_iter()
            .find_map(|message| match message {
                GameplayServerMessage::Snapshot { buildings, .. } => Some(buildings),
                _ => None,
            })
            .expect("snapshot");
        // Training changes the player's economy on every tick of the stall.
        service
            .queue_train(id, 1, buildings[0].id, UnitKind::Villager)
            .await;
        seats.push(rx);
    }
    for _ in 0..40 {
        service.tick().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for rx in &mut seats {
        assert!(!drain(rx).is_empty());
        // Nobody was disconnected: the queue is empty, not closed.
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        // Once there is room, a tick carries what the stall skipped.
        let mut caught_up = false;
        for _ in 0..40 {
            service.tick().await;
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            for message in drain(rx) {
                if let GameplayServerMessage::Tick {
                    changed_players,
                    changed_buildings,
                    ..
                } = message
                {
                    caught_up |= !changed_players.is_empty() || !changed_buildings.is_empty();
                }
            }
            if caught_up {
                break;
            }
        }
        assert!(
            caught_up,
            "the economy change reaches the reader after the stall"
        );
    }
}

#[tokio::test]
async fn a_stalled_reader_still_receives_every_ack_in_order() {
    let service = match_service(&[false]);
    let info = service.slot_infos().await.remove(0);
    // A tiny queue nobody reads: acks meet a full queue on every tick.
    let (tx, mut rx) = mpsc::channel(2);
    let (id, _) = service.join_slot(info.token, tx).await.unwrap();
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(500, 500)),
        )
        .await;
    for sequence in 1..=8 {
        service.queue_stop(id, sequence, info.primary_unit_id).await;
        service.tick().await;
    }
    // Sequences 1..=8 were applied; a repeated sequence is rejected (also acked).
    service.queue_stop(id, 3, info.primary_unit_id).await;
    let mut acks = Vec::new();
    for _ in 0..80 {
        service.tick().await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        for message in drain(&mut rx) {
            if let GameplayServerMessage::CommandAck {
                sequence, result, ..
            } = message
            {
                acks.push((sequence, result));
            }
        }
        if acks.len() >= 9 {
            break;
        }
    }
    let applied: Vec<u64> = acks
        .iter()
        .filter(|(_, result)| *result == CommandResult::Accepted)
        .map(|(sequence, _)| *sequence)
        .collect();
    assert_eq!(applied, (1..=8).collect::<Vec<_>>(), "{acks:?}");
    assert!(
        acks.contains(&(3, CommandResult::RejectedSequence)),
        "the rejection is delivered too: {acks:?}"
    );
}
