use super::*;
use crate::{AnimalKind, EconomyError, UnitTask};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn stop_sheep_cancels_herding_at_its_exact_position_and_replays() {
    let mut worlds = [world(), world()];
    for w in &mut worlds {
        w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
        w.advance();
        w.issue_herd(PlayerId(0), sheep, at(110, 100)).unwrap();
        for _ in 0..7 {
            w.advance();
        }
        let moving = *w.animal(sheep).unwrap();
        assert!(moving.moving);
        assert_ne!(
            moving.position,
            w.config().snap_ground_position(moving.position)
        );
        assert_eq!(
            w.issue_stop(PlayerId(1), sheep),
            Err(EconomyError::NotOwner)
        );
        w.issue_stop(PlayerId(0), sheep).unwrap();
        assert!(!w.animal(sheep).unwrap().moving);
        for _ in 0..100 {
            w.advance();
        }
        assert_eq!(w.animal(sheep).unwrap().position, moving.position);
        assert!(!w.animal(sheep).unwrap().moving);
        w.issue_herd(PlayerId(0), sheep, at(104, 100)).unwrap();
        w.advance();
        assert!(w.animal(sheep).unwrap().moving, "a later order still works");
    }
    assert_eq!(worlds[0].canonical_hash(), worlds[1].canonical_hash());
}

#[test]
fn stop_rejects_unknown_neutral_dead_and_non_sheep_animals() {
    let mut w = world();
    assert_eq!(
        w.issue_stop(PlayerId(0), EntityId(u32::MAX)),
        Err(EconomyError::UnknownEntity)
    );
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(100, 100)).unwrap();
    assert_eq!(
        w.issue_stop(PlayerId(0), sheep),
        Err(EconomyError::NotOwner)
    );
    let deer = w.spawn_animal(AnimalKind::Deer, at(110, 100)).unwrap();
    assert_eq!(
        w.issue_stop(PlayerId(0), deer),
        Err(EconomyError::InvalidTarget)
    );
    w.strike_animal(sheep, EntityId(0), u16::MAX);
    assert_eq!(
        w.issue_stop(PlayerId(0), sheep),
        Err(EconomyError::InvalidTarget)
    );
}

#[test]
fn stop_units_cancels_movement_and_hunting_without_snapping() {
    let mut w = world();
    let unit = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(108, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[unit], sheep).unwrap();
    for _ in 0..7 {
        w.advance();
    }
    let position = w.unit(unit).unwrap().position;
    assert!(w.unit(unit).unwrap().moving);
    assert_eq!(w.issue_stop(PlayerId(1), unit), Err(EconomyError::NotOwner));
    w.issue_stop(PlayerId(0), unit).unwrap();
    assert_eq!(w.hunting_target(unit), None);
    assert_eq!(w.unit(unit).unwrap().task, UnitTask::Idle);
    for _ in 0..100 {
        w.advance();
    }
    assert_eq!(w.unit(unit).unwrap().position, position);
    assert!(!w.unit(unit).unwrap().moving);
    assert!(!w.unit(unit).unwrap().planning);
}
