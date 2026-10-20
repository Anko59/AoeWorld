//! A real authoritative coastal village: fund, build, train, fish, deposit.
use super::*;
use aoe_map::*;
use aoe_protocol::{BuildingKind, ConstructionState, UnitTask};

#[tokio::test]
async fn coastal_village_builds_dock_trains_ship_and_feeds_fish_depletion() {
    let (mut base, pages) = flat_package();
    base.request.requested_side_meters = 7_680;
    let water = vec![
        WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            ocean_coverage_percent: vec![0, 100, 0, 100],
            inland_coverage_percent: vec![0; 4],
        },
        WaterPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            ocean_coverage_percent: vec![50],
            inland_coverage_percent: vec![0],
        },
    ];
    let mut environment = base.environment;
    environment.water = Some(FieldPyramid {
        levels: water
            .iter()
            .map(|p| PyramidLevel {
                samples_per_axis: u16::from(p.width),
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(p)).unwrap(),
            })
            .collect(),
    });
    let package = MapPackage::with_prepared_environment(
        1,
        base.request,
        Vec::new(),
        base.projection,
        base.provenance,
        environment,
    )
    .unwrap();
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let probe = aoe_simulation::GameWorld::from_prepared_map(
        package.clone(),
        pages.clone(),
        water.clone(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let site = (4..124)
        .flat_map(|y| (4..124).map(move |x| TileCoord::new(x, y)))
        .find(|tile| {
            probe
                .validate_building_placement(aoe_simulation::BuildingKind::Dock, *tile)
                .is_ok()
        })
        .unwrap();
    let nodes: Vec<_> = (0..4)
        .flat_map(|cy| {
            (0..4).flat_map({
                let generator = &generator;
                move |cx| generator.chunk(cx, cy).unwrap().resources
            })
        })
        .collect();
    let fish = nodes
        .iter()
        .find(|n| n.object == ObjectKind::DeepFish)
        .unwrap();
    let service = GameplayService::from_prepared_map(package, pages, water, Vec::new(), Vec::new())
        .unwrap()
        .unwrap();
    let (tx, mut rx) = mpsc::channel(256);
    let (session, _) = service.register(None, tx).await;
    service
        .subscribe(
            session,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(128, 128)),
        )
        .await;
    let GameplayServerMessage::Snapshot { units, .. } = rx.try_recv().unwrap() else {
        panic!("snapshot")
    };
    let workers = units
        .iter()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    let start = units
        .iter()
        .find(|u| u.id == workers[0])
        .unwrap()
        .position
        .tile_floor();
    let mut nodes = nodes.clone();
    for message in drain(&mut rx) {
        if let GameplayServerMessage::ResourceState(state) = message {
            nodes.extend(
                state
                    .changes
                    .iter()
                    .filter_map(|n| ResourceNode::starting_from_id(n.id)),
            );
        }
    }
    let wood = nodes
        .iter()
        .filter(|n| n.kind == ResourceKind::Wood)
        .min_by_key(|n| n.tile.x.abs_diff(start.x).pow(2) + n.tile.y.abs_diff(start.y).pow(2))
        .unwrap();
    service.queue_gather(session, 1, workers[0], wood.id).await;
    let (mut stock, mut stage, mut ship, mut depleted, mut fishing) = (200, 0, None, false, false);
    for _ in 0..12_000 {
        service.tick().await;
        for message in drain(&mut rx) {
            match message {
                GameplayServerMessage::Tick {
                    changed_units,
                    changed_players,
                    changed_buildings,
                    ..
                } => {
                    for p in changed_players {
                        if p.player == PlayerId(0) {
                            stock = p.stockpile.wood;
                            if stage == 3 && p.stockpile.food >= 215 {
                                stage = 4;
                            }
                        }
                    }
                    for b in changed_buildings {
                        if stage == 1
                            && b.kind == BuildingKind::Dock
                            && b.construction == ConstructionState::Complete
                        {
                            service
                                .queue_train(session, 3, b.id, UnitKind::FishingShip)
                                .await;
                            stage = 2;
                        }
                    }
                    for u in changed_units {
                        if stage == 2 && u.kind == UnitKind::FishingShip {
                            ship = Some(u.id);
                            service.queue_gather(session, 4, u.id, fish.id).await;
                            stage = 3;
                        }
                        fishing |= Some(u.id) == ship && u.task == UnitTask::Fishing;
                    }
                }
                GameplayServerMessage::ResourceState(state) => {
                    depleted |= state
                        .changes
                        .iter()
                        .any(|n| n.id == fish.id && n.remaining < fish.initial_amount)
                }
                GameplayServerMessage::CommandAck { result, .. } => {
                    assert_eq!(result, CommandResult::Accepted)
                }
                _ => {}
            }
        }
        if stage == 0 && stock >= 240 {
            service
                .queue_build(session, 2, vec![workers[1]], BuildingKind::Dock, site)
                .await;
            stage = 1;
        }
        if stage == 4 {
            break;
        }
    }
    assert_eq!(stage, 4, "coastal scenario must deposit a ship load");
    assert!(
        fishing && depleted,
        "fish work and depletion must reach the wire"
    );
}
