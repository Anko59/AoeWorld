use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::*;

fn player() -> GameplayPlayerState {
    GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 200,
            wood: 200,
            gold: 100,
            stone: 200,
        },
        population_used: 4,
        population_cap: 5,
    }
}
fn building() -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(1),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::new(TileCoord::new(5, 5), TileCoord::new(9, 9)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 2400,
        max_hp: 2400,
        los: 8,
        food_remaining: None,
        training_queue: vec![TrainingState {
            kind: UnitKind::Villager,
            progress_ticks: 3,
            total_ticks: 500,
        }],
        last_hit: None,
    }
}
fn unit() -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(2),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 1024),
        moving: true,
        planning: false,
        facing: 3,
        kind: UnitKind::Villager,
        task: UnitTask::Returning,
        gathering: Some(ResourceKind::Wood),
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        carried: Some(CarriedResource {
            kind: ResourceKind::Wood,
            amount: 10,
        }),
    }
}
fn snapshot() -> GameplayServerMessage {
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(3),
        units: vec![unit()],
        players: vec![player()],
        buildings: vec![building()],
    }
}

#[test]
fn economy_commands_have_current_golden_wire_and_roundtrip() {
    for (message, bytes) in [
        (
            GameplayClientMessage::CancelTrainingOrder {
                sequence: 8,
                building_id: EntityId(5),
            },
            vec![6, 8, 5],
        ),
        (
            GameplayClientMessage::BuildOrder {
                sequence: 8,
                builder_ids: vec![EntityId(3), EntityId(4)],
                kind: BuildingKind::House,
                origin: TileCoord::new(5, 6),
            },
            vec![8, 8, 2, 3, 4, 1, 10, 12],
        ),
        (
            GameplayClientMessage::BuildWallOrder {
                sequence: 8,
                builder_ids: vec![EntityId(3)],
                from: TileCoord::new(5, 6),
                to: TileCoord::new(5, 9),
            },
            vec![9, 8, 1, 3, 10, 12, 10, 18],
        ),
        (
            GameplayClientMessage::CancelConstructionOrder {
                sequence: 8,
                building_id: EntityId(5),
            },
            vec![10, 8, 5],
        ),
        (
            GameplayClientMessage::FarmOrder {
                sequence: 9,
                entity_id: EntityId(3),
                farm_id: EntityId(7),
            },
            vec![11, 9, 3, 7],
        ),
        (
            GameplayClientMessage::ConstructOrder {
                sequence: 8,
                builder_ids: vec![EntityId(3)],
                building_id: EntityId(7),
            },
            vec![12, 8, 1, 3, 7],
        ),
        (
            GameplayClientMessage::GatherOrder {
                sequence: 9,
                entity_id: EntityId(3),
                resource_id: 2,
            },
            vec![4, 9, 3, 2],
        ),
        (
            GameplayClientMessage::TrainOrder {
                sequence: 9,
                building_id: EntityId(3),
                kind: UnitKind::Villager,
            },
            vec![5, 9, 3, 0],
        ),
    ] {
        assert_eq!(encode_gameplay_client(&message).unwrap(), bytes);
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), message);
    }
    assert!(decode_gameplay_client(&[5, 9, 3, 3]).is_err());
    assert!(decode_gameplay_client(&[4, 9, 3]).is_err());
    // A build order may name at most MAX_BUILD_GROUP villagers.
    let crowd = GameplayClientMessage::BuildOrder {
        sequence: 1,
        builder_ids: (0..=MAX_BUILD_GROUP as u32).map(EntityId).collect(),
        kind: BuildingKind::House,
        origin: TileCoord::new(1, 1),
    };
    assert!(decode_gameplay_client(&postcard::to_allocvec(&crowd).unwrap()).is_err());
}

#[test]
fn full_economy_snapshot_and_delta_roundtrip() {
    let snapshot = snapshot();
    let bytes = encode_gameplay_server(&snapshot).unwrap();
    assert_eq!(
        bytes,
        vec![
            1, 1, 3, 1, 2, 0, 128, 16, 128, 16, 1, 0, 3, 0, 4, 1, 1, 10, 1, 1, 25, 25, 0, 0, 1, 0,
            200, 1, 200, 1, 100, 200, 1, 4, 5, 1, 1, 0, 0, 10, 10, 18, 18, 1, 232, 7, 224, 18, 224,
            18, 8, 0, 1, 0, 3, 244, 3, 0
        ]
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), snapshot);
    let delta = GameplayServerMessage::Tick {
        revision: 7,
        tick: Tick(99),
        changed_units: vec![unit()],
        removals: vec![],
        changed_players: vec![player()],
        changed_buildings: vec![building()],
        building_removals: vec![],
    };
    assert_eq!(
        decode_gameplay_server(&encode_gameplay_server(&delta).unwrap()).unwrap(),
        delta
    );
    for result in [
        CommandResult::RejectedNotOwner,
        CommandResult::RejectedWrongUnitKind,
        CommandResult::RejectedInvalidResource,
        CommandResult::RejectedInsufficientFood,
        CommandResult::RejectedPopulationCap,
        CommandResult::RejectedTrainingQueueFull,
    ] {
        let ack = GameplayServerMessage::CommandAck {
            sequence: 1,
            result,
            applied_tick: Tick(1),
        };
        assert_eq!(
            decode_gameplay_server(&encode_gameplay_server(&ack).unwrap()).unwrap(),
            ack
        );
    }
}

