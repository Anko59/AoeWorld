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
    let sites: Vec<_> = (4..124)
        .flat_map(|y| (4..124).map(move |x| TileCoord::new(x, y)))
        .filter(|tile| {
            probe
                .validate_building_placement(aoe_simulation::BuildingKind::Dock, *tile)
                .is_ok()
        })
        .collect();
    let nodes: Vec<_> = (0..4)
        .flat_map(|cy| {
            (0..4).flat_map({
                let generator = &generator;
                move |cx| generator.chunk(cx, cy).unwrap().resources
            })
        })
        .collect();
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
    let GameplayServerMessage::Snapshot {
        units, buildings, ..
    } = rx.try_recv().unwrap()
    else {
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
    let mut fog = None;
    for message in drain(&mut rx) {
        if let GameplayServerMessage::FogState(state) = &message {
            fog = Some(state.clone());
        }
        if let GameplayServerMessage::ResourceState(state) = message {
            nodes.extend(
                state
                    .changes
                    .iter()
                    .filter_map(|n| ResourceNode::starting_from_id(n.id)),
            );
        }
    }
    let fog = fog.expect("initial tile-exact exploration");
    // Survey a free shore with an ordinary scout move before building. The
    // starting village need not have explored any suitable coast yet.
    let site = sites
        .into_iter()
        .filter(|origin| {
            let rect = TileRect::from_xywh(origin.x, origin.y, 3, 3);
            (rect.min.y..rect.max.y).all(|y| {
                (rect.min.x..rect.max.x).all(|x| {
                    let tile = TileCoord::new(x, y);
                    !units.iter().any(|u| u.position.tile_floor() == tile)
                        && !buildings.iter().any(|b| b.footprint.contains(tile))
                        && !nodes.iter().any(|n| n.tile == tile)
                })
            })
        })
        .min_by_key(|tile| tile.x.abs_diff(start.x).pow(2) + tile.y.abs_diff(start.y).pow(2))
        .expect("the map has a free coast");
    let fish = nodes
        .iter()
        .filter(|n| n.object == ObjectKind::DeepFish)
        .min_by_key(|n| n.tile.x.abs_diff(site.x + 1).pow(2) + n.tile.y.abs_diff(site.y + 1).pow(2))
        .unwrap();
    let fishing_ground = (fish.tile.y - 1..=fish.tile.y + 1)
        .flat_map(|y| (fish.tile.x - 1..=fish.tile.x + 1).map(move |x| TileCoord::new(x, y)))
        .filter(|tile| {
            generator.tile_at(*tile).is_some_and(|t| {
                matches!(
                    t.water,
                    WaterKind::Ocean | WaterKind::Lake | WaterKind::River
                ) && t.material != GroundMaterial::Ford
            }) && !nodes.iter().any(|n| n.tile == *tile)
                && !TileRect::from_xywh(site.x, site.y, 3, 3).contains(*tile)
                && !buildings.iter().any(|b| b.footprint.contains(*tile))
        })
        .min_by_key(|tile| tile.x.abs_diff(site.x).pow(2) + tile.y.abs_diff(site.y).pow(2))
        .expect("open water beside the fish: resource tiles block movement");
    let scout = units.iter().find(|u| u.kind == UnitKind::Scout).unwrap().id;
    let survey = [
        TileCoord::new(site.x - 1, site.y + 1),
        TileCoord::new(site.x + 3, site.y + 1),
        TileCoord::new(site.x + 1, site.y - 1),
        TileCoord::new(site.x + 1, site.y + 3),
    ]
    .into_iter()
    .find(|tile| {
        probe.tile_passable(*tile)
            && !nodes.iter().any(|n| n.tile == *tile)
            && !buildings.iter().any(|b| b.footprint.contains(*tile))
    })
    .expect("coast has a land approach within scout sight of the entire dock");
    service
        .queue_move(
            session,
            1,
            scout,
            aoe_core::WorldPosition::from_tile_center(survey).unwrap(),
        )
        .await;
    let mut fog = fog;
    for _ in 0..2000 {
        if dock_explored(&fog, site) {
            break;
        }
        service.tick().await;
        for message in drain(&mut rx) {
            match message {
                GameplayServerMessage::FogState(state) => fog = state,
                GameplayServerMessage::CommandAck { result, .. } => {
                    assert_eq!(result, CommandResult::Accepted)
                }
                _ => {}
            }
        }
    }
    assert!(dock_explored(&fog, site), "scout must survey the dock site");
    let wood = nodes
        .iter()
        .filter(|n| n.kind == ResourceKind::Wood)
        .min_by_key(|n| n.tile.x.abs_diff(start.x).pow(2) + n.tile.y.abs_diff(start.y).pow(2))
        .unwrap();
    service.queue_gather(session, 2, workers[0], wood.id).await;
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
                            if stage == 4 && p.stockpile.food >= 215 {
                                stage = 5;
                            }
                        }
                    }
                    for b in changed_buildings {
                        if stage == 1
                            && b.kind == BuildingKind::Dock
                            && b.construction == ConstructionState::Complete
                        {
                            service
                                .queue_train(session, 4, b.id, UnitKind::FishingShip)
                                .await;
                            stage = 2;
                        }
                    }
                    for u in changed_units {
                        if stage == 2 && u.kind == UnitKind::FishingShip {
                            ship = Some(u.id);
                            // Scout the fishing grounds before gathering a
                            // discovered resource through the ordinary orders.
                            service
                                .queue_move(
                                    session,
                                    5,
                                    u.id,
                                    aoe_core::WorldPosition::from_tile_center(fishing_ground)
                                        .unwrap(),
                                )
                                .await;
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
                GameplayServerMessage::FogState(state) => fog = state,
                GameplayServerMessage::CommandAck { result, .. } => {
                    assert_eq!(result, CommandResult::Accepted)
                }
                _ => {}
            }
        }
        if stage == 3 && tile_explored(&fog, fish.tile) {
            service
                .queue_gather(session, 6, ship.unwrap(), fish.id)
                .await;
            stage = 4;
        }
        if stage == 0 && stock >= 240 {
            service
                .queue_build(session, 3, vec![workers[1]], BuildingKind::Dock, site)
                .await;
            stage = 1;
        }
        if stage == 5 {
            break;
        }
    }
    assert_eq!(stage, 5, "coastal scenario must deposit a ship load");
    assert!(
        fishing && depleted,
        "fish work and depletion must reach the wire"
    );
}

fn dock_explored(fog: &aoe_protocol::FogState, site: TileCoord) -> bool {
    (site.y..site.y + 3)
        .all(|y| (site.x..site.x + 3).all(|x| tile_explored(fog, TileCoord::new(x, y))))
}

fn tile_explored(fog: &aoe_protocol::FogState, tile: TileCoord) -> bool {
    fog.region.contains(tile)
        && fog.tiles[((tile.y - fog.region.min.y) * fog.region.width() + tile.x - fog.region.min.x)
            as usize]
            > 0
}
