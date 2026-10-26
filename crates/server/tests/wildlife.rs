//! The real service and current binary wire, without a UI or a listening port.
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect};
use aoe_protocol::*;
use aoe_server::GameplayService;
use tokio::sync::mpsc;

fn drain(rx: &mut mpsc::Receiver<GameplayServerMessage>) -> Vec<GameplayServerMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        let bytes = encode_gameplay_server(&message).unwrap();
        messages.push(decode_gameplay_server(&bytes).unwrap());
    }
    messages
}

#[tokio::test]
async fn wildlife_hunting_scenario_over_the_wire() {
    let service = GameplayService::new(Seed(1));
    service.set_fog_disabled(true).await;
    let (tx, mut rx) = mpsc::channel(256);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
    service.subscribe(id, 1, region).await;
    let initial = drain(&mut rx);
    let units = initial
        .iter()
        .find_map(|m| match m {
            GameplayServerMessage::Snapshot { units, .. } => Some(units),
            _ => None,
        })
        .unwrap();
    let animals = initial
        .iter()
        .find_map(|m| match m {
            GameplayServerMessage::WildlifeState { animals, .. } => Some(animals),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        animals
            .iter()
            .filter(|a| a.kind == AnimalKind::Sheep)
            .count(),
        8
    );
    assert_eq!(
        animals
            .iter()
            .filter(|a| a.kind == AnimalKind::Boar)
            .count(),
        2
    );
    assert_eq!(
        animals
            .iter()
            .filter(|a| a.kind == AnimalKind::Deer)
            .count(),
        4
    );
    let worker = units
        .iter()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    let sheep = animals
        .iter()
        .find(|a| a.kind == AnimalKind::Sheep)
        .unwrap()
        .id;
    let order = GameplayClientMessage::HuntOrder {
        sequence: 1,
        unit_ids: vec![worker],
        animal_id: sheep,
    };
    let raw = encode_gameplay_client(&order).unwrap();
    let GameplayClientMessage::HuntOrder {
        sequence,
        unit_ids,
        animal_id,
    } = decode_gameplay_client(&raw).unwrap()
    else {
        unreachable!()
    };
    service.queue_hunt(id, sequence, unit_ids, animal_id).await;
    let mut log = vec!["village: 8 sheep, 2 boar, 4 deer; food 200".to_owned()];
    let (mut owned, mut dead, mut gathered, mut delivered) = (false, false, false, false);
    let mut accepted = false;
    for tick in 1..=10_000 {
        service.tick().await;
        for message in drain(&mut rx) {
            match message {
                GameplayServerMessage::CommandAck {
                    sequence: 1,
                    result,
                    ..
                } => {
                    assert_eq!(result, CommandResult::Accepted);
                    accepted = true;
                    log.push(format!("tick {tick}: HuntOrder Accepted"));
                }
                GameplayServerMessage::WildlifeState { animals, .. } => {
                    // A harvested carcass leaves the world once its decay ends.
                    let Some(a) = animals.iter().find(|a| a.id == sheep) else {
                        assert!(dead, "the living sheep stays visible");
                        gathered = true;
                        continue;
                    };
                    if !owned && a.owner == Some(PlayerId(0)) {
                        owned = true;
                        log.push(format!("tick {tick}: sheep converts to player 0"));
                    }
                    if !dead && a.hp == 0 {
                        dead = true;
                        log.push(format!(
                            "tick {tick}: sheep slaughtered; carcass food {}",
                            a.food_remaining
                        ));
                    }
                    if dead && a.food_remaining == 0 {
                        gathered = true;
                    }
                }
                GameplayServerMessage::Tick {
                    changed_players, ..
                } => {
                    if let Some(p) = changed_players.iter().find(|p| p.player == PlayerId(0)) {
                        log.push(format!("tick {tick}: food {}", p.stockpile.food));
                        delivered = p.stockpile.food == 300;
                    }
                }
                _ => {}
            }
        }
        if delivered {
            break;
        }
    }
    assert!(
        accepted && owned && dead && gathered && delivered,
        "accepted={accepted} owned={owned} dead={dead} gathered={gathered} delivered={delivered}; {log:?}"
    );
    // Spectator and sequencing checks share the existing bounded command path.
    let (tx, mut observer) = mpsc::channel(16);
    let (spectator, _) = service.register(None, tx).await;
    service.queue_hunt(spectator, 1, vec![worker], sheep).await;
    assert!(matches!(
        drain(&mut observer)[0],
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedNotController,
            ..
        }
    ));
    service.queue_hunt(id, 1, vec![EntityId(0)], sheep).await;
    assert!(matches!(
        drain(&mut rx)[0],
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedSequence,
            ..
        }
    ));
    service.resync(id, 2).await;
    // The carcass, slaughtered beside the Town Center, is exhausted; its
    // decay may already have removed it when the last load is delivered.
    assert!(drain(&mut rx).iter().any(|m| matches!(m, GameplayServerMessage::WildlifeState { revision: 2, animals, .. } if animals.iter().all(|a| a.id != sheep || a.food_remaining == 0))));
    log.push(
        "scenario PASS: carcass exhausted, 100 food delivered; resync and authority verified"
            .to_owned(),
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/wildlife");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("scenario.log"), log.join("\n") + "\n").unwrap();
}

