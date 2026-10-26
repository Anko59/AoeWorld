use super::*;
use aoe_map::*;
use aoe_simulation::UnitKind;

fn coast() -> GameWorld {
    let elevation = [(0, 2), (1, 1)].map(|(level, axis)| ElevationPage {
        level,
        x: 0,
        y: 0,
        width: axis,
        height: axis,
        geographic_height_centimeters: vec![0; usize::from(axis).pow(2)],
    });
    let water = [(0, 2), (1, 1)].map(|(level, axis)| WaterPage {
        level,
        x: 0,
        y: 0,
        width: axis,
        height: axis,
        ocean_coverage_percent: if level == 0 {
            vec![0, 100, 0, 100]
        } else {
            vec![50]
        },
        inland_coverage_percent: vec![0; usize::from(axis).pow(2)],
    });
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: elevation
                .iter()
                .map(|page| PyramidLevel {
                    samples_per_axis: u16::from(page.width),
                    ordered_page_root: ordered_page_root(std::slice::from_ref(page)).unwrap(),
                })
                .collect(),
        },
        water: Some(FieldPyramid {
            levels: water
                .iter()
                .map(|page| PyramidLevel {
                    samples_per_axis: u16::from(page.width),
                    ordered_page_root: ordered_water_page_root(std::slice::from_ref(page)).unwrap(),
                })
                .collect(),
        }),
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 15_360,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .unwrap();
    GameWorld::from_prepared_map(
        package,
        elevation.to_vec(),
        water.to_vec(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
}

fn bank_fixture() -> (GameWorld, TileCoord, TileCoord, EntityId, EntityId) {
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let (land, water) = (20..100)
        .find_map(|y| {
            (20..world.config().width_tiles - 4).find_map(|x| {
                let land = TileCoord::new(x, y);
                let water = TileCoord::new(x + 1, y);
                if !world.tile_passable(land) {
                    return None;
                }
                let at = WorldPosition::from_tile_center(water).unwrap();
                let transport = world
                    .spawn_kind(PlayerId(0), UnitKind::TransportShip, at)
                    .ok()?;
                Some((land, (water, transport)))
            })
        })
        .expect("coast needs an adjacent passable bank and navigable water");
    let (water, transport) = water;
    let passenger = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(land).unwrap())
        .unwrap();
    (world, land, water, transport, passenger)
}

#[test]
fn unit_frames_deliver_cargo_counts_and_hide_passengers_from_owners_and_enemies() {
    let (mut world, land, water, transport, passenger) = bank_fixture();
    world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Galley,
            WorldPosition::from_tile_center(water).unwrap(),
        )
        .unwrap();
    let region = TileRect::from_xywh(land.x - 2, land.y - 2, 8, 8);
    let states = |world: &GameWorld, viewer, primary, region| {
        let frame = UnitFrame::build(world, &[PlayerId(0), PlayerId(1)], 1).unwrap();
        frame
            .current(
                world,
                &Lens {
                    viewer,
                    primary,
                    region,
                },
                &mut Scratch::default(),
            )
            .into_iter()
            .map(|slot| *frame.state(slot))
            .collect::<Vec<_>>()
    };
    assert!(
        states(&world, PlayerId(0), passenger, region)
            .iter()
            .any(|u| u.id == passenger)
    );
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    for disabled in [false, true] {
        world.set_fog_disabled(disabled);
        for viewer in [PlayerId(0), PlayerId(1)] {
            let units = states(&world, viewer, passenger, region);
            assert!(!units.iter().any(|u| u.id == passenger));
            assert_eq!(
                units
                    .iter()
                    .find(|u| u.id == transport)
                    .unwrap()
                    .cargo_count,
                1
            );
        }
        // Owned units are also sent outside the subscribed camera region.
        let units = states(
            &world,
            PlayerId(0),
            passenger,
            TileRect::from_xywh(0, 0, 4, 4),
        );
        assert!(!units.iter().any(|u| u.id == passenger));
        assert_eq!(
            units
                .iter()
                .find(|u| u.id == transport)
                .unwrap()
                .cargo_count,
            1
        );
    }
    world.issue_unload(PlayerId(0), transport, land).unwrap();
    let units = states(&world, PlayerId(0), passenger, region);
    assert!(units.iter().any(|u| u.id == passenger));
    assert_eq!(
        units
            .iter()
            .find(|u| u.id == transport)
            .unwrap()
            .cargo_count,
        0
    );
}

#[tokio::test]
async fn queued_transport_commands_apply_on_ticks_and_deliver_cargo_and_passenger_changes() {
    use crate::gameplay::GameplayService;
    use aoe_protocol::{CommandResult, GameplayServerMessage as M, GameplayUnitState};
    use tokio::sync::mpsc;

    fn receive(
        receiver: &mut mpsc::Receiver<M>,
        units: &mut BTreeMap<EntityId, GameplayUnitState>,
    ) -> Vec<(u64, CommandResult)> {
        let mut acks = Vec::new();
        while let Ok(message) = receiver.try_recv() {
            match message {
                M::Snapshot {
                    units: snapshot, ..
                } => {
                    *units = snapshot.into_iter().map(|unit| (unit.id, unit)).collect();
                }
                M::Tick {
                    changed_units,
                    removals,
                    ..
                } => {
                    for id in removals {
                        units.remove(&id);
                    }
                    units.extend(changed_units.into_iter().map(|unit| (unit.id, unit)));
                }
                M::CommandAck {
                    sequence, result, ..
                } => acks.push((sequence, result)),
                _ => {}
            }
        }
        acks
    }

    let (world, land, _, transport, passenger) = bank_fixture();
    let service = GameplayService::from_scale_world(world, passenger);
    let (sender, mut receiver) = mpsc::channel(64);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(
            session,
            1,
            TileRect::from_xywh(land.x - 2, land.y - 2, 8, 8),
        )
        .await;
    tokio::task::yield_now().await;
    let mut units = BTreeMap::new();
    assert!(receive(&mut receiver, &mut units).is_empty());
    assert!(units.contains_key(&passenger));
    assert_eq!(units[&transport].cargo_count, 0);

    service
        .queue_load(session, 1, vec![passenger], transport)
        .await;
    assert!(
        service
            .world
            .read()
            .await
            .transport_cargo(transport)
            .is_empty(),
        "orders are pending until the simulation tick"
    );
    service.tick().await;
    tokio::task::yield_now().await;
    assert_eq!(
        receive(&mut receiver, &mut units),
        [(1, CommandResult::Accepted)]
    );
    assert_eq!(units[&transport].cargo_count, 1);
    assert!(
        !units.contains_key(&passenger),
        "the authoritative load removes the passenger from delivered map units"
    );
    assert_eq!(
        service.world.read().await.transport_cargo(transport),
        &[passenger]
    );

    service.queue_unload(session, 2, transport, land).await;
    assert_eq!(
        service.world.read().await.transport_cargo(transport),
        &[passenger]
    );
    service.tick().await;
    tokio::task::yield_now().await;
    assert_eq!(
        receive(&mut receiver, &mut units),
        [(2, CommandResult::Accepted)]
    );
    assert_eq!(units[&transport].cargo_count, 0);
    assert_eq!(units[&passenger].position.tile_floor(), land);
    assert!(units[&passenger].hp > 0);
    assert!(
        service
            .world
            .read()
            .await
            .transport_cargo(transport)
            .is_empty()
    );
}
