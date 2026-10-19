//! Browser mocks share these current wire fragments with the Rust codec.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    GameplayBuildingState, GameplayServerMessage, GameplayUnitState, UnitKind, UnitTask,
    decode_gameplay_server, encode_gameplay_server,
};

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn snapshot(
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
    assert_eq!(encode_gameplay_server(&message).unwrap(), expected);
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
        };
        let mut expected = vec![
            1, 1, 0, 1, 7, 1, 128, 32, 128, 64, 0, 0, 2, kind as u8, 0, 0, 0,
        ];
        expected.extend(hex(fragment));
        expected.extend([0, 0]); // Players and buildings.
        assert_wire(snapshot(vec![unit], vec![]), expected);
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
    };
    let mut expected = vec![1, 1, 0, 0, 0, 1, 1, 0, 0, 10, 10, 18, 18, 0];
    expected.extend(hex(include_str!(
        "../../../browser/tests/wire/town-center.hex"
    )));
    assert_wire(snapshot(vec![], vec![building]), expected);
}
