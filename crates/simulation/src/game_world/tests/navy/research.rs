use super::*;
use crate::{Age, BuildingKind, Stockpile, Tech};

#[test]
fn arrow_research_immediately_reveals_stationary_galley_and_war_galley_vision() {
    for kind in [UnitKind::Galley, UnitKind::WarGalley] {
        let mut world = coast();
        world
            .place_town_center(PlayerId(0), TileCoord::new(10, 10))
            .unwrap();
        let (_, water) = bank(&world);
        let id = ship(&mut world, kind, water, 0);
        let base = i32::from(crate::unit_combat(kind).los);
        for (offset, tech) in [(1, Tech::Fletching), (2, Tech::BodkinArrow)] {
            let tile = TileCoord::new(water.x + base + offset, water.y);
            assert_eq!(
                world.visibility(PlayerId(0), tile),
                crate::TileVisibility::Unexplored
            );
            world.complete_research(PlayerId(0), tech, &mut Vec::new());
            assert_eq!(
                world.visibility(PlayerId(0), tile),
                crate::TileVisibility::Visible,
                "{kind:?} immediately reveals its new LOS after {tech:?}"
            );
            assert_eq!(world.unit(id).unwrap().position, at(water));
        }
    }
}

#[test]
fn castle_camps_keep_the_same_los_as_new_camps_with_or_without_town_watch() {
    for town_watch in [false, true] {
        let mut world = world();
        let mut camps = Vec::new();
        for (i, kind) in [
            BuildingKind::LumberCamp,
            BuildingKind::MiningCamp,
            BuildingKind::Mill,
        ]
        .into_iter()
        .enumerate()
        {
            let id = world
                .insert_building(
                    PlayerId(0),
                    kind,
                    TileRect::from_xywh(20 + i as i32 * 8, 20, 2, 2),
                    true,
                )
                .unwrap();
            camps.push((kind, id));
        }
        world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        if town_watch {
            world.complete_research(PlayerId(0), Tech::TownWatch, &mut Vec::new());
        }
        let expected = if town_watch { 10 } else { 6 };
        assert!(
            camps
                .iter()
                .all(|(_, id)| world.building(*id).unwrap().los == expected)
        );
        world.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        for (i, (kind, old)) in camps.into_iter().enumerate() {
            let new = world
                .insert_building(
                    PlayerId(0),
                    kind,
                    TileRect::from_xywh(20 + i as i32 * 8, 30, 2, 2),
                    true,
                )
                .unwrap();
            assert_eq!(
                (
                    world.building(old).unwrap().los,
                    world.building(new).unwrap().los
                ),
                (expected, expected),
                "{kind:?}, TownWatch={town_watch}"
            );
        }
    }
}

#[test]
fn castle_age_enables_war_galley_and_upgrades_existing_queued_and_future_ships() {
    let mut world = coast();
    let center = world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let (land, water) = bank(&world);
    let dock = |world: &mut GameWorld, dy| {
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::Dock,
                TileRect::from_xywh(land.x - 1, land.y + dy, 3, 3),
                true,
            )
            .unwrap()
    };
    let research_dock = dock(&mut world, 4);
    let training_dock = dock(&mut world, 12);
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.stockpile = Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 0,
    };
    player.age = Age::Feudal;
    player.researched |= Tech::FeudalAge.bit();
    assert_eq!(
        world.issue_research(PlayerId(0), research_dock, Tech::WarGalley),
        Err(EconomyError::RequiresCastleAge)
    );
    assert_eq!(
        world.issue_train(PlayerId(0), training_dock, UnitKind::WarGalley),
        Err(EconomyError::RequiresCastleAge)
    );
    // Combined Castle progression requires two distinct Feudal prerequisites.
    for (kind, dy) in [(BuildingKind::Blacksmith, 20), (BuildingKind::Market, 26)] {
        let side = kind.rules().side;
        world
            .insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(land.x - side - 2, land.y + dy, side, side),
                true,
            )
            .unwrap();
    }
    world
        .issue_research(PlayerId(0), center, Tech::CastleAge)
        .unwrap();
    advance(&mut world, Tech::CastleAge.rules().seconds * 20 - 1);
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Feudal);
    world.advance();
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Castle);
    assert!(
        world.is_feudal(PlayerId(0)),
        "Castle retains Feudal prerequisites"
    );

    let damaged = ship(&mut world, UnitKind::Galley, water, 0);
    world.units[world.lookup[&damaged]].state.hp = 90;
    let enemy = ship(
        &mut world,
        UnitKind::Galley,
        TileCoord::new(water.x + 2, water.y),
        1,
    );
    let fishing = ship(
        &mut world,
        UnitKind::FishingShip,
        TileCoord::new(water.x + 3, water.y),
        0,
    );
    // Research timing is measured without the two idle fleets fighting.
    for unit in &mut world.units {
        unit.state.stance = aoe_core::CombatStance::NoAttack;
    }
    for _ in 0..2 {
        world
            .issue_train(PlayerId(0), training_dock, UnitKind::Galley)
            .unwrap();
    }
    world
        .issue_research(PlayerId(0), research_dock, Tech::WarGalley)
        .unwrap();
    advance(&mut world, Tech::WarGalley.rules().seconds * 20 - 1);
    assert_eq!(world.unit(damaged).unwrap().kind, UnitKind::Galley);
    world.advance();
    let upgraded = world.unit(damaged).unwrap();
    assert_eq!(
        (upgraded.kind, upgraded.hp, upgraded.max_hp),
        (UnitKind::WarGalley, 105, 135)
    );
    assert_eq!(world.unit(enemy).unwrap().kind, UnitKind::Galley);
    assert_eq!(world.unit(fishing).unwrap().kind, UnitKind::FishingShip);
    let queue = &world.building(training_dock).unwrap().training_queue;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].kind, UnitKind::WarGalley);
    assert!(
        world
            .units()
            .filter(|unit| unit.player == PlayerId(0))
            .all(|unit| unit.kind != UnitKind::Galley)
    );
    advance(&mut world, 23 * 20);
    assert!(
        world
            .building(training_dock)
            .unwrap()
            .training_queue
            .is_empty()
    );
    assert_eq!(
        world
            .units()
            .filter(|unit| unit.player == PlayerId(0) && unit.kind == UnitKind::WarGalley)
            .count(),
        3
    );
    world
        .issue_train(PlayerId(0), training_dock, UnitKind::Galley)
        .unwrap();
    assert_eq!(
        world.building(training_dock).unwrap().training_queue[0].kind,
        UnitKind::WarGalley
    );
    advance(&mut world, 36 * 20);
    assert_eq!(
        world
            .units()
            .filter(|unit| unit.player == PlayerId(0) && unit.kind == UnitKind::WarGalley)
            .count(),
        4
    );
}
