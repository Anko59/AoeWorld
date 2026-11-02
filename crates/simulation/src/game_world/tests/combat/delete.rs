//! Owner delete: units die and buildings fall through the normal death and
//! destruction paths; foundations refund; other players' entities are refused.
use super::*;
use crate::{BuildingKind, EconomyError, UnitKind, UnitTask};

fn enter(world: &mut GameWorld, building: EntityId, kind: UnitKind) -> EntityId {
    let unit = world.spawn_kind(PlayerId(0), kind, at(99, 100)).unwrap();
    world.issue_garrison(PlayerId(0), unit, building).unwrap();
    world.advance();
    assert!(world.unit(unit).is_none(), "garrisoned unit left the map");
    unit
}

fn wood(world: &GameWorld, player: PlayerId) -> u32 {
    world.player_economy(player).unwrap().stockpile.wood
}

#[test]
fn delete_kills_an_own_unit_through_the_normal_death_path() {
    let mut world = world();
    world
        .place_town_center(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    let unit = world.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let used = world.player_economy(PlayerId(0)).unwrap().population_used;
    world.issue_delete(PlayerId(0), &[unit]).unwrap();
    let state = world.unit(unit).expect("the body keeps its death playback");
    assert_eq!((state.hp, state.task), (0, UnitTask::Dying));
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        used - 1,
        "population is freed at once, as for any death"
    );
    assert!(
        world.combat.dying.iter().any(|&(_, id)| id == unit),
        "the body decays on the ordinary corpse timer"
    );
}

#[test]
fn delete_destroys_an_own_building_and_ejects_its_occupants() {
    let mut world = world();
    world.set_fog_disabled(true);
    let side = BuildingKind::WatchTower.rules().side;
    let tower = world
        .insert_building(
            PlayerId(0),
            BuildingKind::WatchTower,
            TileRect::from_xywh(100, 100, side, side),
            true,
        )
        .unwrap();
    let occupant = enter(&mut world, tower, UnitKind::Villager);
    world.issue_delete(PlayerId(0), &[tower]).unwrap();
    assert!(world.building(tower).is_none());
    let unit = world
        .unit(occupant)
        .expect("the destroyed building ejects its occupant");
    assert!(unit.hp > 0);
    assert!(world.economy.ejections.is_empty());
}

#[test]
fn delete_cancels_an_own_foundation_and_refunds_what_was_paid() {
    let mut world = world();
    world
        .place_town_center(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .wood = 100;
    let builder = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    let paid = 100 - wood(&world, PlayerId(0));
    assert!(paid > 0, "placing the foundation charged wood");
    world.issue_delete(PlayerId(0), &[house]).unwrap();
    assert!(world.building(house).is_none());
    assert_eq!(wood(&world, PlayerId(0)), 100, "the full cost came back");
}

#[test]
fn delete_refuses_other_players_entities_without_side_effects() {
    let mut world = world();
    let own = world.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let theirs = world.spawn_villager(PlayerId(1), at(110, 100)).unwrap();
    let their_center = world
        .place_town_center(PlayerId(1), TileCoord::new(120, 60))
        .unwrap();
    assert!(matches!(
        world.issue_delete(PlayerId(0), &[own, theirs]),
        Err(EconomyError::NotOwner)
    ));
    assert!(matches!(
        world.issue_delete(PlayerId(0), &[their_center]),
        Err(EconomyError::NotOwner)
    ));
    assert!(matches!(
        world.issue_delete(PlayerId(1), &[own]),
        Err(EconomyError::NotOwner)
    ));
    assert!(world.unit(own).unwrap().hp > 0);
    assert!(world.unit(theirs).unwrap().hp > 0);
    assert!(world.building(their_center).is_some());
    assert!(matches!(
        world.issue_delete(PlayerId(0), &[EntityId(9_999)]),
        Err(EconomyError::UnknownEntity)
    ));
    assert!(matches!(
        world.issue_delete(PlayerId(0), &[own, own]),
        Err(EconomyError::OrderTooLarge)
    ));
    assert!(matches!(
        world.issue_delete(PlayerId(0), &[]),
        Err(EconomyError::OrderTooLarge)
    ));
    assert!(world.unit(own).unwrap().hp > 0);
}
