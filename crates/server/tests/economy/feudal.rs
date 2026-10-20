//! Authoritative accelerated demo: ordinary queues, research, wire and deltas.
use super::*;
use aoe_protocol::{Age, BuildingKind, Tech, decode_gameplay_server, encode_gameplay_server};

#[tokio::test]
async fn feudal_demo_rejects_dark_building_then_builds_and_researches_over_the_wire() {
    let service = GameplayService::feudal_demo(Seed(1)).unwrap();
    let (tx, mut rx) = mpsc::channel(256);
    let (session, _) = service.register(None, tx).await;
    service
        .subscribe(
            session,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(256, 256)),
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
    assert_eq!(players[0].age, Age::Dark);
    let center = buildings
        .iter()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let lumber = buildings
        .iter()
        .find(|b| b.kind == BuildingKind::LumberCamp)
        .unwrap()
        .id;
    let worker = units
        .iter()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    let at = TileCoord::new(138, 136);
    service
        .queue_build(session, 1, vec![worker], BuildingKind::Blacksmith, at)
        .await;
    service.tick().await;
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::RejectedRequiresFeudalAge,
            ..
        }
    )));
    service
        .queue_research(session, 2, center, Tech::FeudalAge)
        .await;
    let mut feudal = false;
    for _ in 0..130 {
        service.tick().await;
        for message in drain(&mut rx) {
            let message =
                decode_gameplay_server(&encode_gameplay_server(&message).unwrap()).unwrap();
            if let GameplayServerMessage::Tick {
                changed_players, ..
            } = message
            {
                feudal |= changed_players.iter().any(|p| p.age == Age::Feudal);
            }
        }
    }
    assert!(feudal);
    service
        .queue_build(session, 3, vec![worker], BuildingKind::Blacksmith, at)
        .await;
    let mut complete = false;
    for _ in 0..100 {
        service.tick().await;
        for m in drain(&mut rx) {
            let m = decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap();
            if let GameplayServerMessage::Tick {
                changed_buildings, ..
            } = m
            {
                complete |= changed_buildings.iter().any(|b| {
                    b.kind == BuildingKind::Blacksmith
                        && b.progress_permille == 1000
                        && b.max_hp == 1800
                });
            }
        }
    }
    assert!(complete);
    service
        .queue_research(session, 4, lumber, Tech::DoubleBitAxe)
        .await;
    let mut researched = false;
    for _ in 0..25 {
        service.tick().await;
        for m in drain(&mut rx) {
            let m = decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap();
            if let GameplayServerMessage::Tick {
                changed_players, ..
            } = m
            {
                researched |= changed_players
                    .iter()
                    .any(|p| p.researched & Tech::DoubleBitAxe.bit() != 0);
            }
        }
    }
    assert!(researched);
    std::fs::create_dir_all("../../.cache/feudal").unwrap();
    std::fs::write("../../.cache/feudal/scenario.log","PASS: Dark Blacksmith rejected; Feudal Age researched in 130 ticks; Blacksmith completed; Double-Bit Axe researched in 25 ticks; all deltas codec-validated. Demo uses one game tick/second at the server's 20 Hz clock (20x).\n").unwrap();
}
