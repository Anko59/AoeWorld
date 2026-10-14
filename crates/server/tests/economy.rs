use aoe_core::{PlayerId, Seed, TileCoord, TileRect};
use aoe_map::{MapPackage, MapRequest, ResourceKind};
use aoe_protocol::{CommandResult, GameplayServerMessage, UnitKind};
use aoe_server::GameplayService;
use tokio::sync::mpsc;

fn drain(rx: &mut mpsc::Receiver<GameplayServerMessage>) -> Vec<GameplayServerMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

#[tokio::test]
async fn default_game_starts_with_four_units() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(8000, 8000), TileCoord::new(8384, 8384)),
        )
        .await;
    let GameplayServerMessage::Snapshot { units, .. } = rx.try_recv().unwrap() else {
        panic!("snapshot")
    };
    assert_eq!(units.len(), 4);
}

#[tokio::test]
async fn headless_village_gathers_wood_deposits_and_trains_a_villager() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
    let generator = package.generator();
    let service = GameplayService::from_map(package).unwrap();
    let (tx, mut rx) = mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(512, 512)),
        )
        .await;
    let GameplayServerMessage::Snapshot {
        units,
        players,
        buildings,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("snapshot")
    };
    let villager = units
        .iter()
        .find(|unit| unit.kind == UnitKind::Villager)
        .unwrap();
    let start = villager.position.tile_floor();
    let mut resources = Vec::new();
    for y in (start.y - 10).max(0)..=start.y + 10 {
        for x in (start.x - 10).max(0)..=start.x + 10 {
            if let Some(node) = generator.object_at(TileCoord::new(x, y))
                && node.kind == ResourceKind::Wood
            {
                resources.push(node);
            }
        }
    }
    resources.sort_by_key(|node| {
        (
            node.tile.x.abs_diff(start.x).pow(2) + node.tile.y.abs_diff(start.y).pow(2),
            node.id,
        )
    });
    let node = resources[0];
    let center = buildings[0].id;
    let initial = players
        .iter()
        .find(|player| player.player == PlayerId(0))
        .unwrap();
    assert_eq!(initial.stockpile.wood, 200);
    service.queue_gather(id, 1, villager.id, node.id).await;
    service.queue_train(id, 2, center, UnitKind::Villager).await;
    let mut wood = 200;
    let mut population = 4;
    let mut accepted = 0;
    let mut depleted = false;
    for tick in 1..=3000 {
        service.tick().await;
        for message in drain(&mut rx) {
            match message {
                GameplayServerMessage::CommandAck {
                    result: CommandResult::Accepted,
                    ..
                } => accepted += 1,
                GameplayServerMessage::Tick {
                    changed_players, ..
                } => {
                    for player in changed_players {
                        if player.player == PlayerId(0) {
                            if player.stockpile.wood != wood {
                                println!(
                                    "tick {tick}: wood deposited, stockpile {} -> {}",
                                    wood, player.stockpile.wood
                                );
                            }
                            if player.population_used != population {
                                println!(
                                    "tick {tick}: villager trained, population {} / {}",
                                    player.population_used, player.population_cap
                                );
                            }
                            wood = player.stockpile.wood;
                            population = player.population_used;
                        }
                    }
                }
                GameplayServerMessage::ResourceState(state) => {
                    depleted |= state.changes.iter().any(|change| {
                        change.id == node.id && change.remaining < node.initial_amount
                    })
                }
                _ => {}
            }
        }
    }
    assert_eq!(accepted, 2);
    assert!(wood >= 210, "wood stockpile must rise after return trips");
    assert_eq!(population, 5);
    assert!(
        depleted,
        "gathering must publish through the existing depletion overlay"
    );
    println!(
        "headless village PASS: wood {wood}, population {population}/5, commands {accepted}/2"
    );
    service.resync(id, 2).await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message, GameplayServerMessage::Snapshot { revision: 2, units, players, buildings, .. } if units.len() == 5 && players[0].stockpile.wood == wood && buildings[0].training_queue.is_empty())));
}
