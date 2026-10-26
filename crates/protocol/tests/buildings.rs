//! Wire rules for Dark Age buildings: kinds, construction state and bounds.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect};
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
        age: aoe_protocol::Age::Dark,
        researched: 0,
        market_prices: aoe_protocol::MarketPrices::default(),
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
        research: None,
        training_queue: vec![],
        last_hit: None,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        age: aoe_protocol::Age::Dark,
    }
}

fn farm(construction: ConstructionState, permille: u16) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(9),
        kind: BuildingKind::Farm,
        footprint: TileRect::new(TileCoord::new(20, 20), TileCoord::new(23, 23)),
        construction,
        progress_permille: permille,
        hp: 480,
        max_hp: 480,
        los: 2,
        food_remaining: Some(175),
        training_queue: vec![],
        ..building()
    }
}

#[test]
fn garrison_occupants_are_bounded_alive_sorted_and_eligible() {
    let mut center = building();
    center.garrison = vec![GarrisonUnitState {
        id: EntityId(7),
        kind: UnitKind::Villager,
        hp: 10,
        max_hp: 25,
    }];
    let snapshot = |b| GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![],
        buildings: vec![b],
    };
    let valid = snapshot(center.clone());
    let bytes = encode_gameplay_server(&valid).unwrap();
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), valid);
    // The current building tail appends the occupant list and its compact records.
    assert!(bytes.ends_with(&[1, 7, 0, 10, 25]));
    for invalid in [
        GarrisonUnitState {
            hp: 0,
            ..center.garrison[0]
        },
        GarrisonUnitState {
            hp: 26,
            ..center.garrison[0]
        },
        GarrisonUnitState {
            kind: UnitKind::Scout,
            ..center.garrison[0]
        },
        GarrisonUnitState {
            id: center.id,
            ..center.garrison[0]
        },
    ] {
        let mut b = center.clone();
        b.garrison = vec![invalid];
        assert!(encode_gameplay_server(&snapshot(b.clone())).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&snapshot(b)).unwrap()).is_err());
    }
    center.garrison = (0..21)
        .map(|id| GarrisonUnitState {
            id: EntityId(id + 2),
            kind: UnitKind::Villager,
            hp: 25,
            max_hp: 25,
        })
        .collect();
    assert!(decode_gameplay_server(&postcard::to_allocvec(&snapshot(center)).unwrap()).is_err());
}

#[test]
fn prepaid_reseeds_are_bounded_and_belong_only_to_completed_mills() {
    let message = |building| GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![player()],
        buildings: vec![building],
    };
    let mut mill = building();
    mill.kind = BuildingKind::Mill;
    mill.footprint.max = TileCoord::new(7, 7);
    mill.farm_reseeds = 99;
    let valid = message(mill.clone());
    assert_eq!(
        decode_gameplay_server(&encode_gameplay_server(&valid).unwrap()).unwrap(),
        valid
    );
    for (kind, side, state, progress, count) in [
        (
            BuildingKind::Mill,
            2,
            ConstructionState::Complete,
            1000,
            100,
        ),
        (
            BuildingKind::TownCenter,
            4,
            ConstructionState::Complete,
            1000,
            1,
        ),
        (
            BuildingKind::Mill,
            2,
            ConstructionState::UnderConstruction,
            0,
            1,
        ),
    ] {
        let mut invalid = mill.clone();
        invalid.kind = kind;
        invalid.footprint.max = TileCoord::new(5 + side, 5 + side);
        invalid.construction = state;
        invalid.progress_permille = progress;
        invalid.farm_reseeds = count;
        let invalid = message(invalid);
        assert!(encode_gameplay_server(&invalid).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    }
}

#[test]
fn every_building_kind_round_trips_with_its_construction_state() {
    let kinds = [
        BuildingKind::TownCenter,
        BuildingKind::House,
        BuildingKind::LumberCamp,
        BuildingKind::MiningCamp,
        BuildingKind::Mill,
        BuildingKind::Farm,
        BuildingKind::Barracks,
        BuildingKind::PalisadeWall,
        BuildingKind::Outpost,
    ];
    let buildings = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| GameplayBuildingState {
            id: EntityId(index as u32 + 1),
            kind: *kind,
            footprint: TileRect::new(
                TileCoord::new(10 * index as i32, 0),
                TileCoord::new(10 * index as i32 + kind.side(), kind.side()),
            ),
            construction: ConstructionState::UnderConstruction,
            progress_permille: 250,
            hp: 20,
            max_hp: 600,
            food_remaining: (*kind == BuildingKind::Farm).then_some(175),
            training_queue: vec![],
            ..building()
        })
        .collect::<Vec<_>>();
    let message = GameplayServerMessage::Snapshot {
        revision: 3,
        tick: Tick(5),
        units: vec![],
        players: vec![],
        buildings: buildings.clone(),
    };
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    let done = farm(ConstructionState::Complete, 1000);
    let message = GameplayServerMessage::Tick {
        revision: 4,
        tick: Tick(6),
        changed_units: vec![],
        removals: vec![],
        changed_players: vec![],
        changed_buildings: vec![done],
        building_removals: vec![EntityId(9)],
    };
    assert_eq!(
        decode_gameplay_server(&encode_gameplay_server(&message).unwrap()).unwrap(),
        message
    );
}

#[test]
fn inconsistent_building_state_is_rejected() {
    let mut cases = vec![
        GameplayBuildingState {
            hp: 2401,
            ..building()
        },
        GameplayBuildingState {
            progress_permille: 999,
            ..building()
        },
        GameplayBuildingState {
            max_hp: 0,
            hp: 0,
            ..building()
        },
        farm(ConstructionState::UnderConstruction, 1000),
        farm(ConstructionState::Complete, 500),
        GameplayBuildingState {
            food_remaining: None,
            ..farm(ConstructionState::Complete, 1000)
        },
        GameplayBuildingState {
            kind: BuildingKind::House,
            ..farm(ConstructionState::Complete, 1000)
        },
        // A foundation cannot train.
        GameplayBuildingState {
            construction: ConstructionState::UnderConstruction,
            progress_permille: 10,
            training_queue: vec![TrainingState {
                kind: UnitKind::Villager,
                progress_ticks: 0,
                total_ticks: 500,
            }],
            ..building()
        },
    ];
    cases.push(GameplayBuildingState {
        kind: BuildingKind::Outpost,
        footprint: TileRect::new(TileCoord::new(0, 0), TileCoord::new(2, 2)),
        training_queue: vec![],
        ..building()
    });
    for building in cases {
        let message = GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: vec![],
            players: vec![],
            buildings: vec![building],
        };
        assert!(encode_gameplay_server(&message).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
    }
    let mut player = player();
    player.population_cap = MAX_MATCH_POPULATION + 1;
    let message = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![player],
        buildings: vec![],
    };
    assert!(encode_gameplay_server(&message).is_err());
}
