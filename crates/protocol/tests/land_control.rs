use aoe_core::PlayerId;
use aoe_protocol::HoldingSpan;
#[test]
fn holding_span_has_an_appended_stable_wire_layout() {
    let span = HoldingSpan {
        player: PlayerId(3),
        y: 5,
        x0: 7,
        x1: 9,
    };
    assert_eq!(postcard::to_allocvec(&span).unwrap(), [3, 10, 14, 18]);
    assert_eq!(
        postcard::from_bytes::<HoldingSpan>(&[3, 10, 14, 18]).unwrap(),
        span
    );
}

fn report() -> aoe_protocol::GameplayServerMessage {
    use aoe_protocol::*;
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: aoe_core::Tick(0),
        units: vec![],
        buildings: vec![],
        players: vec![GameplayPlayerState {
            player: PlayerId(0),
            stockpile: Stockpile::default(),
            population_used: 0,
            population_cap: 0,
            age: Age::Dark,
            researched: 0,
            market_prices: MarketPrices::default(),
            recent_deliveries: [0; 4],
            unit_counts: [0; UnitKind::ALL.len()],
            held_tiles: 10,
            held_share_ppm: 100,
            holdings: vec![HoldingSpan {
                player: PlayerId(3),
                y: 5,
                x0: 7,
                x1: 9,
            }],
            holdings_complete: true,
        }],
    }
}
#[test]
fn populated_land_fields_are_appended_and_invalid_or_unbounded_runs_are_refused() {
    use aoe_protocol::*;
    let message = report();
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 9..],
        &[10, 100, 1, 3, 10, 14, 18, 1, 0][..]
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    let GameplayServerMessage::Snapshot { mut players, .. } = report() else {
        unreachable!()
    };
    players[0].holdings[0].x1 = 7;
    let invalid = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: aoe_core::Tick(0),
        units: vec![],
        buildings: vec![],
        players: players.clone(),
    };
    assert!(encode_gameplay_server(&invalid).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    players[0].holdings = vec![
        HoldingSpan {
            player: PlayerId(0),
            y: 0,
            x0: 0,
            x1: 1
        };
        MAX_HOLDING_SPANS + 1
    ];
    let oversized = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: aoe_core::Tick(0),
        units: vec![],
        buildings: vec![],
        players,
    };
    assert!(encode_gameplay_server(&oversized).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&oversized).unwrap()).is_err());
}
