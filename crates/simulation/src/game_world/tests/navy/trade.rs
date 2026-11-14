//! Trade Cogs sail between two own Docks and are paid on arrival.
use super::*;
use crate::{BuildingKind, Stockpile, Tech};

/// A dock on land beside the water tile `water` (its east edge touches the bank).
fn dock(world: &mut GameWorld, water: TileCoord) -> EntityId {
    let rect = TileRect::from_xywh(water.x - 3, water.y - 1, 3, 3);
    world
        .insert_building(PlayerId(0), BuildingKind::Dock, rect, true)
        .unwrap()
}

#[test]
fn a_trade_cog_shuttles_between_two_docks_and_is_paid_for_each_leg() {
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
    let (_, water) = bank(&world);
    // The same bank, twenty tiles on: the coast is straight in the fixture.
    let far = TileCoord::new(water.x, water.y + 20);
    let first = dock(&mut world, water);
    let second = dock(&mut world, far);
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile = Stockpile {
        food: 0,
        wood: 100,
        gold: 50,
        stone: 0,
    };
    world
        .issue_train(PlayerId(0), first, UnitKind::TradeCog)
        .unwrap();
    let cog = loop_until_trained(&mut world);
    assert!(world.unit(cog).unwrap().kind.is_naval());
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile = Stockpile::default();
    let mut paid = 0;
    for _ in 0..4000 {
        world.advance();
        paid = world.player_economy(PlayerId(0)).unwrap().stockpile.gold;
        if paid > 0 {
            break;
        }
    }
    assert!(paid > 0, "the cog reached the other dock and was paid");
    let to = world.economy.trade.get(&cog).map(|r| r.from);
    assert_eq!(to, Some(second), "the next leg starts at the second dock");
    // A manual sailing order ends the run.
    world.issue_move(cog, at(water)).unwrap();
    assert!(!world.economy.trade.contains_key(&cog));
}

fn loop_until_trained(world: &mut GameWorld) -> EntityId {
    for _ in 0..2000 {
        world.advance();
        if let Some(u) = world.units().find(|u| u.kind == UnitKind::TradeCog) {
            return u.id;
        }
    }
    panic!("the Dock trains a Trade Cog");
}
