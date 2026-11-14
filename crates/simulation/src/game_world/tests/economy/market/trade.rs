//! Trade Cart runs between own markets: training, distance-based pay, order cancellation.
use super::super::buildings::{set_stock, stock, villager_at};
use super::super::*;
use crate::Tech;

/// Gold of a leg whose centres are `tiles` apart on x and `dy` on y.
fn gold(kind: UnitKind, tiles: u32, dy: u32, edge: i32) -> u32 {
    crate::game_world::economy::leg_gold(kind, tiles * 2, dy * 2, edge)
}

fn market(world: &mut GameWorld, x: i32, y: i32) -> EntityId {
    let rect = TileRect::from_xywh(x, y, 4, 4);
    world
        .insert_building(PlayerId(0), BuildingKind::Market, rect, true)
        .unwrap()
}

fn feudal_world_with_markets(far: i32) -> (GameWorld, EntityId, EntityId) {
    let (mut world, _, _) = village();
    world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
    let a = market(&mut world, 90, 80);
    let b = market(&mut world, 90 + far, 80);
    (world, a, b)
}

fn train_cart(world: &mut GameWorld, market: EntityId) -> EntityId {
    set_stock(world, 0, 100, 50, 0);
    let before = world.unit_count();
    world
        .issue_train(PlayerId(0), market, UnitKind::TradeCart)
        .unwrap();
    for _ in 0..2000 {
        world.advance();
        if world.unit_count() > before {
            break;
        }
    }
    world
        .units()
        .find(|u| u.kind == UnitKind::TradeCart)
        .expect("cart trained")
        .id
}

#[test]
fn trade_cart_costs_and_is_trained_only_at_a_market() {
    let (mut world, a, _) = feudal_world_with_markets(40);
    set_stock(&mut world, 0, 99, 50, 0);
    assert!(
        world
            .issue_train(PlayerId(0), a, UnitKind::TradeCart)
            .is_err()
    );
    let (_, center) = (
        0,
        world
            .buildings()
            .find(|b| b.kind == BuildingKind::TownCenter)
            .unwrap()
            .id,
    );
    set_stock(&mut world, 0, 100, 50, 0);
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::TradeCart),
        Err(EconomyError::WrongUnitKind)
    );
    world
        .issue_train(PlayerId(0), a, UnitKind::TradeCart)
        .unwrap();
    assert_eq!(stock(&world), (0, 0, 0, 0));
}

#[test]
fn trade_cart_shuttles_and_is_paid_on_arrival_at_each_end() {
    let (mut world, a, b) = feudal_world_with_markets(60);
    let cart = train_cart(&mut world, a);
    set_stock(&mut world, 0, 0, 0, 0);
    let expected = gold(UnitKind::TradeCart, 60, 0, 256);
    assert!(
        expected > 3,
        "a 60 tile route pays real gold, got {expected}"
    );
    let mut paid = Vec::new();
    let mut last = 0;
    for _ in 0..6000 {
        world.advance();
        let gold = stock(&world).2;
        if gold != last {
            paid.push(gold - last);
            last = gold;
        }
        if paid.len() == 2 {
            break;
        }
    }
    assert_eq!(paid, vec![expected, expected], "paid at b, then back at a");
    let unit = world.unit(cart).unwrap();
    let (fa, fb) = (
        world.building(a).unwrap().footprint,
        world.building(b).unwrap().footprint,
    );
    let near = |f: aoe_core::TileRect| {
        let t = unit.position.tile_floor();
        t.x >= f.min.x - 2 && t.x <= f.max.x + 1 && t.y >= f.min.y - 2 && t.y <= f.max.y + 1
    };
    assert!(near(fa) || near(fb));
}

#[test]
fn longer_routes_pay_more_and_a_manual_move_ends_the_run() {
    assert!(gold(UnitKind::TradeCart, 100, 0, 256) > 2 * gold(UnitKind::TradeCart, 50, 0, 256));
    assert!(gold(UnitKind::TradeCog, 60, 0, 256) > gold(UnitKind::TradeCart, 60, 0, 256));
    assert_eq!(gold(UnitKind::TradeCart, 5, 0, 256), 0);

    let (mut world, a, _) = feudal_world_with_markets(60);
    let cart = train_cart(&mut world, a);
    advance(&mut world, 20);
    assert!(world.economy.trade.contains_key(&cart));
    let to = WorldPosition::from_tile_center(TileCoord::new(80, 100)).unwrap();
    world.issue_move(cart, to).unwrap();
    assert!(!world.economy.trade.contains_key(&cart));
    // Ordering the cart to trade starts the loop again.
    world.issue_trade(PlayerId(0), cart, a).unwrap();
    assert!(world.economy.trade.contains_key(&cart));
}

