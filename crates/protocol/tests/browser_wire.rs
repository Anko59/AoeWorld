//! Browser mocks share these current wire fragments with the Rust codec.
//! Building fragments include the new authoritative prepaid reseed count.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayBuildingState, GameplayPlayerState,
    GameplayServerMessage, GameplayUnitState, Stockpile, UnitKind, UnitTask,
    decode_gameplay_server, encode_gameplay_server,
};

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn combat_snapshot(
    units: Vec<GameplayUnitState>,
    buildings: Vec<GameplayBuildingState>,
) -> GameplayServerMessage {
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units,
        players: vec![],
        buildings,
    }
}

fn assert_wire(message: GameplayServerMessage, expected: Vec<u8>) {
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(bytes, expected, "current codec bytes: {bytes:?}");
    assert_eq!(decode_gameplay_server(&expected).unwrap(), message);
}

#[test]
fn browser_unit_combat_fragments_match_the_current_codec() {
    for (kind, hp, fragment) in [
        (
            UnitKind::Villager,
            25,
            include_str!("../../../browser/tests/wire/villager.hex"),
        ),
        (
            UnitKind::Scout,
            45,
            include_str!("../../../browser/tests/wire/scout.hex"),
        ),
        (
            UnitKind::FishingShip,
            50,
            include_str!("../../../browser/tests/wire/fishing-ship.hex"),
        ),
        (
            UnitKind::Militia,
            40,
            include_str!("../../../browser/tests/wire/militia.hex"),
        ),
    ] {
        let unit = GameplayUnitState {
            id: EntityId(7),
            player: PlayerId(1),
            position: WorldPosition::new(2048, 4096),
            moving: false,
            planning: false,
            facing: 2,
            kind,
            task: UnitTask::Idle,
            carried: None,
            gathering: None,
            hp,
            max_hp: hp,
            attack_target: None,
            last_hit: None,
            death_tick: None,
            hunt_target: None,
            death_hunt: None,
            auto_scout: false,
        };
        let mut expected = vec![
            1, 1, 0, 1, 7, 1, 128, 32, 128, 64, 0, 0, 2, kind as u8, 0, 0, 0,
        ];
        expected.extend(hex(fragment));
        expected.extend([0, 0]); // Players and buildings.
        assert_wire(combat_snapshot(vec![unit], vec![]), expected);
    }
}

#[test]
fn browser_building_combat_fragment_matches_the_current_codec() {
    let building = GameplayBuildingState {
        id: EntityId(1),
        player: PlayerId(0),
        kind: aoe_protocol::BuildingKind::TownCenter,
        footprint: TileRect::new(TileCoord::new(5, 5), TileCoord::new(9, 9)),
        training_queue: vec![],
        hp: 2400,
        max_hp: 2400,
        last_hit: None,
        age: aoe_protocol::Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        los: 8,
        food_remaining: None,
        research: None,
    };
    let mut expected = vec![1, 1, 0, 0, 0, 1, 1, 0, 0, 10, 10, 18, 18];
    expected.extend(hex(include_str!(
        "../../../browser/tests/wire/town-center.hex"
    )));
    assert_wire(combat_snapshot(vec![], vec![building]), expected);
}

#[test]
fn browser_dock_fragment_matches_current_codec() {
    let building = GameplayBuildingState {
        id: EntityId(1),
        player: PlayerId(0),
        kind: BuildingKind::Dock,
        age: aoe_protocol::Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        footprint: TileRect::new(TileCoord::new(5, 5), TileCoord::new(8, 8)),
        training_queue: vec![],
        hp: 1800,
        max_hp: 1800,
        last_hit: None,
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        los: 8,
        food_remaining: None,
        research: None,
    };
    let mut expected = vec![1, 1, 0, 0, 0, 1, 1, 0, 9, 10, 10, 16, 16];
    expected.extend(hex(include_str!("../../../browser/tests/wire/dock.hex")));
    assert_wire(combat_snapshot(vec![], vec![building]), expected);
}

