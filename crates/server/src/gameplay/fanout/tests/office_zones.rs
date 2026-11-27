use super::*;
use aoe_protocol::GameplayServerMessage as M;
use aoe_simulation::{BuildingKind, UnitKind};

#[tokio::test]
async fn office_shapes_are_private_and_camera_changes_do_not_resend_them() {
    let (player, other) = (PlayerId(0), PlayerId(1));
    let (mut world, primary) = GameWorld::vassal_defense_demo(Seed(7)).unwrap();
    let town = world
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .into_owned();
    let vassal = world
        .units()
        .find(|u| u.kind == UnitKind::Vassal)
        .unwrap()
        .id;
    let foreign = world.units().find(|u| u.player == other).unwrap().id;
    let at = WorldPosition::from_tile_center(TileCoord::new(
        town.footprint.min.x - 1,
        town.footprint.min.y,
    ))
    .unwrap();
    let steward = world.spawn_kind(player, UnitKind::Steward, at).unwrap();
    world.issue_garrison(player, steward, town.id).unwrap();
    world.advance();
    world
        .issue_vassal_development(player, vassal, false)
        .unwrap();
    let service = crate::GameplayService::from_scale_world_slots(
        world,
        [(player, primary, true), (other, foreign, true)],
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    let (tx, mut foreign_rx) = tokio::sync::mpsc::channel(64);
    let (foreign_id, _) = service.register(None, tx).await;
    let view = TileRect::from_xywh(0, 0, 64, 64);
    service.subscribe(id, 1, view, true).await;
    service.subscribe(foreign_id, 1, view, true).await;
    for phase in 0..3 {
        if phase == 1 {
            service
                .subscribe(id, 2, TileRect::from_xywh(64, 64, 64, 64), false)
                .await;
        }
        if phase == 2 {
            service
                .world
                .write()
                .await
                .issue_ungarrison(player, town.id, Some(steward))
                .unwrap();
        }
        let mut batches = Vec::new();
        for _ in 0..25 {
            service.tick().await;
            while let Ok(m) = rx.try_recv() {
                if let M::OfficeZone {
                    office,
                    vassal,
                    start,
                    complete,
                    chunks,
                } = m
                {
                    assert!(start && complete);
                    batches.push((office, vassal, chunks.is_empty()));
                }
            }
            while let Ok(m) = foreign_rx.try_recv() {
                assert!(
                    !matches!(m, M::OfficeZone { .. }),
                    "another seat received an office zone"
                );
            }
        }
        batches.sort_unstable();
        match phase {
            0 => assert_eq!(batches, [(vassal, true, false), (steward, false, false)]),
            1 => assert!(batches.is_empty()),
            _ => assert_eq!(batches, [(steward, false, true)]),
        }
    }
}

#[tokio::test]
async fn an_initial_view_includes_its_office_shape_without_needing_a_new_tick() {
    let (mut world, primary) = GameWorld::steward_kingdom_demo(Seed(7)).unwrap();
    for _ in 0..10 {
        world.advance();
    }
    let service = crate::GameplayService::from_scale_world(world, primary);
    service.set_paused(true);
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let (id, _) = service.register(None, tx).await;
    let view = TileRect::from_xywh(0, 0, 64, 64);
    service.subscribe(id, 1, view, true).await;
    service.subscribe(id, 2, view, true).await;
    assert!(matches!(
        rx.recv().await,
        Some(M::Snapshot { revision: 1, .. })
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut zones = 0;
        while let Some(message) = rx.recv().await {
            match message {
                M::OfficeZone {
                    start,
                    complete,
                    chunks,
                    ..
                } => {
                    assert!(start && complete && !chunks.is_empty());
                    zones += 1;
                }
                M::Snapshot { revision: 2, .. } => break,
                _ => {}
            }
        }
        assert_eq!(zones, 1);
    })
    .await
    .unwrap();
}
