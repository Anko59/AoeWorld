//! Appended enum values and larger Wheelbarrow loads pin the current wire.
use aoe_core::{EntityId, Tick, TileCoord};
use aoe_protocol::*;

#[test]
fn feudal_build_and_research_orders_and_age_rejection_have_appended_goldens() {
    let cases = [
        (BuildingKind::ArcheryRange, 10),
        (BuildingKind::Stable, 11),
        (BuildingKind::Blacksmith, 12),
        (BuildingKind::Market, 13),
        (BuildingKind::WatchTower, 14),
        (BuildingKind::PalisadeGate, 15),
    ];
    let actual: Vec<_> = cases
        .iter()
        .map(|&(kind, _)| {
            let order = GameplayClientMessage::BuildOrder {
                sequence: 1,
                builder_ids: vec![EntityId(2)],
                kind,
                origin: TileCoord::new(10, 20),
            };
            let bytes = encode_gameplay_client(&order).unwrap();
            assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);
            bytes
        })
        .collect();
    let expected: Vec<_> = cases
        .iter()
        .map(|&(_, index)| vec![8, 1, 1, 2, index, 20, 40])
        .collect();
    assert_eq!(actual, expected, "current codec bytes: {actual:?}");
    for (tech, index) in [
        (Tech::DoubleBitAxe, 3),
        (Tech::HorseCollar, 4),
        (Tech::GoldMining, 5),
        (Tech::StoneMining, 6),
        (Tech::Wheelbarrow, 7),
    ] {
        let order = GameplayClientMessage::ResearchOrder {
            sequence: 1,
            building_id: EntityId(2),
            tech,
        };
        assert_eq!(
            encode_gameplay_client(&order).unwrap(),
            vec![13, 1, 2, index]
        );
        assert_eq!(decode_gameplay_client(&[13, 1, 2, index]).unwrap(), order);
    }
    let ack = GameplayServerMessage::CommandAck {
        sequence: 1,
        result: CommandResult::RejectedRequiresFeudalAge,
        applied_tick: Tick(2),
    };
    let bytes = encode_gameplay_server(&ack).unwrap();
    assert_eq!(bytes, vec![3, 1, 28, 2]);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), ack);
}

#[test]
fn dark_players_cannot_claim_feudal_techs_and_wheelbarrow_loads_round_trip() {
    use aoe_core::{PlayerId, WorldPosition};
    let player = GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile::default(),
        population_used: 1,
        population_cap: 5,
        age: Age::Dark,
        researched: Tech::DoubleBitAxe.bit(),
        market_prices: aoe_protocol::MarketPrices::default(),
    };
    let mut snapshot = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(1),
        units: vec![],
        players: vec![player],
        buildings: vec![],
    };
    assert!(encode_gameplay_server(&snapshot).is_err());
    let GameplayServerMessage::Snapshot { units, players, .. } = &mut snapshot else {
        unreachable!()
    };
    players[0].age = Age::Feudal;
    players[0].researched |= Tech::FeudalAge.bit() | Tech::Wheelbarrow.bit();
    units.push(GameplayUnitState {
        id: EntityId(1),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task: UnitTask::Returning,
        carried: Some(CarriedResource {
            kind: ResourceKind::Wood,
            amount: 13,
        }),
        gathering: Some(ResourceKind::Wood),
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
    });
    let bytes = encode_gameplay_server(&snapshot).unwrap();
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), snapshot);
}