fn snapshot() -> GameplayServerMessage {
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![GameplayPlayerState {
            player: PlayerId(0),
            stockpile: Stockpile {
                food: 200,
                wood: 200,
                gold: 100,
                stone: 200,
            },
            population_used: 6,
            population_cap: 10,
            age: aoe_protocol::Age::Dark,
            researched: 0,
            market_prices: aoe_protocol::MarketPrices::default(),
        }],
        buildings: vec![GameplayBuildingState {
            id: EntityId(1),
            player: PlayerId(0),
            kind: BuildingKind::TownCenter,
            footprint: TileRect::new(TileCoord::new(8190, 8190), TileCoord::new(8194, 8194)),
            construction: ConstructionState::Complete,
            progress_permille: 1000,
            hp: 2400,
            max_hp: 2400,
            los: 8,
            food_remaining: None,
            research: None,
            training_queue: vec![],
            last_hit: None,
            age: aoe_protocol::Age::Dark,
            gate_open: false,
            rally_point: None,
            farm_reseeds: 0,
        }],
    }
}

fn browser_bytes() -> Vec<u8> {
    let mut bytes = vec![
        1, 1, 0, 0, // Snapshot, revision, tick, units.
        1, 0, 200, 1, 200, 1, 100, 200, 1, 6, 10, // Player and stockpile.
    ];
    bytes.extend(hex(include_str!("../../../browser/tests/wire/player.hex")));
    bytes.extend(hex(include_str!(
        "../../../browser/tests/wire/market-prices.hex"
    ))); // Market prices: 100 gold per 100 units each.
    bytes.extend([
        1, 1, 0, 0, // Building count, id, player, TownCenter.
        252, 127, 252, 127, 132, 128, 1, 132, 128, 1, // Footprint.
    ]);
    bytes.extend(
        include_str!("../../../browser/tests/wire/town-center.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap()),
    );
    bytes
}

#[test]
fn browser_building_fixture_matches_current_encoder() {
    assert_eq!(
        encode_gameplay_server(&snapshot()).unwrap(),
        browser_bytes()
    );
}

#[test]
fn browser_building_fixture_matches_current_decoder() {
    assert_eq!(
        decode_gameplay_server(&browser_bytes()).unwrap(),
        snapshot()
    );
}

#[test]
fn browser_feudal_player_fragment_matches_the_current_codec() {
    let mut message = snapshot();
    let GameplayServerMessage::Snapshot { players, .. } = &mut message else {
        unreachable!()
    };
    players[0].age = aoe_protocol::Age::Feudal;
    players[0].researched = aoe_protocol::Tech::FeudalAge.bit();
    for wheelbarrow in [false, true] {
        let mut message = message.clone();
        if wheelbarrow {
            let GameplayServerMessage::Snapshot { players, .. } = &mut message else {
                unreachable!()
            };
            players[0].researched |= aoe_protocol::Tech::Wheelbarrow.bit();
        }
        let mut expected = browser_bytes();
        let fragment = hex(if wheelbarrow {
            include_str!("../../../browser/tests/wire/wheelbarrow-player.hex")
        } else {
            include_str!("../../../browser/tests/wire/feudal-player.hex")
        });
        expected.splice(15..17, fragment); // Age and researched only.
        assert_wire(message, expected);
    }
}

#[test]
fn browser_wildlife_fixture_matches_the_current_codec() {
    let message = GameplayServerMessage::WildlifeState {
        revision: 300,
        tick: Tick(0),
        animals: vec![aoe_protocol::AnimalState {
            id: EntityId(7),
            kind: aoe_protocol::AnimalKind::Sheep,
            owner: Some(PlayerId(0)),
            hp: 7,
            position: WorldPosition::from_tile_center(TileCoord::new(8194, 8190)).unwrap(),
            task: aoe_protocol::AnimalTask::Idle,
            food_remaining: 100,
            target: None,
            facing: 0,
            moving: false,
            death_tick: None,
        }],
    };
    let mut expected = vec![8, 172, 2, 0, 1];
    expected.extend(hex(include_str!("../../../browser/tests/wire/sheep.hex")));
    assert_wire(message, expected);
}
