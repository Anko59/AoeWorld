//! Real service commands, projected shots and codec validation.
use super::*;
use aoe_protocol::{BuildingKind, Tech, decode_gameplay_server, encode_gameplay_server};

#[tokio::test]
async fn feudal_military_trains_researches_and_resolves_ranged_combat_over_wire() {
    let service = GameplayService::military_demo(Seed(1)).unwrap();
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
        units, buildings, ..
    } = rx.try_recv().unwrap()
    else {
        panic!("snapshot");
    };
    let site = |kind| buildings.iter().find(|b| b.kind == kind).unwrap().id;
    let target = units.iter().find(|u| u.player != PlayerId(0)).unwrap().id;
    service
        .queue_train(
            session,
            1,
            site(BuildingKind::ArcheryRange),
            UnitKind::Archer,
        )
        .await;
    service
        .queue_train(session, 2, site(BuildingKind::Barracks), UnitKind::Spearman)
        .await;
    service
        .queue_research(session, 3, site(BuildingKind::Blacksmith), Tech::Fletching)
        .await;
    let mut archer = None;
    let mut spear = None;
    let mut researched = false;
    for _ in 0..180 {
        service.tick().await;
        for m in drain(&mut rx) {
            let m = decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap();
            if let GameplayServerMessage::Tick {
                changed_units,
                changed_players,
                ..
            } = m
            {
                archer = changed_units
                    .iter()
                    .find(|u| u.kind == UnitKind::Archer)
                    .map(|u| u.id)
                    .or(archer);
                spear = changed_units
                    .iter()
                    .find(|u| u.kind == UnitKind::Spearman)
                    .map(|u| u.id)
                    .or(spear);
                researched |= changed_players
                    .iter()
                    .any(|p| p.researched & Tech::Fletching.bit() != 0);
            }
        }
    }
    assert!(spear.is_some() && researched);
    service
        .queue_attack(session, 4, archer.unwrap(), target)
        .await;
    let (mut shot, mut hit) = (false, false);
    for _ in 0..200 {
        service.tick().await;
        for m in drain(&mut rx) {
            let m = decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap();
            match m {
                GameplayServerMessage::Projectiles { projectiles, .. } => {
                    shot |= !projectiles.is_empty()
                }
                GameplayServerMessage::Tick { changed_units, .. } => {
                    hit |= changed_units
                        .iter()
                        .any(|u| u.id == target && u.hp < u.max_hp)
                }
                _ => {}
            }
        }
    }
    assert!(shot && hit);
    std::fs::create_dir_all("../../.cache/military").unwrap();
    std::fs::write("../../.cache/military/scenario.log", "PASS: trained Archer and Spearman, researched Fletching, observed projectile and target damage; all service messages codec-validated.\n").unwrap();
}
