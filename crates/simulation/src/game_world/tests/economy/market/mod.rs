//! Market trading regressions pinned to the DE constants cited above.
use super::buildings::{set_stock, stock};
use super::*;
use crate::{MARKET_RULES, MarketPrices};

fn market(world: &mut GameWorld, x: i32) -> EntityId {
    let rect = TileRect::new(
        TileCoord::new(x, 80),
        TileCoord::new(x + BuildingKind::Market.rules().side, 84),
    );
    world
        .insert_building(PlayerId(0), BuildingKind::Market, rect, true)
        .unwrap()
}

fn prices(world: &GameWorld) -> MarketPrices {
    world.player_economy(PlayerId(0)).unwrap().market_prices
}

#[test]
fn village_players_start_at_the_de_base_price() {
    let (world, _, _) = village();
    assert_eq!(
        prices(&world),
        MarketPrices {
            wood: 100,
            food: 100,
            stone: 100
        }
    );
}

#[test]
fn buying_charges_the_commission_then_raises_the_price() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 0, 0, 263, 0);
    world
        .issue_market_buy(PlayerId(0), id, ResourceKind::Food)
        .unwrap();
    let (food, wood, gold, stone) = stock(&world);
    assert_eq!((food, wood, gold, stone), (100, 0, 133, 0));
    assert_eq!(prices(&world).food, 103);
    // The next lot costs the new price plus commission: 103 * 1.3 = 133 (floor).
    world
        .issue_market_buy(PlayerId(0), id, ResourceKind::Food)
        .unwrap();
    let (food, _, gold, _) = stock(&world);
    assert_eq!((food, gold), (200, 0));
    assert_eq!(prices(&world).food, 106);
}

#[test]
fn selling_pays_the_price_minus_the_commission_then_lowers_it() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 0, 300, 0, 0);
    world
        .issue_market_sell(PlayerId(0), id, ResourceKind::Wood)
        .unwrap();
    let (food, wood, gold, stone) = stock(&world);
    assert_eq!((food, wood, gold, stone), (0, 200, 70, 0));
    assert_eq!(prices(&world).wood, 97);
    // 97 * 0.7 = 67 for the next lot.
    world
        .issue_market_sell(PlayerId(0), id, ResourceKind::Wood)
        .unwrap();
    let (_, wood, gold, _) = stock(&world);
    assert_eq!((wood, gold), (100, 137));
    assert_eq!(prices(&world).wood, 94);
}

#[test]
fn prices_never_leave_the_de_clamp() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 0, u32::MAX / 2, u32::MAX / 2, 0);
    let sells = (MARKET_RULES.base_price - MARKET_RULES.min_price) / MARKET_RULES.price_step + 2;
    for _ in 0..sells {
        world
            .issue_market_sell(PlayerId(0), id, ResourceKind::Wood)
            .unwrap();
    }
    assert_eq!(prices(&world).wood, MARKET_RULES.min_price);
    let buys = (MARKET_RULES.max_price - MARKET_RULES.min_price) / MARKET_RULES.price_step + 2;
    for _ in 0..buys {
        world
            .issue_market_buy(PlayerId(0), id, ResourceKind::Stone)
            .unwrap();
    }
    assert_eq!(prices(&world).stone, MARKET_RULES.max_price);
}

#[test]
fn trades_require_an_owned_completed_market() {
    let (mut world, _, _) = village();
    assert_eq!(
        world.issue_market_buy(PlayerId(0), EntityId(9999), ResourceKind::Food),
        Err(EconomyError::UnknownEntity)
    );
    let rect = TileRect::new(TileCoord::new(100, 80), TileCoord::new(104, 84));
    let foreign = world
        .insert_building(PlayerId(1), BuildingKind::Market, rect, true)
        .unwrap();
    assert_eq!(
        world.issue_market_sell(PlayerId(0), foreign, ResourceKind::Food),
        Err(EconomyError::NotOwner)
    );
    let rect = TileRect::new(TileCoord::new(110, 80), TileCoord::new(114, 84));
    let foundation = world
        .insert_building(PlayerId(0), BuildingKind::Market, rect, false)
        .unwrap();
    assert_eq!(
        world.issue_market_buy(PlayerId(0), foundation, ResourceKind::Food),
        Err(EconomyError::UnderConstruction)
    );
    assert_eq!(
        world.issue_market_sell(PlayerId(0), foundation, ResourceKind::Food),
        Err(EconomyError::UnderConstruction)
    );
    let rect = TileRect::new(TileCoord::new(120, 80), TileCoord::new(122, 82));
    let house = world
        .insert_building(PlayerId(0), BuildingKind::House, rect, true)
        .unwrap();
    assert_eq!(
        world.issue_market_buy(PlayerId(0), house, ResourceKind::Food),
        Err(EconomyError::NoMarket)
    );
    let id = market(&mut world, 100);
    assert_eq!(
        world.issue_market_buy(PlayerId(0), id, ResourceKind::Gold),
        Err(EconomyError::InvalidResource)
    );
    assert_eq!(
        world.issue_market_sell(PlayerId(0), id, ResourceKind::Gold),
        Err(EconomyError::InvalidResource)
    );
}

