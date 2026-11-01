#[path = "economy/feudal.rs"]
mod feudal;
#[path = "economy/market.rs"]
mod market;
#[path = "economy/military.rs"]
mod military;
#[path = "economy/techs.rs"]
mod techs;
use aoe_core::{PlayerId, Seed, TileCoord, TileRect};
use aoe_map::{MapPackage, ResourceKind};
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
    let (package, pages) = flat_package();
    let side = package.estimate.tiles_per_side as i32;
    let generator = package
        .generator_with_environment(pages.clone(), Vec::new(), Vec::new(), Vec::new())
        .unwrap();
    let service =
        GameplayService::from_prepared_map(package, pages, Vec::new(), Vec::new(), Vec::new())
            .unwrap()
            .unwrap();
    let (tx, mut rx) = mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(side, side)),
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
    // Wolves and boars hunt now that animals move, and the wildlife seeding
    // puts them 9 to 16 tiles east of the village. The economy under test must
    // not be a predator's meal, so work a tree well away from that side.
    let predators = [
        TileCoord::new(start.x + 16, start.y),
        TileCoord::new(start.x + 9, start.y + 3),
    ];
    let mut resources = Vec::new();
    for y in 0..side {
        for x in 0..side {
            if let Some(node) = generator.object_at(TileCoord::new(x, y))
                && node.kind == ResourceKind::Wood
            {
                let safe = predators
                    .iter()
                    .all(|wild| wild.x.abs_diff(x).pow(2) + wild.y.abs_diff(y).pow(2) > 12 * 12);
                if safe {
                    resources.push(node);
                }
            }
        }
    }
    resources.sort_by_key(|node| {
        (
            node.tile.x.abs_diff(start.x).pow(2) + node.tile.y.abs_diff(start.y).pow(2),
            node.id,
        )
    });
    let node = *resources
        .first()
        .expect("headless fixture needs a wood node");
    let center = buildings[0].id;
    let initial = players
        .iter()
        .find(|player| player.player == PlayerId(0))
        .unwrap();
    assert_eq!(initial.stockpile.wood, 200);
    service.queue_gather(id, 1, villager.id, node.id).await;
    service.queue_train(id, 2, center, UnitKind::Villager).await;
    let mut log = vec!["headless game: town center, three villagers, scout, wood 200".to_owned()];
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
                    changed_players,
                    changed_units,
                    ..
                } => {
                    for unit in changed_units {
                        if unit.id == villager.id {
                            assert_eq!(unit.gathering, Some(aoe_protocol::ResourceKind::Wood));
                        }
                    }
                    for player in changed_players {
                        if player.player == PlayerId(0) {
                            if player.stockpile.wood != wood {
                                let line = format!(
                                    "tick {tick}: wood deposited, stockpile {} -> {}",
                                    wood, player.stockpile.wood
                                );
                                println!("{line}");
                                log.push(line);
                            }
                            if player.population_used != population {
                                let line = format!(
                                    "tick {tick}: villager trained, population {}",
                                    player.population_used
                                );
                                println!("{line}");
                                log.push(line);
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
    let debug = if wood < 210 {
        service
            .subscribe(
                id,
                99,
                TileRect::new(TileCoord::new(0, 0), TileCoord::new(side, side)),
            )
            .await;
        format!(
            "node={node:?}, snapshots={:?}",
            drain(&mut rx)
                .into_iter()
                .filter(|m| matches!(m, GameplayServerMessage::Snapshot { .. }))
                .collect::<Vec<_>>()
        )
    } else {
        String::new()
    };
    assert!(
        wood >= 210,
        "wood stockpile must rise after return trips: {debug}"
    );
    assert_eq!(population, 5);
    assert!(
        depleted,
        "gathering must publish through the existing depletion overlay"
    );
    let line = format!(
        "headless village PASS: wood {wood}, population {population}/5, commands {accepted}/2"
    );
    println!("{line}");
    log.push(line);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/economy");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("headless.log"), log.join("\n") + "\n").unwrap();
    service.resync(id, 2).await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message, GameplayServerMessage::Snapshot { revision: 2, units, players, buildings, .. } if units.len() == 5 && units.iter().find(|unit| unit.id == villager.id).unwrap().gathering == Some(aoe_protocol::ResourceKind::Wood) && players[0].stockpile.wood == wood && buildings[0].training_queue.is_empty())));
}

#[path = "economy/construction.rs"]
mod construction;
#[path = "economy/fishing.rs"]
mod fishing;
#[path = "economy/wire_tests.rs"]
mod wire_tests;

fn flat_package() -> (MapPackage, Vec<aoe_map::ElevationPage>) {
    use aoe_map::*;
    let zero = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&zero)).unwrap(),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview)).unwrap(),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .unwrap();
    (package, vec![zero, overview])
}