#[test]
fn oversized_players_buildings_and_training_queues_fail_encode_and_decode() {
    for message in [
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![player(); MAX_ECONOMY_PLAYERS + 1],
            buildings: vec![],
        },
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![],
            buildings: vec![building(); MAX_ECONOMY_BUILDINGS + 1],
        },
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![],
            buildings: vec![GameplayBuildingState {
                training_queue: vec![
                    TrainingState {
                        kind: UnitKind::Villager,
                        progress_ticks: 0,
                        total_ticks: 500
                    };
                    MAX_TRAINING_QUEUE + 1
                ],
                ..building()
            }],
        },
    ] {
        assert!(encode_gameplay_server(&message).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
    }
}

#[test]
fn invalid_load_progress_footprint_and_population_are_rejected() {
    let mut load = unit();
    load.carried.as_mut().unwrap().amount = 11;
    let mut progress = building();
    progress.training_queue[0].progress_ticks = 501;
    let mut footprint = building();
    footprint.footprint.max.x -= 1;
    let mut population = player();
    population.population_cap = MAX_POPULATION + 1;
    for message in [
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![load],
            players: vec![],
            buildings: vec![],
        },
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![],
            buildings: vec![progress],
        },
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![],
            buildings: vec![footprint],
        },
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![population],
            buildings: vec![],
        },
    ] {
        assert!(encode_gameplay_server(&message).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
    }
}

#[test]
fn maximum_combined_frame_fits_and_rejects_one_more_building_removal() {
    let message = GameplayServerMessage::Tick {
        revision: u64::MAX,
        tick: Tick(u64::MAX),
        changed_units: vec![unit(); MAX_SUBSCRIBED_UNITS],
        removals: vec![EntityId(u32::MAX); MAX_SUBSCRIBED_UNITS],
        changed_players: (0..MAX_ECONOMY_PLAYERS)
            .map(|i| GameplayPlayerState {
                player: PlayerId(i as u16),
                ..player()
            })
            .collect(),
        changed_buildings: (0..MAX_ECONOMY_BUILDINGS)
            .map(|i| GameplayBuildingState {
                id: EntityId(i as u32),
                training_queue: vec![
                    TrainingState {
                        kind: UnitKind::Villager,
                        progress_ticks: 1,
                        total_ticks: 500
                    };
                    MAX_TRAINING_QUEUE
                ],
                ..building()
            })
            .collect(),
        building_removals: vec![EntityId(u32::MAX); MAX_ECONOMY_BUILDINGS],
    };
    let bytes = encode_gameplay_server(&message).unwrap();
    assert!(bytes.len() <= GAMEPLAY_MAX_MESSAGE);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    let GameplayServerMessage::Tick {
        revision,
        tick,
        changed_units,
        removals,
        changed_players,
        changed_buildings,
        mut building_removals,
    } = message
    else {
        unreachable!()
    };
    building_removals.push(EntityId(0));
    let oversized = GameplayServerMessage::Tick {
        revision,
        tick,
        changed_units,
        removals,
        changed_players,
        changed_buildings,
        building_removals,
    };
    assert!(encode_gameplay_server(&oversized).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&oversized).unwrap()).is_err());
}

#[test]
fn zero_load_gatherer_kind_roundtrips_without_terrain_or_inventory() {
    for kind in [
        ResourceKind::Food,
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let mut worker = unit();
        worker.task = UnitTask::Gathering;
        worker.carried = None;
        worker.gathering = Some(kind);
        let message = GameplayServerMessage::Snapshot {
            revision: 3,
            tick: Tick(20),
            units: vec![worker],
            players: vec![],
            buildings: vec![],
        };
        assert_eq!(
            decode_gameplay_server(&encode_gameplay_server(&message).unwrap()).unwrap(),
            message
        );
    }
    let mut scout = unit();
    scout.kind = UnitKind::Scout;
    scout.carried = None;
    let message = GameplayServerMessage::Snapshot {
        revision: 3,
        tick: Tick(20),
        units: vec![scout],
        players: vec![],
        buildings: vec![],
    };
    assert!(encode_gameplay_server(&message).is_err());
}