#[test]
fn unaffordable_trades_are_refused_without_touching_state() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 5_000, 0, 129, 0);
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_market_buy(PlayerId(0), id, ResourceKind::Food),
        Err(EconomyError::InsufficientResources)
    );
    let (prices, s) = (prices(&world), world.player_economy(PlayerId(0)).unwrap());
    assert_eq!(
        (prices.food, s.stockpile.food, s.stockpile.gold),
        (100, 5_000, 129)
    );
    assert_eq!(world.canonical_hash(), before);
    assert_eq!(
        world.issue_market_sell(PlayerId(0), id, ResourceKind::Wood),
        Err(EconomyError::InsufficientResources)
    );
    assert_eq!(world.canonical_hash(), before);
    // Exactly 130 gold buys exactly one lot.
    set_stock(&mut world, 5_000, 0, 130, 0);
    world
        .issue_market_buy(PlayerId(0), id, ResourceKind::Food)
        .unwrap();
    let (food, _, gold, _) = stock(&world);
    assert_eq!((food, gold), (5_100, 0));
}

#[test]
fn trading_is_deterministic_and_prices_reach_the_replay_hash() {
    fn traded() -> GameWorld {
        let (mut world, _, _) = village();
        let id = market(&mut world, 100);
        set_stock(&mut world, 4_000, 3_000, 900, 2_000);
        world
            .issue_market_buy(PlayerId(0), id, ResourceKind::Food)
            .unwrap();
        world
            .issue_market_sell(PlayerId(0), id, ResourceKind::Wood)
            .unwrap();
        world.advance();
        world
    }
    let a = traded();
    let b = traded();
    assert_eq!(a.canonical_hash(), b.canonical_hash());
    assert_eq!(prices(&a).food, 103);
    assert_eq!(prices(&a).wood, 97);
    // The player block of the hash covers the prices: resetting one by hand
    // changes it.
    let mut c = traded();
    if let Some(player) = c.economy.players.get_mut(&PlayerId(0)) {
        player.market_prices.food = 100;
    }
    assert_ne!(c.canonical_hash(), a.canonical_hash());
}

#[test]
fn only_trading_moves_prices() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    let before = prices(&world);
    world.advance();
    let rect = TileRect::new(TileCoord::new(106, 80), TileCoord::new(107, 82));
    world
        .insert_building(PlayerId(0), BuildingKind::House, rect, true)
        .unwrap();
    assert_eq!(prices(&world), before);
    set_stock(&mut world, 0, 0, 300, 0);
    world
        .issue_market_buy(PlayerId(0), id, ResourceKind::Stone)
        .unwrap();
    assert_eq!(prices(&world).stone, 103);
    assert_eq!((before.stone, before.food, before.wood), (100, 100, 100));
}

#[test]
fn a_bulk_sale_is_exactly_the_same_lots_one_after_another() {
    let (mut single, _, _) = village();
    let (mut bulk, _, _) = village();
    let a = market(&mut single, 100);
    let b = market(&mut bulk, 100);
    set_stock(&mut single, 90_000, 0, 0, 0);
    set_stock(&mut bulk, 90_000, 0, 0, 0);
    for _ in 0..900 {
        single
            .issue_market_sell(PlayerId(0), a, ResourceKind::Food)
            .unwrap();
    }
    assert_eq!(
        bulk.issue_market_trade(PlayerId(0), b, ResourceKind::Food, false, 900),
        Ok(900)
    );
    assert_eq!(stock(&single), stock(&bulk));
    assert_eq!(prices(&single), prices(&bulk));
    // 900 lots walk the price to its 20-gold floor, so most lots pay 14.
    assert_eq!(prices(&bulk).food, MARKET_RULES.min_price);
}

#[test]
fn a_bulk_order_stops_where_the_stockpile_ends_and_keeps_what_it_did() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 0, 250, 0, 0);
    // Only two whole lots exist: the order sells those and reports two.
    assert_eq!(
        world.issue_market_trade(PlayerId(0), id, ResourceKind::Wood, false, 20_000),
        Ok(2)
    );
    assert_eq!(stock(&world).1, 50);
    assert_eq!(prices(&world).wood, 94);
    // No whole lot left: nothing changes and the order is refused.
    assert_eq!(
        world.issue_market_trade(PlayerId(0), id, ResourceKind::Wood, false, 5),
        Err(EconomyError::InsufficientResources)
    );
}

#[test]
fn a_bulk_purchase_spends_gold_lot_by_lot_at_rising_prices() {
    let (mut world, _, _) = village();
    let id = market(&mut world, 100);
    set_stock(&mut world, 0, 0, 266, 0);
    // 130 + 133 = 263 gold buys two lots; a third at 136 does not fit.
    assert_eq!(
        world.issue_market_trade(PlayerId(0), id, ResourceKind::Stone, true, 10),
        Ok(2)
    );
    let (_, _, gold, stone) = stock(&world);
    assert_eq!((gold, stone), (3, 200));
    assert_eq!(prices(&world).stone, 106);
}
