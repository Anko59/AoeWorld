//! Research wire state: the research orders, the player's age and researched
//! bitset, and a building's research progress.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect};
use aoe_protocol::{
    Age, BuildingKind, CommandResult, ConstructionState, GameplayBuildingState,
    GameplayClientMessage, GameplayError, GameplayPlayerState, GameplayServerMessage,
    ResearchState, Stockpile, Tech, TrainingState, UnitKind, decode_gameplay_client,
    decode_gameplay_server, encode_gameplay_client, encode_gameplay_server,
};

fn player(age: Age, researched: u32) -> GameplayPlayerState {
    GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile::default(),
        population_used: 4,
        population_cap: 5,
        age,
        researched,
    }
}

fn center(research: Option<ResearchState>) -> GameplayBuildingState {
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
        training_queue: vec![],
        last_hit: None,
        age: aoe_protocol::Age::Dark,
        research,
    }
}

fn loom(progress_ticks: u64, total_ticks: u64) -> Option<ResearchState> {
    Some(ResearchState {
        tech: Tech::Loom,
        progress_ticks,
        total_ticks,
    })
}

fn tick_with(
    players: Vec<GameplayPlayerState>,
    buildings: Vec<GameplayBuildingState>,
) -> GameplayServerMessage {
    GameplayServerMessage::Tick {
        revision: 2,
        tick: Tick(9),
        changed_units: vec![],
        removals: vec![],
        changed_players: players,
        changed_buildings: buildings,
        building_removals: vec![],
    }
}

fn refused(message: GameplayServerMessage) {
    assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
    assert!(matches!(
        encode_gameplay_server(&message),
        Err(GameplayError::InvalidEconomyState)
    ));
}

#[test]
fn research_orders_have_current_golden_wire_and_round_trip() {
    for (message, bytes) in [
        (
            GameplayClientMessage::ResearchOrder {
                sequence: 7,
                building_id: EntityId(1),
                tech: Tech::Loom,
            },
            vec![13, 7, 1, 0],
        ),
        (
            GameplayClientMessage::ResearchOrder {
                sequence: 8,
                building_id: EntityId(1),
                tech: Tech::FeudalAge,
            },
            vec![13, 8, 1, 2],
        ),
        (
            GameplayClientMessage::CancelResearch {
                sequence: 9,
                building_id: EntityId(1),
            },
            vec![14, 9, 1],
        ),
    ] {
        assert_eq!(encode_gameplay_client(&message).unwrap(), bytes);
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), message);
    }
    // An unknown technology is not a message.
    assert!(decode_gameplay_client(&[13, 7, 1, 8]).is_err());
}

#[test]
fn research_state_has_current_golden_wire_and_round_trips() {
    let message = tick_with(
        vec![player(
            Age::Feudal,
            Tech::FeudalAge.bit() | Tech::Loom.bit(),
        )],
        vec![center(loom(125, 500))],
    );
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    assert_eq!(bytes, GOLDEN_TICK);
}

const GOLDEN_TICK: &[u8] = &[
    2, 2, 9, 0, 0, 1, 0, 0, 0, 0, 0, 4, 5, 1, 5, 1, 1, 0, 0, 10, 10, 18, 18, 1, 232, 7, 224, 18,
    224, 18, 8, 0, 0, 1, 0, 125, 244, 3, 0, 0, 0,
];

#[test]
fn the_new_rejections_round_trip() {
    for result in [
        CommandResult::RejectedNotResearchable,
        CommandResult::RejectedAlreadyResearched,
        CommandResult::RejectedResearchBusy,
        CommandResult::RejectedTechRequirement,
    ] {
        let ack = GameplayServerMessage::CommandAck {
            sequence: 3,
            result,
            applied_tick: Tick(4),
        };
        let bytes = encode_gameplay_server(&ack).unwrap();
        assert_eq!(decode_gameplay_server(&bytes).unwrap(), ack);
    }
}

#[test]
fn inconsistent_research_state_is_refused() {
    refused(tick_with(vec![player(Age::Feudal, 0)], vec![]));
    refused(tick_with(
        vec![player(Age::Dark, Tech::FeudalAge.bit())],
        vec![],
    ));
    refused(tick_with(vec![player(Age::Dark, 1 << 8)], vec![]));
    refused(tick_with(vec![], vec![center(loom(501, 500))]));
    refused(tick_with(vec![], vec![center(loom(0, 0))]));
    let mut foundation = center(loom(1, 500));
    foundation.construction = ConstructionState::UnderConstruction;
    foundation.progress_permille = 10;
    foundation.hp = 1;
    refused(tick_with(vec![], vec![foundation]));
    let mut busy = center(loom(1, 500));
    busy.training_queue = vec![TrainingState {
        kind: UnitKind::Villager,
        progress_ticks: 1,
        total_ticks: 500,
    }];
    refused(tick_with(vec![], vec![busy]));
}

#[test]
fn research_and_militia_queues_require_their_production_buildings() {
    let mut house = center(loom(1, 500));
    house.kind = BuildingKind::House;
    house.footprint = TileRect::new(TileCoord::new(5, 5), TileCoord::new(7, 7));
    refused(tick_with(vec![], vec![house]));
    let mut barracks = center(None);
    barracks.kind = BuildingKind::Barracks;
    barracks.footprint = TileRect::new(TileCoord::new(5, 5), TileCoord::new(8, 8));
    barracks.training_queue = vec![TrainingState {
        kind: UnitKind::Militia,
        progress_ticks: 1,
        total_ticks: 420,
    }];
    let message = tick_with(vec![], vec![barracks.clone()]);
    assert_eq!(
        decode_gameplay_server(&encode_gameplay_server(&message).unwrap()).unwrap(),
        message
    );
    barracks.kind = BuildingKind::TownCenter;
    barracks.footprint.max = TileCoord::new(9, 9);
    refused(tick_with(vec![], vec![barracks]));
}
