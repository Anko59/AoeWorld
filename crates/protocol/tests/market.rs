//! Market trading wire: append-only orders, the ack reason and the prices.
use aoe_core::{EntityId, PlayerId, Tick};
use aoe_protocol::{
    Age, CommandResult, GameplayClientMessage, GameplayError, GameplayPlayerState,
    GameplayServerMessage, MARKET_MAX_PRICE, MARKET_MIN_PRICE, MarketPrices, Stockpile,
    TradeResource, decode_gameplay_client, decode_gameplay_server, encode_gameplay_client,
    encode_gameplay_server,
};

fn buy(resource: TradeResource) -> GameplayClientMessage {
    GameplayClientMessage::MarketBuy {
        sequence: 7,
        building_id: EntityId(1),
        resource,
    }
}
fn sell(resource: TradeResource) -> GameplayClientMessage {
    GameplayClientMessage::MarketSell {
        sequence: 8,
        building_id: EntityId(1),
        resource,
    }
}

#[test]
fn market_orders_have_current_golden_wire_and_round_trip() {
    for (message, bytes) in [
        (buy(TradeResource::Wood), vec![17, 7, 1, 0]),
        (buy(TradeResource::Food), vec![17, 7, 1, 1]),
        (sell(TradeResource::Stone), vec![18, 8, 1, 2]),
    ] {
        assert_eq!(encode_gameplay_client(&message).unwrap(), bytes);
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), message);
    }
    // A fourth commodity (gold is the currency) is not a message.
    assert!(decode_gameplay_client(&[17, 7, 1, 3]).is_err());
}

fn player(prices: MarketPrices) -> GameplayPlayerState {
    GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 100,
            wood: 100,
            gold: 100,
            stone: 100,
        },
        population_used: 1,
        age: Age::Dark,
        researched: 0,
        market_prices: prices,
    }
}
fn snapshot(prices: MarketPrices) -> GameplayServerMessage {
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        players: vec![player(prices)],
        buildings: vec![],
    }
}

#[test]
fn prices_have_current_golden_wire_and_round_trip() {
    let message = snapshot(MarketPrices {
        wood: 103,
        food: 97,
        stone: 20,
    });
    let bytes = encode_gameplay_server(&message).unwrap();
    assert_eq!(
        bytes,
        [
            1, 1, 0, 0, 1, 0, 100, 100, 100, 100, 1, 0, 0, 103, 97, 20, 0
        ]
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    assert_eq!(MarketPrices::default().wood, 100);
}

#[test]
fn prices_outside_the_de_clamp_are_refused() {
    for prices in [
        MarketPrices {
            wood: MARKET_MIN_PRICE - 1,
            ..MarketPrices::default()
        },
        MarketPrices {
            stone: MARKET_MAX_PRICE + 1,
            ..MarketPrices::default()
        },
        MarketPrices {
            food: 0,
            ..MarketPrices::default()
        },
    ] {
        let message = snapshot(prices);
        assert!(decode_gameplay_server(&postcard::to_allocvec(&message).unwrap()).is_err());
        assert!(matches!(
            encode_gameplay_server(&message),
            Err(GameplayError::InvalidEconomyState)
        ));
    }
    for prices in [
        MarketPrices {
            wood: MARKET_MIN_PRICE,
            ..MarketPrices::default()
        },
        MarketPrices {
            stone: MARKET_MAX_PRICE,
            ..MarketPrices::default()
        },
    ] {
        assert!(encode_gameplay_server(&snapshot(prices)).is_ok());
    }
}

#[test]
fn the_no_market_rejection_has_a_golden_wire_and_round_trips() {
    let ack = GameplayServerMessage::CommandAck {
        sequence: 3,
        result: CommandResult::RejectedNoMarket,
        applied_tick: Tick(4),
    };
    let bytes = encode_gameplay_server(&ack).unwrap();
    assert_eq!(bytes, [3, 3, 28, 4]);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), ack);
}
