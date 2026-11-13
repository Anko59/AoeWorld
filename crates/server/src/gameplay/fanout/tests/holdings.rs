use super::*;
use aoe_protocol::GameplayServerMessage as M;

#[tokio::test]
async fn subscribe_and_resync_keep_land_scalars_and_send_independent_boundaries_once() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    let primary = world
        .initialize_village(PlayerId(0), TileCoord::new(30, 30))
        .unwrap();
    let town_center = world
        .buildings()
        .find(|b| b.kind == aoe_simulation::BuildingKind::TownCenter)
        .unwrap()
        .id;
    let service = super::super::super::GameplayService::from_scale_world(world, primary);
    // With the natural start layout one rim tile of the village's first
    // exploration (32, 25) joins the clipped holdings at the first one-second
    // land refresh: a real clip change, not a stock-driven resend. Let the
    // opening second pass so the test pins only unchanged-boundary behaviour.
    for _ in 0..21 {
        service.tick().await;
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let (id, _) = service.register(None, tx).await;
    for revision in 1..=2 {
        service
            .subscribe(id, revision, TileRect::from_xywh(0, 0, 64, 64))
            .await;
        let mut area = 0;
        let mut boundaries = 0;
        while let Ok(m) = rx.try_recv() {
            match m {
                M::Snapshot { players, .. } => {
                    area = players[0].held_tiles;
                    assert!(area > 0);
                }
                M::Holdings { shapes, .. } => {
                    boundaries += 1;
                    assert!(!shapes.is_empty());
                }
                _ => {}
            }
        }
        assert!(area > 0);
        assert_eq!(boundaries, 1);
        let mut scalar_updates = 0;
        for n in 0..24 {
            {
                let mut world = service.world.write().await;
                if n % 2 == 0 {
                    world
                        .issue_train(PlayerId(0), town_center, aoe_simulation::UnitKind::Villager)
                        .unwrap();
                } else {
                    world
                        .issue_cancel_training(PlayerId(0), town_center)
                        .unwrap();
                }
            }
            service.tick().await;
            while let Ok(m) = rx.try_recv() {
                if let M::Tick {
                    changed_players, ..
                } = &m
                {
                    scalar_updates += usize::from(!changed_players.is_empty());
                }
                assert!(
                    !matches!(m, M::Holdings { .. }),
                    "unchanged boundaries must not ride tick/player updates"
                );
            }
        }
        assert!(
            scalar_updates >= 20,
            "stock changes exercise scalar deltas without boundary resends"
        );
    }
    service
        .world
        .write()
        .await
        .place_town_center(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    let mut changed_boundaries = 0;
    for _ in 0..40 {
        service.tick().await;
        while let Ok(m) = rx.try_recv() {
            if let M::Holdings { tick, .. } = m {
                assert!(tick.0.is_multiple_of(20));
                changed_boundaries += 1;
            }
        }
    }
    assert_eq!(
        changed_boundaries, 1,
        "new claims update once at the low-rate cadence"
    );
}

#[test]
fn capped_boundaries_keep_whole_rows_across_the_map() {
    let land = aoe_simulation::LandControl {
        tiles: 9000,
        share_ppm: 1,
        shapes: (0..9000)
            .map(|y| aoe_simulation::HoldingSpan {
                player: PlayerId(0),
                y,
                x0: 0,
                x1: 1,
            })
            .collect(),
    };
    let M::Holdings {
        shapes, complete, ..
    } = crate::gameplay::economy::holdings::message(&land, 1, Tick(0))
    else {
        unreachable!()
    };
    assert!(!complete);
    assert!(shapes.len() <= aoe_protocol::MAX_HOLDING_SPANS);
    assert!(shapes.first().unwrap().y < 10);
    assert!(shapes.last().unwrap().y > 8500);
}
