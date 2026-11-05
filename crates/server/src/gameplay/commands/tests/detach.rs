use super::*;
use aoe_core::{PlayerId, Seed, TileCoord};
use aoe_simulation::{GameWorld, UnitKind};

fn world() -> GameWorld {
    GameWorld::new(aoe_core::WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap()
}
fn at(x: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap()
}
fn run(world: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        world.advance();
    }
}

#[test]
fn an_order_to_a_soldier_never_in_an_army_keeps_a_later_general_from_taking_it() {
    let mut world = world();
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(102))
        .unwrap();
    let stop = PendingAction::Stop { entity_id: soldier };
    assert_eq!(
        apply(&mut world, PlayerId(0), stop),
        CommandResult::Accepted
    );
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(100))
        .unwrap();
    run(&mut world, 100);
    assert_eq!(world.general_controller(soldier), None);
    assert!(world.unit(general).is_some());
}

#[test]
fn a_villager_order_records_no_detachment() {
    let mut world = world();
    let villager = world
        .spawn_kind(PlayerId(0), UnitKind::Villager, at(102))
        .unwrap();
    let stop = PendingAction::Stop {
        entity_id: villager,
    };
    apply(&mut world, PlayerId(0), stop);
    assert_eq!(world.general_detached_count(), 0);
}

#[test]
fn moving_a_general_alone_readmits_idle_detached_soldiers() {
    let mut world = world();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(100))
        .unwrap();
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(102))
        .unwrap();
    run(&mut world, 30);
    assert_eq!(world.general_controller(soldier), Some(general));
    let stop = PendingAction::Stop { entity_id: soldier };
    apply(&mut world, PlayerId(0), stop);
    assert_eq!(world.general_controller(soldier), None);
    run(&mut world, 400);
    assert_eq!(world.general_controller(soldier), None, "still detached");
    let alone = PendingAction::GeneralMoveAlone {
        entity_id: general,
        destination: at(104),
    };
    assert_eq!(
        apply(&mut world, PlayerId(0), alone),
        CommandResult::Accepted
    );
    run(&mut world, 100);
    assert_eq!(world.general_controller(soldier), Some(general));
}
