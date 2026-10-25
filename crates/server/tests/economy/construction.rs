//! A scripted Dark Age scenario over the real gameplay service: villagers
//! build a house, a mill and a farm, and the wire carries every step. The
//! observed steps are written to `.cache/buildings/scenario.log` for the
//! local narrated demonstration.
use super::*;
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState, UnitTask};

#[tokio::test]
async fn villagers_build_a_house_a_mill_and_a_farm_over_the_wire() {
    let service = GameplayService::new(Seed(1));
    let (tx, mut rx) = mpsc::channel(256);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(
            id,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(256, 256)),
        )
        .await;
    let GameplayServerMessage::Snapshot {
        units, buildings, ..
    } = rx.try_recv().unwrap()
    else {
        panic!("snapshot")
    };
    let villagers: Vec<_> = units
        .iter()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    assert_eq!(villagers.len(), 3);
    let tc = buildings[0].footprint;
    assert_eq!(buildings[0].kind, BuildingKind::TownCenter);
    assert_eq!(
        (buildings[0].hp, buildings[0].max_hp, buildings[0].los),
        (2400, 2400, 8)
    );
    let house_at = TileCoord::new(tc.max.x + 4, tc.min.y);
    // The mill fits inside the Town Center's surveyed disc; once complete,
    // its LOS explores the entire farm footprint further south.
    let mill_at = TileCoord::new(tc.max.x + 3, tc.min.y + 3);
    let farm_at = TileCoord::new(tc.max.x + 3, tc.min.y + 5);

    let mut log =
        vec!["village: town center, three villagers; wood 200, population cap 5".to_owned()];
    let mut sequence = 0;
    let order = |sequence: &mut u64| {
        *sequence += 1;
        *sequence
    };
    service
        .queue_build(
            id,
            order(&mut sequence),
            villagers.clone(),
            BuildingKind::House,
            house_at,
        )
        .await;
    let mut seen = std::collections::BTreeMap::<aoe_core::EntityId, GameplayBuildingState>::new();
    let (mut cap, mut wood, mut food) = (5_u16, 200_u32, 200_u32);
    let mut accepted = 0;
    let mut stage = 0;
    let mut farmer_seen = false;
    let mut insufficient = false;
    for tick in 1..=6000_u32 {
        service.tick().await;
        for message in drain(&mut rx) {
            match message {
                GameplayServerMessage::CommandAck { result, .. } => {
                    accepted += usize::from(result == CommandResult::Accepted);
                    insufficient |= result == CommandResult::RejectedInsufficientResources;
                }
                GameplayServerMessage::Tick {
                    changed_players,
                    changed_buildings,
                    changed_units,
                    ..
                } => {
                    for b in changed_buildings {
                        let first = !seen.contains_key(&b.id);
                        let was = seen.insert(b.id, b.clone());
                        if first {
                            log.push(format!(
                                "tick {tick}: {:?} foundation placed ({}x{} tiles, hp {}/{})",
                                b.kind,
                                b.footprint.width(),
                                b.footprint.height(),
                                b.hp,
                                b.max_hp
                            ));
                        }
                        if b.construction == ConstructionState::Complete
                            && was.is_some_and(|w| w.construction != ConstructionState::Complete)
                        {
                            log.push(format!("tick {tick}: {:?} complete, hp {}", b.kind, b.hp));
                        }
                    }
                    for p in changed_players {
                        if p.player != PlayerId(0) {
                            continue;
                        }
                        if p.population_cap != cap {
                            log.push(format!(
                                "tick {tick}: population cap {cap} -> {}",
                                p.population_cap
                            ));
                            cap = p.population_cap;
                        }
                        if p.stockpile.wood != wood {
                            log.push(format!("tick {tick}: wood {wood} -> {}", p.stockpile.wood));
                            wood = p.stockpile.wood;
                        }
                        if p.stockpile.food != food {
                            if food == 200 {
                                log.push(format!(
                                    "tick {tick}: farmer delivers, food 200 -> {}",
                                    p.stockpile.food
                                ));
                            }
                            food = p.stockpile.food;
                        }
                    }
                    for unit in changed_units {
                        if unit.task == UnitTask::Farming && !farmer_seen {
                            farmer_seen = true;
                            log.push(format!("tick {tick}: villager is farming"));
                        }
                    }
                }
                _ => {}
            }
        }
        let complete = |kind| {
            seen.values()
                .any(|b| b.kind == kind && b.construction == ConstructionState::Complete)
        };
        if stage == 0 && complete(BuildingKind::House) {
            stage = 1;
            // The farm needs a mill: asking first is refused.
            service
                .queue_build(
                    id,
                    order(&mut sequence),
                    villagers.clone(),
                    BuildingKind::Farm,
                    farm_at,
                )
                .await;
            service
                .queue_build(
                    id,
                    order(&mut sequence),
                    villagers.clone(),
                    BuildingKind::Mill,
                    mill_at,
                )
                .await;
        }
        if stage == 1 && complete(BuildingKind::Mill) {
            stage = 2;
            service
                .queue_build(
                    id,
                    order(&mut sequence),
                    villagers.clone(),
                    BuildingKind::Farm,
                    farm_at,
                )
                .await;
            // Wood is short for a barracks: 175 needed.
            service
                .queue_build(
                    id,
                    order(&mut sequence),
                    villagers.clone(),
                    BuildingKind::Barracks,
                    TileCoord::new(tc.min.x, tc.max.y + 6),
                )
                .await;
        }
        if stage == 2 && farmer_seen && food >= 210 {
            break;
        }
    }
    assert_eq!(cap, 10, "one completed house adds five housing");
    assert!(farmer_seen, "the builder works the finished farm");
    assert!(food >= 210, "the farmer's food reaches the stockpile");
    assert!(insufficient, "a barracks is unaffordable");
    assert_eq!(
        accepted, 3,
        "house, mill and farm accepted; the early farm refused"
    );
    let farm = seen
        .values()
        .find(|b| b.kind == BuildingKind::Farm)
        .unwrap();
    assert!(farm.food_remaining.unwrap() < 175);
    log.push(format!(
        "scenario PASS: cap {cap}, wood {wood}, food {food}, farm food left {}",
        farm.food_remaining.unwrap()
    ));
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/buildings");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("scenario.log"), log.join("\n") + "\n").unwrap();
}