#[tokio::test]
async fn wildlife_subscription_replaces_the_region_and_a_full_queue_keeps_the_controller() {
    let service = GameplayService::new(Seed(1));
    service.set_fog_disabled(true).await;
    let (tx, mut rx) = mpsc::channel(16);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384)),
        )
        .await;
    assert!(drain(&mut rx).iter().any(|m| matches!(m, GameplayServerMessage::WildlifeState { revision: 1, animals, .. } if animals.len() == 15)));
    service
        .subscribe(
            id,
            2,
            TileRect::new(TileCoord::new(1, 1), TileCoord::new(100, 100)),
        )
        .await;
    assert!(drain(&mut rx).iter().any(|m| matches!(m, GameplayServerMessage::WildlifeState { revision: 2, animals, .. } if animals.is_empty())));
    service.resync(id, 3).await;
    assert!(drain(&mut rx).iter().any(|m| matches!(m, GameplayServerMessage::WildlifeState { revision: 3, animals, .. } if animals.is_empty())));

    // One slot holds the snapshot, but not the animal replacement: the
    // controller keeps its lease and the replacement waits for room.
    let service = GameplayService::new(Seed(1));
    let (tx, _blocked) = mpsc::channel(1);
    let (id, welcome) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384)),
        )
        .await;
    let GameplayServerMessage::Welcome {
        resume_token: Some(token),
        ..
    } = welcome
    else {
        panic!("controller token");
    };
    assert!(service.is_controller(token).await);
    service
        .queue_hunt(id, 1, vec![EntityId(2)], EntityId(5))
        .await;
    service.tick().await;
}

#[tokio::test]
async fn a_military_hit_publishes_a_valid_retaliating_predator_in_the_same_tick() {
    for kind in [AnimalKind::Wolf, AnimalKind::Boar] {
        let service = GameplayService::new(Seed(1));
        let (tx, mut rx) = mpsc::channel(256);
        let (session, _) = service.register(None, tx).await;
        let region = TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384));
        service.subscribe(session, 1, region).await;
        let initial = drain(&mut rx);
        let center = initial
            .iter()
            .find_map(|message| match message {
                GameplayServerMessage::Snapshot { buildings, .. } => buildings
                    .iter()
                    .find(|building| building.kind == BuildingKind::TownCenter)
                    .map(|building| {
                        TileCoord::new(
                            (building.footprint.min.x + building.footprint.max.x) / 2,
                            (building.footprint.min.y + building.footprint.max.y) / 2,
                        )
                    }),
                _ => None,
            })
            .expect("starting Town Center");
        let predator = |message| match message {
            GameplayServerMessage::WildlifeState { animals, .. } => {
                animals.into_iter().find(|a| a.kind == kind)
            }
            _ => None,
        };
        let mut target = initial.into_iter().find_map(predator);
        // The starting predators are outside LOS. Discover their identities
        // through real scout movement before issuing the combat order.
        service
            .queue_move(
                session,
                1,
                EntityId(0),
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(center.x + 20, center.y))
                    .unwrap(),
            )
            .await;
        for _ in 0..1000 {
            if target.is_some() {
                break;
            }
            service.tick().await;
            target = drain(&mut rx).into_iter().find_map(predator);
        }
        let target = target.expect("scout must reveal the predator before attacking");
        service
            .queue_attack(session, 2, EntityId(0), target.id)
            .await;
        let mut hit = false;
        for _ in 0..2000 {
            service.tick().await;
            for message in drain(&mut rx) {
                if let GameplayServerMessage::CommandAck {
                    sequence: 2,
                    result,
                    ..
                } = &message
                {
                    assert_eq!(*result, CommandResult::Accepted);
                }
                if let GameplayServerMessage::WildlifeState { animals, .. } = message
                    && let Some(animal) = animals
                        .iter()
                        .find(|a| a.id == target.id && a.hp < target.hp)
                {
                    assert_eq!(animal.task, AnimalTask::Attacking);
                    assert_eq!(animal.target, Some(EntityId(0)));
                    hit = true;
                }
            }
            if hit {
                break;
            }
        }
        assert!(hit, "military attack never reached {kind:?}");
    }
}