#[test]
fn a_destroyed_partner_is_replaced_by_the_surviving_market_and_a_lone_one_has_no_route() {
    let (mut world, a, b) = feudal_world_with_markets(60);
    let cart = train_cart(&mut world, a);
    let c = market(&mut world, 90, 80 + 30);
    advance(&mut world, 5);
    world.remove_building(b);
    advance(&mut world, 200);
    let run = world
        .economy
        .trade
        .get(&cart)
        .copied()
        .expect("still trading");
    assert_eq!(
        (run.from, run.to),
        (a, Some(c)),
        "the run points at the survivor"
    );
    world.remove_building(c);
    // The cart notices when its walk ends.
    advance(&mut world, 1500);
    let run = world
        .economy
        .trade
        .get(&cart)
        .copied()
        .expect("still waiting");
    assert_eq!(run.to, None, "a lone market is not a route");
}

#[test]
fn partners_closer_than_twelve_tiles_are_not_a_route() {
    let (mut world, a, _) = feudal_world_with_markets(8);
    let cart = train_cart(&mut world, a);
    set_stock(&mut world, 0, 0, 0, 0);
    advance(&mut world, 400);
    assert_eq!(world.economy.trade.get(&cart).and_then(|r| r.to), None);
    assert_eq!(stock(&world).2, 0);
    // Twelve tiles and a little is a route.
    let (mut world, a, _) = feudal_world_with_markets(13);
    let cart = train_cart(&mut world, a);
    advance(&mut world, 400);
    assert!(world.economy.trade.get(&cart).and_then(|r| r.to).is_some());
}

#[test]
fn ordering_a_cart_parked_at_the_far_market_pays_only_after_it_walked_the_leg() {
    let (mut world, a, b) = feudal_world_with_markets(60);
    set_stock(&mut world, 0, 0, 0, 0);
    // Parked beside B, ordered to trade at A, repeatedly.
    let at = WorldPosition::from_tile_center(TileCoord::new(149, 82)).unwrap();
    let cart = world
        .spawn_kind(PlayerId(0), UnitKind::TradeCart, at)
        .unwrap();
    let footprint = world.building(a).unwrap().footprint;
    let mut seen_at_a = false;
    let mut paid = false;
    for tick in 0..8000 {
        if tick < 6 {
            world.issue_trade(PlayerId(0), cart, a).unwrap();
            assert_eq!(stock(&world).2, 0, "re-issuing the order earns nothing");
        }
        world.advance();
        let tile = world.unit(cart).unwrap().position.tile_floor();
        seen_at_a |= crate::game_world::economy::placement::adjacent(footprint, tile);
        if stock(&world).2 > 0 {
            assert!(seen_at_a, "paid before it ever reached A");
            paid = true;
            break;
        }
    }
    assert!(paid, "the A to B leg pays once walked");
    let _ = b;
}

#[test]
fn the_wonder_needs_imperial_age_and_its_dat_price_and_has_a_5x5_footprint() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 70, 70);
    let rules = BuildingKind::Wonder.rules();
    assert_eq!((rules.side, rules.depth, rules.max_hp), (5, 5, 4800));
    assert_eq!(
        (
            rules.cost.wood,
            rules.cost.stone,
            rules.cost.gold,
            rules.build_seconds
        ),
        (1000, 1000, 1000, 3500)
    );
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile = crate::Stockpile {
        food: 0,
        wood: 5000,
        gold: 5000,
        stone: 5000,
    };
    let origin = TileCoord::new(76, 70);
    super::super::fixture::survey(&mut world, PlayerId(0), TileCoord::new(78, 72), 12);
    let early = world.issue_build(PlayerId(0), &[builder], BuildingKind::Wonder, origin, false);
    assert!(early.is_err(), "Dark Age cannot build the Wonder");
    world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
    world.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    world.complete_research(PlayerId(0), Tech::ImperialAge, &mut Vec::new());
    let id = world
        .issue_build(PlayerId(0), &[builder], BuildingKind::Wonder, origin, false)
        .expect("Imperial Age builds the Wonder");
    let (_, wood, gold, stone) = stock(&world);
    assert_eq!((wood, gold, stone), (4000, 4000, 4000));
    let b = world.building(id).unwrap();
    assert_eq!(
        (b.footprint.width(), b.footprint.height(), b.max_hp),
        (5, 5, 4800)
    );
}

#[test]
fn caravan_speeds_trade_carts_and_cogs_by_a_fifth() {
    // DE help text: "Trade Units move +20% faster".
    let caravan = Tech::Caravan.bit();
    for kind in [UnitKind::TradeCart, UnitKind::TradeCog] {
        let base =
            crate::game_world::combat::military::apply_movement_speed(kind, Some(100), true, 0);
        let fast = crate::game_world::combat::military::apply_movement_speed(
            kind,
            Some(100),
            true,
            caravan,
        );
        assert_eq!((base, fast), (Some(100), Some(120)), "{kind:?}");
    }
    let villager = crate::game_world::combat::military::apply_movement_speed(
        UnitKind::Villager,
        Some(100),
        true,
        caravan,
    );
    assert_eq!(villager, Some(100));
}
