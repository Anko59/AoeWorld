use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig, WorldPosition};
use aoe_protocol::UnitTask;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// A Dying body keeps its General records in the simulation until removed; the
/// wire must not carry them, or every Snapshot and Tick fails validation.
#[test]
fn dying_commanded_soldiers_and_generals_carry_no_command_fields() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let soldier = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::Militia, at(30, 30))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::General, at(32, 30))
        .unwrap();
    let living = |id| unit_state(&world, &world.unit(id).unwrap());
    assert_eq!(living(soldier).commanded_by, Some(general));
    assert!(living(general).general.is_some());
    for id in [soldier, general] {
        let mut body = world.unit(id).unwrap();
        body.hp = 0;
        body.task = aoe_simulation::UnitTask::Dying;
        let mut state = unit_state(&world, &body);
        assert_eq!(state.task, UnitTask::Dying);
        assert_eq!(
            (state.commanded_by, state.home_castle, state.managed_by),
            (None, None, None)
        );
        assert!(state.general.is_none());
        state.death_tick = body.death_tick.or(Some(aoe_core::Tick(1)));
        let message = aoe_protocol::GameplayServerMessage::Snapshot {
            revision: 1,
            tick: aoe_core::Tick(2),
            units: vec![state],
            players: vec![],
            buildings: vec![],
        };
        assert!(aoe_protocol::gameplay_server_violations(&message).is_empty());
    }
}

/// A corpse whose task was rewritten after death still reaches the wire as Dying.
#[test]
fn a_body_at_zero_hp_is_always_dying_on_the_wire() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::Militia, at(30, 30))
        .unwrap();
    let mut body = world.unit(id).unwrap();
    body.hp = 0;
    body.task = aoe_simulation::UnitTask::Idle;
    body.moving = true;
    body.attack_target = Some(id);
    let state = unit_state(&world, &body);
    assert_eq!(state.task, UnitTask::Dying);
    assert!(!state.moving && state.attack_target.is_none());
}
