//! A scripted research scenario over the real gameplay service: the Town
//! Center researches Loom (with a cancel and a refund) and Town Watch, and the
//! Feudal Age is refused until two building types stand. The wire carries
//! every step; the observed steps are written to `.cache/techs/scenario.log`
//! for the local narrated demonstration.
use super::*;
use aoe_protocol::{Age, BuildingKind, Tech, decode_gameplay_server, encode_gameplay_server};

fn on_wire(message: GameplayServerMessage) -> GameplayServerMessage {
    decode_gameplay_server(&encode_gameplay_server(&message).unwrap()).unwrap()
}

fn villager_hp(units: &[aoe_protocol::GameplayUnitState]) -> Vec<(u16, u16)> {
    units
        .iter()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| (u.hp, u.max_hp))
        .collect()
}

#[tokio::test]
async fn the_town_center_researches_loom_and_gates_town_watch_over_the_wire() {
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
        units,
        players,
        buildings,
        ..
    } = on_wire(rx.try_recv().unwrap())
    else {
        panic!("snapshot")
    };
    let center = buildings[0].id;
    assert_eq!(buildings[0].kind, BuildingKind::TownCenter);
    assert_eq!(players[0].age, Age::Dark);
    assert_eq!(players[0].researched, 0);
    assert_eq!(villager_hp(&units), vec![(25, 25); 3]);
    let mut log = vec![format!(
        "village: age Dark, food {} gold {}, three villagers at 25 hp, town center sight {}",
        players[0].stockpile.food, players[0].stockpile.gold, buildings[0].los
    )];

    // (sequence, what was asked)
    let asked = std::cell::RefCell::new(Vec::<(u64, &str)>::new());
    let mut sequence = 0;
    let mut send = |label: &'static str| {
        sequence += 1;
        asked.borrow_mut().push((sequence, label));
        sequence
    };
    let feudal = send("Feudal Age with no buildings");
    service
        .queue_research(id, feudal, center, Tech::FeudalAge)
        .await;
    let loom = send("Loom");
    service.queue_research(id, loom, center, Tech::Loom).await;
    let again = send("Loom again while it runs");
    service.queue_research(id, again, center, Tech::Loom).await;
    let cancel_at = 40;
    let (mut gold, mut food, mut age) = (100_u32, 200_u32, Age::Dark);
    let (mut hp, mut los, mut researched) = (vec![(25, 25); 3], 8_u8, 0_u64);
    let mut stage = 0;
    let mut rejected = Vec::new();
    for tick in 1..=2400_u32 {
        service.tick().await;
        for message in drain(&mut rx).into_iter().map(on_wire) {
            match message {
                GameplayServerMessage::CommandAck {
                    sequence, result, ..
                } => {
                    let label = asked
                        .borrow()
                        .iter()
                        .find(|(s, _)| *s == sequence)
                        .map_or("?", |(_, label)| *label);
                    log.push(format!("tick {tick}: {label}: {result:?}"));
                    if result != CommandResult::Accepted {
                        rejected.push((label, result));
                    }
                }
                GameplayServerMessage::Tick {
                    changed_players,
                    changed_buildings,
                    changed_units,
                    ..
                } => {
                    for p in changed_players.iter().filter(|p| p.player == PlayerId(0)) {
                        if p.stockpile.gold != gold {
                            log.push(format!("tick {tick}: gold {gold} -> {}", p.stockpile.gold));
                            gold = p.stockpile.gold;
                        }
                        if p.stockpile.food != food {
                            log.push(format!("tick {tick}: food {food} -> {}", p.stockpile.food));
                            food = p.stockpile.food;
                        }
                        if p.researched != researched {
                            log.push(format!(
                                "tick {tick}: researched bits {researched:#b} -> {:#b}",
                                p.researched
                            ));
                            researched = p.researched;
                        }
                        age = p.age;
                    }
                    for b in changed_buildings.iter().filter(|b| b.id == center) {
                        if b.los != los {
                            log.push(format!("tick {tick}: town center sight {los} -> {}", b.los));
                            los = b.los;
                        }
                        if let Some(r) = b.research
                            && r.progress_ticks == 1
                        {
                            log.push(format!(
                                "tick {tick}: researching {:?}, {} ticks",
                                r.tech, r.total_ticks
                            ));
                        }
                    }
                    for unit in changed_units
                        .iter()
                        .filter(|u| u.kind == UnitKind::Villager)
                    {
                        if unit.max_hp != 25
                            && let Some(slot) = hp.iter_mut().find(|(_, max)| *max == 25)
                        {
                            *slot = (unit.hp, unit.max_hp);
                            log.push(format!(
                                "tick {tick}: villager now {}/{} hp",
                                unit.hp, unit.max_hp
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        if stage == 0 && tick == cancel_at {
            stage = 1;
            let n = send("cancel Loom");
            service.queue_cancel_research(id, n, center).await;
            let n = send("Loom after the refund");
            service.queue_research(id, n, center, Tech::Loom).await;
        }
        if stage == 1 && researched & 1 != 0 {
            stage = 2;
            let n = send("Loom again, now researched");
            service.queue_research(id, n, center, Tech::Loom).await;
            let n = send("Town Watch");
            service.queue_research(id, n, center, Tech::TownWatch).await;
        }
        if stage == 2 && rejected.iter().any(|(label, _)| *label == "Town Watch") {
            break;
        }
    }
    assert_eq!(researched, 0b1, "Loom finished; Town Watch needs Feudal");
    assert_eq!(
        (gold, food),
        (50, 200),
        "Loom paid once; refused Town Watch costs nothing"
    );
    assert_eq!(los, 8, "Town Watch was refused before Feudal");
    assert_eq!(age, Age::Dark, "the Feudal Age was never reachable");
    assert!(
        hp.iter().any(|&(now, max)| max == 40 && now == 40),
        "Loom adds 15 hit points"
    );
    assert_eq!(
        rejected,
        vec![
            (
                "Feudal Age with no buildings",
                CommandResult::RejectedTechRequirement
            ),
            (
                "Loom again while it runs",
                CommandResult::RejectedResearchBusy
            ),
            (
                "Loom again, now researched",
                CommandResult::RejectedAlreadyResearched
            ),
            ("Town Watch", CommandResult::RejectedRequiresFeudalAge),
        ]
    );
    log.push(format!(
        "scenario PASS: age {age:?}, researched {researched:#b}, gold {gold}, food {food}, sight {los}"
    ));
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/techs");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("scenario.log"), log.join("\n") + "\n").unwrap();
}
