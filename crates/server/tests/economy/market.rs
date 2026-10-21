//! Market trading over the real gameplay service and the wire codec: a Dark
//! Age or Town Center trade is refused, then a completed Market sells food
//! and buys wood, moving only the traded price.
use super::*;
use aoe_protocol::{
    BuildingKind, MarketPrices, Tech, TradeResource, decode_gameplay_server, encode_gameplay_server,
};

fn acks(rx: &mut mpsc::Receiver<GameplayServerMessage>) -> Vec<(u64, CommandResult)> {
    drain(rx)
        .into_iter()
        .filter_map(|m| {
            match decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap() {
                GameplayServerMessage::CommandAck {
                    sequence, result, ..
                } => Some((sequence, result)),
                _ => None,
            }
        })
        .collect()
}

#[tokio::test]
async fn a_completed_market_trades_and_moves_prices_over_the_wire() {
    let service = GameplayService::feudal_demo(Seed(1)).unwrap();
    let (tx, mut rx) = mpsc::channel(256);
    let (session, _) = service.register(None, tx).await;
    service
        .subscribe(
            session,
            1,
            TileRect::new(TileCoord::new(0, 0), TileCoord::new(256, 256)),
        )
        .await;
    let GameplayServerMessage::Snapshot {
        units,
        players,
        buildings,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("snapshot")
    };
    assert_eq!(players[0].market_prices, MarketPrices::default());
    let center = buildings
        .iter()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let worker = units
        .iter()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    // A Town Center is not a Market.
    service
        .queue_market_sell(session, 1, center, TradeResource::Food)
        .await;
    service.tick().await;
    assert_eq!(acks(&mut rx), [(1, CommandResult::RejectedNoMarket)]);

    service
        .queue_research(session, 2, center, Tech::FeudalAge)
        .await;
    for _ in 0..130 {
        service.tick().await;
    }
    drain(&mut rx);
    service
        .queue_build(
            session,
            3,
            vec![worker],
            BuildingKind::Market,
            TileCoord::new(138, 136),
        )
        .await;
    let mut market = None;
    for _ in 0..200 {
        service.tick().await;
        for m in drain(&mut rx) {
            if let GameplayServerMessage::Tick {
                changed_buildings, ..
            } = decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap()
            {
                market = market.or(changed_buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::Market && b.progress_permille == 1000)
                    .map(|b| b.id));
            }
        }
        if market.is_some() {
            break;
        }
    }
    let market = market.expect("the Market completes");

    service
        .queue_market_sell(session, 4, market, TradeResource::Food)
        .await;
    service
        .queue_market_buy(session, 5, market, TradeResource::Wood)
        .await;
    service.tick().await;
    let mut accepted = Vec::new();
    let mut prices = None;
    for m in drain(&mut rx) {
        match decode_gameplay_server(&encode_gameplay_server(&m).unwrap()).unwrap() {
            GameplayServerMessage::CommandAck {
                sequence, result, ..
            } => accepted.push((sequence, result)),
            GameplayServerMessage::Tick {
                changed_players, ..
            } => prices = changed_players.first().map(|p| p.market_prices),
            _ => {}
        }
    }
    assert_eq!(
        accepted,
        [(4, CommandResult::Accepted), (5, CommandResult::Accepted)]
    );
    // Food sold: 100 -> 97; wood bought: 100 -> 103; stone untouched.
    assert_eq!(
        prices,
        Some(MarketPrices {
            wood: 103,
            food: 97,
            stone: 100
        })
    );
}
